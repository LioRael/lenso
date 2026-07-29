//! Service-owned Runtime Operations Capability Provider.

use async_trait::async_trait;
use axum::{Extension, Json, extract::Path, http::StatusCode};
use chrono::{DateTime, Utc};
use lenso_service::system_plane::{
    ManagementIntent, RUNTIME_OPERATIONS_FEATURE_EVIDENCE,
    RUNTIME_OPERATIONS_FEATURE_FUNCTION_RETRY, RUNTIME_OPERATIONS_PATH,
    RUNTIME_OPERATIONS_PROTOCOL, RuntimeOperationAcknowledgement,
    RuntimeOperationAvailabilityImpact, RuntimeOperationCompensationSupport,
    RuntimeOperationDesiredOutcome, RuntimeOperationEvidence, RuntimeOperationPlanReceipt,
    RuntimeOperationRisk, RuntimeOperationState, RuntimeOperationSubmission,
    RuntimeOperationTarget, RuntimeOperationTargetKind, RuntimeOperationTargetSnapshot,
    RuntimeOperationTargetStatus, management_intent_digest, runtime_operation_plan_digest,
    runtime_operations_schema_digest,
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
        target_id: &str,
        now_unix_ms: u64,
    ) -> Result<RuntimeOperationTargetSnapshot, RuntimeOperationsError> {
        let row = fetch_target(&self.pool, target_id).await?;
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
        let target = self
            .target_snapshot(&intent.target.target_id, now_unix_ms)
            .await?;
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
            expected_effects: vec![
                "schedule the exact Function Run for one additional runtime attempt".to_owned(),
            ],
            risks: vec![
                RuntimeOperationRisk::DuplicateExternalEffect,
                RuntimeOperationRisk::RepeatedBusinessNotification,
            ],
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
            select evidence
            from platform.system_plane_runtime_operation_evidence
            where operation_id = $1
            order by sequence desc
            limit 1
            "#,
        )
        .bind(operation_id)
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
        .bind(&submission.intent.target.target_id)
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
        let before =
            fetch_target_for_update(&mut transaction, &submission.intent.target.target_id).await?;
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
            ) values ($1, $2, $3, $4, $5, $6, 'function_run', $7, $8,
                      'accepted', $9, $10, $11)
            "#,
        )
        .bind(&operation_id)
        .bind(&self.service_id)
        .bind(&submission.intent.idempotency_key)
        .bind(&request_digest)
        .bind(&intent_digest)
        .bind(&submission.plan.plan_digest)
        .bind(&submission.intent.target.target_id)
        .bind(&before_snapshot.target_revision)
        .bind(authority_json)
        .bind(acknowledgement_json)
        .bind(to_i64(now_unix_ms)?)
        .execute(&mut *transaction)
        .await
        .map_err(store_error)?;
        let after = sqlx::query_as::<_, FunctionRunRow>(
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
        .bind(&submission.intent.target.target_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(store_error)?
        .ok_or_else(|| {
            operation_error(
                RuntimeOperationsErrorCode::StaleTargetRevision,
                "Function Run stopped being retryable before the effect was applied",
            )
        })?;
        let after_snapshot =
            snapshot(&self.service_id, &self.service_revision, after, now_unix_ms)?;
        let evidence = RuntimeOperationEvidence {
            protocol: RUNTIME_OPERATIONS_PROTOCOL.to_owned(),
            operation_id: operation_id.clone(),
            sequence: 1,
            state: RuntimeOperationState::Succeeded,
            recorded_at_unix_ms: now_unix_ms,
            service_id: self.service_id.clone(),
            service_revision: self.service_revision.clone(),
            target: submission.intent.target.clone(),
            target_revision_before: before_snapshot.target_revision,
            target_revision_after: Some(after_snapshot.target_revision.clone()),
            code: "function_run_retry_scheduled".to_owned(),
            message: "Function Run was durably scheduled for another runtime attempt".to_owned(),
        };
        let evidence_json = serde_json::to_value(&evidence).map_err(serialization_error)?;
        sqlx::query(
            r#"
            insert into platform.system_plane_runtime_operation_evidence (
                operation_id, sequence, state, evidence
            ) values ($1, 1, 'succeeded', $2)
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

#[must_use]
pub fn router<S>(provider: Option<Arc<RuntimeOperationsProvider>>) -> OpenApiRouter<S>
where
    S: Clone + Send + Sync + 'static,
{
    OpenApiRouter::new()
        .routes(routes!(get_function_run_target))
        .routes(routes!(plan_runtime_operation))
        .routes(routes!(submit_runtime_operation))
        .routes(routes!(get_runtime_operation_evidence))
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
    require_grant(&caller)?;
    provider
        .target_snapshot(&id, now_unix_ms())
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
    require_grant(&caller)?;
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
    require_grant(&caller)?;
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
    require_grant(&caller)?;
    provider.evidence(&id).await.map(Json).map_err(rejection)
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

fn require_grant(caller: &AuthorizedSystemPlaneCaller) -> Result<(), SystemPlaneRejection> {
    caller.require_capability(
        RUNTIME_OPERATIONS_PROTOCOL,
        &runtime_operations_schema_digest(),
        [
            RUNTIME_OPERATIONS_FEATURE_FUNCTION_RETRY,
            RUNTIME_OPERATIONS_FEATURE_EVIDENCE,
        ],
    )
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
        || intent.target.kind != RuntimeOperationTargetKind::FunctionRun
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
        || plan.expected_effects
            != ["schedule the exact Function Run for one additional runtime attempt"]
        || plan.risks
            != [
                RuntimeOperationRisk::DuplicateExternalEffect,
                RuntimeOperationRisk::RepeatedBusinessNotification,
            ]
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
    target_id: &str,
) -> Result<FunctionRunRow, RuntimeOperationsError> {
    sqlx::query_as::<_, FunctionRunRow>(
        r#"
        select id, function_name, status, attempts, max_attempts, updated_at
        from runtime.function_runs
        where id = $1
        "#,
    )
    .bind(target_id)
    .fetch_optional(pool)
    .await
    .map_err(store_error)?
    .ok_or_else(|| {
        operation_error(
            RuntimeOperationsErrorCode::NotFound,
            "Function Run target was not found",
        )
    })
}

async fn fetch_target_for_update(
    transaction: &mut Transaction<'_, Postgres>,
    target_id: &str,
) -> Result<FunctionRunRow, RuntimeOperationsError> {
    sqlx::query_as::<_, FunctionRunRow>(
        r#"
        select id, function_name, status, attempts, max_attempts, updated_at
        from runtime.function_runs
        where id = $1
        for update
        "#,
    )
    .bind(target_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(store_error)?
    .ok_or_else(|| {
        operation_error(
            RuntimeOperationsErrorCode::NotFound,
            "Function Run target was not found",
        )
    })
}

fn snapshot(
    service_id: &str,
    service_revision: &str,
    row: FunctionRunRow,
    now_unix_ms: u64,
) -> Result<RuntimeOperationTargetSnapshot, RuntimeOperationsError> {
    let attempts = u32::try_from(row.attempts).map_err(|_| {
        operation_error(
            RuntimeOperationsErrorCode::StoreUnavailable,
            "Function Run has an invalid stored attempt count",
        )
    })?;
    let max_attempts = u32::try_from(row.max_attempts).map_err(|_| {
        operation_error(
            RuntimeOperationsErrorCode::StoreUnavailable,
            "Function Run has an invalid stored maximum attempt count",
        )
    })?;
    let target_revision = digest_json(&(
        &row.id,
        &row.function_name,
        &row.status,
        row.attempts,
        row.max_attempts,
        row.updated_at.timestamp_micros(),
    ));
    Ok(RuntimeOperationTargetSnapshot {
        protocol: RUNTIME_OPERATIONS_PROTOCOL.to_owned(),
        service_id: service_id.to_owned(),
        service_revision: service_revision.to_owned(),
        target: RuntimeOperationTarget {
            kind: RuntimeOperationTargetKind::FunctionRun,
            target_id: row.id,
        },
        target_revision,
        observed_at_unix_ms: now_unix_ms,
        function_name: row.function_name,
        status: parse_status(&row.status)?,
        attempts,
        max_attempts,
    })
}

fn parse_status(status: &str) -> Result<RuntimeOperationTargetStatus, RuntimeOperationsError> {
    match status {
        "pending" => Ok(RuntimeOperationTargetStatus::Pending),
        "processing" => Ok(RuntimeOperationTargetStatus::Processing),
        "running" => Ok(RuntimeOperationTargetStatus::Running),
        "completed" => Ok(RuntimeOperationTargetStatus::Completed),
        "failed" => Ok(RuntimeOperationTargetStatus::Failed),
        "dead" => Ok(RuntimeOperationTargetStatus::Dead),
        _ => Err(operation_error(
            RuntimeOperationsErrorCode::StoreUnavailable,
            "Function Run has an unsupported stored status",
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
            "Only failed or dead Function Runs can be retried",
        ))
    }
}

fn rejection(error: RuntimeOperationsError) -> SystemPlaneRejection {
    let (status, code, next_action) = match error.code {
        RuntimeOperationsErrorCode::InvalidIntent | RuntimeOperationsErrorCode::PlanMismatch => (
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
