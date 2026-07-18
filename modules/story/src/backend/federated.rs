#[allow(clippy::wildcard_imports)]
use super::*;
use crate::federation::{
    FederatedRuntimeStory, FederatedStoryReader, FederatedStorySegment,
    FederatedStoryTechnicalEvidenceKind, OpenTelemetryFederatedStoryEnrichmentProvider,
};
use sqlx::FromRow;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Debug, FromRow)]
struct FederatedSummaryRow {
    story_id: String,
    source_service_id: String,
    status: String,
    operation_kind: String,
    definition_name: Option<String>,
    started_at: DateTime<Utc>,
    completed_at: DateTime<Utc>,
}

pub(super) async fn fetch_federated_story_summaries(
    ctx: &AppContext,
    request_ctx: &RequestContext,
    created_before: Option<DateTime<Utc>>,
    limit: i64,
) -> Result<Vec<AdminRuntimeStoryListItem>, ApiErrorResponse> {
    let tenant_scope = request_ctx
        .tenant_id
        .as_ref()
        .map_or("", |tenant_id| tenant_id.0.as_str());
    let rows = sqlx::query_as::<_, FederatedSummaryRow>(
        r#"
        with latest_segments as (
            select distinct on (source_service_id, segment_id)
                story_id,
                source_service_id,
                segment_id,
                segment->>'status' as status,
                segment->'operation'->>'kind' as operation_kind,
                segment->'workflow'->>'definitionName' as definition_name,
                (segment->>'startedAt')::timestamptz as started_at,
                (segment->>'completedAt')::timestamptz as completed_at
            from platform.federated_story_segments
            where tenant_scope = $1
            order by source_service_id, segment_id, evidence_revision desc
        ), story_keys as (
            select story_id, max(completed_at) as updated_at
            from latest_segments
            group by story_id
            having ($2::timestamptz is null or max(completed_at) < $2)
            order by updated_at desc, story_id
            limit $3
        )
        select latest.*
        from latest_segments latest
        join story_keys using (story_id)
        order by latest.story_id, latest.started_at, latest.source_service_id, latest.segment_id
        "#,
    )
    .bind(tenant_scope)
    .bind(created_before)
    .bind(limit)
    .fetch_all(&ctx.db)
    .await
    .map_err(|source| query_error(source, request_ctx))?;

    let mut grouped = BTreeMap::<String, Vec<FederatedSummaryRow>>::new();
    for row in rows {
        grouped.entry(row.story_id.clone()).or_default().push(row);
    }
    let mut summaries = grouped
        .into_values()
        .map(|rows| summary_from_rows(&rows))
        .collect::<Vec<_>>();
    summaries.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.correlation_id.cmp(&right.correlation_id))
    });
    Ok(summaries)
}

pub(super) async fn fetch_federated_story_detail(
    ctx: &AppContext,
    request_ctx: &RequestContext,
    story_id: &str,
) -> Result<AdminRuntimeStoryDetail, ApiErrorResponse> {
    Ok(project_federated_story(
        fetch_federated_story(ctx, request_ctx, story_id).await?,
    ))
}

pub(super) async fn fetch_federated_story(
    ctx: &AppContext,
    request_ctx: &RequestContext,
    story_id: &str,
) -> Result<FederatedRuntimeStory, ApiErrorResponse> {
    let tenant_id = request_ctx
        .tenant_id
        .as_ref()
        .map(|tenant_id| tenant_id.0.as_str());
    FederatedStoryReader::new(ctx.db.clone())
        .with_enrichment_provider(Arc::new(
            OpenTelemetryFederatedStoryEnrichmentProvider::new(ctx.telemetry_spans.clone()),
        ))
        .story(story_id, tenant_id)
        .await
        .map_err(|error| ApiErrorResponse::with_context(error, request_ctx))
}

