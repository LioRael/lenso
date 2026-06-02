use crate::db::{DbPool, DbTransaction};
use crate::error::{AppError, AppResult, ErrorCode};
use crate::events::EventEnvelope;
use crate::execution_logs::{
    ExecutionLogRecord, ExecutionLogSeverity, insert_execution_log_projection,
};
use crate::{RuntimeSpanAttributes, record_runtime_span_attributes, trace_context_from_headers};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::sync::Arc;
use tracing::Instrument;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboxStatus {
    Pending,
    Processing,
    Published,
    Failed,
    Dead,
}

impl OutboxStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Published => "published",
            Self::Failed => "failed",
            Self::Dead => "dead",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OutboxEvent {
    pub id: String,
    pub event_name: String,
    pub event_version: u16,
    pub source_module: String,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub correlation_id: String,
    pub causation_id: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub payload: Value,
    pub headers: Value,
}

impl OutboxEvent {
    pub fn from_envelope(aggregate_type: impl Into<String>, event: &EventEnvelope) -> Self {
        Self {
            id: event.event_id.clone(),
            event_name: event.event_name.clone(),
            event_version: event.event_version,
            source_module: event.source_module.clone(),
            aggregate_type: aggregate_type.into(),
            aggregate_id: event.subject.clone(),
            correlation_id: event.correlation_id.0.clone(),
            causation_id: event.causation_id.clone(),
            occurred_at: event.occurred_at,
            payload: event.payload.clone(),
            headers: json!({
                "actor": event.actor,
                "schema_ref": event.schema_ref,
                "trace": event.trace,
            }),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClaimedOutboxEvent {
    pub id: String,
    pub event_name: String,
    pub event_version: u16,
    pub source_module: String,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub correlation_id: String,
    pub causation_id: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub payload: Value,
    pub headers: Value,
    pub attempts: i32,
    pub max_attempts: i32,
}

#[derive(Debug, Clone, Default)]
pub struct OutboxPublisher;

impl OutboxPublisher {
    pub async fn publish_in_tx(
        &self,
        tx: &mut DbTransaction<'_>,
        event: &OutboxEvent,
    ) -> AppResult<()> {
        let span = tracing::info_span!(
            "outbox_publish",
            lenso.correlation_id = tracing::field::Empty,
            lenso.story_id = tracing::field::Empty,
            lenso.outbox_event_id = tracing::field::Empty,
            lenso.execution.kind = tracing::field::Empty,
            lenso.execution.name = tracing::field::Empty,
        );
        record_runtime_span_attributes(
            &span,
            &RuntimeSpanAttributes::outbox(
                event.correlation_id.clone(),
                event.id.clone(),
                event.event_name.clone(),
            ),
        );

        async {
            sqlx::query(
                r#"
            insert into platform.outbox (
                id,
                event_name,
                event_version,
                source_module,
                aggregate_type,
                aggregate_id,
                correlation_id,
                causation_id,
                occurred_at,
                payload,
                headers
            )
            values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
            )
            .bind(&event.id)
            .bind(&event.event_name)
            .bind(i32::from(event.event_version))
            .bind(&event.source_module)
            .bind(&event.aggregate_type)
            .bind(&event.aggregate_id)
            .bind(&event.correlation_id)
            .bind(&event.causation_id)
            .bind(event.occurred_at)
            .bind(&event.payload)
            .bind(&event.headers)
            .execute(&mut **tx)
            .await
            .map(|_| ())
            .map_err(map_outbox_error)
        }
        .instrument(span)
        .await
    }

    pub async fn pending_count(&self, pool: &DbPool) -> AppResult<i64> {
        sqlx::query_scalar(
            r#"
            select count(*)
            from platform.outbox
            where status = 'pending'
            "#,
        )
        .fetch_one(pool)
        .await
        .map_err(map_outbox_error)
    }
}

#[async_trait]
pub trait EventDispatcher: Debug + Send + Sync {
    async fn dispatch(&self, event: &ClaimedOutboxEvent) -> AppResult<()>;
}

#[async_trait]
pub trait EventHandler: Debug + Send + Sync {
    fn event_name(&self) -> &'static str;
    async fn handle(&self, event: &ClaimedOutboxEvent) -> AppResult<()>;
}

#[derive(Debug, Clone, Default)]
pub struct EventHandlerRegistry {
    handlers: BTreeMap<&'static str, Vec<Arc<dyn EventHandler>>>,
}

impl EventHandlerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, handler: Arc<dyn EventHandler>) {
        self.handlers
            .entry(handler.event_name())
            .or_default()
            .push(handler);
    }

    pub fn register_all(&mut self, handlers: impl IntoIterator<Item = Arc<dyn EventHandler>>) {
        for handler in handlers {
            self.register(handler);
        }
    }

    pub fn handler_count(&self, event_name: &str) -> usize {
        self.handlers.get(event_name).map_or(0, Vec::len)
    }
}

#[async_trait]
impl EventDispatcher for EventHandlerRegistry {
    async fn dispatch(&self, event: &ClaimedOutboxEvent) -> AppResult<()> {
        let Some(handlers) = self.handlers.get(event.event_name.as_str()) else {
            tracing::debug!(
                event_name = %event.event_name,
                outbox_id = %event.id,
                "no in-process event handlers registered"
            );
            return Ok(());
        };

        for handler in handlers {
            handler.handle(event).await?;
        }

        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct LoggingEventDispatcher;

#[async_trait]
impl EventDispatcher for LoggingEventDispatcher {
    async fn dispatch(&self, event: &ClaimedOutboxEvent) -> AppResult<()> {
        tracing::info!(
            outbox_id = %event.id,
            event_name = %event.event_name,
            event_version = event.event_version,
            aggregate_id = %event.aggregate_id,
            correlation_id = %event.correlation_id,
            "outbox event dispatched"
        );
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct OutboxRelay {
    pool: DbPool,
    worker_id: String,
    batch_size: i64,
}

impl OutboxRelay {
    pub fn new(pool: DbPool, worker_id: impl Into<String>, batch_size: i64) -> Self {
        Self {
            pool,
            worker_id: worker_id.into(),
            batch_size,
        }
    }

    pub async fn claim_batch(&self) -> AppResult<Vec<ClaimedOutboxEvent>> {
        let span = tracing::info_span!(
            "outbox_claim_batch",
            worker_id = %self.worker_id,
            lenso.execution.kind = "outbox_claim",
            lenso.execution.name = "outbox.claim_batch",
        );

        async {
            let events = sqlx::query_as::<_, OutboxRow>(
                r#"
            with claimed as (
                select id
                from platform.outbox
                where status in ('pending', 'failed')
                  and available_at <= now()
                order by available_at asc, created_at asc
                limit $1
                for update skip locked
            )
            update platform.outbox outbox
            set status = 'processing',
                locked_at = now(),
                locked_by = $2,
                last_error = null
            from claimed
            where outbox.id = claimed.id
            returning
                outbox.id,
                outbox.event_name,
                outbox.event_version,
                outbox.source_module,
                outbox.aggregate_type,
                outbox.aggregate_id,
                outbox.correlation_id,
                outbox.causation_id,
                outbox.occurred_at,
                outbox.payload,
                outbox.headers,
                outbox.attempts,
                outbox.max_attempts
            "#,
            )
            .bind(self.batch_size)
            .bind(&self.worker_id)
            .fetch_all(&self.pool)
            .await
            .map(|rows| rows.into_iter().map(Into::into).collect())
            .map_err(map_outbox_error)?;

            for event in &events {
                self.record_outbox_execution_log(
                    event,
                    ExecutionLogSeverity::Info,
                    "Outbox event claimed",
                    json!({
                        "attempt": event.attempts + 1,
                        "max_attempts": event.max_attempts,
                        "worker_id": self.worker_id,
                    }),
                )
                .await;
            }

            Ok(events)
        }
        .instrument(span)
        .await
    }

    pub async fn relay_once(&self, dispatcher: &dyn EventDispatcher) -> AppResult<usize> {
        let span = tracing::info_span!(
            "outbox_relay_once",
            worker_id = %self.worker_id,
            lenso.execution.kind = "outbox_relay",
            lenso.execution.name = "outbox.relay_once",
        );

        async {
            let events = self.claim_batch().await?;
            let count = events.len();

            for event in events {
                let event_span = tracing::info_span!(
                    "outbox_dispatch",
                    lenso.correlation_id = tracing::field::Empty,
                    lenso.story_id = tracing::field::Empty,
                    lenso.outbox_event_id = tracing::field::Empty,
                    lenso.execution.kind = tracing::field::Empty,
                    lenso.execution.name = tracing::field::Empty,
                );
                record_runtime_span_attributes(
                    &event_span,
                    &RuntimeSpanAttributes::outbox(
                        event.correlation_id.clone(),
                        event.id.clone(),
                        event.event_name.clone(),
                    ),
                );

                async {
                    self.record_outbox_execution_log(
                        &event,
                        ExecutionLogSeverity::Info,
                        "Outbox event dispatch started",
                        json!({
                            "event_name": event.event_name,
                            "attempt": event.attempts + 1,
                            "worker_id": self.worker_id,
                        }),
                    )
                    .await;
                    match dispatcher.dispatch(&event).await {
                        Ok(()) => self.mark_published(&event).await?,
                        Err(error) => self.mark_dispatch_failed(&event, &error).await?,
                    }

                    Ok::<(), AppError>(())
                }
                .instrument(event_span)
                .await?;
            }

            Ok(count)
        }
        .instrument(span)
        .await
    }

    pub async fn mark_published(&self, event: &ClaimedOutboxEvent) -> AppResult<()> {
        sqlx::query(
            r#"
            update platform.outbox
            set status = 'published',
                published_at = now(),
                locked_at = null,
                locked_by = null,
                last_error = null
            where id = $1
            "#,
        )
        .bind(&event.id)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(map_outbox_error)?;

        self.record_outbox_execution_log(
            event,
            ExecutionLogSeverity::Info,
            "Outbox event published",
            json!({
                "event_name": event.event_name,
                "attempt": event.attempts + 1,
                "worker_id": self.worker_id,
            }),
        )
        .await;

        Ok(())
    }

    pub async fn mark_dispatch_failed(
        &self,
        event: &ClaimedOutboxEvent,
        error: &AppError,
    ) -> AppResult<()> {
        let next_attempt = event.attempts + 1;
        let status = if next_attempt >= event.max_attempts {
            OutboxStatus::Dead
        } else if error.retryable {
            OutboxStatus::Failed
        } else {
            OutboxStatus::Dead
        };

        let span = tracing::info_span!(
            "outbox_retry",
            lenso.correlation_id = tracing::field::Empty,
            lenso.story_id = tracing::field::Empty,
            lenso.outbox_event_id = tracing::field::Empty,
            lenso.execution.kind = tracing::field::Empty,
            lenso.execution.name = tracing::field::Empty,
        );
        record_runtime_span_attributes(
            &span,
            &RuntimeSpanAttributes::outbox(
                event.correlation_id.clone(),
                event.id.clone(),
                event.event_name.clone(),
            ),
        );

        async {
            sqlx::query(
                r#"
            update platform.outbox
            set status = $2,
                attempts = attempts + 1,
                available_at = case when $2 = 'failed' then now() else available_at end,
                locked_at = null,
                locked_by = null,
                last_error = $3
            where id = $1
            "#,
            )
            .bind(&event.id)
            .bind(status.as_str())
            .bind(error.public_message.as_str())
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(map_outbox_error)?;

            self.record_outbox_execution_log(
                event,
                ExecutionLogSeverity::Error,
                if status == OutboxStatus::Dead {
                    "Outbox event marked dead"
                } else {
                    "Outbox event failed"
                },
                json!({
                    "attempt": next_attempt,
                    "max_attempts": event.max_attempts,
                    "status": status.as_str(),
                    "retryable": error.retryable,
                    "error": error.public_message,
                    "worker_id": self.worker_id,
                }),
            )
            .await;

            Ok(())
        }
        .instrument(span)
        .await
    }

    async fn record_outbox_execution_log(
        &self,
        event: &ClaimedOutboxEvent,
        severity: ExecutionLogSeverity,
        body: &'static str,
        attributes: Value,
    ) {
        emit_outbox_lifecycle_event(event, severity, body, &attributes, Some(&self.worker_id));
        if let Err(error) = insert_execution_log_projection(
            &self.pool,
            outbox_log_record(event, severity, body, attributes),
        )
        .await
        {
            tracing::warn!(
                error = ?error,
                outbox_id = %event.id,
                "failed to write outbox execution log"
            );
        }
    }
}

type OutboxRow = (
    String,
    String,
    i32,
    String,
    String,
    String,
    String,
    Option<String>,
    DateTime<Utc>,
    Value,
    Value,
    i32,
    i32,
);

impl From<OutboxRow> for ClaimedOutboxEvent {
    fn from(row: OutboxRow) -> Self {
        let (
            id,
            event_name,
            event_version,
            source_module,
            aggregate_type,
            aggregate_id,
            correlation_id,
            causation_id,
            occurred_at,
            payload,
            headers,
            attempts,
            max_attempts,
        ) = row;

        Self {
            id,
            event_name,
            event_version: event_version
                .try_into()
                .expect("event_version should fit into u16"),
            source_module,
            aggregate_type,
            aggregate_id,
            correlation_id,
            causation_id,
            occurred_at,
            payload,
            headers,
            attempts,
            max_attempts,
        }
    }
}

fn map_outbox_error(source: sqlx::Error) -> AppError {
    AppError::new(ErrorCode::Internal, "Outbox operation failed").with_source(source)
}

fn emit_outbox_lifecycle_event(
    event: &ClaimedOutboxEvent,
    severity: ExecutionLogSeverity,
    body: &'static str,
    attributes: &Value,
    worker_id: Option<&str>,
) {
    match severity {
        ExecutionLogSeverity::Error => {
            tracing::error!(
                outbox_id = %event.id,
                event_name = %event.event_name,
                correlation_id = %event.correlation_id,
                worker_id = worker_id.unwrap_or(""),
                attributes = %attributes,
                "{body}"
            );
        }
        ExecutionLogSeverity::Warn => {
            tracing::warn!(
                outbox_id = %event.id,
                event_name = %event.event_name,
                correlation_id = %event.correlation_id,
                worker_id = worker_id.unwrap_or(""),
                attributes = %attributes,
                "{body}"
            );
        }
        _ => {
            tracing::info!(
                outbox_id = %event.id,
                event_name = %event.event_name,
                correlation_id = %event.correlation_id,
                worker_id = worker_id.unwrap_or(""),
                attributes = %attributes,
                "{body}"
            );
        }
    }
}

fn outbox_log_record(
    event: &impl OutboxLogSource,
    severity: ExecutionLogSeverity,
    body: impl Into<String>,
    attributes: Value,
) -> ExecutionLogRecord {
    ExecutionLogRecord::from_runtime_attrs(
        RuntimeSpanAttributes::outbox(event.correlation_id(), event.id(), event.execution_name()),
        severity,
        body,
    )
    .with_attributes(attributes)
    .with_trace(trace_context_from_headers(event.headers()))
}

trait OutboxLogSource {
    fn id(&self) -> String;
    fn correlation_id(&self) -> String;
    fn execution_name(&self) -> String;
    fn headers(&self) -> &Value;
}

impl OutboxLogSource for OutboxEvent {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn correlation_id(&self) -> String {
        self.correlation_id.clone()
    }

    fn execution_name(&self) -> String {
        self.event_name.clone()
    }

    fn headers(&self) -> &Value {
        &self.headers
    }
}

impl OutboxLogSource for ClaimedOutboxEvent {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn correlation_id(&self) -> String {
        self.correlation_id.clone()
    }

    fn execution_name(&self) -> String {
        self.event_name.clone()
    }

    fn headers(&self) -> &Value {
        &self.headers
    }
}
