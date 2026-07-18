use crate::{ServiceRuntimeState, postgres_now};
use axum::{
    Extension, Json,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use hmac::{Hmac, KeyInit, Mac};
use lenso_service::{
    AuthenticatedTransportBinding, ServiceTenancyMode, WorkloadIdentityProvider,
    WorkloadIdentityVerification,
};
use platform_core::{AppError, ErrorCode};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::Arc,
    time::Duration,
};
use utoipa::{IntoParams, ToSchema};

pub const STORY_SEGMENT_FEED_PROTOCOL: &str = "lenso.story-segment-feed.v1";
const STORY_SEGMENT_CURSOR_PROTOCOL: &str = "lenso.story-segment-cursor.v1";
const DEFAULT_PAGE_LIMIT: u16 = 100;
const MAX_PAGE_LIMIT: u16 = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorySegmentTenantAccess {
    /// The reader may request any single tenant or the unscoped partition.
    ServiceWide,
    /// The reader may request only the listed tenant partitions.
    Tenants(Vec<String>),
}

impl StorySegmentTenantAccess {
    fn allows(&self, tenant_id: Option<&str>) -> bool {
        match self {
            Self::ServiceWide => true,
            Self::Tenants(tenants) => tenant_id
                .is_some_and(|tenant_id| tenants.iter().any(|allowed| allowed == tenant_id)),
        }
    }

    fn valid(&self) -> bool {
        match self {
            Self::ServiceWide => true,
            Self::Tenants(tenants) => {
                !tenants.is_empty()
                    && tenants.iter().all(|tenant| !tenant.trim().is_empty())
                    && tenants.iter().collect::<BTreeSet<_>>().len() == tenants.len()
            }
        }
    }
}

#[derive(Clone)]
pub struct StorySegmentFeedConfig {
    provider: Arc<dyn WorkloadIdentityProvider>,
    audience: String,
    retention_window: Duration,
    cursor_signing_key: Arc<[u8]>,
    readers: BTreeMap<String, StorySegmentTenantAccess>,
}

impl fmt::Debug for StorySegmentFeedConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StorySegmentFeedConfig")
            .field("provider", &self.provider)
            .field("audience", &self.audience)
            .field("retention_window", &self.retention_window)
            .field("cursor_signing_key", &"[REDACTED]")
            .field("readers", &self.readers)
            .finish()
    }
}

impl PartialEq for StorySegmentFeedConfig {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.provider, &other.provider)
            && self.audience == other.audience
            && self.retention_window == other.retention_window
            && self.cursor_signing_key == other.cursor_signing_key
            && self.readers == other.readers
    }
}

impl Eq for StorySegmentFeedConfig {}