pub(super) fn project_federated_story(story: FederatedRuntimeStory) -> AdminRuntimeStoryDetail {
    let summary = summary_from_segments(&story.segments, !story.gaps.is_empty());
    let node_ids = story
        .segments
        .iter()
        .map(|segment| (segment.segment.segment_id.as_str(), segment.id.as_str()))
        .collect::<BTreeMap<_, _>>();
    let nodes = story
        .segments
        .iter()
        .map(federated_node)
        .collect::<Vec<_>>();
    let edges = story
        .segments
        .iter()
        .filter_map(|segment| federated_edge(segment, &node_ids))
        .collect::<Vec<_>>();
    let timeline_items = story
        .segments
        .iter()
        .map(federated_timeline_item)
        .collect::<Vec<_>>();
    let workflow_entities = workflow_entities(&story.segments);
    let federation = AdminFederatedStoryEvidence {
        protocol: story.protocol,
        tenant_id: story.tenant_id,
        assembled_at: story.assembled_at,
        gaps: story.gaps,
        workflow_entities,
        reliability: story.reliability,
    };
    AdminRuntimeStoryDetail {
        summary,
        nodes,
        edges,
        timeline_items,
        federation: Some(federation),
    }
}

pub(super) fn federated_technical_operations(
    story: &FederatedRuntimeStory,
) -> Vec<AdminRuntimeTechnicalOperation> {
    let mut operations = story
        .segments
        .iter()
        .flat_map(|segment| {
            segment
                .technical_evidence
                .iter()
                .map(move |evidence| AdminRuntimeTechnicalOperation {
                    id: evidence.id.clone(),
                    story_id: story.story_id.clone(),
                    correlation_id: story.story_id.clone(),
                    related_node_id: Some(segment.id.clone()),
                    category: match evidence.kind {
                        FederatedStoryTechnicalEvidenceKind::Trace => "runtime",
                        FederatedStoryTechnicalEvidenceKind::Metric => "runtime",
                        FederatedStoryTechnicalEvidenceKind::Log => "runtime",
                    }
                    .to_owned(),
                    name: evidence
                        .attributes
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("Federated Story telemetry")
                        .to_owned(),
                    status: evidence
                        .attributes
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("observed")
                        .to_owned(),
                    started_at: segment.segment.started_at,
                    ended_at: segment.segment.completed_at,
                    duration_ms: segment
                        .segment
                        .completed_at
                        .signed_duration_since(segment.segment.started_at)
                        .num_milliseconds()
                        .max(0),
                    attributes: evidence.attributes.clone(),
                    source: evidence.source.clone(),
                })
        })
        .collect::<Vec<_>>();
    operations.sort_by(|left, right| {
        left.started_at
            .cmp(&right.started_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    operations
}

fn summary_from_rows(rows: &[FederatedSummaryRow]) -> AdminRuntimeStoryListItem {
    let story_id = rows
        .first()
        .map(|row| row.story_id.clone())
        .unwrap_or_default();
    let created_at = rows
        .iter()
        .map(|row| row.started_at)
        .min()
        .unwrap_or_else(Utc::now);
    let updated_at = rows
        .iter()
        .map(|row| row.completed_at)
        .max()
        .unwrap_or(created_at);
    let services = unique_strings(rows.iter().map(|row| row.source_service_id.clone()));
    let pattern = collapse_strings(rows.iter().map(|row| row.operation_kind.clone()));
    let status = federated_status(rows.iter().map(|row| row.status.as_str()), false);
    let title = rows
        .iter()
        .find_map(|row| row.definition_name.as_ref())
        .map_or_else(
            || format!("Federated Runtime Story {story_id}"),
            |name| format!("{} Federated Workflow", humanize(name)),
        );
    AdminRuntimeStoryListItem {
        story_kind: "federated".to_owned(),
        title,
        correlation_id: story_id,
        status: status.to_owned(),
        duration: updated_at
            .signed_duration_since(created_at)
            .num_milliseconds()
            .max(0),
        node_count: rows.len(),
        error_count: rows
            .iter()
            .filter(|row| is_federated_error(&row.status))
            .count(),
        services,
        pattern,
        root_error: rows
            .iter()
            .find(|row| is_federated_error(&row.status))
            .map(|row| format!("{}: {}", row.operation_kind, row.status)),
        created_at,
        updated_at,
    }
}

fn summary_from_segments(
    segments: &[FederatedStorySegment],
    has_gaps: bool,
) -> AdminRuntimeStoryListItem {
    let story_id = segments
        .first()
        .map(|segment| segment.segment.story_id.clone())
        .unwrap_or_default();
    let created_at = segments
        .iter()
        .map(|segment| segment.segment.started_at)
        .min()
        .unwrap_or_else(Utc::now);
    let updated_at = segments
        .iter()
        .map(|segment| segment.segment.completed_at)
        .max()
        .unwrap_or(created_at);
    let services = unique_strings(
        segments
            .iter()
            .map(|segment| segment.segment.source.service_id.clone()),
    );
    let pattern = collapse_strings(
        segments
            .iter()
            .map(|segment| segment.segment.operation.kind.clone()),
    );
    let title = segments
        .iter()
        .find_map(|segment| segment.segment.workflow.as_ref())
        .map_or_else(
            || format!("Federated Runtime Story {story_id}"),
            |workflow| format!("{} Federated Workflow", humanize(&workflow.definition_name)),
        );
    AdminRuntimeStoryListItem {
        story_kind: "federated".to_owned(),
        title,
        correlation_id: story_id,
        status: federated_status(
            segments
                .iter()
                .map(|segment| segment.segment.status.as_str()),
            has_gaps,
        )
        .to_owned(),
        duration: updated_at
            .signed_duration_since(created_at)
            .num_milliseconds()
            .max(0),
        node_count: segments.len(),
        error_count: segments
            .iter()
            .filter(|segment| is_federated_error(&segment.segment.status))
            .count(),
        services,
        pattern,
        root_error: segments
            .iter()
            .find(|segment| is_federated_error(&segment.segment.status))
            .map(|segment| {
                format!(
                    "{}: {}",
                    segment.segment.operation.operation_id, segment.segment.status
                )
            }),
        created_at,
        updated_at,
    }
}

fn federated_node(segment: &FederatedStorySegment) -> AdminRuntimeStoryNode {
    let raw_status = &segment.segment.status;
    AdminRuntimeStoryNode {
        id: segment.id.clone(),
        node_type: federated_node_type(segment).to_owned(),
        name: segment.segment.operation.operation_id.clone(),
        display_name: humanize(&segment.segment.operation.operation_id),
        status: console_status(raw_status).to_owned(),
        service: segment.segment.source.service_id.clone(),
        timestamp: segment.segment.started_at,
        duration_ms: segment
            .segment
            .completed_at
            .signed_duration_since(segment.segment.started_at)
            .num_milliseconds()
            .max(0),
        error: is_federated_error(raw_status)
            .then(|| format!("Federated evidence state: {raw_status}")),
        metadata: serde_json::json!({
            "story_id": segment.segment.story_id,
            "segment_id": segment.segment.segment_id,
            "evidence_revision": segment.segment.evidence_revision,
            "evidence_status": segment.segment.status,
            "attempt": segment.segment.attempt,
            "source": segment.segment.source,
            "operation": segment.segment.operation,
            "contract": segment.segment.contract,
            "tenant_id": segment.segment.tenant_id,
            "workflow": segment.segment.workflow,
            "technical_evidence": segment.technical_evidence,
        }),
    }
}

fn federated_edge(
    segment: &FederatedStorySegment,
    node_ids: &BTreeMap<&str, &str>,
) -> Option<AdminRuntimeStoryEdge> {
    let source_segment_id = segment
        .segment
        .parent_segment_id
        .as_deref()
        .or(segment.segment.causation_id.as_deref())?;
    let source = node_ids.get(source_segment_id)?;
    Some(AdminRuntimeStoryEdge {
        id: format!("{source}:{}:federated-causation", segment.id),
        source: (*source).to_owned(),
        target: segment.id.clone(),
        edge_type: "causation".to_owned(),
        label: Some("cross-service evidence".to_owned()),
    })
}

fn federated_timeline_item(segment: &FederatedStorySegment) -> AdminRuntimeTimelineItem {
    AdminRuntimeTimelineItem {
        item_type: federated_node_type(segment).to_owned(),
        id: segment.id.clone(),
        name: humanize(&segment.segment.operation.operation_id),
        status: console_status(&segment.segment.status).to_owned(),
        attempts: i32::try_from(segment.segment.attempt).unwrap_or(i32::MAX),
        max_attempts: i32::try_from(segment.segment.attempt).unwrap_or(i32::MAX),
        created_at: segment.segment.recorded_at,
        started_at: Some(segment.segment.started_at),
        completed_at: Some(segment.segment.completed_at),
        last_error: is_federated_error(&segment.segment.status)
            .then(|| format!("Federated evidence state: {}", segment.segment.status)),
        correlation_id: segment.segment.story_id.clone(),
        related_node_id: Some(segment.id.clone()),
    }
}

fn workflow_entities(segments: &[FederatedStorySegment]) -> Vec<AdminFederatedWorkflowEntity> {
    let mut entities = BTreeMap::<String, AdminFederatedWorkflowEntity>::new();
    for segment in segments {
        let Some(workflow) = &segment.segment.workflow else {
            continue;
        };
        insert_entity(
            &mut entities,
            AdminFederatedWorkflowEntity {
                kind: AdminFederatedWorkflowEntityKind::Instance,
                id: workflow.instance_id.clone(),
                node_id: segment.id.clone(),
                instance_id: workflow.instance_id.clone(),
                parent_id: workflow.parent_instance_id.clone(),
                label: format!(
                    "{}/{}@{}",
                    workflow.definition_owner,
                    workflow.definition_name,
                    workflow.definition_version
                ),
                state: segment.segment.status.clone(),
                service_id: segment.segment.source.service_id.clone(),
                attempt: segment.segment.attempt,
                observed_at: segment.segment.recorded_at,
            },
        );
        if let Some(step_id) = &workflow.step_id {
            insert_entity(
                &mut entities,
                entity_for_segment(
                    segment,
                    AdminFederatedWorkflowEntityKind::Step,
                    step_id,
                    Some(&workflow.instance_id),
                    format!("Step {step_id}"),
                ),
            );
            insert_entity(
                &mut entities,
                entity_for_segment(
                    segment,
                    AdminFederatedWorkflowEntityKind::Attempt,
                    &format!("{step_id}:attempt:{}", segment.segment.attempt),
                    Some(step_id),
                    format!("Attempt {}", segment.segment.attempt),
                ),
            );
        }
        if let Some(parent_instance_id) = &workflow.parent_instance_id {
            insert_entity(
                &mut entities,
                entity_for_segment(
                    segment,
                    AdminFederatedWorkflowEntityKind::Child,
                    &workflow.instance_id,
                    Some(parent_instance_id),
                    format!("Child workflow {}", workflow.definition_name),
                ),
            );
        }
        if let Some(compensation_id) = &workflow.compensation_id {
            insert_entity(
                &mut entities,
                entity_for_segment(
                    segment,
                    AdminFederatedWorkflowEntityKind::Compensation,
                    compensation_id,
                    workflow.step_id.as_deref(),
                    format!("Compensation {compensation_id}"),
                ),
            );
        }
        if let Some(intervention_id) = &workflow.intervention_id {
            insert_entity(
                &mut entities,
                entity_for_segment(
                    segment,
                    AdminFederatedWorkflowEntityKind::Intervention,
                    intervention_id,
                    workflow.step_id.as_deref().or(Some(&workflow.instance_id)),
                    format!("Intervention {intervention_id}"),
                ),
            );
        }
        if is_timer_segment(segment) {
            insert_entity(
                &mut entities,
                entity_for_segment(
                    segment,
                    AdminFederatedWorkflowEntityKind::Timer,
                    &segment.segment.segment_id,
                    workflow.step_id.as_deref(),
                    if segment.segment.status == "retry_scheduled" {
                        "Retry timer".to_owned()
                    } else {
                        "Workflow timer".to_owned()
                    },
                ),
            );
        }
    }
    let mut entities = entities.into_values().collect::<Vec<_>>();
    entities.sort_by(|left, right| {
        left.observed_at
            .cmp(&right.observed_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    entities
}

fn entity_for_segment(
    segment: &FederatedStorySegment,
    kind: AdminFederatedWorkflowEntityKind,
    id: &str,
    parent_id: Option<&str>,
    label: String,
) -> AdminFederatedWorkflowEntity {
    let workflow = segment
        .segment
        .workflow
        .as_ref()
        .expect("workflow entity requires workflow evidence");
    AdminFederatedWorkflowEntity {
        kind,
        id: id.to_owned(),
        node_id: segment.id.clone(),
        instance_id: workflow.instance_id.clone(),
        parent_id: parent_id.map(str::to_owned),
        label,
        state: segment.segment.status.clone(),
        service_id: segment.segment.source.service_id.clone(),
        attempt: segment.segment.attempt,
        observed_at: segment.segment.recorded_at,
    }
}

fn insert_entity(
    entities: &mut BTreeMap<String, AdminFederatedWorkflowEntity>,
    entity: AdminFederatedWorkflowEntity,
) {
    let key = format!("{:?}:{}", entity.kind, entity.id);
    let replace = entities
        .get(&key)
        .is_none_or(|existing| existing.observed_at <= entity.observed_at);
    if replace {
        entities.insert(key, entity);
    }
}

fn federated_node_type(segment: &FederatedStorySegment) -> &'static str {
    let workflow = segment.segment.workflow.as_ref();
    if workflow
        .and_then(|workflow| workflow.compensation_id.as_ref())
        .is_some()
    {
        return "compensation";
    }
    if workflow
        .and_then(|workflow| workflow.intervention_id.as_ref())
        .is_some()
    {
        return "intervention";
    }
    if is_timer_segment(segment) {
        return "timer";
    }
    match segment.segment.operation.kind.as_str() {
        "direct_http" | "direct_grpc" | "http" => "http_request",
        "event_contract" | "event" => "outbox_event",
        "durable_workflow" => "workflow",
        _ => "runtime",
    }
}

fn is_timer_segment(segment: &FederatedStorySegment) -> bool {
    segment.segment.contract.contract_id == "lenso.workflow-timer"
        || segment.segment.status == "retry_scheduled"
        || segment.segment.operation.operation_id.contains("timer")
        || segment.segment.operation.operation_id.contains("timeout")
}

fn federated_status<'a>(
    statuses: impl IntoIterator<Item = &'a str>,
    has_gaps: bool,
) -> &'static str {
    let statuses = statuses.into_iter().collect::<Vec<_>>();
    if statuses.iter().any(|status| is_federated_error(status)) {
        return "failed";
    }
    if statuses.iter().any(|status| is_federated_running(status)) {
        return "running";
    }
    if has_gaps {
        return "pending";
    }
    if statuses.iter().all(|status| is_federated_complete(status)) {
        return "completed";
    }
    "pending"
}

fn console_status(status: &str) -> &'static str {
    if is_federated_error(status) {
        "failed"
    } else if is_federated_running(status) {
        "running"
    } else if is_federated_complete(status) {
        "completed"
    } else if matches!(status, "retry_scheduled" | "dispatched" | "scheduled") {
        "processing"
    } else {
        "pending"
    }
}

fn is_federated_error(status: &str) -> bool {
    matches!(
        status,
        "failed"
            | "dead"
            | "exhausted"
            | "compensation_failed"
            | "intervention_required"
            | "unauthorized"
    )
}

fn is_federated_running(status: &str) -> bool {
    matches!(
        status,
        "started" | "running" | "paused" | "cancelling" | "compensating"
    )
}

fn is_federated_complete(status: &str) -> bool {
    matches!(
        status,
        "completed"
            | "published"
            | "closed"
            | "compensated"
            | "cancelled"
            | "terminated"
            | "intervention_recorded"
    )
}

fn unique_strings(values: impl IntoIterator<Item = String>) -> Vec<String> {
    values
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn collapse_strings(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut collapsed = Vec::new();
    for value in values {
        if collapsed.last() != Some(&value) {
            collapsed.push(value);
        }
    }
    collapsed
}

fn humanize(value: &str) -> String {
    value
        .split(['.', '_', '-'])
        .filter(|part| !part.is_empty())
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
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::federation::{
        FEDERATED_RUNTIME_STORY_PROTOCOL, FederatedStoryGap, FederatedStoryGapKind,
        FederatedStoryReliabilityEvidence, FederatedStoryReliabilityStatus,
    };
    use lenso_service::{
        EffectiveReliabilityValues, ReliabilityEnforcementBoundary, ReliabilityHealthResult,
        ReliabilityLivenessSemantics, ReliabilityProfile, ReliabilityProfileOverrides,
        ReliabilityReadinessSemantics, ReliabilityServiceState, StorySegment, StorySegmentContract,
        StorySegmentOperation, StorySegmentSource, StorySegmentWorkflow,
    };

    fn workflow_segment(
        service_id: &str,
        segment_id: &str,
        status: &str,
        attempt: u32,
        compensation_id: Option<&str>,
        intervention_id: Option<&str>,
    ) -> FederatedStorySegment {
        let observed_at = DateTime::parse_from_rfc3339("2026-07-18T08:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let segment = StorySegment {
            story_id: "story-support".to_owned(),
            segment_id: segment_id.to_owned(),
            evidence_revision: attempt,
            source: StorySegmentSource {
                service_id: service_id.to_owned(),
                workload_id: format!("{service_id}-worker"),
            },
            operation: StorySegmentOperation {
                kind: "durable_workflow".to_owned(),
                operation_id: format!("workflow.support.{segment_id}"),
            },
            contract: StorySegmentContract {
                contract_id: if status == "timed_out" {
                    "lenso.workflow-timer"
                } else {
                    "support-sla"
                }
                .to_owned(),
                version: "v1".to_owned(),
            },
            status: status.to_owned(),
            attempt,
            started_at: observed_at,
            completed_at: observed_at,
            recorded_at: observed_at,
            tenant_id: Some("tenant_a".to_owned()),
            parent_segment_id: None,
            causation_id: None,
            workflow: Some(StorySegmentWorkflow {
                instance_id: "workflow-1".to_owned(),
                definition_owner: "support".to_owned(),
                definition_name: "support-sla".to_owned(),
                definition_version: "v1".to_owned(),
                step_id: Some("step-1".to_owned()),
                parent_instance_id: None,
                compensation_id: compensation_id.map(str::to_owned),
                intervention_id: intervention_id.map(str::to_owned),
            }),
        };
        FederatedStorySegment {
            id: format!("node-{segment_id}"),
            segment,
            technical_evidence: Vec::new(),
        }
    }

    #[test]
    fn projects_workflow_states_gaps_and_reliability_without_frontend_rules() {
        let assembled_at = Utc::now();
        let report = lenso_service::ReliabilityReport {
            protocol: lenso_service::RELIABILITY_REPORT_PROTOCOL.to_owned(),
            service_id: "support-sla".to_owned(),
            contract_id: "support-reliability".to_owned(),
            contract_version: "v1".to_owned(),
            profile: ReliabilityProfile::Critical,
            overrides: ReliabilityProfileOverrides {
                workflow_backlog_limit: Some(5),
                ..ReliabilityProfileOverrides::default()
            },
            effective_values: EffectiveReliabilityValues {
                availability_target_basis_points: 9_999,
                latency_target_ms: 300,
                queue_backlog_limit: 10,
                workflow_backlog_limit: 5,
                timer_lag_limit_ms: 1_000,
                retry_exhaustion_limit: 1,
                compensation_pressure_limit: 1,
                error_budget: "43m per 30d".to_owned(),
                error_budget_consumed_limit_basis_points: 8_000,
                readiness: ReliabilityReadinessSemantics::Healthy,
                liveness: ReliabilityLivenessSemantics::RuntimeOperational,
            },
            state: ReliabilityServiceState::Degraded,
            liveness: ReliabilityHealthResult {
                healthy: true,
                semantics: "runtime_operational".to_owned(),
                issue_codes: Vec::new(),
            },
            readiness: ReliabilityHealthResult {
                healthy: true,
                semantics: "serving".to_owned(),
                issue_codes: Vec::new(),
            },
            active_degraded_modes: Vec::new(),
            checks: Vec::new(),
            enforcement: ReliabilityEnforcementBoundary::default(),
        };
        let detail = project_federated_story(FederatedRuntimeStory {
            protocol: FEDERATED_RUNTIME_STORY_PROTOCOL.to_owned(),
            story_id: "story-support".to_owned(),
            tenant_id: Some("tenant_a".to_owned()),
            assembled_at,
            segments: vec![
                workflow_segment("support-ticket", "started", "started", 1, None, None),
                workflow_segment(
                    "support-sla",
                    "compensation",
                    "intervention_required",
                    2,
                    Some("compensation-1"),
                    Some("intervention-1"),
                ),
            ],
            gaps: vec![FederatedStoryGap {
                source_service_id: "support-identity".to_owned(),
                tenant_id: Some("tenant_a".to_owned()),
                kind: FederatedStoryGapKind::Unauthorized,
                detected_at: assembled_at,
                last_observed_at: assembled_at,
                detail: "reader forbidden".to_owned(),
                next_action: "refresh_story_segment_feed_authorization".to_owned(),
            }],
            reliability: vec![FederatedStoryReliabilityEvidence {
                source_service_id: "support-sla".to_owned(),
                observed_at: assembled_at,
                status: FederatedStoryReliabilityStatus::Available,
                report: Some(report),
                detail: None,
                next_action: None,
            }],
        });

        assert_eq!(detail.summary.story_kind, "federated");
        assert_eq!(detail.summary.status, "failed");
        assert_eq!(detail.nodes.len(), 2);
        let federation = detail.federation.unwrap();
        assert_eq!(federation.gaps[0].kind, FederatedStoryGapKind::Unauthorized);
        assert!(federation.workflow_entities.iter().any(|entity| {
            entity.kind == AdminFederatedWorkflowEntityKind::Compensation
                && entity.state == "intervention_required"
        }));
        assert!(federation.workflow_entities.iter().any(|entity| {
            entity.kind == AdminFederatedWorkflowEntityKind::Intervention
                && entity.id == "intervention-1"
        }));
        assert_eq!(
            federation.reliability[0]
                .report
                .as_ref()
                .unwrap()
                .overrides
                .workflow_backlog_limit,
            Some(5)
        );
    }

    #[test]
    fn runtime_story_openapi_exposes_the_federated_console_contract() {
        let document = serde_json::to_value(crate::backend::router().to_openapi()).unwrap();
        let schemas = &document["components"]["schemas"];

        assert_eq!(
            schemas["AdminRuntimeStoryListItem"]["required"],
            serde_json::json!([
                "story_kind",
                "title",
                "correlation_id",
                "status",
                "duration",
                "node_count",
                "error_count",
                "services",
                "pattern",
                "created_at",
                "updated_at"
            ])
        );
        assert_eq!(
            schemas["AdminFederatedWorkflowEntityKind"]["enum"],
            serde_json::json!([
                "instance",
                "step",
                "attempt",
                "timer",
                "child",
                "compensation",
                "intervention"
            ])
        );
        assert_eq!(
            schemas["FederatedStoryGapKind"]["enum"],
            serde_json::json!([
                "unreachable",
                "stale",
                "unauthorized",
                "truncated",
                "retention_expired"
            ])
        );
        assert!(schemas["ReliabilityReport"]["properties"]["overrides"].is_object());
    }
}
