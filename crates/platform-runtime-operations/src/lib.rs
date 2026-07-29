//! Service-owned Runtime Operations Capability Provider.

use async_trait::async_trait;
use axum::{
    Extension, Json,
    extract::{Path, Query},
    http::StatusCode,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use lenso_service::system_plane::{
    ManagementIntent, RUNTIME_OPERATIONS_FEATURE_EVIDENCE,
    RUNTIME_OPERATIONS_FEATURE_FUNCTION_RETRY, RUNTIME_OPERATIONS_FEATURE_OUTBOX_RETRY,
    RUNTIME_OPERATIONS_PATH, RUNTIME_OPERATIONS_PROTOCOL, RuntimeOperationAcknowledgement,
    RuntimeOperationAvailabilityImpact, RuntimeOperationCompensationSupport,
    RuntimeOperationDesiredOutcome, RuntimeOperationEvidence, RuntimeOperationEvidencePage,
    RuntimeOperationPlanReceipt, RuntimeOperationRecovery, RuntimeOperationRisk,
    RuntimeOperationState, RuntimeOperationSubmission, RuntimeOperationTarget,
    RuntimeOperationTargetKind, RuntimeOperationTargetSnapshot, RuntimeOperationTargetStatus,
    management_intent_digest, runtime_operation_plan_digest, runtime_operations_schema_digest,
};
use platform_core::Migration;
use platform_system_plane::{
    AuthorizedSystemPlaneCaller, SystemPlaneErrorBody, SystemPlaneRejection,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use std::{collections::BTreeSet, sync::Arc};
use utoipa_axum::{router::OpenApiRouter, routes};

pub const RUNTIME_OPERATIONS_MIGRATIONS: &[Migration] = &[Migration {
    name: "runtime-operations/0001_create_runtime_operations",
    sql: include_str!("../migrations/0001_create_runtime_operations.sql"),
}];
const RUNTIME_OPERATION_EVIDENCE_CURSOR_PROTOCOL: &str =
    "lenso.system-plane.runtime-operation-evidence-cursor.v1";

#[derive(Debug, Clone)]
pub struct RuntimeOperationsProvider {
    pool: PgPool,
    service_id: String,
    service_revision: String,
    authority_verifier: Option<Arc<dyn ManagementAuthorityVerifier>>,
}

impl RuntimeOperationsProvider {
    #[must_use]
    pub fn new(
        pool: PgPool,
        service_id: impl Into<String>,
        service_revision: impl Into<String>,
    ) -> Self {
        Self {
            pool,
            service_id: service_id.into(),
            service_revision: service_revision.into(),
            authority_verifier: None,
        }
    }

    #[must_use]
    pub fn with_authority_verifier(
        mut self,
        verifier: Arc<dyn ManagementAuthorityVerifier>,
    ) -> Self {
        self.authority_verifier = Some(verifier);
        self
    }

    #[must_use]
    pub fn advertisement() -> lenso_service::system_plane::CapabilityAdvertisement {
        lenso_service::system_plane::CapabilityAdvertisement {
            contract_id: RUNTIME_OPERATIONS_PROTOCOL.to_owned(),
            major_version: 1,
            feature_ids: BTreeSet::from([
                RUNTIME_OPERATIONS_FEATURE_EVIDENCE.to_owned(),
                RUNTIME_OPERATIONS_FEATURE_FUNCTION_RETRY.to_owned(),
                RUNTIME_OPERATIONS_FEATURE_OUTBOX_RETRY.to_owned(),
            ]),
            schema_digest: runtime_operations_schema_digest(),
            endpoint: RUNTIME_OPERATIONS_PATH.to_owned(),
        }
    }

    #[must_use]
    pub fn service_id(&self) -> &str {
        &self.service_id
    }

    #[must_use]
    pub fn service_revision(&self) -> &str {
        &self.service_revision
    }

    #[must_use]
    pub fn validation_error(&self) -> Option<&'static str> {
        if self.service_id.trim().is_empty() {
            return Some("Runtime Operations provider Service identity must not be empty");
        }
        if self.service_revision.trim().is_empty() {
            return Some("Runtime Operations provider Service revision must not be empty");
        }
        None
    }

    pub async fn target_snapshot(
        &self,
        target: &RuntimeOperationTarget,
        now_unix_ms: u64,
    ) -> Result<RuntimeOperationTargetSnapshot, RuntimeOperationsError> {
        let row = fetch_target(&self.pool, target).await?;
        snapshot(&self.service_id, &self.service_revision, row, now_unix_ms)
    }

    pub async fn plan(
        &self,
        intent: &ManagementIntent,
        now_unix_ms: u64,
    ) -> Result<RuntimeOperationPlanReceipt, RuntimeOperationsError> {
        validate_intent(
            intent,
            &self.service_id,
            &self.service_revision,
            now_unix_ms,
        )?;
        let target = self.target_snapshot(&intent.target, now_unix_ms).await?;
        if target.target_revision != intent.expected_target_revision {
            return Err(operation_error(
                RuntimeOperationsErrorCode::StaleTargetRevision,
                "Management Intent target revision is no longer current",
            ));
        }
        ensure_retryable(target.status)?;
        let mut receipt = RuntimeOperationPlanReceipt {
            protocol: RUNTIME_OPERATIONS_PROTOCOL.to_owned(),
            intent_digest: management_intent_digest(intent),
            plan_digest: String::new(),
            service_id: self.service_id.clone(),
            service_revision: self.service_revision.clone(),
            target: intent.target.clone(),
            expected_target_revision: intent.expected_target_revision.clone(),
            expected_effects: expected_effects(intent.target.kind),
            risks: operation_risks(intent.target.kind),
            availability_impact: RuntimeOperationAvailabilityImpact::None,
            compensation_support: RuntimeOperationCompensationSupport::NotAvailable,
            approval_required: true,
            expires_at_unix_ms: intent
                .deadline_unix_ms
                .min(now_unix_ms.saturating_add(300_000)),
        };
        receipt.plan_digest = runtime_operation_plan_digest(&receipt);
        Ok(receipt)
    }

    pub async fn submit(
        &self,
        submission: &RuntimeOperationSubmission,
        caller: &AuthorizedSystemPlaneCaller,
        now_unix_ms: u64,
    ) -> Result<RuntimeOperationAcknowledgement, RuntimeOperationsError> {
        if let Some(acknowledgement) = self.existing_acknowledgement(submission).await? {
            return Ok(acknowledgement);
        }
        validate_intent(
            &submission.intent,
            &self.service_id,
            &self.service_revision,
            now_unix_ms,
        )?;
        validate_submitted_plan(
            submission,
            &self.service_id,
            &self.service_revision,
            now_unix_ms,
        )?;
        let verifier = self.authority_verifier.as_ref().ok_or_else(|| {
            operation_error(
                RuntimeOperationsErrorCode::AuthorityUnavailable,
                "Runtime Operations mutation authority is not configured",
            )
        })?;
        let authority = verifier
            .verify(ManagementAuthorityRequest {
                intent: submission.intent.clone(),
                console_service_principal: caller.service_principal.clone(),
                authorization_epoch: caller.enrollment.authorization_epoch,
                enrollment_receipt_digest: caller.enrollment.receipt_digest.clone(),
                now_unix_ms,
            })
            .await?;
        self.persist_and_retry(submission, caller, authority, now_unix_ms)
            .await
    }

    async fn existing_acknowledgement(
        &self,
        submission: &RuntimeOperationSubmission,
    ) -> Result<Option<RuntimeOperationAcknowledgement>, RuntimeOperationsError> {
        let request_digest = digest_json(submission);
        let existing = sqlx::query_as::<_, (String, serde_json::Value)>(
            r#"
            select request_digest, acknowledgement
            from platform.system_plane_runtime_operations
            where service_id = $1 and idempotency_key = $2
            "#,
        )
        .bind(&self.service_id)
        .bind(&submission.intent.idempotency_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(store_error)?;
        let Some((stored_request_digest, acknowledgement)) = existing else {
            return Ok(None);
        };
        if stored_request_digest != request_digest {
            return Err(operation_error(
                RuntimeOperationsErrorCode::IdempotencyConflict,
                "Idempotency key is already bound to a different Runtime Operation request",
            ));
        }
        serde_json::from_value(acknowledgement)
            .map(Some)
            .map_err(serialization_error)
    }

    pub async fn evidence(
        &self,
        operation_id: &str,
    ) -> Result<RuntimeOperationEvidence, RuntimeOperationsError> {
        let evidence = sqlx::query_scalar::<_, serde_json::Value>(
            r#"
            select evidence.evidence
            from platform.system_plane_runtime_operation_evidence evidence
            join platform.system_plane_runtime_operations operation_record
              on operation_record.operation_id = evidence.operation_id
            where evidence.operation_id = $1 and operation_record.service_id = $2
            order by evidence.sequence desc
            limit 1
            "#,
        )
        .bind(operation_id)
        .bind(&self.service_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(store_error)?
        .ok_or_else(|| {
            operation_error(
                RuntimeOperationsErrorCode::NotFound,
                "Runtime Operation evidence was not found",
            )
        })?;
        serde_json::from_value(evidence).map_err(serialization_error)
    }

    pub async fn evidence_page(
        &self,
        operation_id: &str,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<RuntimeOperationEvidencePage, RuntimeOperationsError> {
        if operation_id.trim().is_empty() || !(1..=100).contains(&limit) {
            return Err(operation_error(
                RuntimeOperationsErrorCode::InvalidEvidenceQuery,
                "Operation Evidence queries require an operation identity and a limit from 1 to 100",
            ));
        }
        self.ensure_operation_exists(operation_id).await?;
        let after_sequence = if let Some(cursor) = cursor {
            let cursor = decode_evidence_cursor(cursor)?;
            if cursor.operation_id != operation_id {
                return Err(operation_error(
                    RuntimeOperationsErrorCode::InvalidEvidenceCursor,
                    "Operation Evidence cursor belongs to a different operation",
                ));
            }
            cursor.after_sequence
        } else {
            0
        };
        let fetch_limit = i64::from(limit) + 1;
        let mut rows = sqlx::query_as::<_, (i64, serde_json::Value)>(
            r#"
            select evidence.sequence, evidence.evidence
            from platform.system_plane_runtime_operation_evidence evidence
            join platform.system_plane_runtime_operations operation_record
              on operation_record.operation_id = evidence.operation_id
            where evidence.operation_id = $1 and operation_record.service_id = $2
              and evidence.sequence > $3
            order by evidence.sequence asc
            limit $4
            "#,
        )
        .bind(operation_id)
        .bind(&self.service_id)
        .bind(to_i64(after_sequence)?)
        .bind(fetch_limit)
        .fetch_all(&self.pool)
        .await
        .map_err(store_error)?;
        if rows.is_empty() && after_sequence == 0 {
            return Err(operation_error(
                RuntimeOperationsErrorCode::NotFound,
                "Runtime Operation evidence was not found",
            ));
        }
        let has_more = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next_cursor = if has_more {
            rows.last()
                .map(|(sequence, _)| {
                    encode_evidence_cursor(&RuntimeOperationEvidenceCursor {
                        protocol: RUNTIME_OPERATION_EVIDENCE_CURSOR_PROTOCOL.to_owned(),
                        operation_id: operation_id.to_owned(),
                        after_sequence: u64::try_from(*sequence).map_err(|_| {
                            operation_error(
                                RuntimeOperationsErrorCode::StoreUnavailable,
                                "Operation Evidence sequence is invalid in the Service Store",
                            )
                        })?,
                    })
                })
                .transpose()?
        } else {
            None
        };
        let items = rows
            .into_iter()
            .map(|(_, evidence)| serde_json::from_value(evidence).map_err(serialization_error))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(RuntimeOperationEvidencePage {
            protocol: RUNTIME_OPERATIONS_PROTOCOL.to_owned(),
            operation_id: operation_id.to_owned(),
            items,
            next_cursor,
        })
    }

    async fn ensure_operation_exists(
        &self,
        operation_id: &str,
    ) -> Result<(), RuntimeOperationsError> {
        let exists = sqlx::query_scalar::<_, bool>(
            r#"
            select exists (
                select 1
                from platform.system_plane_runtime_operations
                where operation_id = $1 and service_id = $2
            )
            "#,
        )
        .bind(operation_id)
        .bind(&self.service_id)
        .fetch_one(&self.pool)
        .await
        .map_err(store_error)?;
        if exists {
            Ok(())
        } else {
            Err(operation_error(
                RuntimeOperationsErrorCode::NotFound,
                "Runtime Operation was not found",
            ))
        }
    }

    pub async fn recover_by_idempotency_key(
        &self,
        idempotency_key: &str,
    ) -> Result<RuntimeOperationRecovery, RuntimeOperationsError> {
        if idempotency_key.trim().is_empty() {
            return Err(operation_error(
                RuntimeOperationsErrorCode::InvalidEvidenceQuery,
                "Runtime Operation recovery requires an idempotency key",
            ));
        }
        let acknowledgement = sqlx::query_scalar::<_, serde_json::Value>(
            r#"
            select acknowledgement
            from platform.system_plane_runtime_operations
            where service_id = $1 and idempotency_key = $2
            "#,
        )
        .bind(&self.service_id)
        .bind(idempotency_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(store_error)?
        .ok_or_else(|| {
            operation_error(
                RuntimeOperationsErrorCode::NotFound,
                "Runtime Operation acknowledgement was not found",
            )
        })?;
        let acknowledgement: RuntimeOperationAcknowledgement =
            serde_json::from_value(acknowledgement).map_err(serialization_error)?;
        let latest_evidence = self.evidence(&acknowledgement.operation_id).await?;
        Ok(RuntimeOperationRecovery {
            protocol: RUNTIME_OPERATIONS_PROTOCOL.to_owned(),
            acknowledgement,
            latest_evidence,
        })
    }

    async fn persist_and_retry(
        &self,
        submission: &RuntimeOperationSubmission,
        caller: &AuthorizedSystemPlaneCaller,
        authority: VerifiedManagementAuthority,
        now_unix_ms: u64,
    ) -> Result<RuntimeOperationAcknowledgement, RuntimeOperationsError> {
        let intent_digest = management_intent_digest(&submission.intent);
        let request_digest = digest_json(submission);
        let operation_id = format!("runtime-operation:{}", &request_digest[7..]);
        let mut transaction = self.pool.begin().await.map_err(store_error)?;
        sqlx::query(
            "select pg_advisory_xact_lock(hashtextextended($1, 0)), pg_advisory_xact_lock(hashtextextended($2, 1))",
        )
        .bind(&submission.intent.idempotency_key)
        .bind(format!(
            "{}:{}",
            target_kind_store(submission.intent.target.kind),
            submission.intent.target.target_id
        ))
        .execute(&mut *transaction)
        .await
        .map_err(store_error)?;
        if let Some((stored_request_digest, acknowledgement)) =
            sqlx::query_as::<_, (String, serde_json::Value)>(
                r#"
                select request_digest, acknowledgement
                from platform.system_plane_runtime_operations
                where service_id = $1 and idempotency_key = $2
                "#,
            )
            .bind(&self.service_id)
            .bind(&submission.intent.idempotency_key)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(store_error)?
        {
            if stored_request_digest != request_digest {
                return Err(operation_error(
                    RuntimeOperationsErrorCode::IdempotencyConflict,
                    "Idempotency key is already bound to a different Runtime Operation request",
                ));
            }
            let acknowledgement =
                serde_json::from_value(acknowledgement).map_err(serialization_error)?;
            transaction.commit().await.map_err(store_error)?;
            return Ok(acknowledgement);
        }
        let before = fetch_target_for_update(&mut transaction, &submission.intent.target).await?;
        let before_snapshot = snapshot(
            &self.service_id,
            &self.service_revision,
            before,
            now_unix_ms,
        )?;
        if before_snapshot.target_revision != submission.intent.expected_target_revision {
            return Err(operation_error(
                RuntimeOperationsErrorCode::StaleTargetRevision,
                "Runtime Operation target changed before durable acceptance",
            ));
        }
        ensure_retryable(before_snapshot.status)?;
        let acknowledgement = RuntimeOperationAcknowledgement {
            protocol: RUNTIME_OPERATIONS_PROTOCOL.to_owned(),
            operation_id: operation_id.clone(),
            idempotency_key: submission.intent.idempotency_key.clone(),
            intent_digest: intent_digest.clone(),
            plan_digest: submission.plan.plan_digest.clone(),
            state: RuntimeOperationState::Accepted,
            accepted_at_unix_ms: now_unix_ms,
            authorization_epoch: caller.enrollment.authorization_epoch,
            enrollment_receipt_digest: caller.enrollment.receipt_digest.clone(),
        };
        let acknowledgement_json =
            serde_json::to_value(&acknowledgement).map_err(serialization_error)?;
        let authority_json = serde_json::to_value(&authority).map_err(serialization_error)?;
        sqlx::query(
            r#"
            insert into platform.system_plane_runtime_operations (
                operation_id, service_id, idempotency_key, request_digest,
                intent_digest, plan_digest, target_kind, target_id,
                target_revision_before, state, authorization_evidence,
                acknowledgement, accepted_at_unix_ms
            ) values ($1, $2, $3, $4, $5, $6, $7, $8, $9,
                      'accepted', $10, $11, $12)
            "#,
        )
        .bind(&operation_id)
        .bind(&self.service_id)
        .bind(&submission.intent.idempotency_key)
        .bind(&request_digest)
        .bind(&intent_digest)
        .bind(&submission.plan.plan_digest)
        .bind(target_kind_store(submission.intent.target.kind))
        .bind(&submission.intent.target.target_id)
        .bind(&before_snapshot.target_revision)
        .bind(authority_json)
        .bind(acknowledgement_json)
        .bind(to_i64(now_unix_ms)?)
        .execute(&mut *transaction)
        .await
        .map_err(store_error)?;
        let accepted_evidence = RuntimeOperationEvidence {
            protocol: RUNTIME_OPERATIONS_PROTOCOL.to_owned(),
            operation_id: operation_id.clone(),
            sequence: 1,
            state: RuntimeOperationState::Accepted,
            recorded_at_unix_ms: now_unix_ms,
            service_id: self.service_id.clone(),
            service_revision: self.service_revision.clone(),
            target: submission.intent.target.clone(),
            target_revision_before: before_snapshot.target_revision.clone(),
            target_revision_after: None,
            code: "runtime_operation_accepted".to_owned(),
            message: "Runtime Operation and authorization evidence were durably accepted"
                .to_owned(),
        };
        sqlx::query(
            r#"
            insert into platform.system_plane_runtime_operation_evidence (
                operation_id, sequence, state, evidence
            ) values ($1, 1, 'accepted', $2)
            "#,
        )
        .bind(&operation_id)
        .bind(serde_json::to_value(&accepted_evidence).map_err(serialization_error)?)
        .execute(&mut *transaction)
        .await
        .map_err(store_error)?;
        let after = retry_target(&mut transaction, &submission.intent.target).await?;
        let after_snapshot =
            snapshot(&self.service_id, &self.service_revision, after, now_unix_ms)?;
        let evidence = RuntimeOperationEvidence {
            protocol: RUNTIME_OPERATIONS_PROTOCOL.to_owned(),
            operation_id: operation_id.clone(),
            sequence: 2,
            state: RuntimeOperationState::Succeeded,
            recorded_at_unix_ms: now_unix_ms,
            service_id: self.service_id.clone(),
            service_revision: self.service_revision.clone(),
            target: submission.intent.target.clone(),
            target_revision_before: before_snapshot.target_revision,
            target_revision_after: Some(after_snapshot.target_revision.clone()),
            code: retry_evidence_code(submission.intent.target.kind).to_owned(),
            message: retry_evidence_message(submission.intent.target.kind).to_owned(),
        };
        let evidence_json = serde_json::to_value(&evidence).map_err(serialization_error)?;
        sqlx::query(
            r#"
            insert into platform.system_plane_runtime_operation_evidence (
                operation_id, sequence, state, evidence
            ) values ($1, 2, 'succeeded', $2)
            "#,
        )
        .bind(&operation_id)
        .bind(evidence_json)
        .execute(&mut *transaction)
        .await
        .map_err(store_error)?;
        sqlx::query(
            r#"
            update platform.system_plane_runtime_operations
            set state = 'succeeded', target_revision_after = $2, updated_at = now()
            where operation_id = $1
            "#,
        )
        .bind(&operation_id)
        .bind(&after_snapshot.target_revision)
        .execute(&mut *transaction)
        .await
        .map_err(store_error)?;
        transaction.commit().await.map_err(store_error)?;
        Ok(acknowledgement)
    }
}

#[derive(Debug, Clone)]
pub struct ManagementAuthorityRequest {
    pub intent: ManagementIntent,
    pub console_service_principal: String,
    pub authorization_epoch: u64,
    pub enrollment_receipt_digest: String,
    pub now_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerifiedManagementAuthority {
    pub actor_subject: String,
    pub delegated_authority_digest: String,
    pub approval_digests: Vec<String>,
    pub console_service_principal: String,
    pub authorization_epoch: u64,
    pub enrollment_receipt_digest: String,
    pub verified_at_unix_ms: u64,
}

#[async_trait]
pub trait ManagementAuthorityVerifier: std::fmt::Debug + Send + Sync {
    async fn verify(
        &self,
        request: ManagementAuthorityRequest,
    ) -> Result<VerifiedManagementAuthority, RuntimeOperationsError>;
}

#[derive(Debug, Clone)]
pub struct SystemSandboxManagementAuthorityVerifier;

impl SystemSandboxManagementAuthorityVerifier {
    pub fn new(environment: &str) -> Result<Self, RuntimeOperationsError> {
        if !matches!(environment, "local" | "development" | "test") {
            return Err(operation_error(
                RuntimeOperationsErrorCode::AuthorityRejected,
                "System Sandbox Management Authority is forbidden outside local development and tests",
            ));
        }
        Ok(Self)
    }
}

#[async_trait]
impl ManagementAuthorityVerifier for SystemSandboxManagementAuthorityVerifier {
    async fn verify(
        &self,
        request: ManagementAuthorityRequest,
    ) -> Result<VerifiedManagementAuthority, RuntimeOperationsError> {
        if request.intent.actor.subject.trim().is_empty()
            || !canonical_digest(&request.intent.actor.delegated_authority_digest)
            || request.intent.approvals.is_empty()
            || request
                .intent
                .approvals
                .iter()
                .any(|approval| !canonical_digest(&approval.approval_digest))
            || request.intent.deadline_unix_ms <= request.now_unix_ms
        {
            return Err(operation_error(
                RuntimeOperationsErrorCode::AuthorityRejected,
                "Management authority, approval evidence, or deadline was rejected",
            ));
        }
        Ok(VerifiedManagementAuthority {
            actor_subject: request.intent.actor.subject,
            delegated_authority_digest: request.intent.actor.delegated_authority_digest,
            approval_digests: request
                .intent
                .approvals
                .into_iter()
                .map(|approval| approval.approval_digest)
                .collect(),
            console_service_principal: request.console_service_principal,
            authorization_epoch: request.authorization_epoch,
            enrollment_receipt_digest: request.enrollment_receipt_digest,
            verified_at_unix_ms: request.now_unix_ms,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeOperationsErrorCode {
    InvalidIntent,
    InvalidEvidenceQuery,
    InvalidEvidenceCursor,
    NotFound,
    TargetNotRetryable,
    StaleTargetRevision,
    PlanMismatch,
    AuthorityUnavailable,
    AuthorityRejected,
    IdempotencyConflict,
    StoreUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeOperationsError {
    pub code: RuntimeOperationsErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct FunctionRunRow {
    id: String,
    function_name: String,
    status: String,
    attempts: i32,
    max_attempts: i32,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct OutboxEventRow {
    id: String,
    event_name: String,
    status: String,
    attempts: i32,
    max_attempts: i32,
    available_at: DateTime<Utc>,
    locked_at: Option<DateTime<Utc>>,
    published_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
enum RuntimeTargetRow {
    FunctionRun(FunctionRunRow),
    OutboxEvent(OutboxEventRow),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeOperationEvidenceCursor {
    protocol: String,
    operation_id: String,
    after_sequence: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeOperationEvidenceQuery {
    cursor: Option<String>,
    limit: Option<u32>,
}

#[must_use]
pub fn router<S>(provider: Option<Arc<RuntimeOperationsProvider>>) -> OpenApiRouter<S>
where
    S: Clone + Send + Sync + 'static,
{
    OpenApiRouter::new()
        .routes(routes!(get_function_run_target))
        .routes(routes!(get_outbox_event_target))
        .routes(routes!(plan_runtime_operation))
        .routes(routes!(submit_runtime_operation))
        .routes(routes!(get_runtime_operation_evidence))
        .routes(routes!(get_runtime_operation_evidence_page))
        .routes(routes!(recover_runtime_operation))
        .layer(Extension(provider))
}

#[utoipa::path(
    get,
    path = "/system-plane/v1/runtime-operations/function-runs/{id}",
    params(("id" = String, Path, description = "Function Run identifier")),
    responses(
        (status = 200, body = RuntimeOperationTargetSnapshot),
        (status = 401, body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 403, body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 404, body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 503, body = SystemPlaneErrorBody, content_type = "application/problem+json")
    ),
    security(("bearer_auth" = [])),
    tag = "system-plane-runtime-operations"
)]
async fn get_function_run_target(
    caller: AuthorizedSystemPlaneCaller,
    Extension(provider): Extension<Option<Arc<RuntimeOperationsProvider>>>,
    Path(id): Path<String>,
) -> Result<Json<RuntimeOperationTargetSnapshot>, SystemPlaneRejection> {
    let provider = require_provider(provider)?;
    require_grant(&caller, &[RUNTIME_OPERATIONS_FEATURE_FUNCTION_RETRY])?;
    provider
        .target_snapshot(
            &RuntimeOperationTarget {
                kind: RuntimeOperationTargetKind::FunctionRun,
                target_id: id,
            },
            now_unix_ms(),
        )
        .await
        .map(Json)
        .map_err(rejection)
}

#[utoipa::path(
    get,
    path = "/system-plane/v1/runtime-operations/outbox-events/{id}",
    params(("id" = String, Path, description = "Outbox Event identifier")),
    responses(
        (status = 200, body = RuntimeOperationTargetSnapshot),
        (status = 401, body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 403, body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 404, body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 503, body = SystemPlaneErrorBody, content_type = "application/problem+json")
    ),
    security(("bearer_auth" = [])),
    tag = "system-plane-runtime-operations"
)]
async fn get_outbox_event_target(
    caller: AuthorizedSystemPlaneCaller,
    Extension(provider): Extension<Option<Arc<RuntimeOperationsProvider>>>,
    Path(id): Path<String>,
) -> Result<Json<RuntimeOperationTargetSnapshot>, SystemPlaneRejection> {
    let provider = require_provider(provider)?;
    require_grant(&caller, &[RUNTIME_OPERATIONS_FEATURE_OUTBOX_RETRY])?;
    provider
        .target_snapshot(
            &RuntimeOperationTarget {
                kind: RuntimeOperationTargetKind::OutboxEvent,
                target_id: id,
            },
            now_unix_ms(),
        )
        .await
        .map(Json)
        .map_err(rejection)
}

#[utoipa::path(
    post,
    path = "/system-plane/v1/runtime-operations/plans",
    request_body = ManagementIntent,
    responses(
        (status = 200, body = RuntimeOperationPlanReceipt),
        (status = 400, body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 401, body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 403, body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 409, body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 503, body = SystemPlaneErrorBody, content_type = "application/problem+json")
    ),
    security(("bearer_auth" = [])),
    tag = "system-plane-runtime-operations"
)]
async fn plan_runtime_operation(
    caller: AuthorizedSystemPlaneCaller,
    Extension(provider): Extension<Option<Arc<RuntimeOperationsProvider>>>,
    Json(intent): Json<ManagementIntent>,
) -> Result<Json<RuntimeOperationPlanReceipt>, SystemPlaneRejection> {
    let provider = require_provider(provider)?;
    require_grant(&caller, &[target_feature(intent.target.kind)])?;
    provider
        .plan(&intent, now_unix_ms())
        .await
        .map(Json)
        .map_err(rejection)
}

#[utoipa::path(
    post,
    path = "/system-plane/v1/runtime-operations/operations",
    request_body = RuntimeOperationSubmission,
    responses(
        (status = 202, body = RuntimeOperationAcknowledgement),
        (status = 400, body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 401, body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 403, body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 409, body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 503, body = SystemPlaneErrorBody, content_type = "application/problem+json")
    ),
    security(("bearer_auth" = [])),
    tag = "system-plane-runtime-operations"
)]
async fn submit_runtime_operation(
    caller: AuthorizedSystemPlaneCaller,
    Extension(provider): Extension<Option<Arc<RuntimeOperationsProvider>>>,
    Json(submission): Json<RuntimeOperationSubmission>,
) -> Result<(StatusCode, Json<RuntimeOperationAcknowledgement>), SystemPlaneRejection> {
    let provider = require_provider(provider)?;
    require_grant(
        &caller,
        &[
            target_feature(submission.intent.target.kind),
            RUNTIME_OPERATIONS_FEATURE_EVIDENCE,
        ],
    )?;
    provider
        .submit(&submission, &caller, now_unix_ms())
        .await
        .map(|acknowledgement| (StatusCode::ACCEPTED, Json(acknowledgement)))
        .map_err(rejection)
}

#[utoipa::path(
    get,
    path = "/system-plane/v1/runtime-operations/operations/{id}",
    params(("id" = String, Path, description = "Service-owned Runtime Operation identifier")),
    responses(
        (status = 200, body = RuntimeOperationEvidence),
        (status = 401, body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 403, body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 404, body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 503, body = SystemPlaneErrorBody, content_type = "application/problem+json")
    ),
    security(("bearer_auth" = [])),
    tag = "system-plane-runtime-operations"
)]
async fn get_runtime_operation_evidence(
    caller: AuthorizedSystemPlaneCaller,
    Extension(provider): Extension<Option<Arc<RuntimeOperationsProvider>>>,
    Path(id): Path<String>,
) -> Result<Json<RuntimeOperationEvidence>, SystemPlaneRejection> {
    let provider = require_provider(provider)?;
    require_grant(&caller, &[RUNTIME_OPERATIONS_FEATURE_EVIDENCE])?;
    provider.evidence(&id).await.map(Json).map_err(rejection)
}

#[utoipa::path(
    get,
    path = "/system-plane/v1/runtime-operations/operations/{id}/evidence",
    params(
        ("id" = String, Path, description = "Service-owned Runtime Operation identifier"),
        ("cursor" = Option<String>, Query, description = "Opaque cursor returned by the prior page"),
        ("limit" = Option<u32>, Query, description = "Evidence records to return, from 1 to 100")
    ),
    responses(
        (status = 200, body = RuntimeOperationEvidencePage),
        (status = 400, body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 401, body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 403, body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 404, body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 503, body = SystemPlaneErrorBody, content_type = "application/problem+json")
    ),
    security(("bearer_auth" = [])),
    tag = "system-plane-runtime-operations"
)]
async fn get_runtime_operation_evidence_page(
    caller: AuthorizedSystemPlaneCaller,
    Extension(provider): Extension<Option<Arc<RuntimeOperationsProvider>>>,
    Path(id): Path<String>,
    Query(query): Query<RuntimeOperationEvidenceQuery>,
) -> Result<Json<RuntimeOperationEvidencePage>, SystemPlaneRejection> {
    let provider = require_provider(provider)?;
    require_grant(&caller, &[RUNTIME_OPERATIONS_FEATURE_EVIDENCE])?;
    provider
        .evidence_page(&id, query.cursor.as_deref(), query.limit.unwrap_or(50))
        .await
        .map(Json)
        .map_err(rejection)
}

#[utoipa::path(
    get,
    path = "/system-plane/v1/runtime-operations/operations/by-idempotency-key/{idempotency_key}",
    params(("idempotency_key" = String, Path, description = "Management Intent idempotency key")),
    responses(
        (status = 200, body = RuntimeOperationRecovery),
        (status = 400, body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 401, body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 403, body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 404, body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 503, body = SystemPlaneErrorBody, content_type = "application/problem+json")
    ),
    security(("bearer_auth" = [])),
    tag = "system-plane-runtime-operations"
)]
async fn recover_runtime_operation(
    caller: AuthorizedSystemPlaneCaller,
    Extension(provider): Extension<Option<Arc<RuntimeOperationsProvider>>>,
    Path(idempotency_key): Path<String>,
) -> Result<Json<RuntimeOperationRecovery>, SystemPlaneRejection> {
    let provider = require_provider(provider)?;
    require_grant(&caller, &[RUNTIME_OPERATIONS_FEATURE_EVIDENCE])?;
    provider
        .recover_by_idempotency_key(&idempotency_key)
        .await
        .map(Json)
        .map_err(rejection)
}

fn require_provider(
    provider: Option<Arc<RuntimeOperationsProvider>>,
) -> Result<Arc<RuntimeOperationsProvider>, SystemPlaneRejection> {
    provider.ok_or_else(|| {
        SystemPlaneRejection::unavailable(
            "runtime_operations_unavailable",
            "Runtime Operations capability is not configured for this Service",
            "configure_runtime_operations",
        )
    })
}

fn require_grant(
    caller: &AuthorizedSystemPlaneCaller,
    required_features: &[&str],
) -> Result<(), SystemPlaneRejection> {
    caller.require_capability(
        RUNTIME_OPERATIONS_PROTOCOL,
        &runtime_operations_schema_digest(),
        required_features,
    )
}

fn target_feature(kind: RuntimeOperationTargetKind) -> &'static str {
    match kind {
        RuntimeOperationTargetKind::FunctionRun => RUNTIME_OPERATIONS_FEATURE_FUNCTION_RETRY,
        RuntimeOperationTargetKind::OutboxEvent => RUNTIME_OPERATIONS_FEATURE_OUTBOX_RETRY,
    }
}

fn validate_intent(
    intent: &ManagementIntent,
    service_id: &str,
    service_revision: &str,
    now_unix_ms: u64,
) -> Result<(), RuntimeOperationsError> {
    if intent.protocol != RUNTIME_OPERATIONS_PROTOCOL
        || intent.intent_id.trim().is_empty()
        || intent.service_id != service_id
        || intent.service_revision != service_revision
        || intent.target.target_id.trim().is_empty()
        || intent.desired_outcome != RuntimeOperationDesiredOutcome::Retry
        || !canonical_digest(&intent.expected_target_revision)
        || intent.actor.subject.trim().is_empty()
        || !canonical_digest(&intent.actor.delegated_authority_digest)
        || intent.approvals.is_empty()
        || intent.approvals.iter().any(|approval| {
            approval.approval_id.trim().is_empty() || !canonical_digest(&approval.approval_digest)
        })
        || intent.deadline_unix_ms <= now_unix_ms
        || intent.idempotency_key.trim().is_empty()
        || intent.capability_contract_id != RUNTIME_OPERATIONS_PROTOCOL
        || intent.capability_schema_digest != runtime_operations_schema_digest()
    {
        return Err(operation_error(
            RuntimeOperationsErrorCode::InvalidIntent,
            "Management Intent is incomplete, expired, or bound to the wrong Service contract",
        ));
    }
    Ok(())
}

fn validate_submitted_plan(
    submission: &RuntimeOperationSubmission,
    service_id: &str,
    service_revision: &str,
    now_unix_ms: u64,
) -> Result<(), RuntimeOperationsError> {
    let plan = &submission.plan;
    let intent = &submission.intent;
    if plan.protocol != RUNTIME_OPERATIONS_PROTOCOL
        || plan.intent_digest != management_intent_digest(intent)
        || plan.plan_digest != runtime_operation_plan_digest(plan)
        || plan.service_id != service_id
        || plan.service_revision != service_revision
        || plan.target != intent.target
        || plan.expected_target_revision != intent.expected_target_revision
        || plan.expected_effects != expected_effects(intent.target.kind)
        || plan.risks != operation_risks(intent.target.kind)
        || plan.availability_impact != RuntimeOperationAvailabilityImpact::None
        || plan.compensation_support != RuntimeOperationCompensationSupport::NotAvailable
        || !plan.approval_required
        || plan.expires_at_unix_ms <= now_unix_ms
        || plan.expires_at_unix_ms > intent.deadline_unix_ms
    {
        return Err(operation_error(
            RuntimeOperationsErrorCode::PlanMismatch,
            "Submitted plan is changed, expired, or no longer bound to its Management Intent",
        ));
    }
    Ok(())
}

async fn fetch_target(
    pool: &PgPool,
    target: &RuntimeOperationTarget,
) -> Result<RuntimeTargetRow, RuntimeOperationsError> {
    match target.kind {
        RuntimeOperationTargetKind::FunctionRun => sqlx::query_as::<_, FunctionRunRow>(
            r#"
            select id, function_name, status, attempts, max_attempts, updated_at
            from runtime.function_runs
            where id = $1
            "#,
        )
        .bind(&target.target_id)
        .fetch_optional(pool)
        .await
        .map_err(store_error)?
        .map(RuntimeTargetRow::FunctionRun)
        .ok_or_else(|| target_not_found(target.kind)),
        RuntimeOperationTargetKind::OutboxEvent => sqlx::query_as::<_, OutboxEventRow>(
            r#"
            select id, event_name, status, attempts, max_attempts,
                   available_at, locked_at, published_at, created_at
            from platform.outbox
            where id = $1
            "#,
        )
        .bind(&target.target_id)
        .fetch_optional(pool)
        .await
        .map_err(store_error)?
        .map(RuntimeTargetRow::OutboxEvent)
        .ok_or_else(|| target_not_found(target.kind)),
    }
}

async fn fetch_target_for_update(
    transaction: &mut Transaction<'_, Postgres>,
    target: &RuntimeOperationTarget,
) -> Result<RuntimeTargetRow, RuntimeOperationsError> {
    match target.kind {
        RuntimeOperationTargetKind::FunctionRun => sqlx::query_as::<_, FunctionRunRow>(
            r#"
            select id, function_name, status, attempts, max_attempts, updated_at
            from runtime.function_runs
            where id = $1
            for update
            "#,
        )
        .bind(&target.target_id)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(store_error)?
        .map(RuntimeTargetRow::FunctionRun)
        .ok_or_else(|| target_not_found(target.kind)),
        RuntimeOperationTargetKind::OutboxEvent => sqlx::query_as::<_, OutboxEventRow>(
            r#"
            select id, event_name, status, attempts, max_attempts,
                   available_at, locked_at, published_at, created_at
            from platform.outbox
            where id = $1
            for update
            "#,
        )
        .bind(&target.target_id)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(store_error)?
        .map(RuntimeTargetRow::OutboxEvent)
        .ok_or_else(|| target_not_found(target.kind)),
    }
}

async fn retry_target(
    transaction: &mut Transaction<'_, Postgres>,
    target: &RuntimeOperationTarget,
) -> Result<RuntimeTargetRow, RuntimeOperationsError> {
    let row = match target.kind {
        RuntimeOperationTargetKind::FunctionRun => sqlx::query_as::<_, FunctionRunRow>(
            r#"
            update runtime.function_runs
            set status = 'pending',
                available_at = now(),
                locked_at = null,
                locked_by = null,
                last_error = null,
                updated_at = now()
            where id = $1 and status in ('failed', 'dead')
            returning id, function_name, status, attempts, max_attempts, updated_at
            "#,
        )
        .bind(&target.target_id)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(store_error)?
        .map(RuntimeTargetRow::FunctionRun),
        RuntimeOperationTargetKind::OutboxEvent => sqlx::query_as::<_, OutboxEventRow>(
            r#"
            update platform.outbox
            set status = 'pending',
                available_at = now(),
                locked_at = null,
                locked_by = null,
                last_error = null
            where id = $1 and status in ('failed', 'dead')
            returning id, event_name, status, attempts, max_attempts,
                      available_at, locked_at, published_at, created_at
            "#,
        )
        .bind(&target.target_id)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(store_error)?
        .map(RuntimeTargetRow::OutboxEvent),
    };
    row.ok_or_else(|| {
        operation_error(
            RuntimeOperationsErrorCode::StaleTargetRevision,
            "Runtime Operation target stopped being retryable before the effect was applied",
        )
    })
}

fn target_not_found(kind: RuntimeOperationTargetKind) -> RuntimeOperationsError {
    operation_error(
        RuntimeOperationsErrorCode::NotFound,
        format!("{} target was not found", target_kind_label(kind)),
    )
}

fn target_kind_store(kind: RuntimeOperationTargetKind) -> &'static str {
    match kind {
        RuntimeOperationTargetKind::FunctionRun => "function_run",
        RuntimeOperationTargetKind::OutboxEvent => "outbox_event",
    }
}