impl StorySegmentFeedConfig {
    #[must_use]
    pub fn new(
        provider: Arc<dyn WorkloadIdentityProvider>,
        audience: impl Into<String>,
        retention_window: Duration,
        cursor_signing_key: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            provider,
            audience: audience.into(),
            retention_window,
            cursor_signing_key: cursor_signing_key.into().into(),
            readers: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn with_reader(
        mut self,
        service_principal: impl Into<String>,
        access: StorySegmentTenantAccess,
    ) -> Self {
        self.readers.insert(service_principal.into(), access);
        self
    }

    pub(crate) fn validation_error(&self) -> Option<&'static str> {
        if self.audience.trim().is_empty() {
            return Some("Story Segment Feed audience must not be empty");
        }
        if self.retention_window.is_zero() {
            return Some("Story Segment Feed retention window must be greater than zero");
        }
        if self.cursor_signing_key.len() < 32 {
            return Some("Story Segment Feed cursor signing key must contain at least 32 bytes");
        }
        if self.readers.is_empty()
            || self
                .readers
                .iter()
                .any(|(principal, access)| principal.trim().is_empty() || !access.valid())
        {
            return Some(
                "Story Segment Feed reader policies must contain valid principals and tenant access",
            );
        }
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StorySegmentSource {
    pub service_id: String,
    pub workload_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StorySegmentOperation {
    pub kind: String,
    pub operation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StorySegmentContract {
    pub contract_id: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StorySegmentWorkflow {
    pub instance_id: String,
    pub definition_owner: String,
    pub definition_name: String,
    pub definition_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_instance_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compensation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intervention_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StorySegment {
    pub story_id: String,
    pub segment_id: String,
    pub evidence_revision: u32,
    pub source: StorySegmentSource,
    pub operation: StorySegmentOperation,
    pub contract: StorySegmentContract,
    pub status: String,
    pub attempt: u32,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub recorded_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_segment_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub causation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow: Option<StorySegmentWorkflow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StorySegmentFeed {
    pub protocol: String,
    pub source_service_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    pub retention_window_seconds: u64,
    pub as_of: DateTime<Utc>,
    pub segments: Vec<StorySegment>,
    pub next_cursor: String,
}

#[derive(Debug, Clone, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub(crate) struct StorySegmentFeedQuery {
    /// Opaque cursor returned by a prior feed response.
    cursor: Option<String>,
    /// Maximum number of entries to return, from 1 through 500.
    #[param(minimum = 1, maximum = 500)]
    limit: Option<u16>,
    /// Required for tenant-aware Services; omitted reads only unscoped evidence.
    tenant_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorySegmentRecord {
    pub story_id: String,
    pub segment_id: String,
    pub evidence_revision: u32,
    pub operation: StorySegmentOperation,
    pub contract: StorySegmentContract,
    pub status: String,
    pub attempt: u32,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub recorded_at: DateTime<Utc>,
    pub tenant_id: Option<String>,
    pub parent_segment_id: Option<String>,
    pub causation_id: Option<String>,
    pub workflow: Option<StorySegmentWorkflow>,
}

impl StorySegmentRecord {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        story_id: impl Into<String>,
        segment_id: impl Into<String>,
        operation_kind: impl Into<String>,
        operation_id: impl Into<String>,
        contract_id: impl Into<String>,
        contract_version: impl Into<String>,
        status: impl Into<String>,
        recorded_at: DateTime<Utc>,
    ) -> Self {
        Self {
            story_id: story_id.into(),
            segment_id: segment_id.into(),
            evidence_revision: 1,
            operation: StorySegmentOperation {
                kind: operation_kind.into(),
                operation_id: operation_id.into(),
            },
            contract: StorySegmentContract {
                contract_id: contract_id.into(),
                version: contract_version.into(),
            },
            status: status.into(),
            attempt: 1,
            started_at: recorded_at,
            completed_at: recorded_at,
            recorded_at,
            tenant_id: None,
            parent_segment_id: None,
            causation_id: None,
            workflow: None,
        }
    }

    #[must_use]
    pub const fn with_revision(mut self, evidence_revision: u32) -> Self {
        self.evidence_revision = evidence_revision;
        self
    }

    #[must_use]
    pub const fn with_attempt(mut self, attempt: u32) -> Self {
        self.attempt = attempt;
        self
    }

    #[must_use]
    pub const fn with_timestamps(
        mut self,
        started_at: DateTime<Utc>,
        completed_at: DateTime<Utc>,
    ) -> Self {
        self.started_at = started_at;
        self.completed_at = completed_at;
        self
    }

    #[must_use]
    pub fn with_tenant(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    #[must_use]
    pub fn with_parent_segment(mut self, parent_segment_id: impl Into<String>) -> Self {
        self.parent_segment_id = Some(parent_segment_id.into());
        self
    }

    #[must_use]
    pub fn with_causation(mut self, causation_id: impl Into<String>) -> Self {
        self.causation_id = Some(causation_id.into());
        self
    }

    #[must_use]
    pub fn with_workflow(mut self, workflow: StorySegmentWorkflow) -> Self {
        self.workflow = Some(workflow);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorySegmentWriteDisposition {
    Appended,
    Duplicate,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StorySegmentCursor {
    protocol: String,
    service_id: String,
    service_principal: String,
    tenant_id: Option<String>,
    after_sequence: i64,
}

#[derive(Debug, FromRow)]
struct StoredStorySegment {
    feed_sequence: i64,
    story_id: String,
    segment_id: String,
    evidence_revision: i32,
    service_id: String,
    workload_id: String,
    operation_kind: String,
    operation: String,
    contract_id: String,
    contract_version: String,
    status: String,
    attempt: i32,
    started_at: DateTime<Utc>,
    completed_at: DateTime<Utc>,
    recorded_at: DateTime<Utc>,
    tenant_id: Option<String>,
    parent_segment_id: Option<String>,
    causation_id: Option<String>,
    workflow_instance_id: Option<String>,
    workflow_definition_owner: Option<String>,
    workflow_definition_name: Option<String>,
    workflow_definition_version: Option<String>,
    workflow_step_id: Option<String>,
    parent_workflow_instance_id: Option<String>,
    compensation_id: Option<String>,
    intervention_id: Option<String>,
}

impl StoredStorySegment {
    fn into_segment(self) -> Result<StorySegment, AppError> {
        let evidence_revision = u32::try_from(self.evidence_revision)
            .map_err(|_| stored_segment_invalid("evidence revision"))?;
        let attempt = u32::try_from(self.attempt).map_err(|_| stored_segment_invalid("attempt"))?;
        let workflow = match self.workflow_instance_id {
            Some(instance_id) => Some(StorySegmentWorkflow {
                instance_id,
                definition_owner: self
                    .workflow_definition_owner
                    .ok_or_else(|| stored_segment_invalid("workflow definition owner"))?,
                definition_name: self
                    .workflow_definition_name
                    .ok_or_else(|| stored_segment_invalid("workflow definition name"))?,
                definition_version: self
                    .workflow_definition_version
                    .ok_or_else(|| stored_segment_invalid("workflow definition version"))?,
                step_id: self.workflow_step_id,
                parent_instance_id: self.parent_workflow_instance_id,
                compensation_id: self.compensation_id,
                intervention_id: self.intervention_id,
            }),
            None => None,
        };
        Ok(StorySegment {
            story_id: self.story_id,
            segment_id: self.segment_id,
            evidence_revision,
            source: StorySegmentSource {
                service_id: self.service_id,
                workload_id: self.workload_id,
            },
            operation: StorySegmentOperation {
                kind: self.operation_kind,
                operation_id: self.operation,
            },
            contract: StorySegmentContract {
                contract_id: self.contract_id,
                version: self.contract_version,
            },
            status: self.status,
            attempt,
            started_at: self.started_at,
            completed_at: self.completed_at,
            recorded_at: self.recorded_at,
            tenant_id: self.tenant_id,
            parent_segment_id: self.parent_segment_id,
            causation_id: self.causation_id,
            workflow,
        })
    }
}

fn stored_segment_invalid(field: &str) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("Stored Story Segment has an invalid {field}"),
    )
}

const LOAD_SEGMENT_REVISION_SQL: &str = r#"
    select feed_sequence, story_id, segment_id, evidence_revision,
           service_id, workload_id, operation_kind, operation,
           contract_id, contract_version, status, attempt,
           started_at, completed_at, recorded_at, tenant_id,
           parent_segment_id, causation_id, workflow_instance_id,
           workflow_definition_owner, workflow_definition_name,
           workflow_definition_version, workflow_step_id,
           parent_workflow_instance_id, compensation_id, intervention_id
    from platform.service_story_segments
    where service_id = $1 and segment_id = $2 and evidence_revision = $3
"#;

const READ_STORY_SEGMENT_FEED_SQL: &str = r#"
    select feed_sequence, story_id, segment_id, evidence_revision,
           service_id, workload_id, operation_kind, operation,
           contract_id, contract_version, status, attempt,
           started_at, completed_at, recorded_at, tenant_id,
           parent_segment_id, causation_id, workflow_instance_id,
           workflow_definition_owner, workflow_definition_name,
           workflow_definition_version, workflow_step_id,
           parent_workflow_instance_id, compensation_id, intervention_id
    from platform.service_story_segments
    where service_id = $1
      and feed_sequence > $2
      and (($3::text is null and tenant_id is null) or tenant_id = $3)
      and recorded_at >= $4
    order by feed_sequence
    limit $5
"#;

pub async fn append_story_segment(
    state: &ServiceRuntimeState,
    record: &StorySegmentRecord,
) -> Result<StorySegmentWriteDisposition, AppError> {
    let pool = state.store()?;
    let mut transaction = pool.begin().await.map_err(story_store_error)?;
    let disposition = append_story_segment_in_tx(state, &mut transaction, record).await?;
    transaction.commit().await.map_err(story_store_error)?;
    Ok(disposition)
}

pub async fn append_story_segment_in_tx(
    state: &ServiceRuntimeState,
    transaction: &mut Transaction<'_, Postgres>,
    record: &StorySegmentRecord,
) -> Result<StorySegmentWriteDisposition, AppError> {
    validate_record(record)?;
    let workflow = record.workflow.as_ref();
    let inserted_sequence = sqlx::query_scalar::<_, i64>(
        r#"
        insert into platform.service_story_segments (
            story_id, segment_id, evidence_revision, service_id, workload_id,
            operation_kind, operation, contract_id, contract_version,
            status, attempt, started_at, completed_at, recorded_at, tenant_id,
            parent_segment_id, causation_id, workflow_instance_id,
            workflow_definition_owner, workflow_definition_name,
            workflow_definition_version, workflow_step_id,
            parent_workflow_instance_id, compensation_id, intervention_id
        ) values (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
            $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25
        )
        on conflict (service_id, segment_id, evidence_revision) do nothing
        returning feed_sequence
        "#,
    )
    .bind(&record.story_id)
    .bind(&record.segment_id)
    .bind(i32::try_from(record.evidence_revision).unwrap_or(i32::MAX))
    .bind(&state.identity.service_id)
    .bind(&state.identity.api_workload_id)
    .bind(&record.operation.kind)
    .bind(&record.operation.operation_id)
    .bind(&record.contract.contract_id)
    .bind(&record.contract.version)
    .bind(&record.status)
    .bind(i32::try_from(record.attempt).unwrap_or(i32::MAX))
    .bind(record.started_at)
    .bind(record.completed_at)
    .bind(record.recorded_at)
    .bind(&record.tenant_id)
    .bind(&record.parent_segment_id)
    .bind(&record.causation_id)
    .bind(workflow.map(|workflow| &workflow.instance_id))
    .bind(workflow.map(|workflow| &workflow.definition_owner))
    .bind(workflow.map(|workflow| &workflow.definition_name))
    .bind(workflow.map(|workflow| &workflow.definition_version))
    .bind(workflow.and_then(|workflow| workflow.step_id.as_ref()))
    .bind(workflow.and_then(|workflow| workflow.parent_instance_id.as_ref()))
    .bind(workflow.and_then(|workflow| workflow.compensation_id.as_ref()))
    .bind(workflow.and_then(|workflow| workflow.intervention_id.as_ref()))
    .fetch_optional(&mut **transaction)
    .await
    .map_err(story_store_error)?;
    if inserted_sequence.is_some() {
        return Ok(StorySegmentWriteDisposition::Appended);
    }

    let existing = load_segment_revision(
        &mut **transaction,
        &state.identity.service_id,
        &record.segment_id,
        record.evidence_revision,
    )
    .await?
    .into_segment()?;
    let expected = segment_from_record(state, record, &state.identity.api_workload_id);
    if existing == expected {
        Ok(StorySegmentWriteDisposition::Duplicate)
    } else {
        Err(AppError::new(
            ErrorCode::Conflict,
            "Story Segment identity and evidence revision already exist with different evidence",
        ))
    }
}

pub(crate) async fn append_worker_story_segment_in_tx(
    state: &ServiceRuntimeState,
    transaction: &mut Transaction<'_, Postgres>,
    record: &StorySegmentRecord,
) -> Result<StorySegmentWriteDisposition, AppError> {
    validate_record(record)?;
    append_with_workload_in_tx(
        state,
        transaction,
        record,
        &state.identity.worker_workload_id,
    )
    .await
}

async fn append_with_workload_in_tx(
    state: &ServiceRuntimeState,
    transaction: &mut Transaction<'_, Postgres>,
    record: &StorySegmentRecord,
    workload_id: &str,
) -> Result<StorySegmentWriteDisposition, AppError> {
    let workflow = record.workflow.as_ref();
    let inserted_sequence = sqlx::query_scalar::<_, i64>(
        r#"
        insert into platform.service_story_segments (
            story_id, segment_id, evidence_revision, service_id, workload_id,
            operation_kind, operation, contract_id, contract_version,
            status, attempt, started_at, completed_at, recorded_at, tenant_id,
            parent_segment_id, causation_id, workflow_instance_id,
            workflow_definition_owner, workflow_definition_name,
            workflow_definition_version, workflow_step_id,
            parent_workflow_instance_id, compensation_id, intervention_id
        ) values (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
            $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25
        )
        on conflict (service_id, segment_id, evidence_revision) do nothing
        returning feed_sequence
        "#,
    )
    .bind(&record.story_id)
    .bind(&record.segment_id)
    .bind(i32::try_from(record.evidence_revision).unwrap_or(i32::MAX))
    .bind(&state.identity.service_id)
    .bind(workload_id)
    .bind(&record.operation.kind)
    .bind(&record.operation.operation_id)
    .bind(&record.contract.contract_id)
    .bind(&record.contract.version)
    .bind(&record.status)
    .bind(i32::try_from(record.attempt).unwrap_or(i32::MAX))
    .bind(record.started_at)
    .bind(record.completed_at)
    .bind(record.recorded_at)
    .bind(&record.tenant_id)
    .bind(&record.parent_segment_id)
    .bind(&record.causation_id)
    .bind(workflow.map(|workflow| &workflow.instance_id))
    .bind(workflow.map(|workflow| &workflow.definition_owner))
    .bind(workflow.map(|workflow| &workflow.definition_name))
    .bind(workflow.map(|workflow| &workflow.definition_version))
    .bind(workflow.and_then(|workflow| workflow.step_id.as_ref()))
    .bind(workflow.and_then(|workflow| workflow.parent_instance_id.as_ref()))
    .bind(workflow.and_then(|workflow| workflow.compensation_id.as_ref()))
    .bind(workflow.and_then(|workflow| workflow.intervention_id.as_ref()))
    .fetch_optional(&mut **transaction)
    .await
    .map_err(story_store_error)?;
    if inserted_sequence.is_some() {
        return Ok(StorySegmentWriteDisposition::Appended);
    }
    let existing = load_segment_revision(
        &mut **transaction,
        &state.identity.service_id,
        &record.segment_id,
        record.evidence_revision,
    )
    .await?
    .into_segment()?;
    if existing == segment_from_record(state, record, workload_id) {
        Ok(StorySegmentWriteDisposition::Duplicate)
    } else {
        Err(AppError::new(
            ErrorCode::Conflict,
            "Story Segment identity and evidence revision already exist with different evidence",
        ))
    }
}

fn segment_from_record(
    state: &ServiceRuntimeState,
    record: &StorySegmentRecord,
    workload_id: &str,
) -> StorySegment {
    StorySegment {
        story_id: record.story_id.clone(),
        segment_id: record.segment_id.clone(),
        evidence_revision: record.evidence_revision,
        source: StorySegmentSource {
            service_id: state.identity.service_id.clone(),
            workload_id: workload_id.to_owned(),
        },
        operation: record.operation.clone(),
        contract: record.contract.clone(),
        status: record.status.clone(),
        attempt: record.attempt,
        started_at: postgres_timestamp_precision(record.started_at),
        completed_at: postgres_timestamp_precision(record.completed_at),
        recorded_at: postgres_timestamp_precision(record.recorded_at),
        tenant_id: record.tenant_id.clone(),
        parent_segment_id: record.parent_segment_id.clone(),
        causation_id: record.causation_id.clone(),
        workflow: record.workflow.clone(),
    }
}

fn postgres_timestamp_precision(value: DateTime<Utc>) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp_micros(value.timestamp_micros())
        .expect("Story Segment timestamp must fit PostgreSQL precision")
}

fn validate_record(record: &StorySegmentRecord) -> Result<(), AppError> {
    let required = [
        record.story_id.as_str(),
        record.segment_id.as_str(),
        record.operation.kind.as_str(),
        record.operation.operation_id.as_str(),
        record.contract.contract_id.as_str(),
        record.contract.version.as_str(),
        record.status.as_str(),
    ];
    if required.iter().any(|value| value.trim().is_empty())
        || record.evidence_revision == 0
        || record.attempt == 0
        || record.completed_at < record.started_at
        || record.recorded_at < record.started_at
    {
        return Err(AppError::new(
            ErrorCode::Validation,
            "Story Segment identity, revision, attempt, status, contract, and timestamps must be valid",
        ));
    }
    if let Some(workflow) = &record.workflow
        && [
            workflow.instance_id.as_str(),
            workflow.definition_owner.as_str(),
            workflow.definition_name.as_str(),
            workflow.definition_version.as_str(),
        ]
        .iter()
        .any(|value| value.trim().is_empty())
    {
        return Err(AppError::new(
            ErrorCode::Validation,
            "Workflow-related Story Segments require stable instance and definition identity",
        ));
    }
    Ok(())
}

async fn load_segment_revision<'e, E>(
    executor: E,
    service_id: &str,
    segment_id: &str,
    revision: u32,
) -> Result<StoredStorySegment, AppError>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, StoredStorySegment>(LOAD_SEGMENT_REVISION_SQL)
        .bind(service_id)
        .bind(segment_id)
        .bind(i32::try_from(revision).unwrap_or(i32::MAX))
        .fetch_one(executor)
        .await
        .map_err(story_store_error)
}

#[derive(Debug, FromRow)]
struct WorkflowFeedContext {
    story_id: String,
    parent_segment_id: String,
    tenant_id: Option<String>,
    definition_owner: String,
    definition_name: String,
    definition_version: String,
    parent_instance_id: Option<String>,
    instance_causation_id: Option<String>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn append_persisted_workflow_story_segment_in_tx(
    state: &ServiceRuntimeState,
    transaction: &mut Transaction<'_, Postgres>,
    instance_id: &str,
    step_id: Option<&str>,
    compensation_id: Option<&str>,
    intervention_id: Option<&str>,
    segment_id: &str,
    operation_id: &str,
    contract_id: &str,
    contract_version: &str,
    status: &str,
    attempt: u32,
    causation_id: Option<&str>,
    recorded_at: DateTime<Utc>,
) -> Result<StorySegmentWriteDisposition, AppError> {
    let context = sqlx::query_as::<_, WorkflowFeedContext>(
        r#"
        select instance.story_context->>'storyId' as story_id,
               instance.story_context->>'segmentId' as parent_segment_id,
               instance.tenant_scope->>'tenantId' as tenant_id,
               instance.definition_owner, instance.definition_name,
               instance.definition_version, instance.parent_instance_id,
               instance.causation_id as instance_causation_id
        from platform.service_workflow_instances instance
        where instance.service_id = $1 and instance.instance_id = $2
        "#,
    )
    .bind(&state.identity.service_id)
    .bind(instance_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(story_store_error)?;
    let mut record = StorySegmentRecord::new(
        context.story_id,
        segment_id,
        "durable_workflow",
        operation_id,
        contract_id,
        contract_version,
        status,
        recorded_at,
    )
    .with_attempt(attempt)
    .with_revision(attempt)
    .with_parent_segment(context.parent_segment_id)
    .with_workflow(StorySegmentWorkflow {
        instance_id: instance_id.to_owned(),
        definition_owner: context.definition_owner,
        definition_name: context.definition_name,
        definition_version: context.definition_version,
        step_id: step_id.map(str::to_owned),
        parent_instance_id: context.parent_instance_id,
        compensation_id: compensation_id.map(str::to_owned),
        intervention_id: intervention_id.map(str::to_owned),
    });
    if let Some(tenant_id) = context.tenant_id {
        record = record.with_tenant(tenant_id);
    }
    if let Some(causation_id) = causation_id.or(context.instance_causation_id.as_deref()) {
        record = record.with_causation(causation_id);
    }
    append_worker_story_segment_in_tx(state, transaction, &record).await
}

#[derive(Debug)]
pub(crate) struct StorySegmentFeedError {
    status: StatusCode,
    code: &'static str,
    message: String,
    next_action: &'static str,
}

impl StorySegmentFeedError {
    fn new(
        status: StatusCode,
        code: &'static str,
        message: impl Into<String>,
        next_action: &'static str,
    ) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            next_action,
        }
    }
}

impl IntoResponse for StorySegmentFeedError {
    fn into_response(self) -> Response {
        let body = platform_http::ProblemDetails {
            problem_type: format!("https://lenso.dev/problems/{}", self.code),
            title: "Story Segment Feed request failed".to_owned(),
            status: self.status.as_u16(),
            detail: self.message,
            code: self.code.to_owned(),
            request_id: None,
            correlation_id: None,
            errors: Vec::new(),
            next_actions: Some(vec![self.next_action.to_owned()]),
        };
        let mut response = (self.status, Json(body)).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        response
            .headers_mut()
            .insert("x-lenso-error-code", HeaderValue::from_static(self.code));
        response
    }
}

#[utoipa::path(
    get,
    path = "/runtime/story-segments",
    params(StorySegmentFeedQuery),
    responses(
        (status = 200, body = StorySegmentFeed),
        (status = 400, body = platform_http::ErrorResponse, content_type = "application/problem+json"),
        (status = 401, body = platform_http::ErrorResponse, content_type = "application/problem+json"),
        (status = 403, body = platform_http::ErrorResponse, content_type = "application/problem+json"),
        (status = 410, body = platform_http::ErrorResponse, content_type = "application/problem+json"),
        (status = 503, body = platform_http::ErrorResponse, content_type = "application/problem+json"),
        (status = 500, body = platform_http::ErrorResponse, content_type = "application/problem+json")
    ),
    security(("bearer_auth" = [])),
    tag = "service-runtime"
)]
pub(crate) async fn story_segments(
    State(state): State<ServiceRuntimeState>,
    Query(query): Query<StorySegmentFeedQuery>,
    headers: HeaderMap,
    transport_binding: Option<Extension<AuthenticatedTransportBinding>>,
) -> Result<Json<StorySegmentFeed>, StorySegmentFeedError> {
    let config = state.story_segment_feed.as_ref().ok_or_else(|| {
        StorySegmentFeedError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "story_segment_feed_unavailable",
            "Story Segment Feed access is not configured for this Service",
            "configure_story_segment_feed_access",
        )
    })?;
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            StorySegmentFeedError::new(
                StatusCode::UNAUTHORIZED,
                "story_segment_workload_identity_required",
                "Story Segment Feed requires a Workload Identity Bearer credential",
                "provide_workload_identity",
            )
        })?;
    let binding = transport_binding.ok_or_else(|| {
        StorySegmentFeedError::new(
            StatusCode::UNAUTHORIZED,
            "story_segment_transport_binding_required",
            "Story Segment Feed requires an authenticated transport binding",
            "use_authenticated_transport",
        )
    })?;
    let principal = config
        .provider
        .verify(
            token,
            &WorkloadIdentityVerification::new(&config.audience, &binding.0.0, now_unix_ms()),
        )
        .map_err(|error| {
            StorySegmentFeedError::new(
                StatusCode::UNAUTHORIZED,
                "story_segment_workload_identity_rejected",
                error.message,
                "refresh_workload_identity",
            )
        })?;
    let tenant_id = query
        .tenant_id
        .as_deref()
        .filter(|tenant_id| !tenant_id.trim().is_empty());
    validate_tenant_partition(&state.identity.tenancy_mode, tenant_id)?;
    let access = config
        .readers
        .get(&principal.service_principal)
        .ok_or_else(|| {
            StorySegmentFeedError::new(
                StatusCode::FORBIDDEN,
                "story_segment_reader_forbidden",
                "Authenticated Service Principal is not authorized to read this feed",
                "request_story_segment_feed_access",
            )
        })?;
    if !access.allows(tenant_id) {
        return Err(StorySegmentFeedError::new(
            StatusCode::FORBIDDEN,
            "story_segment_tenant_forbidden",
            "Authenticated Service Principal is not authorized for the requested tenant",
            "request_tenant_feed_access",
        ));
    }
    let limit = query.limit.unwrap_or(DEFAULT_PAGE_LIMIT);
    if limit == 0 || limit > MAX_PAGE_LIMIT {
        return Err(StorySegmentFeedError::new(
            StatusCode::BAD_REQUEST,
            "story_segment_limit_invalid",
            "Story Segment Feed limit must be between 1 and 500",
            "use_valid_story_segment_limit",
        ));
    }
    let cursor = query
        .cursor
        .as_deref()
        .map(|cursor| decode_cursor(config, cursor))
        .transpose()?;
    if let Some(cursor) = &cursor
        && (cursor.service_id != state.identity.service_id
            || cursor.service_principal != principal.service_principal
            || cursor.tenant_id.as_deref() != tenant_id)
    {
        return Err(StorySegmentFeedError::new(
            StatusCode::BAD_REQUEST,
            "story_segment_cursor_scope_mismatch",
            "Story Segment cursor does not belong to this Service, reader, and tenant scope",
            "restart_story_segment_feed_read",
        ));
    }
    let after_sequence = cursor.as_ref().map_or(0, |cursor| cursor.after_sequence);
    let as_of = postgres_now();
    let retention_seconds = config.retention_window.as_secs();
    let retention_seconds_i64 = i64::try_from(retention_seconds).map_err(|_| {
        StorySegmentFeedError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "story_segment_retention_invalid",
            "Story Segment Feed retention window is not supported",
            "configure_story_segment_feed_retention",
        )
    })?;
    let cutoff = as_of - ChronoDuration::seconds(retention_seconds_i64);
    let pool = state.store().map_err(|error| {
        StorySegmentFeedError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "story_segment_store_unavailable",
            error.public_message,
            "retry_story_segment_feed",
        )
    })?;
    if cursor.is_some()
        && cursor_has_expired(
            pool,
            &state.identity.service_id,
            tenant_id,
            after_sequence,
            cutoff,
        )
        .await?
    {
        return Err(StorySegmentFeedError::new(
            StatusCode::GONE,
            "story_segment_cursor_expired",
            "Story Segment cursor points outside the declared retention window",
            "restart_story_segment_feed_read",
        ));
    }
    let rows = sqlx::query_as::<_, StoredStorySegment>(READ_STORY_SEGMENT_FEED_SQL)
        .bind(&state.identity.service_id)
        .bind(after_sequence)
        .bind(tenant_id)
        .bind(cutoff)
        .bind(i64::from(limit))
        .fetch_all(pool)
        .await
        .map_err(feed_store_error)?;
    let next_sequence = rows
        .last()
        .map_or(after_sequence, |segment| segment.feed_sequence);
    let segments = rows
        .into_iter()
        .map(StoredStorySegment::into_segment)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            StorySegmentFeedError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "story_segment_stored_evidence_invalid",
                error.public_message,
                "repair_story_segment_store",
            )
        })?;
    let next_cursor = encode_cursor(
        config,
        &StorySegmentCursor {
            protocol: STORY_SEGMENT_CURSOR_PROTOCOL.to_owned(),
            service_id: state.identity.service_id.clone(),
            service_principal: principal.service_principal,
            tenant_id: tenant_id.map(str::to_owned),
            after_sequence: next_sequence,
        },
    )?;
    Ok(Json(StorySegmentFeed {
        protocol: STORY_SEGMENT_FEED_PROTOCOL.to_owned(),
        source_service_id: state.identity.service_id.clone(),
        tenant_id: tenant_id.map(str::to_owned),
        retention_window_seconds: retention_seconds,
        as_of,
        segments,
        next_cursor,
    }))
}

