#[allow(clippy::wildcard_imports)]
use super::*;
use platform_core::{AppError, ErrorCode};
use platform_http::ApiErrorResponse;

#[derive(Debug, Clone, Default)]
pub(crate) struct RuntimeNodeIndex {
    ids: std::collections::BTreeSet<String>,
}

impl RuntimeNodeIndex {
    pub(crate) fn single(id: String) -> Self {
        Self {
            ids: std::collections::BTreeSet::from([id]),
        }
    }

    pub(crate) fn contains(&self, id: &str) -> bool {
        self.ids.contains(id)
    }
}

pub(crate) fn runtime_node_index(rows: &[StoryWorkRow]) -> RuntimeNodeIndex {
    RuntimeNodeIndex {
        ids: rows.iter().map(|row| row.id.clone()).collect(),
    }
}

pub(crate) fn execution_payload_from_outbox(
    detail: AdminOutboxEventDetail,
) -> AdminRuntimeExecutionPayload {
    let mut redacted_fields = Vec::new();
    let input = redact_json_value(detail.payload, "input", &mut redacted_fields);
    let metadata = redact_json_value(
        serde_json::json!({
            "event_name": detail.event_name,
            "event_version": detail.event_version,
            "source_module": detail.source_module,
            "aggregate_type": detail.aggregate_type,
            "aggregate_id": detail.aggregate_id,
            "status": detail.status,
            "attempts": detail.attempts,
            "max_attempts": detail.max_attempts,
            "available_at": detail.available_at,
            "locked_by": detail.locked_by,
            "published_at": detail.published_at,
            "last_error": detail.last_error,
            "correlation_id": detail.correlation_id,
            "causation_id": detail.causation_id,
            "occurred_at": detail.occurred_at,
            "created_at": detail.created_at,
            "actor": detail.actor,
            "trace": detail.trace,
            "headers": detail.headers,
        }),
        "metadata",
        &mut redacted_fields,
    );

    AdminRuntimeExecutionPayload {
        input,
        metadata,
        node_id: detail.id,
        node_type: "event".to_owned(),
        output: None,
        redacted_fields,
    }
}

pub(crate) fn execution_payload_from_function(
    detail: AdminFunctionRunDetail,
) -> AdminRuntimeExecutionPayload {
    let mut redacted_fields = Vec::new();
    let input = redact_json_value(detail.input_json, "input", &mut redacted_fields);
    let metadata = redact_json_value(
        serde_json::json!({
            "function_name": detail.function_name,
            "status": detail.status,
            "attempts": detail.attempts,
            "max_attempts": detail.max_attempts,
            "available_at": detail.available_at,
            "locked_by": detail.locked_by,
            "started_at": detail.started_at,
            "completed_at": detail.completed_at,
            "last_error": detail.last_error,
            "correlation_id": detail.correlation_id,
            "created_at": detail.created_at,
            "actor": detail.actor,
        }),
        "metadata",
        &mut redacted_fields,
    );

    AdminRuntimeExecutionPayload {
        input,
        metadata,
        node_id: detail.id,
        node_type: "function".to_owned(),
        output: None,
        redacted_fields,
    }
}

pub(crate) fn execution_payload_from_story_event(
    detail: StoryEventDetail,
) -> AdminRuntimeExecutionPayload {
    let mut redacted_fields = Vec::new();
    let input = redact_json_value(detail.metadata.clone(), "input", &mut redacted_fields);
    let metadata = redact_json_value(
        serde_json::json!({
            "node_type": detail.node_type,
            "name": detail.name,
            "status": detail.status,
            "service": detail.service,
            "correlation_id": detail.correlation_id,
            "causation_id": detail.causation_id,
            "started_at": detail.started_at,
            "completed_at": detail.completed_at,
            "duration_ms": detail.duration_ms,
            "error": detail.error,
            "trace_id": detail.trace_id,
            "span_id": detail.span_id,
        }),
        "metadata",
        &mut redacted_fields,
    );

    AdminRuntimeExecutionPayload {
        input,
        metadata,
        node_id: detail.id,
        node_type: "story_event".to_owned(),
        output: None,
        redacted_fields,
    }
}

pub(crate) fn row_duration_ms(row: &StoryWorkRow) -> i64 {
    let Some(started_at) = row.started_at else {
        return 0;
    };
    row.completed_at
        .unwrap_or(started_at)
        .signed_duration_since(started_at)
        .num_milliseconds()
        .max(0)
}

pub(crate) fn runtime_status(
    outbox: &AdminRuntimeOutboxSummary,
    functions: &AdminRuntimeFunctionSummary,
) -> &'static str {
    if outbox.dead > 0 || functions.dead > 0 {
        return "failing";
    }

    if outbox.failed > 0 || functions.failed > 0 {
        return "degraded";
    }

    "healthy"
}

