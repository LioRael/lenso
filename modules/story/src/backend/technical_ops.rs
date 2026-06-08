#[allow(clippy::wildcard_imports)]
use super::*;

pub(super) fn technical_operations_from_spans(
    spans: Vec<TelemetrySpan>,
    node_index: &RuntimeNodeIndex,
) -> Vec<AdminRuntimeTechnicalOperation> {
    let mut operations = spans
        .into_iter()
        .map(|span| technical_operation_from_span(span, node_index))
        .collect::<Vec<_>>();
    sort_technical_operations(&mut operations);
    operations
}

fn technical_operation_from_span(
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
    let category = technical_operation_category(&span);
    let related_node_id = related_node_id(&span.attributes, node_index);
    let status = technical_operation_status(&span);

    AdminRuntimeTechnicalOperation {
        attributes: safe_span_attributes(&span.attributes),
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

pub(super) fn admin_action_technical_operations(
    rows: &[StoryWorkRow],
    node_index: &RuntimeNodeIndex,
) -> Vec<AdminRuntimeTechnicalOperation> {
    rows.iter()
        .filter(|row| row.item_type == "admin_action")
        .map(|row| admin_action_to_technical_operation(row, node_index))
        .collect()
}

fn admin_action_to_technical_operation(
    row: &StoryWorkRow,
    node_index: &RuntimeNodeIndex,
) -> AdminRuntimeTechnicalOperation {
    let ended_at = row.completed_at.unwrap_or(row.created_at);
    AdminRuntimeTechnicalOperation {
        attributes: row.metadata.clone(),
        category: "admin".to_owned(),
        correlation_id: row.correlation_id.clone(),
        duration_ms: row_duration_ms(row),
        ended_at,
        id: format!("admin_action:{}", row.id),
        name: row.name.clone(),
        related_node_id: node_index.contains(&row.id).then(|| row.id.clone()),
        source: "admin_action".to_owned(),
        started_at: row.started_at.unwrap_or(row.created_at),
        status: if row.status == "failed" {
            "error".to_owned()
        } else {
            "ok".to_owned()
        },
        story_id: row.correlation_id.clone(),
    }
}

pub(super) async fn remote_proxy_technical_operations(
    ctx: &AppContext,
    request_ctx: &RequestContext,
    correlation_id: &str,
    spans: &[TelemetrySpan],
    node_index: &RuntimeNodeIndex,
) -> Result<Vec<AdminRuntimeTechnicalOperation>, ApiErrorResponse> {
    let rows = sqlx::query(
        r#"
        select
            id,
            module_name,
            method,
            declared_path,
            remote_path,
            capability,
            remote_status,
            duration_ms,
            success,
            error_code,
            retryable,
            request_id,
            correlation_id,
            trace_id,
            span_id,
            path_params,
            error_details,
            occurred_at
        from platform.remote_http_proxy_calls
        where correlation_id = $1
        order by occurred_at asc, id asc
        limit $2
        "#,
    )
    .bind(correlation_id)
    .bind(MAX_LIMIT)
    .fetch_all(&ctx.db)
    .await
    .map_err(|source| query_error(source, request_ctx))?;

    rows.into_iter()
        .map(|row| remote_proxy_call_from_row(&row))
        .map(|result| {
            result.map(|call| {
                let related_node_id = remote_proxy_related_node_id(&call, spans, node_index);
                remote_proxy_call_to_technical_operation(call, related_node_id)
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| query_error(source, request_ctx))
}

fn remote_proxy_call_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<AdminRemoteProxyCall, sqlx::Error> {
    Ok(AdminRemoteProxyCall {
        id: row.try_get("id")?,
        module_name: row.try_get("module_name")?,
        method: row.try_get("method")?,
        declared_path: row.try_get("declared_path")?,
        remote_path: row.try_get("remote_path")?,
        capability: row.try_get("capability")?,
        remote_status: row.try_get("remote_status")?,
        duration_ms: row.try_get("duration_ms")?,
        success: row.try_get("success")?,
        error_code: row.try_get("error_code")?,
        retryable: row.try_get("retryable")?,
        request_id: row.try_get("request_id")?,
        correlation_id: row.try_get("correlation_id")?,
        trace_id: row.try_get("trace_id")?,
        span_id: row.try_get("span_id")?,
        occurred_at: row.try_get("occurred_at")?,
    })
}

fn remote_proxy_related_node_id(
    call: &AdminRemoteProxyCall,
    spans: &[TelemetrySpan],
    node_index: &RuntimeNodeIndex,
) -> Option<String> {
    let remote_proxy_node_id = platform_core::remote_proxy_call_story_event_id(&call.id);
    if node_index.contains(&remote_proxy_node_id) {
        return Some(remote_proxy_node_id);
    }

    if let Some(node_id) = call.span_id.as_deref().and_then(|span_id| {
        spans
            .iter()
            .find(|span| span.id == span_id)
            .and_then(|span| related_node_id(&span.attributes, node_index))
    }) {
        return Some(node_id);
    }

    let trace_id = call.trace_id.as_deref()?;
    spans
        .iter()
        .filter(|span| remote_proxy_span_trace_id(span) == Some(trace_id))
        .find_map(|span| related_node_id(&span.attributes, node_index))
}

fn remote_proxy_span_trace_id(span: &TelemetrySpan) -> Option<&str> {
    [
        "otel.trace_id",
        "trace_id",
        "lenso.trace_id",
        "trace.trace_id",
    ]
    .into_iter()
    .find_map(|key| span_attribute(&span.attributes, key))
}

fn remote_proxy_call_to_technical_operation(
    call: AdminRemoteProxyCall,
    related_node_id: Option<String>,
) -> AdminRuntimeTechnicalOperation {
    let ended_at = call.occurred_at + Duration::milliseconds(call.duration_ms.max(0));
    AdminRuntimeTechnicalOperation {
        attributes: serde_json::json!({
            "module_name": call.module_name,
            "method": call.method,
            "declared_path": call.declared_path,
            "remote_path": call.remote_path,
            "capability": call.capability,
            "remote_status": call.remote_status,
            "duration_ms": call.duration_ms,
            "success": call.success,
            "error_code": call.error_code,
            "retryable": call.retryable,
            "request_id": call.request_id,
            "trace_id": call.trace_id,
            "span_id": call.span_id,
        }),
        category: "external".to_owned(),
        correlation_id: call.correlation_id.clone(),
        duration_ms: call.duration_ms,
        ended_at,
        id: format!("remote_proxy:{}", call.id),
        name: format!(
            "{} {} {}",
            call.module_name, call.method, call.declared_path
        ),
        related_node_id,
        source: "remote_proxy".to_owned(),
        started_at: call.occurred_at,
        status: if call.success { "ok" } else { "error" }.to_owned(),
        story_id: call.correlation_id,
    }
}

pub(super) async fn remote_runtime_technical_operations_by_correlation(
    ctx: &AppContext,
    request_ctx: &RequestContext,
    correlation_id: &str,
    node_index: &RuntimeNodeIndex,
) -> Result<Vec<AdminRuntimeTechnicalOperation>, ApiErrorResponse> {
    let rows = sqlx::query_as::<_, ExecutionLogTuple>(
        r#"
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
        where correlation_id = $1
            and attributes ->> 'source' = 'remote_runtime'
        order by occurred_at asc, id asc
        limit $2
        "#,
    )
    .bind(correlation_id)
    .bind(MAX_LIMIT)
    .fetch_all(&ctx.db)
    .await
    .map_err(|source| query_error(source, request_ctx))?;

    Ok(rows
        .into_iter()
        .map(execution_log_row_from_tuple)
        .map(|log| remote_runtime_log_to_technical_operation(log, node_index))
        .collect())
}

fn remote_runtime_log_to_technical_operation(
    log: ExecutionLogRow,
    node_index: &RuntimeNodeIndex,
) -> AdminRuntimeTechnicalOperation {
    let duration_ms = json_i64_attribute(&log.attributes, "duration_ms").unwrap_or(0);
    let ended_at = log.occurred_at + Duration::milliseconds(duration_ms.max(0));
    let related_node_id = node_index
        .contains(&log.execution_id)
        .then(|| log.execution_id.clone());
    let module_name = span_attribute(&log.attributes, "module_name").map(ToOwned::to_owned);
    let function_name = span_attribute(&log.attributes, "function_name")
        .unwrap_or(log.execution_name.as_str())
        .to_owned();
    let status = match json_bool_attribute(&log.attributes, "success") {
        Some(true) => "ok",
        Some(false) => "error",
        _ if log.severity == "error" => "error",
        _ => "ok",
    };

    AdminRuntimeTechnicalOperation {
        attributes: log.attributes,
        category: "external".to_owned(),
        correlation_id: log.correlation_id.clone(),
        duration_ms,
        ended_at,
        id: format!("remote_runtime:{}", log.id),
        name: module_name
            .map(|module| format!("{module} {function_name}"))
            .unwrap_or(function_name),
        related_node_id,
        source: "remote_runtime".to_owned(),
        started_at: log.occurred_at,
        status: status.to_owned(),
        story_id: log.story_id,
    }
}

fn execution_log_row_from_tuple(row: ExecutionLogTuple) -> ExecutionLogRow {
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

    ExecutionLogRow {
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

fn related_node_id(attributes: &Value, node_index: &RuntimeNodeIndex) -> Option<String> {
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

fn technical_operation_category(span: &TelemetrySpan) -> String {
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

fn technical_operation_status(span: &TelemetrySpan) -> String {
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

fn safe_span_attributes(attributes: &Value) -> Value {
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

fn is_safe_span_attribute(key: &str) -> bool {
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

fn is_safe_attribute_value(value: &Value) -> bool {
    matches!(
        value,
        Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null
    )
}

fn has_attribute_with_prefix(attributes: &Value, prefix: &str) -> bool {
    attributes
        .as_object()
        .is_some_and(|map| map.keys().any(|key| key.starts_with(prefix)))
}

fn span_attribute<'a>(attributes: &'a Value, key: &str) -> Option<&'a str> {
    attributes.get(key).and_then(Value::as_str)
}

fn json_bool_attribute(attributes: &Value, key: &str) -> Option<bool> {
    attributes.get(key).and_then(Value::as_bool)
}

fn json_i64_attribute(attributes: &Value, key: &str) -> Option<i64> {
    attributes.get(key).and_then(Value::as_i64).or_else(|| {
        attributes
            .get(key)
            .and_then(Value::as_u64)
            .and_then(|value| i64::try_from(value).ok())
    })
}

pub(super) fn sort_technical_operations(data: &mut [AdminRuntimeTechnicalOperation]) {
    data.sort_by(|left, right| {
        left.started_at
            .cmp(&right.started_at)
            .then_with(|| left.id.cmp(&right.id))
    });
}
