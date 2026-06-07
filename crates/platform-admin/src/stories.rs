#[allow(clippy::wildcard_imports)]
use super::*;
use chrono::{DateTime, Utc};
use platform_core::{AppError, ErrorCode, StoryDisplayDescriptor, StoryDisplaySource};
use platform_http::ApiErrorResponse;
use serde_json::Value;

pub(crate) fn build_story_summaries(rows: Vec<StoryWorkRow>) -> Vec<AdminRuntimeStoryListItem> {
    let mut grouped: Vec<(String, Vec<StoryWorkRow>)> = Vec::new();
    for row in rows {
        if let Some((_, items)) = grouped
            .iter_mut()
            .find(|(correlation_id, _)| correlation_id == &row.correlation_id)
        {
            items.push(row);
        } else {
            grouped.push((row.correlation_id.clone(), vec![row]));
        }
    }

    let mut summaries: Vec<AdminRuntimeStoryListItem> = grouped
        .into_iter()
        .map(|(_, items)| build_story_summary(&items))
        .collect();
    summaries.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.correlation_id.cmp(&right.correlation_id))
    });
    summaries
}

pub(crate) fn build_story_detail(rows: Vec<StoryWorkRow>) -> AdminRuntimeStoryDetail {
    let summary = build_story_summary(&rows);
    let edges = build_story_edges(&rows);
    let connected_ids = connected_node_ids(&edges);
    let nodes: Vec<AdminRuntimeStoryNode> = rows
        .iter()
        .map(|row| build_story_node(row, &connected_ids))
        .collect();
    let timeline_items = rows.iter().map(Into::into).collect();

    AdminRuntimeStoryDetail {
        summary,
        nodes,
        edges,
        timeline_items,
    }
}

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

pub(crate) fn build_story_summary(rows: &[StoryWorkRow]) -> AdminRuntimeStoryListItem {
    let created_at = rows
        .iter()
        .map(|row| row.created_at)
        .min()
        .unwrap_or_else(Utc::now);
    let updated_at = rows
        .iter()
        .map(story_row_end_timestamp)
        .max()
        .unwrap_or(created_at);
    let services = rows.iter().fold(Vec::new(), |mut services, row| {
        if !services.contains(&row.service) {
            services.push(row.service.clone());
        }
        services
    });
    let pattern = collapse_story_pattern(rows.iter().map(|row| row.item_type.clone()));
    let duration_ms = updated_at
        .signed_duration_since(created_at)
        .num_milliseconds()
        .max(0);

    AdminRuntimeStoryListItem {
        title: story_title(rows),
        correlation_id: rows
            .first()
            .map(|row| row.correlation_id.clone())
            .unwrap_or_default(),
        status: story_status(rows).to_owned(),
        duration: duration_ms,
        node_count: rows.len(),
        error_count: rows
            .iter()
            .filter(|row| matches!(row.status.as_str(), "failed" | "dead"))
            .count(),
        services,
        pattern,
        root_error: story_root_error(rows),
        created_at,
        updated_at,
    }
}

pub(crate) fn build_story_node(
    row: &StoryWorkRow,
    connected_ids: &std::collections::BTreeSet<String>,
) -> AdminRuntimeStoryNode {
    let component = if connected_ids.contains(&row.id) {
        "connected"
    } else {
        "orphan"
    };
    AdminRuntimeStoryNode {
        id: row.id.clone(),
        node_type: row.item_type.clone(),
        name: row.name.clone(),
        display_name: display_name_for_node(row),
        status: row.status.clone(),
        service: row.service.clone(),
        timestamp: row.created_at,
        duration_ms: row_duration_ms(row),
        error: row.last_error.clone(),
        metadata: serde_json::json!({
            "attempts": row.attempts,
            "max_attempts": row.max_attempts,
            "correlation_id": row.correlation_id,
            "causation_id": row.causation_id,
            "component": component,
            "source_metadata": row.metadata,
        }),
    }
}

pub(crate) fn story_title(rows: &[StoryWorkRow]) -> String {
    if let Some(title) = rows
        .iter()
        .find_map(|row| story_display_descriptor(row).and_then(|descriptor| descriptor.story_title))
    {
        return title;
    }

    if let Some(title) = rows.iter().find_map(remote_proxy_story_title) {
        return title;
    }

    if let Some(event_title) = rows
        .iter()
        .find(|row| matches!(row.item_type.as_str(), "event" | "outbox_event"))
        .map(|row| story_title_from_event_name(&row.name))
    {
        return event_title;
    }

    rows.first()
        .map(display_name_for_node)
        .unwrap_or_else(|| "Runtime Story".to_owned())
}

