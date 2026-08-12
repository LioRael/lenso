use crate::db::DbPool;
use crate::error::{AppError, AppResult, ErrorCode};
use crate::telemetry_attrs::RuntimeSpanAttributes;
use crate::{ExecutionContext, TraceContext};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::fmt::Debug;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::field::{Field, Visit};
use tracing::instrument::WithSubscriber as _;
use tracing::span::{Attributes, Id, Record};
use tracing::subscriber::Interest;
use tracing::{Dispatch, Metadata};
use tracing::{Event, Subscriber};
use tracing_core::span::Current;
use uuid::Uuid;

pub const EXECUTION_LOG_TARGET: &str = "lenso::execution";
const EXECUTION_LOG_CHANNEL_CAPACITY: usize = 128;
const EXECUTION_LOG_DRAIN_TIMEOUT: Duration = Duration::from_millis(100);
const MAX_EXECUTION_LOG_BODY_BYTES: usize = 4 * 1024;
const MAX_EXECUTION_LOG_ATTRIBUTES_BYTES: usize = 16 * 1024;
const MAX_EXECUTION_LOG_SCOPE_ATTRIBUTE_BYTES: usize = 1024;
const MAX_EXECUTION_LOG_REDACTED_FIELDS: usize = 128;
const MAX_EXECUTION_LOG_REDACTED_FIELD_BYTES: usize = 4 * 1024;
const REDACTED_VALUE: &str = "[REDACTED]";
const REDACTED_FIELDS_TRUNCATED: &str = "[truncated]";
const SENSITIVE_ATTRIBUTE_TERMS: [&str; 10] = [
    "authorization",
    "cookie",
    "password",
    "passwd",
    "secret",
    "token",
    "apikey",
    "accesskey",
    "credential",
    "email",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[doc(hidden)]
pub enum ExecutionLogSeverity {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl ExecutionLogSeverity {
    fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone)]
#[doc(hidden)]
pub struct ExecutionLogRecord {
    correlation_id: String,
    story_id: String,
    execution_id: String,
    execution_type: String,
    execution_name: String,
    severity: ExecutionLogSeverity,
    body: String,
    attributes: Value,
    trace: TraceContext,
    service_name: String,
    redacted_fields: Vec<String>,
    occurred_at: DateTime<Utc>,
}

impl ExecutionLogRecord {
    pub(crate) fn from_runtime_attrs(
        attrs: RuntimeSpanAttributes,
        severity: ExecutionLogSeverity,
        body: impl Into<String>,
    ) -> Self {
        let execution_id = attrs
            .function_run_id
            .clone()
            .or_else(|| attrs.outbox_event_id.clone())
            .unwrap_or_else(|| attrs.story_id.clone());

        Self {
            correlation_id: attrs.correlation_id,
            story_id: attrs.story_id,
            execution_id,
            execution_type: attrs.execution_kind,
            execution_name: attrs.execution_name,
            severity,
            body: body.into(),
            attributes: Value::Object(Map::default()),
            trace: TraceContext::default(),
            service_name: "lenso".to_owned(),
            redacted_fields: Vec::new(),
            occurred_at: Utc::now(),
        }
    }

    pub(crate) fn with_attributes(mut self, attributes: Value) -> Self {
        self.attributes = attributes;
        self
    }

    pub(crate) fn with_trace(mut self, trace: TraceContext) -> Self {
        self.trace = trace;
        self
    }
}

impl ExecutionLogRecord {
    #[cfg(test)]
    fn attributes(&self) -> &Value {
        &self.attributes
    }

    #[cfg(test)]
    fn redacted_fields(&self) -> &[String] {
        &self.redacted_fields
    }
}

#[doc(hidden)]
pub async fn insert_execution_log_projection(
    pool: &DbPool,
    record: ExecutionLogRecord,
) -> AppResult<String> {
    let id = next_execution_log_id();
    sqlx::query(
        r#"
        insert into platform.execution_logs (
            id,
            correlation_id,
            story_id,
            execution_id,
            execution_type,
            execution_name,
            occurred_at,
            severity,
            body,
            attributes,
            trace_id,
            span_id,
            service_name,
            redacted_fields
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        "#,
    )
    .bind(&id)
    .bind(&record.correlation_id)
    .bind(&record.story_id)
    .bind(&record.execution_id)
    .bind(&record.execution_type)
    .bind(&record.execution_name)
    .bind(record.occurred_at)
    .bind(record.severity.as_str())
    .bind(&record.body)
    .bind(normalize_attributes(record.attributes))
    .bind(&record.trace.trace_id)
    .bind(&record.trace.span_id)
    .bind(&record.service_name)
    .bind(&record.redacted_fields)
    .execute(pool)
    .await
    .map_err(map_execution_log_error)?;

    Ok(id)
}

/// Store adapter for execution logs captured from application `tracing` events.
///
/// This is public only so runtime composition can inject local and test
/// adapters. Application code should emit `tracing` events instead of calling
/// this seam directly.
#[async_trait]
#[doc(hidden)]
pub trait ExecutionLogWriter: Debug + Send + Sync {
    async fn write_execution_log(&self, record: ExecutionLogRecord) -> AppResult<String>;
}

#[derive(Debug, Clone)]
#[doc(hidden)]
pub struct PostgresExecutionLogWriter {
    pool: DbPool,
}

impl PostgresExecutionLogWriter {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ExecutionLogWriter for PostgresExecutionLogWriter {
    async fn write_execution_log(&self, record: ExecutionLogRecord) -> AppResult<String> {
        insert_execution_log_projection(&self.pool, record).await
    }
}

#[derive(Debug, Clone)]
#[doc(hidden)]
pub struct ExecutionLogScope {
    correlation_id: String,
    story_id: String,
    execution_id: String,
    execution_type: String,
    execution_name: String,
    trace: TraceContext,
    service_name: String,
    protected_attributes: Map<String, Value>,
}

impl ExecutionLogScope {
    pub fn function(
        context: &ExecutionContext,
        service_name: impl Into<String>,
        workload_id: impl Into<String>,
    ) -> Self {
        let mut protected_attributes = Map::new();
        let mut scope_truncated = false;
        protected_attributes.insert(
            "lenso.correlation_id".to_owned(),
            bounded_scope_attribute(&context.correlation_id.0, &mut scope_truncated),
        );
        protected_attributes.insert(
            "lenso.story_id".to_owned(),
            bounded_scope_attribute(&context.correlation_id.0, &mut scope_truncated),
        );
        protected_attributes.insert(
            "lenso.function_run_id".to_owned(),
            bounded_scope_attribute(&context.execution_id.0, &mut scope_truncated),
        );
        protected_attributes.insert(
            "lenso.execution.kind".to_owned(),
            Value::String("function_run".to_owned()),
        );
        protected_attributes.insert(
            "lenso.execution.name".to_owned(),
            bounded_scope_attribute(&context.function_name, &mut scope_truncated),
        );
        protected_attributes.insert(
            "lenso.execution.attempt".to_owned(),
            Value::from(context.attempt),
        );
        protected_attributes.insert(
            "lenso.execution.queue".to_owned(),
            bounded_scope_attribute(&context.queue, &mut scope_truncated),
        );
        let workload_id = workload_id.into();
        protected_attributes.insert(
            "lenso.workload.id".to_owned(),
            bounded_scope_attribute(&workload_id, &mut scope_truncated),
        );
        if let Some(tenant_id) = context.tenant_id.as_ref() {
            protected_attributes.insert(
                "lenso.tenant.id".to_owned(),
                bounded_scope_attribute(&tenant_id.0, &mut scope_truncated),
            );
        }
        if scope_truncated {
            protected_attributes.insert("lenso.log.scope_truncated".to_owned(), Value::Bool(true));
        }

        Self {
            correlation_id: context.correlation_id.0.clone(),
            story_id: context.correlation_id.0.clone(),
            execution_id: context.execution_id.0.clone(),
            execution_type: "function_run".to_owned(),
            execution_name: context.function_name.clone(),
            trace: context.trace.clone(),
            service_name: service_name.into(),
            protected_attributes,
        }
    }

    fn record(
        &self,
        severity: ExecutionLogSeverity,
        body: String,
        mut attributes: Map<String, Value>,
    ) -> ExecutionLogRecord {
        let mut redactions = RedactionTracker::default();
        sanitize_attributes(&mut attributes, "", &mut redactions);
        let body = if body_contains_sensitive_content(&body) {
            redactions.record("body");
            REDACTED_VALUE.to_owned()
        } else {
            body
        };
        let redacted_fields = redactions.finish();
        let (body, body_truncated) = truncate_utf8(body, MAX_EXECUTION_LOG_BODY_BYTES);
        let mut protected_attributes = self.protected_attributes.clone();
        if body_truncated {
            protected_attributes.insert("lenso.log.body_truncated".to_owned(), Value::Bool(true));
        }
        let attributes = bound_attributes(attributes, protected_attributes);
        ExecutionLogRecord {
            correlation_id: self.correlation_id.clone(),
            story_id: self.story_id.clone(),
            execution_id: self.execution_id.clone(),
            execution_type: self.execution_type.clone(),
            execution_name: self.execution_name.clone(),
            severity,
            body,
            attributes: Value::Object(attributes),
            trace: self.trace.clone(),
            service_name: self.service_name.clone(),
            redacted_fields,
            occurred_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub enum ExecutionLogCaptureStatus {
    Complete,
    Partial,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub struct ExecutionLogCaptureReport {
    pub status: ExecutionLogCaptureStatus,
    pub observed: u64,
    pub persisted: u64,
    pub dropped: u64,
    pub write_failures: u64,
}

impl ExecutionLogCaptureReport {
    fn disabled() -> Self {
        Self {
            status: ExecutionLogCaptureStatus::Disabled,
            observed: 0,
            persisted: 0,
            dropped: 0,
            write_failures: 0,
        }
    }
}

#[derive(Debug, Clone)]
struct ExecutionLogCaptureContext {
    scope: Arc<ExecutionLogScope>,
    sender: mpsc::Sender<ExecutionLogRecord>,
    observed: Arc<AtomicU64>,
    dropped: Arc<AtomicU64>,
}

#[derive(Debug)]
struct AbortTaskOnDrop<T> {
    handle: JoinHandle<T>,
}

impl<T> AbortTaskOnDrop<T> {
    fn new(handle: JoinHandle<T>) -> Self {
        Self { handle }
    }
}

impl<T> Drop for AbortTaskOnDrop<T> {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

tokio::task_local! {
    static EXECUTION_LOG_CAPTURE: ExecutionLogCaptureContext;
}

/// Captures `target = "lenso::execution"` events while `future` is polled and
/// drains them through the injected writer without entering the business
/// result path.
#[doc(hidden)]
pub async fn capture_execution_logs<F>(
    scope: ExecutionLogScope,
    writer: Option<Arc<dyn ExecutionLogWriter>>,
    future: F,
) -> (F::Output, ExecutionLogCaptureReport)
where
    F: Future,
{
    let Some(writer) = writer else {
        let host_dispatch = tracing::dispatcher::get_default(Clone::clone);
        let capture_dispatch = ExecutionLogCaptureSubscriber::new(host_dispatch);
        return (
            future.with_subscriber(capture_dispatch).await,
            ExecutionLogCaptureReport::disabled(),
        );
    };
    let (sender, mut receiver) = mpsc::channel(EXECUTION_LOG_CHANNEL_CAPACITY);
    let observed = Arc::new(AtomicU64::new(0));
    let dropped = Arc::new(AtomicU64::new(0));
    let persisted = Arc::new(AtomicU64::new(0));
    let write_failures = Arc::new(AtomicU64::new(0));
    let capture = ExecutionLogCaptureContext {
        scope: Arc::new(scope),
        sender,
        observed: observed.clone(),
        dropped: dropped.clone(),
    };
    let host_dispatch = tracing::dispatcher::get_default(Clone::clone);
    let drain_persisted = persisted.clone();
    let drain_write_failures = write_failures.clone();
    let drain_host_dispatch = host_dispatch.clone();
    let drain = async move {
        while let Some(record) = receiver.recv().await {
            tracing::dispatcher::with_default(&drain_host_dispatch, || {
                emit_sanitized_execution_log(&record);
            });
            match writer.write_execution_log(record).await {
                Ok(_) => {
                    drain_persisted.fetch_add(1, Ordering::Relaxed);
                }
                Err(error) => {
                    drain_write_failures.fetch_add(1, Ordering::Relaxed);
                    tracing::warn!(
                        error = ?error,
                        "failed to persist structured execution log"
                    );
                }
            }
        }
    };

    let mut drain = AbortTaskOnDrop::new(tokio::spawn(drain));
    let capture_dispatch = ExecutionLogCaptureSubscriber::new(host_dispatch);
    let output = EXECUTION_LOG_CAPTURE
        .scope(capture, future.with_subscriber(capture_dispatch))
        .await;
    let drain_timed_out = tokio::time::timeout(EXECUTION_LOG_DRAIN_TIMEOUT, &mut drain.handle)
        .await
        .is_err();
    if drain_timed_out {
        drain.handle.abort();
        let _ = (&mut drain.handle).await;
    }

    let observed = observed.load(Ordering::Relaxed);
    let persisted = persisted.load(Ordering::Relaxed);
    let write_failures = write_failures.load(Ordering::Relaxed);
    let directly_dropped = dropped.load(Ordering::Relaxed);
    let abandoned = observed.saturating_sub(
        directly_dropped
            .saturating_add(persisted)
            .saturating_add(write_failures),
    );
    let dropped = directly_dropped.saturating_add(abandoned);
    let status = if dropped == 0 && write_failures == 0 && !drain_timed_out {
        ExecutionLogCaptureStatus::Complete
    } else {
        ExecutionLogCaptureStatus::Partial
    };

    (
        output,
        ExecutionLogCaptureReport {
            status,
            observed,
            persisted,
            dropped,
            write_failures,
        },
    )
}

/// Scoped forwarding subscriber used only while a function handler is polled.
///
/// Standard span and event callbacks are delegated to the Host subscriber, but
/// `Subscriber` does not expose a way to forward its downcast implementation.
/// Dispatch-downcast extension APIs (including `OpenTelemetrySpanExt` context
/// methods) are therefore unavailable inside this scope. Ordinary tracing spans,
/// events, and an already-installed OpenTelemetry layer continue to receive
/// delegated callbacks.
#[derive(Debug, Clone)]
struct ExecutionLogCaptureSubscriber {
    inner: Dispatch,
}

impl ExecutionLogCaptureSubscriber {
    fn new(inner: Dispatch) -> Self {
        Self { inner }
    }
}

impl Subscriber for ExecutionLogCaptureSubscriber {
    fn register_callsite(&self, metadata: &'static Metadata<'static>) -> Interest {
        if metadata.target() == EXECUTION_LOG_TARGET {
            Interest::always()
        } else {
            self.inner.register_callsite(metadata)
        }
    }

    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.target() == EXECUTION_LOG_TARGET || self.inner.enabled(metadata)
    }

    fn max_level_hint(&self) -> Option<tracing::metadata::LevelFilter> {
        None
    }

    fn new_span(&self, span: &Attributes<'_>) -> Id {
        self.inner.new_span(span)
    }

    fn record(&self, span: &Id, values: &Record<'_>) {
        self.inner.record(span, values);
    }

    fn record_follows_from(&self, span: &Id, follows: &Id) {
        self.inner.record_follows_from(span, follows);
    }

    fn event(&self, event: &Event<'_>) {
        if event.metadata().target() == EXECUTION_LOG_TARGET {
            capture_execution_log_event(event);
        } else {
            self.inner.event(event);
        }
    }

    fn enter(&self, span: &Id) {
        self.inner.enter(span);
    }

    fn exit(&self, span: &Id) {
        self.inner.exit(span);
    }

    fn clone_span(&self, id: &Id) -> Id {
        self.inner.clone_span(id)
    }

    fn try_close(&self, id: Id) -> bool {
        self.inner.try_close(id)
    }

    fn current_span(&self) -> Current {
        self.inner.current_span()
    }
}

fn capture_execution_log_event(event: &Event<'_>) {
    let mut visitor = ExecutionLogEventVisitor::default();
    event.record(&mut visitor);
    let body = visitor
        .body
        .unwrap_or_else(|| event.metadata().name().to_owned());
    let severity = severity_from_level(*event.metadata().level());
    let _ = EXECUTION_LOG_CAPTURE.try_with(|capture| {
        capture.observed.fetch_add(1, Ordering::Relaxed);
        let record = capture.scope.record(severity, body, visitor.attributes);
        if capture.sender.try_send(record).is_err() {
            capture.dropped.fetch_add(1, Ordering::Relaxed);
        }
    });
}

#[derive(Debug, Default)]
struct ExecutionLogEventVisitor {
    body: Option<String>,
    attributes: Map<String, Value>,
}

impl ExecutionLogEventVisitor {
    fn record_value(&mut self, field: &Field, value: Value) {
        let name = field.name();
        if name == "message" {
            self.body = value
                .as_str()
                .map(ToOwned::to_owned)
                .or_else(|| Some(value.to_string()));
            return;
        }
        if is_reserved_attribute(name) {
            return;
        }
        if name == "attributes" {
            if let Some(raw) = value.as_str()
                && raw.len() <= MAX_EXECUTION_LOG_ATTRIBUTES_BYTES
                && let Ok(Value::Object(attributes)) = serde_json::from_str::<Value>(raw)
            {
                self.attributes.extend(attributes);
                return;
            }
            self.attributes.insert(
                "attributes".to_owned(),
                Value::String(REDACTED_VALUE.to_owned()),
            );
            return;
        }
        self.attributes.insert(name.to_owned(), value);
    }
}

impl Visit for ExecutionLogEventVisitor {
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.record_value(field, Value::from(value));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.record_value(field, Value::from(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.record_value(field, Value::from(value));
    }

    fn record_i128(&mut self, field: &Field, value: i128) {
        self.record_value(field, Value::String(value.to_string()));
    }

    fn record_u128(&mut self, field: &Field, value: u128) {
        self.record_value(field, Value::String(value.to_string()));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.record_value(field, Value::Bool(value));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.record_value(field, Value::String(value.to_owned()));
    }

    fn record_bytes(&mut self, field: &Field, value: &[u8]) {
        self.record_value(field, Value::String(format!("{value:?}")));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        self.record_value(field, Value::String(format!("{value:?}")));
    }
}

fn severity_from_level(level: tracing::Level) -> ExecutionLogSeverity {
    match level {
        tracing::Level::TRACE => ExecutionLogSeverity::Trace,
        tracing::Level::DEBUG => ExecutionLogSeverity::Debug,
        tracing::Level::INFO => ExecutionLogSeverity::Info,
        tracing::Level::WARN => ExecutionLogSeverity::Warn,
        tracing::Level::ERROR => ExecutionLogSeverity::Error,
    }
}

fn emit_sanitized_execution_log(record: &ExecutionLogRecord) {
    let attributes = record.attributes.to_string();
    match record.severity {
        ExecutionLogSeverity::Trace => tracing::trace!(
            target: "lenso::execution::sanitized",
            {
                lenso.correlation_id = %(&record.correlation_id),
                lenso.story_id = %(&record.story_id),
                lenso.execution.id = %(&record.execution_id),
                lenso.execution.kind = %(&record.execution_type),
                lenso.execution.name = %(&record.execution_name),
                attributes = %attributes,
            },
            "{}", record.body
        ),
        ExecutionLogSeverity::Debug => tracing::debug!(
            target: "lenso::execution::sanitized",
            {
                lenso.correlation_id = %(&record.correlation_id),
                lenso.story_id = %(&record.story_id),
                lenso.execution.id = %(&record.execution_id),
                lenso.execution.kind = %(&record.execution_type),
                lenso.execution.name = %(&record.execution_name),
                attributes = %attributes,
            },
            "{}", record.body
        ),
        ExecutionLogSeverity::Info => tracing::info!(
            target: "lenso::execution::sanitized",
            {
                lenso.correlation_id = %(&record.correlation_id),
                lenso.story_id = %(&record.story_id),
                lenso.execution.id = %(&record.execution_id),
                lenso.execution.kind = %(&record.execution_type),
                lenso.execution.name = %(&record.execution_name),
                attributes = %attributes,
            },
            "{}", record.body
        ),
        ExecutionLogSeverity::Warn => tracing::warn!(
            target: "lenso::execution::sanitized",
            {
                lenso.correlation_id = %(&record.correlation_id),
                lenso.story_id = %(&record.story_id),
                lenso.execution.id = %(&record.execution_id),
                lenso.execution.kind = %(&record.execution_type),
                lenso.execution.name = %(&record.execution_name),
                attributes = %attributes,
            },
            "{}", record.body
        ),
        ExecutionLogSeverity::Error => tracing::error!(
            target: "lenso::execution::sanitized",
            {
                lenso.correlation_id = %(&record.correlation_id),
                lenso.story_id = %(&record.story_id),
                lenso.execution.id = %(&record.execution_id),
                lenso.execution.kind = %(&record.execution_type),
                lenso.execution.name = %(&record.execution_name),
                attributes = %attributes,
            },
            "{}", record.body
        ),
    }
}

#[derive(Debug, Default)]
struct RedactionTracker {
    fields: BTreeSet<String>,
    bytes: usize,
    truncated: bool,
}

impl RedactionTracker {
    fn record(&mut self, path: &str) {
        if self.fields.contains(path) {
            return;
        }
        if self.fields.len() >= MAX_EXECUTION_LOG_REDACTED_FIELDS
            || self.bytes.saturating_add(path.len()) > MAX_EXECUTION_LOG_REDACTED_FIELD_BYTES
        {
            self.truncated = true;
            return;
        }
        self.bytes += path.len();
        self.fields.insert(path.to_owned());
    }

    fn finish(self) -> Vec<String> {
        let mut fields = self.fields.into_iter().collect::<Vec<_>>();
        if self.truncated {
            fields.push(REDACTED_FIELDS_TRUNCATED.to_owned());
        }
        fields
    }
}

fn sanitize_attributes(
    attributes: &mut Map<String, Value>,
    parent: &str,
    redactions: &mut RedactionTracker,
) {
    attributes.retain(|key, value| {
        let path = if parent.is_empty() {
            key.clone()
        } else {
            format!("{parent}.{key}")
        };
        if is_reserved_attribute(key) {
            return false;
        }
        if is_sensitive_attribute(key) {
            *value = Value::String(REDACTED_VALUE.to_owned());
            redactions.record(&path);
            return true;
        }
        sanitize_value(value, &path, redactions);
        true
    });
}

fn sanitize_value(value: &mut Value, path: &str, redactions: &mut RedactionTracker) {
    match value {
        Value::Object(attributes) => sanitize_attributes(attributes, path, redactions),
        Value::Array(values) => {
            for (index, value) in values.iter_mut().enumerate() {
                sanitize_value(value, &format!("{path}[{index}]"), redactions);
            }
        }
        _ => {}
    }
}

fn is_reserved_attribute(name: &str) -> bool {
    name.to_ascii_lowercase().starts_with("lenso.")
}

fn is_sensitive_attribute(name: &str) -> bool {
    let normalized = normalize_sensitive_identifier(name);
    SENSITIVE_ATTRIBUTE_TERMS
        .iter()
        .any(|sensitive| normalized.contains(sensitive))
}

fn normalize_sensitive_identifier(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

fn body_contains_sensitive_content(body: &str) -> bool {
    if body.split_whitespace().any(|token| {
        token
            .trim_matches(|character: char| !character.is_ascii_alphabetic())
            .eq_ignore_ascii_case("bearer")
    }) {
        return true;
    }

    let normalized_assignments = body
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, ':' | '='))
        .flat_map(char::to_lowercase)
        .collect::<String>();
    let has_sensitive_assignment = SENSITIVE_ATTRIBUTE_TERMS.iter().any(|sensitive| {
        normalized_assignments
            .match_indices(sensitive)
            .any(|(offset, _)| {
                normalized_assignments[offset + sensitive.len()..].starts_with([':', '='])
            })
    });
    has_sensitive_assignment || body.split_whitespace().any(looks_like_email)
}

fn looks_like_email(token: &str) -> bool {
    let token = token.trim_matches(|character: char| {
        !character.is_ascii_alphanumeric() && !matches!(character, '@' | '.' | '_' | '-' | '+')
    });
    let Some((local, domain)) = token.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && domain
            .split_once('.')
            .is_some_and(|(name, suffix)| !name.is_empty() && !suffix.is_empty())
}

fn truncate_utf8(mut value: String, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value, false);
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
    (value, true)
}

fn bounded_scope_attribute(value: &str, truncated: &mut bool) -> Value {
    let (value, did_truncate) =
        truncate_json_string(value, MAX_EXECUTION_LOG_SCOPE_ATTRIBUTE_BYTES);
    *truncated |= did_truncate;
    Value::String(value)
}

fn truncate_json_string(value: &str, max_encoded_bytes: usize) -> (String, bool) {
    let mut bounded = String::new();
    let mut encoded_bytes = 2_usize;
    for character in value.chars() {
        let character_bytes = match character {
            '"' | '\\' | '\u{08}' | '\u{0C}' | '\n' | '\r' | '\t' => 2,
            '\u{00}'..='\u{1F}' => 6,
            _ => character.len_utf8(),
        };
        if encoded_bytes.saturating_add(character_bytes) > max_encoded_bytes {
            return (bounded, true);
        }
        bounded.push(character);
        encoded_bytes += character_bytes;
    }
    (bounded, false)
}

fn bound_attributes(
    attributes: Map<String, Value>,
    protected_attributes: Map<String, Value>,
) -> Map<String, Value> {
    let protected_len = serialized_json_object_len(&protected_attributes).unwrap_or(usize::MAX);
    let caller_entries_len = attributes.iter().try_fold(0_usize, |total, (key, value)| {
        serialized_json_entry_len(key, value).and_then(|entry| total.checked_add(entry))
    });
    let caller_commas = attributes.len().saturating_sub(1);
    let combined_len = protected_len
        .checked_add(caller_entries_len.unwrap_or(usize::MAX))
        .and_then(|total| total.checked_add(caller_commas))
        .and_then(|total| {
            total.checked_add(usize::from(
                !protected_attributes.is_empty() && !attributes.is_empty(),
            ))
        });
    if combined_len.is_some_and(|len| len <= MAX_EXECUTION_LOG_ATTRIBUTES_BYTES) {
        let mut combined = attributes;
        combined.extend(protected_attributes);
        return combined;
    }

    let mut bounded = protected_attributes;
    bounded.insert(
        "lenso.log.attributes_truncated".to_owned(),
        Value::Bool(true),
    );
    let mut serialized_len = serialized_json_object_len(&bounded).unwrap_or(usize::MAX);
    for (key, value) in attributes {
        let added = serialized_json_entry_len(&key, &value)
            .and_then(|entry| entry.checked_add(usize::from(!bounded.is_empty())));
        if added
            .and_then(|entry| serialized_len.checked_add(entry))
            .is_some_and(|candidate| candidate <= MAX_EXECUTION_LOG_ATTRIBUTES_BYTES)
        {
            serialized_len += added.expect("checked attribute entry length");
            bounded.insert(key, value);
        }
    }
    debug_assert!(
        serde_json::to_vec(&bounded)
            .is_ok_and(|bytes| bytes.len() <= MAX_EXECUTION_LOG_ATTRIBUTES_BYTES),
        "bounded execution log attributes must stay within the storage budget"
    );
    bounded
}

fn serialized_json_object_len(attributes: &Map<String, Value>) -> Option<usize> {
    attributes
        .iter()
        .try_fold(2_usize, |total, (key, value)| {
            serialized_json_entry_len(key, value).and_then(|entry| total.checked_add(entry))
        })
        .and_then(|total| total.checked_add(attributes.len().saturating_sub(1)))
}

fn serialized_json_entry_len(key: &str, value: &Value) -> Option<usize> {
    serde_json::to_vec(key)
        .ok()?
        .len()
        .checked_add(1)?
        .checked_add(serde_json::to_vec(value).ok()?.len())
}

fn normalize_attributes(attributes: Value) -> Value {
    match attributes {
        Value::Object(_) => attributes,
        other => json!({ "value": other }),
    }
}

fn next_execution_log_id() -> String {
    format!("elog_{}", Uuid::now_v7())
}

fn map_execution_log_error(source: sqlx::Error) -> AppError {
    let unavailable = matches!(
        &source,
        sqlx::Error::Io(_)
            | sqlx::Error::Tls(_)
            | sqlx::Error::PoolTimedOut
            | sqlx::Error::PoolClosed
            | sqlx::Error::WorkerCrashed
    ) || matches!(
        &source,
        sqlx::Error::Database(error)
            if error.code().is_some_and(|code| {
                code.starts_with("08")
                    || code.starts_with("53")
                    || matches!(code.as_ref(), "57P01" | "57P02" | "57P03")
            })
    );
    let error = AppError::new(
        if unavailable {
            ErrorCode::ExternalDependency
        } else {
            ErrorCode::Internal
        },
        "Execution log operation failed",
    )
    .with_source(source);
    if unavailable {
        error.retryable()
    } else {
        error
    }
}

#[derive(Debug, Clone)]
pub struct ExecutionLogRow {
    pub id: String,
    pub correlation_id: String,
    pub story_id: String,
    pub execution_id: String,
    pub execution_type: String,
    pub execution_name: String,
    pub occurred_at: DateTime<Utc>,
    pub severity: String,
    pub body: String,
    pub attributes: Value,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub service_name: String,
    pub redacted_fields: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecutionLogQuery {
    pub execution_id: String,
    pub occurred_before: Option<DateTime<Utc>>,
    pub limit: i64,
}

#[async_trait]
pub trait ExecutionLogProvider: Debug + Send + Sync {
    async fn query_execution_logs(
        &self,
        query: ExecutionLogQuery,
    ) -> AppResult<Vec<ExecutionLogRow>>;
}

#[derive(Debug, Clone)]
pub struct PostgresExecutionLogProvider {
    pool: DbPool,
}

impl PostgresExecutionLogProvider {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ExecutionLogProvider for PostgresExecutionLogProvider {
    async fn query_execution_logs(
        &self,
        query: ExecutionLogQuery,
    ) -> AppResult<Vec<ExecutionLogRow>> {
        let mut rows = sqlx::query_as::<_, ExecutionLogTuple>(
            r#"
            select *
            from (
                select
                    concat('elog_outbox_enqueued_', id) as id,
                    correlation_id,
                    correlation_id as story_id,
                    id as execution_id,
                    'outbox_event'::text as execution_type,
                    event_name as execution_name,
                    created_at as occurred_at,
                    'info'::text as severity,
                    'Outbox event enqueued'::text as body,
                    jsonb_build_object(
                        'event_name', event_name,
                        'event_version', event_version,
                        'aggregate_type', aggregate_type,
                        'aggregate_id', aggregate_id,
                        'source_module', source_module
                    ) as attributes,
                    headers #>> '{trace,trace_id}' as trace_id,
                    headers #>> '{trace,span_id}' as span_id,
                    source_module as service_name,
                    array[]::text[] as redacted_fields
                from platform.outbox
                where id = $1

                union all

                select
                    id,
                    correlation_id,
                    story_id,
                    execution_id,
                    execution_type,
                    execution_name,
                    occurred_at,
                    severity,
                    body,
                    attributes,
                    trace_id,
                    span_id,
                    service_name,
                    redacted_fields
                from platform.execution_logs
                where execution_id = $1
            ) execution_log_rows
            where ($2::timestamptz is null or occurred_at < $2)
            order by occurred_at desc, id desc
            limit $3
            "#,
        )
        .bind(query.execution_id)
        .bind(query.occurred_before)
        .bind(query.limit)
        .fetch_all(&self.pool)
        .await
        .map_err(map_execution_log_error)?
        .into_iter()
        .map(Into::into)
        .collect::<Vec<_>>();

        rows.reverse();
        Ok(rows)
    }
}

type ExecutionLogTuple = (
    String,
    String,
    String,
    String,
    String,
    String,
    DateTime<Utc>,
    String,
    String,
    Value,
    Option<String>,
    Option<String>,
    String,
    Vec<String>,
);

impl From<ExecutionLogTuple> for ExecutionLogRow {
    fn from(row: ExecutionLogTuple) -> Self {
        let (
            id,
            correlation_id,
            story_id,
            execution_id,
            execution_type,
            execution_name,
            occurred_at,
            severity,
            body,
            attributes,
            trace_id,
            span_id,
            service_name,
            redacted_fields,
        ) = row;

        Self {
            id,
            correlation_id,
            story_id,
            execution_id,
            execution_type,
            execution_name,
            occurred_at,
            severity,
            body,
            attributes,
            trace_id,
            span_id,
            service_name,
            redacted_fields,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ActorContext, CorrelationId, ExecutionContext, ExecutionId, TenantId, TraceContext,
    };
    use std::sync::Mutex;
    use tracing_subscriber::layer::{Context, SubscriberExt as _};

    #[test]
    fn execution_log_store_availability_errors_remain_typed_and_retryable() {
        let error = map_execution_log_error(sqlx::Error::PoolClosed);

        assert_eq!(error.code, ErrorCode::ExternalDependency);
        assert!(error.retryable);
    }

    #[test]
    fn execution_log_store_corruption_errors_fail_closed() {
        let error = map_execution_log_error(sqlx::Error::Protocol(
            "invalid execution log row".to_owned(),
        ));

        assert_eq!(error.code, ErrorCode::Internal);
        assert!(!error.retryable);
    }

    #[derive(Debug, Clone)]
    struct RecordingLayer {
        events: Arc<Mutex<Vec<ObservedEvent>>>,
    }

    #[derive(Debug, Clone)]
    struct ObservedEvent {
        target: String,
        fields: String,
    }

    #[derive(Debug, Default)]
    struct ObservedEventVisitor {
        fields: Vec<String>,
    }

    impl Visit for ObservedEventVisitor {
        fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
            self.fields.push(format!("{}={value:?}", field.name()));
        }
    }

    impl<S> tracing_subscriber::Layer<S> for RecordingLayer
    where
        S: Subscriber,
    {
        fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
            let mut visitor = ObservedEventVisitor::default();
            event.record(&mut visitor);
            self.events
                .lock()
                .expect("observed events lock should not be poisoned")
                .push(ObservedEvent {
                    target: event.metadata().target().to_owned(),
                    fields: visitor.fields.join(" "),
                });
        }
    }

    #[derive(Debug, Default)]
    struct MemoryExecutionLogWriter {
        records: Mutex<Vec<ExecutionLogRecord>>,
    }

    #[async_trait]
    impl ExecutionLogWriter for MemoryExecutionLogWriter {
        async fn write_execution_log(&self, record: ExecutionLogRecord) -> AppResult<String> {
            self.records
                .lock()
                .expect("execution log records lock should not be poisoned")
                .push(record);
            Ok("elog_test".to_owned())
        }
    }

    #[derive(Debug)]
    struct StalledExecutionLogWriter;

    #[async_trait]
    impl ExecutionLogWriter for StalledExecutionLogWriter {
        async fn write_execution_log(&self, _record: ExecutionLogRecord) -> AppResult<String> {
            tokio::time::sleep(Duration::from_secs(60)).await;
            Ok("elog_unreachable".to_owned())
        }
    }

    #[derive(Debug)]
    struct CancellationAwareExecutionLogWriter {
        started: Arc<tokio::sync::Notify>,
        cancelled: Arc<tokio::sync::Notify>,
    }

    #[derive(Debug)]
    struct NotifyOnDrop(Arc<tokio::sync::Notify>);

    impl Drop for NotifyOnDrop {
        fn drop(&mut self) {
            self.0.notify_one();
        }
    }

    #[async_trait]
    impl ExecutionLogWriter for CancellationAwareExecutionLogWriter {
        async fn write_execution_log(&self, _record: ExecutionLogRecord) -> AppResult<String> {
            let _notify_on_drop = NotifyOnDrop(self.cancelled.clone());
            self.started.notify_one();
            std::future::pending::<()>().await;
            unreachable!("cancellation-aware writer should be cancelled")
        }
    }

    fn scope() -> ExecutionLogScope {
        scope_with_identity("fnrun_real", "corr_real")
    }

    fn scope_with_identity(execution_id: &str, correlation_id: &str) -> ExecutionLogScope {
        ExecutionLogScope::function(
            &ExecutionContext {
                execution_id: ExecutionId(execution_id.to_owned()),
                function_name: "inventory.reserve.v1".to_owned(),
                attempt: 2,
                queue: "inventory".to_owned(),
                correlation_id: CorrelationId::new(correlation_id),
                causation_id: Some("httpreq_real".to_owned()),
                actor: ActorContext::System,
                tenant_id: Some(TenantId("tenant_real".to_owned())),
                trace: TraceContext {
                    trace_id: Some("trace_real".to_owned()),
                    span_id: Some("span_real".to_owned()),
                    baggage: Vec::new(),
                },
                deadline: None,
            },
            "inventory-service",
            "worker-a",
        )
    }

    #[test]
    fn recursively_redacts_sensitive_values_and_drops_caller_runtime_fields() {
        let record = scope().record(
            ExecutionLogSeverity::Info,
            "checked reservation".to_owned(),
            serde_json::from_value::<Map<String, Value>>(json!({
                "authorization": "Bearer secret",
                "customerEmail": "person@example.test",
                "nested": {
                    "access_key_id": "access-secret",
                    "safe": "kept"
                },
                "lenso.execution.id": "fnrun_forged"
            }))
            .expect("attributes should decode"),
        );

        assert!(record.attributes().get("lenso.execution.id").is_none());
        assert_eq!(record.attributes()["authorization"], REDACTED_VALUE);
        assert_eq!(record.attributes()["customerEmail"], REDACTED_VALUE);
        assert_eq!(
            record.attributes()["nested"]["access_key_id"],
            REDACTED_VALUE
        );
        assert_eq!(record.attributes()["nested"]["safe"], "kept");
        assert_eq!(record.attributes()["lenso.function_run_id"], "fnrun_real");
        assert_eq!(record.attributes()["lenso.story_id"], "corr_real");
        assert!(
            record
                .redacted_fields()
                .iter()
                .any(|field| field == "nested.access_key_id")
        );
    }

    #[test]
    fn redacts_body_credentials_across_identifier_styles_and_whitespace() {
        for body in [
            "accessKey=secret",
            "access-key = secret",
            "api_key: secret",
            "Authorization: Bearer secret",
            "Bearer\nsecret",
        ] {
            let record = scope().record(ExecutionLogSeverity::Info, body.to_owned(), Map::new());

            assert_eq!(record.body, REDACTED_VALUE, "body must be redacted: {body}");
            assert!(record.redacted_fields().iter().any(|field| field == "body"));
        }
    }

    #[test]
    fn high_cardinality_redaction_metadata_and_attributes_remain_bounded() {
        let attributes = (0..2_000)
            .map(|index| {
                (
                    format!("password_{index}"),
                    Value::String("secret".to_owned()),
                )
            })
            .collect::<Map<_, _>>();
        let record = scope().record(
            ExecutionLogSeverity::Info,
            "bounded redactions".to_owned(),
            attributes,
        );

        assert!(record.redacted_fields().len() <= MAX_EXECUTION_LOG_REDACTED_FIELDS + 1);
        assert!(
            record
                .redacted_fields()
                .iter()
                .any(|field| field == REDACTED_FIELDS_TRUNCATED)
        );
        assert!(
            serde_json::to_vec(record.attributes())
                .is_ok_and(|bytes| bytes.len() <= MAX_EXECUTION_LOG_ATTRIBUTES_BYTES)
        );
    }

    #[test]
    fn caller_attribute_limit_never_drops_runtime_scope() {
        let record = scope().record(
            ExecutionLogSeverity::Info,
            "bounded".to_owned(),
            Map::from_iter([
                ("huge".to_owned(), Value::String("x".repeat(32 * 1024))),
                (
                    "lenso.function_run_id".to_owned(),
                    Value::String("fnrun_forged".to_owned()),
                ),
            ]),
        );

        assert_eq!(record.attributes()["lenso.function_run_id"], "fnrun_real");
        assert_eq!(record.attributes()["lenso.story_id"], "corr_real");
        assert_eq!(
            record.attributes()["lenso.execution.name"],
            "inventory.reserve.v1"
        );
        assert_eq!(record.attributes()["lenso.log.attributes_truncated"], true);
        assert!(
            serde_json::to_vec(record.attributes())
                .is_ok_and(|bytes| bytes.len() <= MAX_EXECUTION_LOG_ATTRIBUTES_BYTES)
        );
    }

    #[test]
    fn oversized_runtime_scope_is_bounded_without_changing_canonical_identity() {
        let oversized_id = format!("fnrun_{}", "x".repeat(32 * 1024));
        let scope = scope_with_identity(&oversized_id, "corr_real");
        let record = scope.record(
            ExecutionLogSeverity::Info,
            "bounded scope".to_owned(),
            Map::new(),
        );

        assert_eq!(record.execution_id, oversized_id);
        assert_eq!(record.attributes()["lenso.log.scope_truncated"], true);
        assert!(
            serde_json::to_vec(record.attributes())
                .is_ok_and(|bytes| bytes.len() <= MAX_EXECUTION_LOG_ATTRIBUTES_BYTES)
        );
    }

    #[test]
    fn escaped_runtime_scope_is_bounded_by_serialized_json_bytes() {
        let escaped = "\u{0001}".repeat(32 * 1024);
        let execution_id = format!("fnrun_{escaped}");
        let scope = ExecutionLogScope::function(
            &ExecutionContext {
                execution_id: ExecutionId(execution_id.clone()),
                function_name: escaped.clone(),
                attempt: 1,
                queue: escaped.clone(),
                correlation_id: CorrelationId::new(escaped.clone()),
                causation_id: None,
                actor: ActorContext::System,
                tenant_id: Some(TenantId(escaped.clone())),
                trace: TraceContext::default(),
                deadline: None,
            },
            "inventory-service",
            escaped,
        );
        let record = scope.record(
            ExecutionLogSeverity::Info,
            "bounded escaped scope".to_owned(),
            Map::new(),
        );

        assert_eq!(record.execution_id, execution_id);
        assert_eq!(record.attributes()["lenso.log.scope_truncated"], true);
        assert!(
            serde_json::to_vec(record.attributes())
                .is_ok_and(|bytes| bytes.len() <= MAX_EXECUTION_LOG_ATTRIBUTES_BYTES)
        );
        for key in [
            "lenso.correlation_id",
            "lenso.story_id",
            "lenso.function_run_id",
            "lenso.execution.name",
            "lenso.execution.queue",
            "lenso.workload.id",
            "lenso.tenant.id",
        ] {
            assert!(
                serde_json::to_vec(&record.attributes()[key])
                    .is_ok_and(|bytes| bytes.len() <= MAX_EXECUTION_LOG_SCOPE_ATTRIBUTE_BYTES),
                "{key} must fit the serialized scope-attribute budget"
            );
        }
    }

    #[tokio::test]
    async fn execution_log_capture_withholds_raw_events_and_forwards_sanitized_events_to_host() {
        let observed_events = Arc::new(Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::registry().with(RecordingLayer {
            events: observed_events.clone(),
        });
        let writer = Arc::new(MemoryExecutionLogWriter::default());

        let (output, report) = async {
            capture_execution_logs(scope(), Some(writer.clone()), async {
                let span = tracing::info_span!(target: "inventory", "handler_span");
                let span_id = span.id().expect("host subscriber should create a span id");
                let _entered = span.enter();
                assert_eq!(tracing::Span::current().id(), Some(span_id));
                tracing::info!(target: "inventory", "ordinary host event");
                tracing::info!(
                    target: EXECUTION_LOG_TARGET,
                    attributes = %json!({
                        "nested": { "password": "raw-attribute-secret" },
                        "lenso.execution.id": "fnrun_forged"
                    }),
                    "Authorization: Bearer raw-body-secret"
                );
                42
            })
            .await
        }
        .with_subscriber(subscriber)
        .await;

        assert_eq!(output, 42);
        assert_eq!(report.status, ExecutionLogCaptureStatus::Complete);
        assert_eq!(report.observed, 1);
        assert_eq!(report.persisted, 1);

        let records = writer
            .records
            .lock()
            .expect("execution log records lock should not be poisoned");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].execution_id, "fnrun_real");
        assert_eq!(records[0].body, REDACTED_VALUE);
        assert_eq!(records[0].attributes["nested"]["password"], REDACTED_VALUE);
        assert!(
            records[0]
                .redacted_fields
                .iter()
                .any(|field| field == "body")
        );
        drop(records);

        let observed_events = observed_events
            .lock()
            .expect("observed events lock should not be poisoned");
        assert_eq!(
            observed_events
                .iter()
                .filter(|event| event.target == "inventory")
                .count(),
            1
        );
        assert_eq!(
            observed_events
                .iter()
                .filter(|event| event.target == EXECUTION_LOG_TARGET)
                .count(),
            0
        );
        assert_eq!(
            observed_events
                .iter()
                .filter(|event| event.target == "lenso::execution::sanitized")
                .count(),
            1
        );
        assert!(observed_events.iter().all(|event| {
            !event.fields.contains("raw-body-secret")
                && !event.fields.contains("raw-attribute-secret")
        }));
    }

    #[tokio::test]
    async fn unsupported_attribute_payloads_are_redacted_before_capture_or_forwarding() {
        let observed_events = Arc::new(Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::registry().with(RecordingLayer {
            events: observed_events.clone(),
        });
        let writer = Arc::new(MemoryExecutionLogWriter::default());

        let ((), report) = async {
            capture_execution_logs(scope(), Some(writer.clone()), async {
                tracing::info!(
                    target: EXECUTION_LOG_TARGET,
                    attributes = ?json!({ "password": "debug-secret" }),
                    "debug attributes"
                );
                tracing::info!(
                    target: EXECUTION_LOG_TARGET,
                    attributes = "{\"password\":\"malformed-secret\"",
                    "malformed attributes"
                );
            })
            .await
        }
        .with_subscriber(subscriber)
        .await;

        assert_eq!(report.status, ExecutionLogCaptureStatus::Complete);
        assert_eq!(report.persisted, 2);
        let records = writer
            .records
            .lock()
            .expect("execution log records lock should not be poisoned");
        assert_eq!(records.len(), 2);
        assert!(
            records
                .iter()
                .all(|record| record.attributes["attributes"] == REDACTED_VALUE)
        );
        drop(records);
        let observed_events = observed_events
            .lock()
            .expect("observed events lock should not be poisoned");
        assert!(observed_events.iter().all(|event| {
            !event.fields.contains("debug-secret") && !event.fields.contains("malformed-secret")
        }));
    }

    #[tokio::test]
    async fn missing_writer_withholds_raw_events_and_reports_capture_disabled() {
        let observed_events = Arc::new(Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::registry().with(RecordingLayer {
            events: observed_events.clone(),
        });
        let (output, report) = async {
            capture_execution_logs(scope(), None, async {
                tracing::info!(target: "inventory", "ordinary host event");
                tracing::info!(
                    target: EXECUTION_LOG_TARGET,
                    "password=disabled-writer-secret"
                );
                42
            })
            .await
        }
        .with_subscriber(subscriber)
        .await;

        assert_eq!(output, 42);
        assert_eq!(report.status, ExecutionLogCaptureStatus::Disabled);
        assert_eq!(report.observed, 0);
        let observed_events = observed_events
            .lock()
            .expect("observed events lock should not be poisoned");
        assert_eq!(observed_events.len(), 1);
        assert_eq!(observed_events[0].target, "inventory");
        assert!(!observed_events[0].fields.contains("disabled-writer-secret"));
    }

    #[tokio::test]
    async fn host_level_filter_does_not_disable_info_execution_log_capture() {
        let writer = Arc::new(MemoryExecutionLogWriter::default());
        let subscriber =
            tracing_subscriber::registry().with(tracing_subscriber::filter::LevelFilter::WARN);

        let ((), report) = async {
            capture_execution_logs(scope(), Some(writer.clone()), async {
                tracing::info!(target: EXECUTION_LOG_TARGET, "captured below host level");
            })
            .await
        }
        .with_subscriber(subscriber)
        .await;

        assert_eq!(report.status, ExecutionLogCaptureStatus::Complete);
        assert_eq!(report.observed, 1);
        assert_eq!(report.persisted, 1);
        assert_eq!(
            writer
                .records
                .lock()
                .expect("execution log records lock should not be poisoned")[0]
                .body,
            "captured below host level"
        );
    }

    #[tokio::test]
    async fn concurrent_execution_log_scopes_do_not_mix_runtime_identity() {
        let writer_a = Arc::new(MemoryExecutionLogWriter::default());
        let writer_b = Arc::new(MemoryExecutionLogWriter::default());

        let (capture_a, capture_b) = tokio::join!(
            capture_execution_logs(
                scope_with_identity("fnrun_a", "corr_a"),
                Some(writer_a.clone()),
                async {
                    tokio::task::yield_now().await;
                    tracing::info!(target: EXECUTION_LOG_TARGET, "scope a");
                },
            ),
            capture_execution_logs(
                scope_with_identity("fnrun_b", "corr_b"),
                Some(writer_b.clone()),
                async {
                    tracing::info!(target: EXECUTION_LOG_TARGET, "scope b");
                    tokio::task::yield_now().await;
                },
            ),
        );

        assert_eq!(capture_a.1.status, ExecutionLogCaptureStatus::Complete);
        assert_eq!(capture_b.1.status, ExecutionLogCaptureStatus::Complete);
        let records_a = writer_a
            .records
            .lock()
            .expect("execution log records lock should not be poisoned");
        let records_b = writer_b
            .records
            .lock()
            .expect("execution log records lock should not be poisoned");
        assert_eq!(records_a.len(), 1);
        assert_eq!(records_b.len(), 1);
        assert_eq!(records_a[0].execution_id, "fnrun_a");
        assert_eq!(records_a[0].correlation_id, "corr_a");
        assert_eq!(records_b[0].execution_id, "fnrun_b");
        assert_eq!(records_b[0].correlation_id, "corr_b");
    }

    #[tokio::test]
    async fn stalled_writer_drops_bounded_logs_without_changing_output() {
        let started = std::time::Instant::now();
        let (output, report) =
            capture_execution_logs(scope(), Some(Arc::new(StalledExecutionLogWriter)), async {
                for sequence in 0..300_u64 {
                    tracing::info!(
                        target: EXECUTION_LOG_TARGET,
                        sequence,
                        "bounded execution event"
                    );
                }
                42
            })
            .await;

        assert_eq!(output, 42);
        assert_eq!(report.status, ExecutionLogCaptureStatus::Partial);
        assert_eq!(report.observed, 300);
        assert_eq!(report.persisted, 0);
        assert_eq!(report.dropped, 300);
        assert_eq!(report.write_failures, 0);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[tokio::test]
    async fn cancelling_capture_aborts_the_in_flight_writer() {
        let started = Arc::new(tokio::sync::Notify::new());
        let cancelled = Arc::new(tokio::sync::Notify::new());
        let writer = Arc::new(CancellationAwareExecutionLogWriter {
            started: started.clone(),
            cancelled: cancelled.clone(),
        });
        let capture = tokio::spawn(capture_execution_logs(scope(), Some(writer), async {
            tracing::info!(target: EXECUTION_LOG_TARGET, "wait for cancellation");
            std::future::pending::<()>().await;
        }));

        tokio::time::timeout(Duration::from_secs(1), started.notified())
            .await
            .expect("writer should begin processing");
        capture.abort();
        let _ = capture.await;
        tokio::time::timeout(Duration::from_secs(1), cancelled.notified())
            .await
            .expect("cancelling capture should abort its writer task");
    }
}