fn target_kind_label(kind: RuntimeOperationTargetKind) -> &'static str {
    match kind {
        RuntimeOperationTargetKind::FunctionRun => "Function Run",
        RuntimeOperationTargetKind::OutboxEvent => "Outbox Event",
    }
}

fn expected_effects(kind: RuntimeOperationTargetKind) -> Vec<String> {
    vec![format!(
        "schedule the exact {} for one additional runtime attempt",
        target_kind_label(kind)
    )]
}

fn operation_risks(kind: RuntimeOperationTargetKind) -> Vec<RuntimeOperationRisk> {
    match kind {
        RuntimeOperationTargetKind::FunctionRun => vec![
            RuntimeOperationRisk::DuplicateExternalEffect,
            RuntimeOperationRisk::RepeatedBusinessNotification,
        ],
        RuntimeOperationTargetKind::OutboxEvent => vec![
            RuntimeOperationRisk::DuplicateExternalEffect,
            RuntimeOperationRisk::RepeatedBusinessNotification,
        ],
    }
}

fn retry_evidence_code(kind: RuntimeOperationTargetKind) -> &'static str {
    match kind {
        RuntimeOperationTargetKind::FunctionRun => "function_run_retry_scheduled",
        RuntimeOperationTargetKind::OutboxEvent => "outbox_event_retry_scheduled",
    }
}

