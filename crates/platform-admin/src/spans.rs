#[allow(clippy::wildcard_imports)]
use super::*;
use platform_core::TelemetrySpan;
use serde_json::Value;

pub(crate) fn technical_operations_from_spans(
    spans: Vec<TelemetrySpan>,
    node_index: &RuntimeNodeIndex,
) -> Vec<AdminRuntimeTechnicalOperation> {
    let mut operations = spans
        .into_iter()
        .map(|span| technical_operation_from_span(span, node_index))
        .collect::<Vec<_>>();
    operations.sort_by(|left, right| {
        left.started_at
            .cmp(&right.started_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    operations
}

pub(crate) fn technical_operation_from_span(
    span: TelemetrySpan,
    node_index: &RuntimeNodeIndex,
) -> AdminRuntimeTechnicalOperation {
    let correlation_id = span_attribute(&span.attributes, "lenso.correlation_id")
        .or_else(|| span_attribute(&span.attributes, "lenso.story_id"))
        .unwrap_or("unknown")
        .to_owned();
    let story_id = span_attribute(&span.attributes, "lenso.story_id")
        .unwrap_or(&correlation_id)
        .to_owned();
    let duration_ms = span
        .ended_at
        .signed_duration_since(span.started_at)
        .num_milliseconds()
        .max(0);
    let attributes = safe_span_attributes(&span.attributes);
    let category = technical_operation_category(&span);
    let related_node_id = related_node_id(&span.attributes, node_index);
    let status = technical_operation_status(&span);

    AdminRuntimeTechnicalOperation {
        attributes,
        category,
        correlation_id,
        duration_ms,
        ended_at: span.ended_at,
        id: span.id,
        name: span.name,
        related_node_id,
        source: "otel".to_owned(),
        started_at: span.started_at,
        status,
        story_id,
    }
}

pub(crate) fn related_node_id(attributes: &Value, node_index: &RuntimeNodeIndex) -> Option<String> {
    for key in ["lenso.function_run_id", "lenso.outbox_event_id"] {
        let Some(id) = span_attribute(attributes, key) else {
            continue;
        };
        if node_index.contains(id) {
            return Some(id.to_owned());
        }
    }

    None
}

pub(crate) fn technical_operation_category(span: &TelemetrySpan) -> String {
    if has_attribute_with_prefix(&span.attributes, "redis.")
        || span_attribute(&span.attributes, "db.system") == Some("redis")
    {
        return "redis".to_owned();
    }
    if has_attribute_with_prefix(&span.attributes, "db.") {
        return "db".to_owned();
    }
    if has_attribute_with_prefix(&span.attributes, "http.")
        || matches!(
            span.name.split_whitespace().next(),
            Some("GET" | "POST" | "PUT" | "PATCH" | "DELETE")
        )
    {
        return "http".to_owned();
    }
    if has_attribute_with_prefix(&span.attributes, "aws.s3.")
        || has_attribute_with_prefix(&span.attributes, "s3.")
    {
        return "s3".to_owned();
    }
    if has_attribute_with_prefix(&span.attributes, "aws.ses.")
        || has_attribute_with_prefix(&span.attributes, "ses.")
    {
        return "ses".to_owned();
    }

    match span_attribute(&span.attributes, "lenso.execution.kind") {
        Some("worker_loop" | "outbox_claim" | "function_claim") => "worker".to_owned(),
        Some("outbox_event" | "function_run" | "runtime") => "runtime".to_owned(),
        _ if has_attribute_with_prefix(&span.attributes, "rpc.")
            || has_attribute_with_prefix(&span.attributes, "peer.")
            || has_attribute_with_prefix(&span.attributes, "net.peer.") =>
        {
            "external".to_owned()
        }
        _ => "unknown".to_owned(),
    }
}

pub(crate) fn technical_operation_status(span: &TelemetrySpan) -> String {
    let raw = span
        .status
        .as_deref()
        .or_else(|| span_attribute(&span.attributes, "otel.status_code"));
    match raw.map(str::to_ascii_lowercase).as_deref() {
        Some("ok" | "success") => "ok".to_owned(),
        Some("error" | "err" | "failed" | "failure") => "error".to_owned(),
        Some("unset" | "unknown") | None => {
            if span.attributes.get("error.type").is_some() {
                "error".to_owned()
            } else {
                "unknown".to_owned()
            }
        }
        Some(_) => "unknown".to_owned(),
    }
}

pub(crate) fn safe_span_attributes(attributes: &Value) -> Value {
    let Some(map) = attributes.as_object() else {
        return Value::Object(Default::default());
    };

    let mut safe = serde_json::Map::new();
    for (key, value) in map {
        if is_safe_span_attribute(key) && is_safe_attribute_value(value) {
            safe.insert(key.clone(), value.clone());
        }
    }

    Value::Object(safe)
}

pub(crate) fn is_safe_span_attribute(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    if [
        "authorization",
        "cookie",
        "password",
        "secret",
        "token",
        "api_key",
        "email",
        "statement",
        "query",
        "body",
        "payload",
    ]
    .iter()
    .any(|unsafe_part| lower.contains(unsafe_part))
    {
        return false;
    }

    key.starts_with("lenso.")
        || matches!(
            key,
            "otel.status_code"
                | "error.type"
                | "http.request.method"
                | "http.route"
                | "http.response.status_code"
                | "url.scheme"
                | "server.address"
                | "server.port"
                | "network.peer.address"
                | "network.peer.port"
                | "net.peer.name"
                | "net.peer.port"
                | "db.system"
                | "db.name"
                | "db.namespace"
                | "db.operation"
                | "db.operation.name"
                | "db.collection.name"
                | "db.sql.table"
                | "rpc.system"
                | "rpc.service"
                | "rpc.method"
                | "aws.s3.bucket"
                | "aws.s3.bucket.name"
                | "s3.bucket"
                | "s3.bucket.name"
                | "aws.ses.operation"
                | "ses.operation"
        )
}

pub(crate) fn is_safe_attribute_value(value: &Value) -> bool {
    matches!(
        value,
        Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null
    )
}

pub(crate) fn redact_json_value(
    value: Value,
    path: &str,
    redacted_fields: &mut Vec<String>,
) -> Value {
    match value {
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .enumerate()
                .map(|(index, item)| {
                    redact_json_value(item, &format!("{path}[{index}]"), redacted_fields)
                })
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| {
                    let field_path = format!("{path}.{key}");
                    if is_sensitive_json_key(&key) {
                        redacted_fields.push(field_path);
                        (key, Value::String("[redacted]".to_owned()))
                    } else {
                        (key, redact_json_value(value, &field_path, redacted_fields))
                    }
                })
                .collect(),
        ),
        value => value,
    }
}

pub(crate) fn is_sensitive_json_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    [
        "authorization",
        "cookie",
        "password",
        "passwd",
        "secret",
        "token",
        "api_key",
        "apikey",
        "access_key",
        "credential",
        "email",
    ]
    .iter()
    .any(|unsafe_part| lower.contains(unsafe_part))
}

pub(crate) fn has_attribute_with_prefix(attributes: &Value, prefix: &str) -> bool {
    attributes
        .as_object()
        .is_some_and(|map| map.keys().any(|key| key.starts_with(prefix)))
}

pub(crate) fn span_attribute<'a>(attributes: &'a Value, key: &str) -> Option<&'a str> {
    attributes.get(key).and_then(Value::as_str)
}