pub(crate) fn ensure_retryable_status(
    target_type: &str,
    id: &str,
    status: &str,
    request_ctx: &platform_core::RequestContext,
) -> Result<(), ApiErrorResponse> {
    if matches!(status, "failed" | "dead") {
        return Ok(());
    }

    Err(ApiErrorResponse::with_context(
        AppError::new(
            ErrorCode::Conflict,
            format!("{target_type} {id} cannot be retried from status {status}"),
        ),
        request_ctx,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use platform_core::TelemetrySpan;
    use serde_json::Value;

    #[test]
    fn timeline_item_type_preserves_failure_retry_and_dead_letter_kinds() {
        assert_eq!(timeline_item_type("event", "published", 1), "outbox_event");
        assert_eq!(
            timeline_item_type("outbox_event", "published", 1),
            "outbox_event"
        );
        assert_eq!(
            timeline_item_type("function", "completed", 1),
            "function_run"
        );
        assert_eq!(
            timeline_item_type("function_run", "completed", 1),
            "function_run"
        );
        assert_eq!(timeline_item_type("function", "completed", 2), "retry");
        assert_eq!(timeline_item_type("function", "failed", 2), "failure");
        assert_eq!(timeline_item_type("event", "dead", 3), "dead_letter");
    }

    #[test]
    fn technical_operation_dto_serializes_business_friendly_shape() {
        let operation = AdminRuntimeTechnicalOperation {
            attributes: serde_json::json!({ "db.system": "postgresql" }),
            category: "db".to_owned(),
            correlation_id: "corr_test".to_owned(),
            duration_ms: 25,
            ended_at: parse_time("2026-05-31T00:00:00.025Z"),
            id: "span_1".to_owned(),
            name: "INSERT runtime.function_runs".to_owned(),
            related_node_id: Some("fnrun_test".to_owned()),
            source: "otel".to_owned(),
            started_at: parse_time("2026-05-31T00:00:00Z"),
            status: "ok".to_owned(),
            story_id: "corr_test".to_owned(),
        };

        let value = serde_json::to_value(operation).expect("operation should serialize");

        assert_eq!(value["source"], "otel");
        assert_eq!(value["category"], "db");
        assert_eq!(value["related_node_id"], "fnrun_test");
        assert_eq!(value["attributes"]["db.system"], "postgresql");
    }

    #[test]
    fn telemetry_span_maps_known_function_run_to_execution_node() {
        let rows = vec![story_row(
            "function",
            "fnrun_test",
            None,
            "2026-05-31T00:00:00Z",
        )];
        let operations = technical_operations_from_spans(
            vec![telemetry_span(
                "span_function",
                "SELECT identity.users",
                serde_json::json!({
                    "lenso.correlation_id": "corr_test",
                    "lenso.function_run_id": "fnrun_test",
                    "db.system": "postgresql"
                }),
            )],
            &runtime_node_index(&rows),
        );

        assert_eq!(operations.len(), 1);
        assert_eq!(operations[0].related_node_id.as_deref(), Some("fnrun_test"));
        assert_eq!(operations[0].category, "db");
    }

    #[test]
    fn telemetry_span_maps_known_outbox_event_to_execution_node() {
        let rows = vec![story_row("event", "evt_test", None, "2026-05-31T00:00:00Z")];
        let operations = technical_operations_from_spans(
            vec![telemetry_span(
                "span_outbox",
                "Publish event",
                serde_json::json!({
                    "lenso.correlation_id": "corr_test",
                    "lenso.outbox_event_id": "evt_test",
                    "lenso.execution.kind": "outbox_event"
                }),
            )],
            &runtime_node_index(&rows),
        );

        assert_eq!(operations[0].related_node_id.as_deref(), Some("evt_test"));
        assert_eq!(operations[0].category, "runtime");
    }

    #[test]
    fn unknown_telemetry_span_remains_story_level_unlinked_operation() {
        let operations = technical_operations_from_spans(
            vec![telemetry_span(
                "span_unlinked",
                "GET https://api.example.test",
                serde_json::json!({
                    "lenso.correlation_id": "corr_test",
                    "http.request.method": "GET"
                }),
            )],
            &runtime_node_index(&[]),
        );

        assert_eq!(operations[0].related_node_id, None);
        assert_eq!(operations[0].category, "http");
    }

    #[test]
    fn technical_operation_attributes_are_safe_subset_only() {
        let operations = technical_operations_from_spans(
            vec![telemetry_span(
                "span_sensitive",
                "INSERT identity.users",
                serde_json::json!({
                    "lenso.correlation_id": "corr_test",
                    "db.system": "postgresql",
                    "db.statement": "insert into users(email, password) values('a@example.test', 'secret')",
                    "http.request.header.authorization": "Bearer secret",
                    "user.email": "a@example.test"
                }),
            )],
            &runtime_node_index(&[]),
        );

        assert_eq!(operations[0].attributes["db.system"], "postgresql");
        assert!(operations[0].attributes.get("db.statement").is_none());
        assert!(
            operations[0]
                .attributes
                .get("http.request.header.authorization")
                .is_none()
        );
        assert!(operations[0].attributes.get("user.email").is_none());
    }

    fn story_row(
        item_type: &str,
        id: &str,
        _causation_id: Option<&str>,
        created_at: &str,
    ) -> StoryWorkRow {
        StoryWorkRow {
            item_type: item_type.to_owned(),
            id: id.to_owned(),
            name: id.to_owned(),
            status: if item_type == "event" {
                "published".to_owned()
            } else {
                "completed".to_owned()
            },
            attempts: 1,
            max_attempts: 3,
            correlation_id: "corr_test".to_owned(),
            created_at: parse_time(created_at),
            started_at: Some(parse_time(created_at)),
            completed_at: Some(parse_time(created_at)),
            last_error: None,
            metadata: Value::Object(Default::default()),
        }
    }

    fn parse_time(value: &str) -> DateTime<Utc> {
        value.parse().expect("test timestamp should parse")
    }

    fn telemetry_span(id: &str, name: &str, attributes: Value) -> TelemetrySpan {
        TelemetrySpan {
            attributes,
            ended_at: parse_time("2026-05-31T00:00:01Z"),
            id: id.to_owned(),
            name: name.to_owned(),
            started_at: parse_time("2026-05-31T00:00:00Z"),
            status: Some("ok".to_owned()),
        }
    }
}