pub(crate) fn display_name_for_node(row: &StoryWorkRow) -> String {
    if let Some(descriptor) = story_display_descriptor(row) {
        return descriptor.display_name.clone();
    }

    if row.item_type == "http_request" {
        return http_request_display_name(&row.name);
    }

    humanize_runtime_name(&row.name)
}

pub(crate) fn remote_proxy_story_title(row: &StoryWorkRow) -> Option<String> {
    if row.item_type != "remote_proxy_call" {
        return None;
    }

    json_string(&row.metadata, "story_title")
        .or_else(|| json_string(&row.metadata, "display_name"))
        .map(str::to_owned)
}

pub(crate) fn story_display_descriptor(row: &StoryWorkRow) -> Option<StoryDisplayDescriptor> {
    if row.item_type == "http_request" {
        let (method, path) = row.name.split_once(' ')?;
        return story_display_descriptors().into_iter().find(|descriptor| {
            matches!(
                &descriptor.source,
                StoryDisplaySource::HttpRequest {
                    method: descriptor_method,
                    path: descriptor_path,
                } if descriptor_method == method && descriptor_path == path
            )
        });
    }

    story_display_descriptors().into_iter().find(|descriptor| {
        matches!(
            &descriptor.source,
            StoryDisplaySource::ExecutionName { name } if name == row.name.as_str()
        )
    })
}

pub(crate) fn story_display_descriptors() -> Vec<StoryDisplayDescriptor> {
    crate::story_display_catalog()
}

pub(crate) fn story_title_from_event_name(value: &str) -> String {
    let parts = semantic_name_parts(value);
    if parts.is_empty() {
        return humanize_runtime_name(value);
    }

    if let Some(subject) = parts.strip_suffix(&["registered"]) {
        return format!("{} Registration", humanize_parts(subject));
    }
    if let Some(subject) = parts.strip_suffix(&["uploaded"]) {
        return format!("{} Upload", humanize_parts(subject));
    }
    if let Some(subject) = parts.strip_suffix(&["created"]) {
        return format!("{} Creation", humanize_parts(subject));
    }
    if let Some(subject) = parts.strip_suffix(&["deleted"]) {
        return format!("{} Deletion", humanize_parts(subject));
    }

    humanize_parts(&parts)
}

pub(crate) fn http_request_display_name(value: &str) -> String {
    let Some((method, path)) = value.split_once(' ') else {
        return humanize_runtime_name(value);
    };
    let Some(resource) = path
        .split('/')
        .filter(|segment| {
            !segment.is_empty()
                && !segment.starts_with(':')
                && !segment.starts_with('{')
                && !is_version_path_segment(segment)
        })
        .next_back()
    else {
        return value.to_owned();
    };
    let resource = singularize(resource);
    let action = match method {
        "POST" => "Create",
        "GET" => "Fetch",
        "PUT" | "PATCH" => "Update",
        "DELETE" => "Delete",
        _ => method,
    };

    format!("{action} {} Request", humanize_parts(&[resource.as_str()]))
}

pub(crate) fn humanize_runtime_name(value: &str) -> String {
    if value.contains('/') || value.contains(' ') {
        return value.to_owned();
    }

    let parts = semantic_name_parts(value);
    if parts.is_empty() {
        return value.to_owned();
    }

    humanize_parts(&parts)
}

pub(crate) fn semantic_name_parts(value: &str) -> Vec<&str> {
    let without_version = value
        .rsplit_once(".v")
        .filter(|(_, version)| version.chars().all(|character| character.is_ascii_digit()))
        .map(|(name, _)| name)
        .unwrap_or(value);
    let parts = without_version
        .split(['.', '_', '-'])
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() > 1 {
        parts[1..].to_vec()
    } else {
        parts
    }
}

