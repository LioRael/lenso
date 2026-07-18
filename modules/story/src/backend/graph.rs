use super::catalog::story_display_catalog;
#[allow(clippy::wildcard_imports)]
use super::*;

pub(super) fn build_story_summaries(rows: Vec<StoryWorkRow>) -> Vec<AdminRuntimeStoryListItem> {
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

pub(super) fn build_story_detail(rows: Vec<StoryWorkRow>) -> AdminRuntimeStoryDetail {
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
        federation: None,
    }
}

pub(super) fn runtime_node_index(rows: &[StoryWorkRow]) -> RuntimeNodeIndex {
    RuntimeNodeIndex {
        ids: rows.iter().map(|row| row.id.clone()).collect(),
    }
}

fn build_story_summary(rows: &[StoryWorkRow]) -> AdminRuntimeStoryListItem {
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
        story_kind: "runtime".to_owned(),
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

fn build_story_node(
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

fn story_title(rows: &[StoryWorkRow]) -> String {
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

fn display_name_for_node(row: &StoryWorkRow) -> String {
    if let Some(descriptor) = story_display_descriptor(row) {
        return descriptor.display_name.clone();
    }

    if row.item_type == "http_request" {
        return http_request_display_name(&row.name);
    }

    humanize_runtime_name(&row.name)
}

fn remote_proxy_story_title(row: &StoryWorkRow) -> Option<String> {
    if row.item_type != "remote_proxy_call" {
        return None;
    }

    json_string(&row.metadata, "story_title")
        .or_else(|| json_string(&row.metadata, "display_name"))
        .map(str::to_owned)
}

fn story_display_descriptor(row: &StoryWorkRow) -> Option<StoryDisplayDescriptor> {
    if row.item_type == "http_request" {
        let (method, path) = row.name.split_once(' ')?;
        return story_display_catalog().into_iter().find(|descriptor| {
            matches!(
                &descriptor.source,
                StoryDisplaySource::HttpRequest {
                    method: descriptor_method,
                    path: descriptor_path,
                } if descriptor_method == method && descriptor_path == path
            )
        });
    }

    story_display_catalog().into_iter().find(|descriptor| {
        matches!(
            &descriptor.source,
            StoryDisplaySource::ExecutionName { name } if name == row.name.as_str()
        )
    })
}

fn story_title_from_event_name(value: &str) -> String {
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

fn http_request_display_name(value: &str) -> String {
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

fn humanize_runtime_name(value: &str) -> String {
    if value.contains('/') || value.contains(' ') {
        return value.to_owned();
    }

    let parts = semantic_name_parts(value);
    if parts.is_empty() {
        return value.to_owned();
    }

    humanize_parts(&parts)
}

fn semantic_name_parts(value: &str) -> Vec<&str> {
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

fn humanize_parts(parts: &[&str]) -> String {
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

fn is_version_path_segment(value: &str) -> bool {
    value
        .strip_prefix('v')
        .is_some_and(|version| version.chars().all(|character| character.is_ascii_digit()))
}

fn singularize(value: &str) -> String {
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

fn build_story_edges(rows: &[StoryWorkRow]) -> Vec<AdminRuntimeStoryEdge> {
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

fn explicit_causal_source(
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

fn causal_source_id(
    candidate: &str,
    ids: &std::collections::BTreeSet<&str>,
    current_id: &str,
) -> Option<String> {
    if candidate != current_id && ids.contains(candidate) {
        return Some(candidate.to_owned());
    }

    request_story_node_id(candidate, ids).filter(|source| source != current_id)
}

fn request_story_node_id(
    request_id: &str,
    ids: &std::collections::BTreeSet<&str>,
) -> Option<String> {
    let node_id = format!("httpreq_{request_id}");
    ids.contains(node_id.as_str()).then_some(node_id)
}

fn connected_node_ids(edges: &[AdminRuntimeStoryEdge]) -> std::collections::BTreeSet<String> {
    edges
        .iter()
        .flat_map(|edge| [edge.source.clone(), edge.target.clone()])
        .collect()
}

fn json_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn collapse_story_pattern(types: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut pattern = Vec::new();
    for node_type in types {
        if pattern.last() != Some(&node_type) {
            pattern.push(node_type);
        }
    }
    pattern
}

fn story_status(rows: &[StoryWorkRow]) -> &'static str {
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

fn story_root_error(rows: &[StoryWorkRow]) -> Option<String> {
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

fn story_row_end_timestamp(row: &StoryWorkRow) -> DateTime<Utc> {
    row.completed_at.unwrap_or(row.created_at)
}

pub(super) fn row_duration_ms(row: &StoryWorkRow) -> i64 {
    let Some(started_at) = row.started_at else {
        return 0;
    };
    row.completed_at
        .unwrap_or(started_at)
        .signed_duration_since(started_at)
        .num_milliseconds()
        .max(0)
}

pub(super) fn timeline_item_type(item_type: &str, status: &str, attempts: i32) -> &'static str {
    if status == "dead" {
        return "dead_letter";
    }
    if status == "failed" {
        return "failure";
    }
    if attempts > 1 {
        return "retry";
    }
    match item_type {
        "http" | "http_request" => "http_request",
        "event" | "outbox_event" => "outbox_event",
        "function" | "function_run" => "function_run",
        "remote_proxy_call" => "remote_proxy_call",
        "admin_action" => "admin_action",
        _ => "runtime",
    }
}