fn retry_evidence_message(kind: RuntimeOperationTargetKind) -> &'static str {
    match kind {
        RuntimeOperationTargetKind::FunctionRun => {
            "Function Run was durably scheduled for another runtime attempt"
        }
        RuntimeOperationTargetKind::OutboxEvent => {
            "Outbox Event was durably scheduled for another delivery attempt"
        }
    }
}

fn snapshot(
    service_id: &str,
    service_revision: &str,
    row: RuntimeTargetRow,
    now_unix_ms: u64,
) -> Result<RuntimeOperationTargetSnapshot, RuntimeOperationsError> {
    let (target, target_name, status, attempts, max_attempts, target_revision) = match row {
        RuntimeTargetRow::FunctionRun(row) => (
            RuntimeOperationTarget {
                kind: RuntimeOperationTargetKind::FunctionRun,
                target_id: row.id.clone(),
            },
            row.function_name.clone(),
            parse_status(RuntimeOperationTargetKind::FunctionRun, &row.status)?,
            checked_count(row.attempts, "Function Run attempt count")?,
            checked_count(row.max_attempts, "Function Run maximum attempt count")?,
            digest_json(&(
                &row.id,
                &row.function_name,
                &row.status,
                row.attempts,
                row.max_attempts,
                row.updated_at.timestamp_micros(),
            )),
        ),
        RuntimeTargetRow::OutboxEvent(row) => (
            RuntimeOperationTarget {
                kind: RuntimeOperationTargetKind::OutboxEvent,
                target_id: row.id.clone(),
            },
            row.event_name.clone(),
            parse_status(RuntimeOperationTargetKind::OutboxEvent, &row.status)?,
            checked_count(row.attempts, "Outbox Event attempt count")?,
            checked_count(row.max_attempts, "Outbox Event maximum attempt count")?,
            digest_json(&(
                &row.id,
                &row.event_name,
                &row.status,
                row.attempts,
                row.max_attempts,
                row.available_at.timestamp_micros(),
                row.locked_at.map(|value| value.timestamp_micros()),
                row.published_at.map(|value| value.timestamp_micros()),
                row.created_at.timestamp_micros(),
            )),
        ),
    };
    Ok(RuntimeOperationTargetSnapshot {
        protocol: RUNTIME_OPERATIONS_PROTOCOL.to_owned(),
        service_id: service_id.to_owned(),
        service_revision: service_revision.to_owned(),
        target,
        target_revision,
        observed_at_unix_ms: now_unix_ms,
        target_name,
        status,
        attempts,
        max_attempts,
    })
}