pub(crate) fn humanize_parts(parts: &[&str]) -> String {
    parts
        .iter()
        .map(|part| {
            let mut characters = part.chars();
            let Some(first) = characters.next() else {
                return String::new();
            };
            format!(
                "{}{}",
                first.to_ascii_uppercase(),
                characters.as_str().to_ascii_lowercase()
            )
        })
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn is_version_path_segment(value: &str) -> bool {
    value
        .strip_prefix('v')
        .is_some_and(|version| version.chars().all(|character| character.is_ascii_digit()))
}

pub(crate) fn singularize(value: &str) -> String {
    if let Some(prefix) = value.strip_suffix("ies") {
        return format!("{prefix}y");
    }
    if value.len() > 1 {
        if let Some(prefix) = value.strip_suffix('s') {
            return prefix.to_owned();
        }
    }

    value.to_owned()
}

pub(crate) fn build_story_edges(rows: &[StoryWorkRow]) -> Vec<AdminRuntimeStoryEdge> {
    let ids = rows
        .iter()
        .map(|row| row.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();

    rows.iter()
        .filter_map(|current| {
            let source = explicit_causal_source(current, &ids)?;

            Some(AdminRuntimeStoryEdge {
                id: format!("{source}:{}:causation", current.id),
                source: source.to_owned(),
                target: current.id.clone(),
                edge_type: "causation".to_owned(),
                label: None,
            })
        })
        .collect()
}

pub(crate) fn explicit_causal_source(
    row: &StoryWorkRow,
    ids: &std::collections::BTreeSet<&str>,
) -> Option<String> {
    if let Some(source) = row
        .causation_id
        .as_deref()
        .and_then(|id| causal_source_id(id, ids, &row.id))
    {
        return Some(source);
    }

    for key in [
        "outbox_event_id",
        "event_id",
        "causation_id",
        "parent_id",
        "source_id",
        "function_run_id",
    ] {
        if let Some(source) =
            json_string(&row.metadata, key).and_then(|id| causal_source_id(id, ids, &row.id))
        {
            return Some(source);
        }
    }

    if let Some(runtime_context) = row.metadata.get("_lenso_runtime") {
        for key in ["outbox_event_id", "event_id", "causation_id", "parent_id"] {
            if let Some(source) =
                json_string(runtime_context, key).and_then(|id| causal_source_id(id, ids, &row.id))
            {
                return Some(source);
            }
        }
    }

    if let Some(headers) = row.metadata.get("headers") {
        for key in ["outbox_event_id", "event_id", "causation_id", "parent_id"] {
            if let Some(source) =
                json_string(headers, key).and_then(|id| causal_source_id(id, ids, &row.id))
            {
                return Some(source);
            }
        }
    }

    None
}

pub(crate) fn causal_source_id(
    candidate: &str,
    ids: &std::collections::BTreeSet<&str>,
    current_id: &str,
) -> Option<String> {
    if candidate != current_id && ids.contains(candidate) {
        return Some(candidate.to_owned());
    }

    request_story_node_id(candidate, ids).filter(|source| source != current_id)
}

pub(crate) fn request_story_node_id(
    request_id: &str,
    ids: &std::collections::BTreeSet<&str>,
) -> Option<String> {
    let node_id = format!("httpreq_{request_id}");
    ids.contains(node_id.as_str()).then_some(node_id)
}

pub(crate) fn json_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

pub(crate) fn connected_node_ids(
    edges: &[AdminRuntimeStoryEdge],
) -> std::collections::BTreeSet<String> {
    edges
        .iter()
        .flat_map(|edge| [edge.source.clone(), edge.target.clone()])
        .collect()
}

pub(crate) fn collapse_story_pattern(types: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut pattern = Vec::new();
    for node_type in types {
        if pattern.last() != Some(&node_type) {
            pattern.push(node_type);
        }
    }
    pattern
}

pub(crate) fn story_status(rows: &[StoryWorkRow]) -> &'static str {
    if rows.iter().any(|row| row.status == "dead") {
        return "dead";
    }
    if rows.iter().any(|row| row.status == "failed") {
        return "failed";
    }
    if rows
        .iter()
        .any(|row| matches!(row.status.as_str(), "processing" | "running"))
    {
        return "running";
    }
    if rows
        .iter()
        .all(|row| matches!(row.status.as_str(), "published" | "completed"))
    {
        return "completed";
    }
    "pending"
}

pub(crate) fn story_root_error(rows: &[StoryWorkRow]) -> Option<String> {
    rows.iter()
        .filter(|row| matches!(row.status.as_str(), "failed" | "dead"))
        .min_by_key(|row| row.created_at)
        .map(|row| {
            let error = row
                .last_error
                .clone()
                .unwrap_or_else(|| format!("{} runtime work", row.status));
            format!("{}: {error}", row.name)
        })
}

pub(crate) fn story_row_end_timestamp(row: &StoryWorkRow) -> DateTime<Utc> {
    row.completed_at.unwrap_or(row.created_at)
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
    use platform_core::TelemetrySpan;

    #[test]
    fn story_edges_do_not_guess_sequence_edges_for_unlinked_work() {
        let rows = vec![
            story_row("event", "evt_1", None, "2026-05-31T00:00:00Z"),
            story_row("function", "fnrun_1", None, "2026-05-31T00:00:10Z"),
        ];

        assert!(build_story_edges(&rows).is_empty());
    }

    #[test]
    fn story_edges_preserve_explicit_causality() {
        let mut rows = vec![
            story_row("event", "evt_parent", None, "2026-05-31T00:00:00Z"),
            story_row(
                "function",
                "fnrun_child",
                Some("evt_parent"),
                "2026-05-31T00:00:10Z",
            ),
        ];
        rows[1].causation_id = None;
        rows[1].metadata = serde_json::json!({ "outbox_event_id": "evt_parent" });

        let edges = build_story_edges(&rows);

        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].source, "evt_parent");
        assert_eq!(edges[0].target, "fnrun_child");
        assert_eq!(edges[0].edge_type, "causation");
    }

    #[test]
    fn story_edges_read_runtime_context_causation() {
        let mut rows = vec![
            story_row("event", "evt_parent", None, "2026-05-31T00:00:00Z"),
            story_row("function", "fnrun_child", None, "2026-05-31T00:00:10Z"),
        ];
        rows[1].metadata = serde_json::json!({
            "_lenso_runtime": {
                "causation_id": "evt_parent"
            }
        });

        let edges = build_story_edges(&rows);

        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].source, "evt_parent");
        assert_eq!(edges[0].target, "fnrun_child");
        assert_eq!(edges[0].edge_type, "causation");
    }

    #[test]
    fn story_summary_uses_business_title_without_renaming_nodes() {
        let mut rows = vec![
            story_row(
                "http_request",
                "httpreq_req_1",
                None,
                "2026-05-31T00:00:00Z",
            ),
            story_row("event", "evt_user_registered", None, "2026-05-31T00:00:10Z"),
            story_row(
                "function",
                "fnrun_welcome",
                Some("evt_user_registered"),
                "2026-05-31T00:00:20Z",
            ),
        ];
        rows[0].name = "POST /v1/identity/users".to_owned();
        rows[1].name = "identity.user_registered.v1".to_owned();
        rows[2].name = "notifications.send_welcome_email.v1".to_owned();

        let detail = build_story_detail(rows);

        assert_eq!(detail.summary.title, "User Registration");
        assert_eq!(detail.nodes[1].name, "identity.user_registered.v1");
        assert_eq!(detail.nodes[1].display_name, "User Registered");
        assert_eq!(detail.nodes[2].name, "notifications.send_welcome_email.v1");
        assert_eq!(detail.nodes[2].display_name, "Send Welcome Email");
    }

    #[test]
    fn story_detail_marks_orphan_and_connected_components() {
        let rows = vec![
            story_row("event", "evt_parent", None, "2026-05-31T00:00:00Z"),
            story_row(
                "function",
                "fnrun_child",
                Some("evt_parent"),
                "2026-05-31T00:00:10Z",
            ),
            story_row("event", "evt_orphan", None, "2026-05-31T00:00:20Z"),
        ];

        let detail = build_story_detail(rows);

        let components = detail
            .nodes
            .iter()
            .map(|node| {
                (
                    node.id.as_str(),
                    node.metadata["component"].as_str().unwrap_or_default(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            components,
            vec![
                ("evt_parent", "connected"),
                ("fnrun_child", "connected"),
                ("evt_orphan", "orphan"),
            ]
        );
    }

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
    fn story_summary_cursor_uses_stable_updated_at_boundaries() {
        let mut rows = vec![
            story_row("event", "evt_a", None, "2026-05-31T00:00:00Z"),
            story_row("event", "evt_b", None, "2026-05-31T00:01:00Z"),
        ];
        rows[1].completed_at = Some(parse_time("2026-05-31T00:03:00Z"));

        let summaries = build_story_summaries(rows);

        assert_eq!(summaries[0].correlation_id, "corr_test");
        assert_eq!(summaries[0].updated_at, parse_time("2026-05-31T00:03:00Z"));
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
        causation_id: Option<&str>,
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
            causation_id: causation_id.map(ToOwned::to_owned),
            service: "runtime".to_owned(),
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