fn validate_tenant_partition(
    tenancy_mode: &ServiceTenancyMode,
    tenant_id: Option<&str>,
) -> Result<(), StorySegmentFeedError> {
    match (tenancy_mode, tenant_id) {
        (ServiceTenancyMode::Required, None) => Err(StorySegmentFeedError::new(
            StatusCode::FORBIDDEN,
            "story_segment_tenant_required",
            "This Service requires an explicit tenant partition for Story Segment reads",
            "provide_story_segment_tenant",
        )),
        (ServiceTenancyMode::None, Some(_)) => Err(StorySegmentFeedError::new(
            StatusCode::BAD_REQUEST,
            "story_segment_tenant_incompatible",
            "This Service does not contain tenant-scoped Story Segments",
            "remove_story_segment_tenant",
        )),
        _ => Ok(()),
    }
}

async fn cursor_has_expired(
    pool: &PgPool,
    service_id: &str,
    tenant_id: Option<&str>,
    after_sequence: i64,
    cutoff: DateTime<Utc>,
) -> Result<bool, StorySegmentFeedError> {
    sqlx::query_scalar(
        r#"
        select exists (
            select 1
            from platform.service_story_segments
            where service_id = $1
              and feed_sequence <= $2
              and (($3::text is null and tenant_id is null) or tenant_id = $3)
              and recorded_at < $4
        )
        "#,
    )
    .bind(service_id)
    .bind(after_sequence)
    .bind(tenant_id)
    .bind(cutoff)
    .fetch_one(pool)
    .await
    .map_err(feed_store_error)
}