fn checked_count(value: i32, label: &str) -> Result<u32, RuntimeOperationsError> {
    u32::try_from(value).map_err(|_| {
        operation_error(
            RuntimeOperationsErrorCode::StoreUnavailable,
            format!("{label} is invalid in the Service Store"),
        )
    })
}

fn parse_status(
    kind: RuntimeOperationTargetKind,
    status: &str,
) -> Result<RuntimeOperationTargetStatus, RuntimeOperationsError> {
    match (kind, status) {
        (RuntimeOperationTargetKind::FunctionRun, "pending")
        | (RuntimeOperationTargetKind::OutboxEvent, "pending") => {
            Ok(RuntimeOperationTargetStatus::Pending)
        }
        (RuntimeOperationTargetKind::FunctionRun, "processing")
        | (RuntimeOperationTargetKind::OutboxEvent, "processing") => {
            Ok(RuntimeOperationTargetStatus::Processing)
        }
        (RuntimeOperationTargetKind::FunctionRun, "running") => {
            Ok(RuntimeOperationTargetStatus::Running)
        }
        (RuntimeOperationTargetKind::FunctionRun, "completed") => {
            Ok(RuntimeOperationTargetStatus::Completed)
        }
        (RuntimeOperationTargetKind::OutboxEvent, "published") => {
            Ok(RuntimeOperationTargetStatus::Published)
        }
        (RuntimeOperationTargetKind::FunctionRun, "failed")
        | (RuntimeOperationTargetKind::OutboxEvent, "failed") => {
            Ok(RuntimeOperationTargetStatus::Failed)
        }
        (RuntimeOperationTargetKind::FunctionRun, "dead")
        | (RuntimeOperationTargetKind::OutboxEvent, "dead") => {
            Ok(RuntimeOperationTargetStatus::Dead)
        }
        _ => Err(operation_error(
            RuntimeOperationsErrorCode::StoreUnavailable,
            "Runtime Operation target has an unsupported stored status",
        )),
    }
}

