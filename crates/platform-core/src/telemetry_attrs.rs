use crate::{CorrelationId, TraceContext};
use serde_json::Value;
use tracing::Span;
use uuid::Uuid;

pub const ATTR_CORRELATION_ID: &str = "lenso.correlation_id";
pub const ATTR_STORY_ID: &str = "lenso.story_id";
pub const ATTR_FUNCTION_RUN_ID: &str = "lenso.function_run_id";
pub const ATTR_OUTBOX_EVENT_ID: &str = "lenso.outbox_event_id";
pub const ATTR_EXECUTION_KIND: &str = "lenso.execution.kind";
pub const ATTR_EXECUTION_NAME: &str = "lenso.execution.name";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSpanAttributes {
    pub correlation_id: String,
    pub story_id: String,
    pub execution_kind: String,
    pub execution_name: String,
    pub outbox_event_id: Option<String>,
    pub function_run_id: Option<String>,
}

impl RuntimeSpanAttributes {
    pub fn outbox(
        correlation_id: impl Into<String>,
        outbox_event_id: impl Into<String>,
        execution_name: impl Into<String>,
    ) -> Self {
        let correlation_id = correlation_id.into();
        Self {
            story_id: correlation_id.clone(),
            correlation_id,
            execution_kind: "outbox_event".to_owned(),
            execution_name: execution_name.into(),
            outbox_event_id: Some(outbox_event_id.into()),
            function_run_id: None,
        }
    }

    pub fn function(
        correlation_id: impl Into<String>,
        function_run_id: impl Into<String>,
        execution_name: impl Into<String>,
    ) -> Self {
        let correlation_id = correlation_id.into();
        Self {
            story_id: correlation_id.clone(),
            correlation_id,
            execution_kind: "function_run".to_owned(),
            execution_name: execution_name.into(),
            outbox_event_id: None,
            function_run_id: Some(function_run_id.into()),
        }
    }
}

pub fn record_runtime_span_attributes(span: &Span, attrs: &RuntimeSpanAttributes) {
    span.record(ATTR_CORRELATION_ID, attrs.correlation_id.as_str());
    span.record(ATTR_STORY_ID, attrs.story_id.as_str());
    span.record(ATTR_EXECUTION_KIND, attrs.execution_kind.as_str());
    span.record(ATTR_EXECUTION_NAME, attrs.execution_name.as_str());

    if let Some(outbox_event_id) = attrs.outbox_event_id.as_deref() {
        span.record(ATTR_OUTBOX_EVENT_ID, outbox_event_id);
    }
    if let Some(function_run_id) = attrs.function_run_id.as_deref() {
        span.record(ATTR_FUNCTION_RUN_ID, function_run_id);
    }
}

pub fn trace_context_from_traceparent(value: &str) -> Option<TraceContext> {
    let mut parts = value.split('-');
    let version = parts.next()?;
    let trace_id = parts.next()?;
    let span_id = parts.next()?;
    let flags = parts.next()?;

    if parts.next().is_some()
        || version.len() != 2
        || trace_id.len() != 32
        || span_id.len() != 16
        || flags.len() != 2
        || trace_id.chars().all(|char| char == '0')
        || span_id.chars().all(|char| char == '0')
        || ![version, trace_id, span_id, flags]
            .iter()
            .all(|part| part.chars().all(|char| char.is_ascii_hexdigit()))
    {
        return None;
    }

    Some(TraceContext {
        trace_id: Some(trace_id.to_ascii_lowercase()),
        span_id: Some(span_id.to_ascii_lowercase()),
        baggage: Vec::new(),
    })
}

pub fn generate_trace_context() -> TraceContext {
    let trace_id = Uuid::now_v7().simple().to_string();
    TraceContext {
        span_id: Some(trace_id[..16].to_owned()),
        trace_id: Some(trace_id),
        baggage: Vec::new(),
    }
}

pub fn trace_context_from_headers(headers: &Value) -> TraceContext {
    headers
        .get("trace")
        .and_then(|trace| serde_json::from_value(trace.clone()).ok())
        .unwrap_or_default()
}

pub fn trace_headers(trace: &TraceContext, correlation_id: &CorrelationId) -> Value {
    serde_json::json!({
        "correlation_id": correlation_id.0,
        "trace": trace,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_traceparent_into_existing_trace_context_shape() {
        let trace = trace_context_from_traceparent(
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        )
        .expect("traceparent should parse");

        assert_eq!(
            trace.trace_id.as_deref(),
            Some("4bf92f3577b34da6a3ce929d0e0e4736")
        );
        assert_eq!(trace.span_id.as_deref(), Some("00f067aa0ba902b7"));
    }

    #[test]
    fn rejects_invalid_traceparent_values() {
        assert!(trace_context_from_traceparent("not-a-traceparent").is_none());
        assert!(
            trace_context_from_traceparent(
                "00-00000000000000000000000000000000-00f067aa0ba902b7-01"
            )
            .is_none()
        );
        assert!(
            trace_context_from_traceparent(
                "00-4bf92f3577b34da6a3ce929d0e0e4736-0000000000000000-01"
            )
            .is_none()
        );
    }

    #[test]
    fn generates_trace_context_when_no_incoming_traceparent_exists() {
        let trace = generate_trace_context();

        assert_eq!(trace.trace_id.as_deref().unwrap_or_default().len(), 32);
        assert_eq!(trace.span_id.as_deref().unwrap_or_default().len(), 16);
    }

    #[test]
    fn builds_business_runtime_attributes_for_outbox_events() {
        let attrs = RuntimeSpanAttributes::outbox("corr_1", "evt_1", "identity.user_registered.v1");

        assert_eq!(attrs.correlation_id, "corr_1");
        assert_eq!(attrs.story_id, "corr_1");
        assert_eq!(attrs.execution_kind, "outbox_event");
        assert_eq!(attrs.outbox_event_id.as_deref(), Some("evt_1"));
        assert_eq!(attrs.function_run_id, None);
    }

    #[test]
    fn builds_business_runtime_attributes_for_function_runs() {
        let attrs = RuntimeSpanAttributes::function(
            "corr_1",
            "fnrun_1",
            "notifications.send_welcome_email.v1",
        );

        assert_eq!(attrs.correlation_id, "corr_1");
        assert_eq!(attrs.story_id, "corr_1");
        assert_eq!(attrs.execution_kind, "function_run");
        assert_eq!(attrs.function_run_id.as_deref(), Some("fnrun_1"));
        assert_eq!(attrs.outbox_event_id, None);
    }
}