fn encode_cursor(
    config: &StorySegmentFeedConfig,
    cursor: &StorySegmentCursor,
) -> Result<String, StorySegmentFeedError> {
    let payload = serde_json::to_vec(cursor).map_err(|_| cursor_invalid())?;
    let mut mac =
        Hmac::<Sha256>::new_from_slice(&config.cursor_signing_key).map_err(|_| cursor_invalid())?;
    mac.update(&payload);
    let signature = mac.finalize().into_bytes();
    Ok(format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(payload),
        URL_SAFE_NO_PAD.encode(signature)
    ))
}

fn decode_cursor(
    config: &StorySegmentFeedConfig,
    encoded: &str,
) -> Result<StorySegmentCursor, StorySegmentFeedError> {
    let (payload, signature) = encoded.split_once('.').ok_or_else(cursor_invalid)?;
    let payload = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| cursor_invalid())?;
    let signature = URL_SAFE_NO_PAD
        .decode(signature)
        .map_err(|_| cursor_invalid())?;
    let mut mac =
        Hmac::<Sha256>::new_from_slice(&config.cursor_signing_key).map_err(|_| cursor_invalid())?;
    mac.update(&payload);
    mac.verify_slice(&signature).map_err(|_| cursor_invalid())?;
    let cursor: StorySegmentCursor =
        serde_json::from_slice(&payload).map_err(|_| cursor_invalid())?;
    if cursor.protocol != STORY_SEGMENT_CURSOR_PROTOCOL || cursor.after_sequence < 0 {
        return Err(cursor_invalid());
    }
    Ok(cursor)
}

fn cursor_invalid() -> StorySegmentFeedError {
    StorySegmentFeedError::new(
        StatusCode::BAD_REQUEST,
        "story_segment_cursor_invalid",
        "Story Segment cursor is invalid or has been altered",
        "restart_story_segment_feed_read",
    )
}

fn feed_store_error(error: sqlx::Error) -> StorySegmentFeedError {
    tracing::error!(error = %error, "failed to read Story Segment Feed");
    StorySegmentFeedError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "story_segment_store_failure",
        "Could not read Story Segment Feed",
        "retry_story_segment_feed",
    )
}

fn story_store_error(error: sqlx::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        "Could not persist Story Segment evidence",
    )
    .with_source(error)
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}