fn ensure_retryable(status: RuntimeOperationTargetStatus) -> Result<(), RuntimeOperationsError> {
    if matches!(
        status,
        RuntimeOperationTargetStatus::Failed | RuntimeOperationTargetStatus::Dead
    ) {
        Ok(())
    } else {
        Err(operation_error(
            RuntimeOperationsErrorCode::TargetNotRetryable,
            "Only failed or dead Runtime Operation targets can be retried",
        ))
    }
}

fn rejection(error: RuntimeOperationsError) -> SystemPlaneRejection {
    let (status, code, next_action) = match error.code {
        RuntimeOperationsErrorCode::InvalidIntent
        | RuntimeOperationsErrorCode::InvalidEvidenceQuery
        | RuntimeOperationsErrorCode::InvalidEvidenceCursor
        | RuntimeOperationsErrorCode::PlanMismatch => (
            StatusCode::BAD_REQUEST,
            "runtime_operation_invalid_request",
            "rebuild_runtime_operation_plan",
        ),
        RuntimeOperationsErrorCode::NotFound => (
            StatusCode::NOT_FOUND,
            "runtime_operation_not_found",
            "refresh_runtime_observation",
        ),
        RuntimeOperationsErrorCode::TargetNotRetryable
        | RuntimeOperationsErrorCode::StaleTargetRevision
        | RuntimeOperationsErrorCode::IdempotencyConflict => (
            StatusCode::CONFLICT,
            "runtime_operation_conflict",
            "refresh_runtime_observation",
        ),
        RuntimeOperationsErrorCode::AuthorityUnavailable => (
            StatusCode::SERVICE_UNAVAILABLE,
            "runtime_operation_authority_unavailable",
            "configure_runtime_operation_authority",
        ),
        RuntimeOperationsErrorCode::AuthorityRejected => (
            StatusCode::FORBIDDEN,
            "runtime_operation_authority_rejected",
            "obtain_runtime_operation_approval",
        ),
        RuntimeOperationsErrorCode::StoreUnavailable => (
            StatusCode::SERVICE_UNAVAILABLE,
            "runtime_operation_store_unavailable",
            "restore_service_store",
        ),
    };
    SystemPlaneRejection::new(status, code, error.message, next_action)
}

fn operation_error(
    code: RuntimeOperationsErrorCode,
    message: impl Into<String>,
) -> RuntimeOperationsError {
    RuntimeOperationsError {
        code,
        message: message.into(),
    }
}

fn store_error(_source: sqlx::Error) -> RuntimeOperationsError {
    operation_error(
        RuntimeOperationsErrorCode::StoreUnavailable,
        "Runtime Operations Store operation failed",
    )
}

fn serialization_error(_source: serde_json::Error) -> RuntimeOperationsError {
    operation_error(
        RuntimeOperationsErrorCode::StoreUnavailable,
        "Runtime Operations Store contains invalid evidence",
    )
}

fn canonical_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    })
}

fn digest_json<T: Serialize>(value: &T) -> String {
    let bytes = serde_json::to_vec(value).expect("Runtime Operations value serializes");
    format!("sha256:{}", hex(&Sha256::digest(bytes)))
}

fn encode_evidence_cursor(
    cursor: &RuntimeOperationEvidenceCursor,
) -> Result<String, RuntimeOperationsError> {
    serde_json::to_vec(cursor)
        .map(|bytes| URL_SAFE_NO_PAD.encode(bytes))
        .map_err(serialization_error)
}

fn decode_evidence_cursor(
    encoded: &str,
) -> Result<RuntimeOperationEvidenceCursor, RuntimeOperationsError> {
    if encoded.is_empty() || encoded.len() > 4096 {
        return Err(invalid_evidence_cursor());
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| invalid_evidence_cursor())?;
    let cursor: RuntimeOperationEvidenceCursor =
        serde_json::from_slice(&bytes).map_err(|_| invalid_evidence_cursor())?;
    if cursor.protocol != RUNTIME_OPERATION_EVIDENCE_CURSOR_PROTOCOL
        || cursor.operation_id.trim().is_empty()
        || cursor.after_sequence == 0
        || cursor.after_sequence > i64::MAX as u64
    {
        return Err(invalid_evidence_cursor());
    }
    Ok(cursor)
}

fn invalid_evidence_cursor() -> RuntimeOperationsError {
    operation_error(
        RuntimeOperationsErrorCode::InvalidEvidenceCursor,
        "Operation Evidence cursor is invalid",
    )
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn to_i64(value: u64) -> Result<i64, RuntimeOperationsError> {
    i64::try_from(value).map_err(|_| {
        operation_error(
            RuntimeOperationsErrorCode::InvalidIntent,
            "Runtime Operation timestamp exceeds the supported Store range",
        )
    })
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outbox_snapshot_is_revisioned_and_uses_outbox_feature_scope() {
        let timestamp = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let snapshot = snapshot(
            "support",
            "release:sha256:0123456789abcdef",
            RuntimeTargetRow::OutboxEvent(OutboxEventRow {
                id: "event-1".to_owned(),
                event_name: "tickets.notified.v1".to_owned(),
                status: "dead".to_owned(),
                attempts: 3,
                max_attempts: 3,
                available_at: timestamp,
                locked_at: None,
                published_at: None,
                created_at: timestamp,
            }),
            1_700_000_001_000,
        )
        .unwrap();

        assert_eq!(
            snapshot.target.kind,
            RuntimeOperationTargetKind::OutboxEvent
        );
        assert_eq!(snapshot.target_name, "tickets.notified.v1");
        assert_eq!(snapshot.status, RuntimeOperationTargetStatus::Dead);
        assert!(canonical_digest(&snapshot.target_revision));
        assert_eq!(
            target_feature(snapshot.target.kind),
            RUNTIME_OPERATIONS_FEATURE_OUTBOX_RETRY
        );
        assert_eq!(
            expected_effects(snapshot.target.kind),
            ["schedule the exact Outbox Event for one additional runtime attempt"]
        );
    }

    #[test]
    fn stored_statuses_are_target_kind_specific_and_fail_closed() {
        assert_eq!(
            parse_status(RuntimeOperationTargetKind::FunctionRun, "completed").unwrap(),
            RuntimeOperationTargetStatus::Completed
        );
        assert_eq!(
            parse_status(RuntimeOperationTargetKind::OutboxEvent, "published").unwrap(),
            RuntimeOperationTargetStatus::Published
        );
        assert_eq!(
            parse_status(RuntimeOperationTargetKind::FunctionRun, "published")
                .unwrap_err()
                .code,
            RuntimeOperationsErrorCode::StoreUnavailable
        );
        assert_eq!(
            parse_status(RuntimeOperationTargetKind::OutboxEvent, "completed")
                .unwrap_err()
                .code,
            RuntimeOperationsErrorCode::StoreUnavailable
        );
    }

    #[test]
    fn evidence_cursor_is_opaque_scoped_and_strict() {
        let cursor = RuntimeOperationEvidenceCursor {
            protocol: RUNTIME_OPERATION_EVIDENCE_CURSOR_PROTOCOL.to_owned(),
            operation_id: "runtime-operation:01".to_owned(),
            after_sequence: 2,
        };
        let encoded = encode_evidence_cursor(&cursor).unwrap();
        assert_eq!(decode_evidence_cursor(&encoded).unwrap(), cursor);
        assert_eq!(
            decode_evidence_cursor("not-a-cursor").unwrap_err().code,
            RuntimeOperationsErrorCode::InvalidEvidenceCursor
        );

        let invalid = RuntimeOperationEvidenceCursor {
            protocol: "wrong".to_owned(),
            ..cursor
        };
        assert_eq!(
            decode_evidence_cursor(&encode_evidence_cursor(&invalid).unwrap())
                .unwrap_err()
                .code,
            RuntimeOperationsErrorCode::InvalidEvidenceCursor
        );
    }
}
