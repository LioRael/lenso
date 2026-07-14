use crate::{
    ServiceRuntimeState, TransportAdapter, TransportDeploymentClass, TransportHealthStatus,
    TransportPublication,
};
use lenso_service::EventEnvelope;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use thiserror::Error;

pub const DEAD_LETTER_INSPECTION_PROTOCOL: &str = "lenso.dead-letter-inspection.v1";
pub const DEAD_LETTER_REPLAY_PLAN_PROTOCOL: &str = "lenso.dead-letter-replay-plan.v1";
pub const DEAD_LETTER_REPLAY_RESULT_PROTOCOL: &str = "lenso.dead-letter-replay-result.v1";
pub const DEAD_LETTER_RETENTION_RESULT_PROTOCOL: &str = "lenso.dead-letter-retention-result.v1";
pub const DEAD_LETTER_CLEANUP_PLAN_PROTOCOL: &str = "lenso.dead-letter-cleanup-plan.v1";
pub const DEAD_LETTER_CLEANUP_RESULT_PROTOCOL: &str = "lenso.dead-letter-cleanup-result.v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadLetterInspectQuery {
    pub consumer_id: Option<String>,
    pub event_id: Option<String>,
    pub limit: u32,
}

impl Default for DeadLetterInspectQuery {
    fn default() -> Self {
        Self {
            consumer_id: None,
            event_id: None,
            limit: 100,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeadLetterOperatorErrorCode {
    StoreUnavailable,
    InvalidRequest,
    DeadLetterNotFound,
    StateChanged,
    ApprovalRequired,
    ApprovalMismatch,
    ReplayActive,
    CleanupProtected,
    ReplayDeliveryFailed,
    AuthorizationDenied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeadLetterOperatorEnvironment {
    LocalSandbox,
    Production,
}

impl DeadLetterOperatorEnvironment {
    const fn as_str(self) -> &'static str {
        match self {
            Self::LocalSandbox => "local_sandbox",
            Self::Production => "production",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeadLetterProtectedAction {
    ProductionReplay,
    DestructiveCleanup,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeadLetterApprovalRequest {
    pub action: DeadLetterProtectedAction,
    pub plan_id: String,
    pub service_id: String,
    pub environment: DeadLetterOperatorEnvironment,
}

pub trait DeadLetterAuthorityVerifier: std::fmt::Debug + Send + Sync {
    /// Returns the authority provider's stable approval identifier only when
    /// the credential authorizes this exact request.
    fn verify(
        &self,
        request: &DeadLetterApprovalRequest,
        credential: &str,
    ) -> Result<String, String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedDeadLetterApproval {
    approval_id: String,
    request: DeadLetterApprovalRequest,
}

pub fn verify_dead_letter_authority(
    verifier: &dyn DeadLetterAuthorityVerifier,
    request: DeadLetterApprovalRequest,
    credential: &str,
) -> Result<VerifiedDeadLetterApproval, DeadLetterOperatorError> {
    if credential.trim().is_empty() {
        return Err(DeadLetterOperatorError {
            code: DeadLetterOperatorErrorCode::ApprovalRequired,
            message: "Protected dead-letter action requires an authority credential".to_owned(),
            next_actions: vec!["request_operator_approval".to_owned()],
            source: None,
        });
    }
    let approval_id =
        verifier
            .verify(&request, credential)
            .map_err(|message| DeadLetterOperatorError {
                code: DeadLetterOperatorErrorCode::AuthorizationDenied,
                message,
                next_actions: vec!["request_operator_approval".to_owned()],
                source: None,
            })?;
    if approval_id.trim().is_empty() {
        return Err(DeadLetterOperatorError {
            code: DeadLetterOperatorErrorCode::AuthorizationDenied,
            message: "Authority verifier returned an empty approval identity".to_owned(),
            next_actions: vec!["repair_authority_provider".to_owned()],
            source: None,
        });
    }
    Ok(VerifiedDeadLetterApproval {
        approval_id,
        request,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayApprovalBoundary {
    LocalSandboxOnly,
    ProductionReplay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeadLetterAuthorizationStatus {
    NotRequired,
    Required,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeadLetterAuthorization {
    pub status: DeadLetterAuthorizationStatus,
    pub required_authority: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayIdentity {
    pub event_id: String,
    pub contract_id: String,
    pub contract_version: String,
    pub story_context: Option<serde_json::Value>,
    pub causation: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeadLetterReplayPlan {
    pub protocol: &'static str,
    pub plan_id: String,
    pub mutates_state: bool,
    pub dead_letter_id: String,
    pub affected_service_id: String,
    pub consumer_id: String,
    pub original_delivery_id: String,
    pub identity: ReplayIdentity,
    pub authorization: DeadLetterAuthorization,
    pub approval_boundary: ReplayApprovalBoundary,
    pub intended_delivery: serde_json::Value,
    pub next_actions: Vec<String>,
    pub environment: DeadLetterOperatorEnvironment,
    pub validations: Vec<DeadLetterReplayValidation>,
}

impl DeadLetterReplayPlan {
    #[must_use]
    pub fn approval_request(&self) -> Option<DeadLetterApprovalRequest> {
        (self.environment == DeadLetterOperatorEnvironment::Production).then(|| {
            DeadLetterApprovalRequest {
                action: DeadLetterProtectedAction::ProductionReplay,
                plan_id: self.plan_id.clone(),
                service_id: self.affected_service_id.clone(),
                environment: self.environment,
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeadLetterReplayValidation {
    pub code: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeadLetterReplayResult {
    pub protocol: &'static str,
    pub replay_id: String,
    pub dead_letter_id: String,
    pub event_id: String,
    pub contract_id: String,
    pub contract_version: String,
    pub delivery_id: String,
    pub outcome: String,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeadLetterRetentionResult {
    pub protocol: &'static str,
    pub dead_letter_id: String,
    pub retained_until: chrono::DateTime<chrono::Utc>,
    pub outcome: String,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeadLetterCleanupPlan {
    pub protocol: &'static str,
    pub plan_id: String,
    pub mutates_state: bool,
    pub environment: DeadLetterOperatorEnvironment,
    pub service_id: String,
    pub cutoff: chrono::DateTime<chrono::Utc>,
    pub dead_letter_ids: Vec<String>,
    pub preserved_state: Vec<String>,
    pub authorization: DeadLetterAuthorization,
    pub approval_boundary: DeadLetterCleanupApprovalBoundary,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeadLetterCleanupApprovalBoundary {
    DestructiveCleanup,
}

impl DeadLetterCleanupPlan {
    #[must_use]
    pub fn approval_request(&self) -> DeadLetterApprovalRequest {
        DeadLetterApprovalRequest {
            action: DeadLetterProtectedAction::DestructiveCleanup,
            plan_id: self.plan_id.clone(),
            service_id: self.service_id.clone(),
            environment: self.environment,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeadLetterCleanupResult {
    pub protocol: &'static str,
    pub deleted_dead_letter_ids: Vec<String>,
    pub preserved_state: Vec<String>,
    pub outcome: String,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Error, Serialize)]
#[error("{message}")]
#[serde(rename_all = "camelCase")]
pub struct DeadLetterOperatorError {
    pub code: DeadLetterOperatorErrorCode,
    pub message: String,
    pub next_actions: Vec<String>,
    #[serde(skip)]
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl DeadLetterOperatorError {
    fn store(message: impl Into<String>, error: sqlx::Error) -> Self {
        Self {
            code: DeadLetterOperatorErrorCode::StoreUnavailable,
            message: message.into(),
            next_actions: vec![
                "restore_service_store".to_owned(),
                "retry_command".to_owned(),
            ],
            source: Some(Box::new(error)),
        }
    }

    fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: DeadLetterOperatorErrorCode::InvalidRequest,
            message: message.into(),
            next_actions: vec!["correct_command_input".to_owned()],
            source: None,
        }
    }
}

fn operator_store(state: &ServiceRuntimeState) -> Result<&PgPool, DeadLetterOperatorError> {
    state.store().map_err(|error| DeadLetterOperatorError {
        code: DeadLetterOperatorErrorCode::StoreUnavailable,
        message: error.public_message,
        next_actions: vec![
            "restore_service_store".to_owned(),
            "retry_command".to_owned(),
        ],
        source: None,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeadLetterInspection {
    pub protocol: &'static str,
    pub service_id: String,
    pub items: Vec<DeadLetterItem>,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeadLetterItem {
    pub dead_letter_id: String,
    pub consumer_id: String,
    pub event_id: String,
    pub delivery_id: String,
    pub envelope: serde_json::Value,
    pub contract_id: String,
    pub contract_version: String,
    pub failure_reason: String,
    pub reason_code: String,
    pub diagnostic: String,
    pub attempt_count: i32,
    pub terminal_outcome: String,
    pub delivery_history: serde_json::Value,
    pub max_attempts: i32,
    pub retry_schedule: serde_json::Value,
    pub next_actions: serde_json::Value,
    pub dead_lettered_at: chrono::DateTime<chrono::Utc>,
    pub status: String,
    pub retained_until: Option<chrono::DateTime<chrono::Utc>>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, FromRow)]
struct DeadLetterRow {
    dead_letter_id: String,
    consumer_id: String,
    event_id: String,
    delivery_id: String,
    envelope: serde_json::Value,
    contract_id: String,
    contract_version: String,
    failure_reason: String,
    reason_code: String,
    diagnostic: String,
    attempt_count: i32,
    terminal_outcome: String,
    delivery_history: serde_json::Value,
    max_attempts: i32,
    retry_schedule: serde_json::Value,
    next_actions: serde_json::Value,
    dead_lettered_at: chrono::DateTime<chrono::Utc>,
    status: String,
    retained_until: Option<chrono::DateTime<chrono::Utc>>,
    resolved_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, FromRow)]
struct ReplayInboxRow {
    status: String,
    envelope: serde_json::Value,
    original_envelope: Option<serde_json::Value>,
}

impl From<DeadLetterRow> for DeadLetterItem {
    fn from(row: DeadLetterRow) -> Self {
        Self {
            dead_letter_id: row.dead_letter_id,
            consumer_id: row.consumer_id,
            event_id: row.event_id,
            delivery_id: row.delivery_id,
            envelope: row.envelope,
            contract_id: row.contract_id,
            contract_version: row.contract_version,
            failure_reason: row.failure_reason,
            reason_code: row.reason_code,
            diagnostic: row.diagnostic,
            attempt_count: row.attempt_count,
            terminal_outcome: row.terminal_outcome,
            delivery_history: row.delivery_history,
            max_attempts: row.max_attempts,
            retry_schedule: row.retry_schedule,
            next_actions: row.next_actions,
            dead_lettered_at: row.dead_lettered_at,
            status: row.status,
            retained_until: row.retained_until,
            resolved_at: row.resolved_at,
        }
    }
}

pub async fn inspect_dead_letters(
    state: &ServiceRuntimeState,
    query: DeadLetterInspectQuery,
) -> Result<DeadLetterInspection, DeadLetterOperatorError> {
    if query.limit == 0 || query.limit > 1_000 {
        return Err(DeadLetterOperatorError::invalid(
            "Dead-letter inspection limit must be between 1 and 1000",
        ));
    }
    let pool = state.store().map_err(|error| DeadLetterOperatorError {
        code: DeadLetterOperatorErrorCode::StoreUnavailable,
        message: error.public_message,
        next_actions: vec![
            "restore_service_store".to_owned(),
            "retry_command".to_owned(),
        ],
        source: None,
    })?;
    let rows = sqlx::query_as::<_, DeadLetterRow>(
        r#"
        select dead_letter_id, consumer_id, event_id, delivery_id, envelope,
               contract_id, contract_version, failure_reason, reason_code,
               diagnostic, attempt_count, terminal_outcome, delivery_history,
               max_attempts, retry_schedule, next_actions, dead_lettered_at,
               status, retained_until, resolved_at
        from platform.service_event_dead_letters
        where ($1::text is null or consumer_id = $1)
          and ($2::text is null or event_id = $2)
        order by dead_lettered_at, event_id, dead_letter_id
        limit $3
        "#,
    )
    .bind(query.consumer_id)
    .bind(query.event_id)
    .bind(i64::from(query.limit))
    .fetch_all(pool)
    .await
    .map_err(|error| {
        DeadLetterOperatorError::store("Could not inspect Service dead letters", error)
    })?;
    Ok(DeadLetterInspection {
        protocol: DEAD_LETTER_INSPECTION_PROTOCOL,
        service_id: state.identity.service_id.clone(),
        items: rows.into_iter().map(Into::into).collect(),
        next_actions: vec!["plan_replay".to_owned(), "plan_cleanup".to_owned()],
    })
}

async fn load_dead_letter(
    state: &ServiceRuntimeState,
    dead_letter_id: &str,
) -> Result<DeadLetterRow, DeadLetterOperatorError> {
    let pool = state.store().map_err(|error| DeadLetterOperatorError {
        code: DeadLetterOperatorErrorCode::StoreUnavailable,
        message: error.public_message,
        next_actions: vec![
            "restore_service_store".to_owned(),
            "retry_command".to_owned(),
        ],
        source: None,
    })?;
    sqlx::query_as::<_, DeadLetterRow>(
        r#"
        select dead_letter_id, consumer_id, event_id, delivery_id, envelope,
               contract_id, contract_version, failure_reason, reason_code,
               diagnostic, attempt_count, terminal_outcome, delivery_history,
               max_attempts, retry_schedule, next_actions, dead_lettered_at,
               status, retained_until, resolved_at
        from platform.service_event_dead_letters
        where dead_letter_id = $1
        "#,
    )
    .bind(dead_letter_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        DeadLetterOperatorError::store("Could not inspect Service dead letter", error)
    })?
    .ok_or_else(|| DeadLetterOperatorError {
        code: DeadLetterOperatorErrorCode::DeadLetterNotFound,
        message: format!("Dead letter `{dead_letter_id}` was not found"),
        next_actions: vec!["inspect_dead_letters".to_owned()],
        source: None,
    })
}

pub async fn plan_dead_letter_replay(
    state: &ServiceRuntimeState,
    adapter: &dyn TransportAdapter,
    dead_letter_id: &str,
) -> Result<DeadLetterReplayPlan, DeadLetterOperatorError> {
    let row = load_dead_letter(state, dead_letter_id).await?;
    if row.status == "replay_active" {
        return Err(DeadLetterOperatorError {
            code: DeadLetterOperatorErrorCode::ReplayActive,
            message: format!("Dead letter `{dead_letter_id}` already has an active replay"),
            next_actions: vec!["inspect_replay_status".to_owned()],
            source: None,
        });
    }
    let envelope: EventEnvelope =
        serde_json::from_value(row.envelope.clone()).map_err(|_| DeadLetterOperatorError {
            code: DeadLetterOperatorErrorCode::InvalidRequest,
            message: format!("Dead letter `{dead_letter_id}` has an invalid Event Envelope"),
            next_actions: vec![
                "inspect_payload".to_owned(),
                "retain_dead_letter".to_owned(),
            ],
            source: None,
        })?;
    if row.consumer_id != state.identity.service_id {
        return Err(DeadLetterOperatorError::invalid(format!(
            "Dead letter `{dead_letter_id}` belongs to consumer `{}`, not Service `{}`",
            row.consumer_id, state.identity.service_id
        )));
    }
    if row.event_id != envelope.event_id
        || row.contract_id != envelope.contract_id
        || row.contract_version != envelope.contract_version
    {
        return Err(DeadLetterOperatorError::invalid(format!(
            "Dead letter `{dead_letter_id}` identity does not match its Event Envelope"
        )));
    }
    let pool = operator_store(state)?;
    let inbox = sqlx::query_as::<_, ReplayInboxRow>(
        r"
        select status, envelope, original_envelope
        from platform.service_event_inbox
        where consumer_id = $1 and event_id = $2
        ",
    )
    .bind(&row.consumer_id)
    .bind(&row.event_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        DeadLetterOperatorError::store("Could not validate replay Inbox state", error)
    })?
    .ok_or_else(|| {
        DeadLetterOperatorError::invalid(format!(
            "Dead letter `{dead_letter_id}` has no authoritative Inbox state"
        ))
    })?;
    if !matches!(
        (row.status.as_str(), inbox.status.as_str()),
        ("dead_lettered", "dead_lettered") | ("resolved", "completed")
    ) {
        return Err(DeadLetterOperatorError::invalid(format!(
            "Dead letter `{dead_letter_id}` and Inbox statuses are inconsistent"
        )));
    }
    let authoritative_envelope = inbox.original_envelope.as_ref().unwrap_or(&inbox.envelope);
    if authoritative_envelope != &row.envelope {
        return Err(DeadLetterOperatorError::invalid(format!(
            "Dead letter `{dead_letter_id}` does not match the authoritative Inbox envelope"
        )));
    }
    let transport_health = adapter
        .health()
        .await
        .map_err(|error| DeadLetterOperatorError {
            code: DeadLetterOperatorErrorCode::ReplayDeliveryFailed,
            message: error.message,
            next_actions: vec![
                "restore_transport".to_owned(),
                "plan_replay_again".to_owned(),
            ],
            source: None,
        })?;
    if transport_health.status != TransportHealthStatus::Ready {
        return Err(DeadLetterOperatorError {
            code: DeadLetterOperatorErrorCode::ReplayDeliveryFailed,
            message: "Transport Adapter is not ready for replay".to_owned(),
            next_actions: vec![
                "restore_transport".to_owned(),
                "plan_replay_again".to_owned(),
            ],
            source: None,
        });
    }
    let environment = state.identity.operator_environment;
    let expected_transport = match environment {
        DeadLetterOperatorEnvironment::LocalSandbox => TransportDeploymentClass::LocalSandbox,
        DeadLetterOperatorEnvironment::Production => TransportDeploymentClass::Production,
    };
    if adapter.deployment_class() != expected_transport {
        return Err(DeadLetterOperatorError::invalid(format!(
            "Transport Adapter deployment class does not match the {environment:?} runtime"
        )));
    }
    let (approval_boundary, authorization, next_action) = match environment {
        DeadLetterOperatorEnvironment::LocalSandbox => (
            ReplayApprovalBoundary::LocalSandboxOnly,
            DeadLetterAuthorization {
                status: DeadLetterAuthorizationStatus::NotRequired,
                required_authority: None,
            },
            "execute_local_replay",
        ),
        DeadLetterOperatorEnvironment::Production => (
            ReplayApprovalBoundary::ProductionReplay,
            DeadLetterAuthorization {
                status: DeadLetterAuthorizationStatus::Required,
                required_authority: Some("production_event_replay".to_owned()),
            },
            "request_production_approval",
        ),
    };
    Ok(DeadLetterReplayPlan {
        protocol: DEAD_LETTER_REPLAY_PLAN_PROTOCOL,
        plan_id: format!(
            "replay:{}:{}:{}:{}:{}",
            environment.as_str(),
            row.dead_letter_id,
            row.delivery_id,
            row.attempt_count,
            row.status
        ),
        mutates_state: false,
        dead_letter_id: row.dead_letter_id,
        affected_service_id: state.identity.service_id.clone(),
        consumer_id: row.consumer_id.clone(),
        original_delivery_id: row.delivery_id.clone(),
        identity: ReplayIdentity {
            event_id: envelope.event_id.clone(),
            contract_id: envelope.contract_id.clone(),
            contract_version: envelope.contract_version.clone(),
            story_context: envelope
                .context
                .story
                .as_ref()
                .and_then(|value| serde_json::to_value(value).ok()),
            causation: envelope
                .context
                .causation
                .as_ref()
                .and_then(|value| serde_json::to_value(value).ok()),
        },
        authorization,
        approval_boundary,
        intended_delivery: serde_json::json!({
            "consumerId": row.consumer_id,
            "preservesBusinessEventIdentity": true,
            "createsNewDeliveryAttempt": true,
            "originalDeliveryId": row.delivery_id,
        }),
        next_actions: vec![next_action.to_owned()],
        environment,
        validations: vec![
            DeadLetterReplayValidation {
                code: "dead_letter_identity_consistent".to_owned(),
                status: "passed".to_owned(),
            },
            DeadLetterReplayValidation {
                code: "inbox_state_authoritative".to_owned(),
                status: "passed".to_owned(),
            },
            DeadLetterReplayValidation {
                code: "transport_ready".to_owned(),
                status: "passed".to_owned(),
            },
            DeadLetterReplayValidation {
                code: "transport_environment_matches".to_owned(),
                status: "passed".to_owned(),
            },
        ],
    })
}

pub async fn replay_dead_letter(
    state: &ServiceRuntimeState,
    adapter: &dyn TransportAdapter,
    plan: &DeadLetterReplayPlan,
    approval: Option<&VerifiedDeadLetterApproval>,
) -> Result<DeadLetterReplayResult, DeadLetterOperatorError> {
    match plan.approval_request() {
        None if approval.is_some() => {
            return Err(DeadLetterOperatorError {
                code: DeadLetterOperatorErrorCode::ApprovalMismatch,
                message: "Production approval cannot authorize a local-sandbox replay".to_owned(),
                next_actions: vec!["execute_local_replay_without_approval".to_owned()],
                source: None,
            });
        }
        Some(_) if approval.is_none() => {
            return Err(DeadLetterOperatorError {
                code: DeadLetterOperatorErrorCode::ApprovalRequired,
                message: "Production event replay requires explicit approved authority".to_owned(),
                next_actions: vec!["request_production_approval".to_owned()],
                source: None,
            });
        }
        _ => {}
    }
    if let (Some(expected), Some(approval)) = (plan.approval_request(), approval)
        && approval.request != expected
    {
        return Err(DeadLetterOperatorError {
            code: DeadLetterOperatorErrorCode::ApprovalMismatch,
            message: "Verified approval does not authorize this replay plan".to_owned(),
            next_actions: vec!["verify_replay_approval".to_owned()],
            source: None,
        });
    }
    let current = plan_dead_letter_replay(state, adapter, &plan.dead_letter_id).await?;
    if current != *plan {
        return Err(DeadLetterOperatorError {
            code: DeadLetterOperatorErrorCode::StateChanged,
            message: "Dead-letter state changed after the replay plan was created".to_owned(),
            next_actions: vec!["plan_replay_again".to_owned()],
            source: None,
        });
    }
    let row = load_dead_letter(state, &plan.dead_letter_id).await?;
    let envelope: EventEnvelope = serde_json::from_value(row.envelope.clone()).map_err(|_| {
        DeadLetterOperatorError::invalid("Stored dead-letter Event Envelope is invalid")
    })?;
    let pool = operator_store(state)?;
    let replay_id = uuid::Uuid::new_v4().to_string();
    let replay_delivery_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now();
    let mut transaction = pool.begin().await.map_err(|error| {
        DeadLetterOperatorError::store("Could not begin dead-letter replay", error)
    })?;
    let activated = sqlx::query(
        r#"
        update platform.service_event_dead_letters
        set status = 'replay_active'
        where dead_letter_id = $1 and status <> 'replay_active'
        "#,
    )
    .bind(&row.dead_letter_id)
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        DeadLetterOperatorError::store("Could not activate dead-letter replay", error)
    })?;
    if activated.rows_affected() != 1 {
        return Err(DeadLetterOperatorError {
            code: DeadLetterOperatorErrorCode::ReplayActive,
            message: "Dead letter already has an active replay".to_owned(),
            next_actions: vec!["inspect_replay_status".to_owned()],
            source: None,
        });
    }
    sqlx::query(
        r#"
        update platform.service_event_inbox
        set status = 'retryable', next_attempt_at = null
        where consumer_id = $1 and event_id = $2 and status = 'dead_lettered'
        "#,
    )
    .bind(&row.consumer_id)
    .bind(&row.event_id)
    .execute(&mut *transaction)
    .await
    .map_err(|error| DeadLetterOperatorError::store("Could not prepare Inbox replay", error))?;
    sqlx::query(
        r#"
        insert into platform.service_event_replays (
            replay_id, dead_letter_id, consumer_id, event_id,
            original_delivery_id, replay_delivery_id, environment, approval_id,
            plan_id, status, created_at
        ) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'preparing', $10)
        "#,
    )
    .bind(&replay_id)
    .bind(&row.dead_letter_id)
    .bind(&row.consumer_id)
    .bind(&row.event_id)
    .bind(&row.delivery_id)
    .bind(&replay_delivery_id)
    .bind(plan.environment.as_str())
    .bind(approval.map(|value| value.approval_id.as_str()))
    .bind(&plan.plan_id)
    .bind(now)
    .execute(&mut *transaction)
    .await
    .map_err(|error| DeadLetterOperatorError::store("Could not record replay attempt", error))?;
    transaction.commit().await.map_err(|error| {
        DeadLetterOperatorError::store("Could not commit replay preparation", error)
    })?;

    let receipt = match adapter
        .publish_replay(
            TransportPublication {
                consumer_id: row.consumer_id.clone(),
                envelope: envelope.clone(),
            },
            &replay_delivery_id,
        )
        .await
    {
        Ok(receipt) => receipt,
        Err(error) => {
            let mut compensation = pool.begin().await.map_err(|store_error| {
                DeadLetterOperatorError::store("Could not begin replay recovery", store_error)
            })?;
            sqlx::query("update platform.service_event_replays set status = 'failed', completed_at = $2 where replay_id = $1")
                .bind(&replay_id)
                .bind(now)
                .execute(&mut *compensation)
                .await
                .map_err(|store_error| DeadLetterOperatorError::store("Could not record failed replay", store_error))?;
            sqlx::query("update platform.service_event_dead_letters set status = 'dead_lettered' where dead_letter_id = $1 and status = 'replay_active'")
                .bind(&row.dead_letter_id)
                .execute(&mut *compensation)
                .await
                .map_err(|store_error| DeadLetterOperatorError::store("Could not restore dead-letter state", store_error))?;
            sqlx::query("update platform.service_event_inbox set status = 'dead_lettered' where consumer_id = $1 and event_id = $2 and status = 'retryable'")
                .bind(&row.consumer_id)
                .bind(&row.event_id)
                .execute(&mut *compensation)
                .await
                .map_err(|store_error| DeadLetterOperatorError::store("Could not restore Inbox state", store_error))?;
            compensation.commit().await.map_err(|store_error| {
                DeadLetterOperatorError::store("Could not commit replay recovery", store_error)
            })?;
            return Err(DeadLetterOperatorError {
                code: DeadLetterOperatorErrorCode::ReplayDeliveryFailed,
                message: error.message,
                next_actions: vec![
                    "restore_transport".to_owned(),
                    "plan_replay_again".to_owned(),
                ],
                source: None,
            });
        }
    };
    if receipt.delivery_id != replay_delivery_id {
        return Err(DeadLetterOperatorError {
            code: DeadLetterOperatorErrorCode::ReplayDeliveryFailed,
            message: "Transport Adapter changed the durable replay delivery identity".to_owned(),
            next_actions: vec!["repair_transport_adapter".to_owned()],
            source: None,
        });
    }
    let mut transaction = pool.begin().await.map_err(|error| {
        DeadLetterOperatorError::store("Could not record replay delivery", error)
    })?;
    sqlx::query(
        "update platform.service_event_replays set status = 'published' where replay_id = $1 and replay_delivery_id = $2",
    )
    .bind(&replay_id)
    .bind(&receipt.delivery_id)
    .execute(&mut *transaction)
    .await
    .map_err(|error| DeadLetterOperatorError::store("Could not record replay delivery", error))?;
    sqlx::query(
        r#"
        insert into platform.service_event_delivery_evidence (
            evidence_id, stage, outcome, event_id, delivery_id, detail, recorded_at
        ) values ($1, 'replay', 'published', $2, $3, $4, $5)
        "#,
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&row.event_id)
    .bind(&receipt.delivery_id)
    .bind(serde_json::json!({
        "replayId": replay_id,
        "deadLetterId": row.dead_letter_id,
        "originalDeliveryId": row.delivery_id,
        "environment": plan.environment.as_str(),
        "approvalId": approval.map(|value| value.approval_id.as_str()),
    }))
    .bind(now)
    .execute(&mut *transaction)
    .await
    .map_err(|error| DeadLetterOperatorError::store("Could not record replay evidence", error))?;
    transaction.commit().await.map_err(|error| {
        DeadLetterOperatorError::store("Could not commit replay delivery", error)
    })?;
    Ok(DeadLetterReplayResult {
        protocol: DEAD_LETTER_REPLAY_RESULT_PROTOCOL,
        replay_id,
        dead_letter_id: row.dead_letter_id,
        event_id: envelope.event_id,
        contract_id: envelope.contract_id,
        contract_version: envelope.contract_version,
        delivery_id: receipt.delivery_id,
        outcome: "published".to_owned(),
        next_actions: vec![
            "consume_replay".to_owned(),
            "inspect_replay_status".to_owned(),
        ],
    })
}

pub async fn retain_dead_letter_until(
    state: &ServiceRuntimeState,
    dead_letter_id: &str,
    retained_until: chrono::DateTime<chrono::Utc>,
) -> Result<DeadLetterRetentionResult, DeadLetterOperatorError> {
    let row = load_dead_letter(state, dead_letter_id).await?;
    if retained_until <= row.dead_lettered_at {
        return Err(DeadLetterOperatorError::invalid(
            "Dead-letter retention must end after the dead-lettered time",
        ));
    }
    let pool = operator_store(state)?;
    let retained = sqlx::query(
        "update platform.service_event_dead_letters set retained_until = $2 where dead_letter_id = $1",
    )
    .bind(dead_letter_id)
    .bind(retained_until)
    .execute(pool)
    .await
    .map_err(|error| DeadLetterOperatorError::store("Could not retain Service dead letter", error))?;
    if retained.rows_affected() != 1 {
        return Err(DeadLetterOperatorError {
            code: DeadLetterOperatorErrorCode::StateChanged,
            message: "Dead letter changed while retention was being applied".to_owned(),
            next_actions: vec!["inspect_dead_letters".to_owned()],
            source: None,
        });
    }
    Ok(DeadLetterRetentionResult {
        protocol: DEAD_LETTER_RETENTION_RESULT_PROTOCOL,
        dead_letter_id: dead_letter_id.to_owned(),
        retained_until,
        outcome: "retained".to_owned(),
        next_actions: vec!["inspect_dead_letters".to_owned()],
    })
}

pub async fn plan_dead_letter_cleanup(
    state: &ServiceRuntimeState,
    cutoff: chrono::DateTime<chrono::Utc>,
) -> Result<DeadLetterCleanupPlan, DeadLetterOperatorError> {
    let pool = operator_store(state)?;
    let environment = state.identity.operator_environment;
    let dead_letter_ids = sqlx::query_scalar::<_, String>(
        r#"
        select dead_letter_id
        from platform.service_event_dead_letters
        where status = 'resolved'
          and coalesce(resolved_at, dead_lettered_at) <= $1
          and (retained_until is null or retained_until <= $1)
          and not exists (
              select 1
              from platform.service_event_replays replay
              where replay.dead_letter_id = service_event_dead_letters.dead_letter_id
                and replay.status in ('preparing', 'published')
          )
        order by dead_lettered_at, event_id, dead_letter_id
        "#,
    )
    .bind(cutoff)
    .fetch_all(pool)
    .await
    .map_err(|error| {
        DeadLetterOperatorError::store("Could not plan Service dead-letter cleanup", error)
    })?;
    let plan_id = format!(
        "cleanup:{}:{}:{}",
        environment.as_str(),
        cutoff.to_rfc3339(),
        dead_letter_ids.join(",")
    );
    Ok(DeadLetterCleanupPlan {
        protocol: DEAD_LETTER_CLEANUP_PLAN_PROTOCOL,
        plan_id,
        mutates_state: false,
        environment,
        service_id: state.identity.service_id.clone(),
        cutoff,
        dead_letter_ids,
        preserved_state: preserved_cleanup_state(),
        authorization: DeadLetterAuthorization {
            status: DeadLetterAuthorizationStatus::Required,
            required_authority: Some(
                match environment {
                    DeadLetterOperatorEnvironment::LocalSandbox => "dead_letter_cleanup",
                    DeadLetterOperatorEnvironment::Production => "production_dead_letter_cleanup",
                }
                .to_owned(),
            ),
        },
        approval_boundary: DeadLetterCleanupApprovalBoundary::DestructiveCleanup,
        next_actions: vec!["request_cleanup_approval".to_owned()],
    })
}

pub async fn cleanup_dead_letters(
    state: &ServiceRuntimeState,
    plan: &DeadLetterCleanupPlan,
    approval: Option<&VerifiedDeadLetterApproval>,
) -> Result<DeadLetterCleanupResult, DeadLetterOperatorError> {
    let Some(approval) = approval else {
        return Err(DeadLetterOperatorError {
            code: DeadLetterOperatorErrorCode::ApprovalRequired,
            message: "Destructive dead-letter cleanup requires explicit approved authority"
                .to_owned(),
            next_actions: vec!["request_cleanup_approval".to_owned()],
            source: None,
        });
    };
    if approval.request != plan.approval_request() {
        return Err(DeadLetterOperatorError {
            code: DeadLetterOperatorErrorCode::ApprovalMismatch,
            message: "Verified approval does not authorize this cleanup plan".to_owned(),
            next_actions: vec!["verify_cleanup_approval".to_owned()],
            source: None,
        });
    }
    let current = plan_dead_letter_cleanup(state, plan.cutoff).await?;
    if current != *plan {
        return Err(DeadLetterOperatorError {
            code: DeadLetterOperatorErrorCode::StateChanged,
            message: "Dead-letter state changed after the cleanup plan was created".to_owned(),
            next_actions: vec!["plan_cleanup_again".to_owned()],
            source: None,
        });
    }
    let pool = operator_store(state)?;
    let mut transaction = pool.begin().await.map_err(|error| {
        DeadLetterOperatorError::store("Could not begin dead-letter cleanup", error)
    })?;
    for dead_letter_id in &plan.dead_letter_ids {
        let event_id = sqlx::query_scalar::<_, String>(
            r#"
            select event_id
            from platform.service_event_dead_letters
            where dead_letter_id = $1 and status = 'resolved'
              and coalesce(resolved_at, dead_lettered_at) <= $2
              and (retained_until is null or retained_until <= $2)
            for update
            "#,
        )
        .bind(dead_letter_id)
        .bind(plan.cutoff)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| {
            DeadLetterOperatorError::store("Could not lock dead letter for cleanup", error)
        })?
        .ok_or_else(|| DeadLetterOperatorError {
            code: DeadLetterOperatorErrorCode::CleanupProtected,
            message: format!("Dead letter `{dead_letter_id}` is no longer safe to clean up"),
            next_actions: vec!["plan_cleanup_again".to_owned()],
            source: None,
        })?;
        let active_replay: bool = sqlx::query_scalar(
            r#"
            select exists (
                select 1 from platform.service_event_replays
                where dead_letter_id = $1 and status in ('preparing', 'published')
            )
            "#,
        )
        .bind(dead_letter_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| {
            DeadLetterOperatorError::store("Could not verify replay cleanup protection", error)
        })?;
        if active_replay {
            return Err(DeadLetterOperatorError {
                code: DeadLetterOperatorErrorCode::CleanupProtected,
                message: format!("Dead letter `{dead_letter_id}` has an active replay"),
                next_actions: vec!["inspect_replay_status".to_owned()],
                source: None,
            });
        }
        sqlx::query(
            r#"
            insert into platform.service_event_delivery_evidence (
                evidence_id, stage, outcome, event_id, detail
            ) values ($1, 'cleanup', 'dead_letter_removed', $2, $3)
            "#,
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&event_id)
        .bind(serde_json::json!({
            "deadLetterId": dead_letter_id,
            "approvalId": approval.approval_id,
            "environment": plan.environment.as_str(),
            "preservedState": preserved_cleanup_state(),
        }))
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            DeadLetterOperatorError::store("Could not record dead-letter cleanup evidence", error)
        })?;
        sqlx::query("delete from platform.service_event_dead_letters where dead_letter_id = $1")
            .bind(dead_letter_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| {
                DeadLetterOperatorError::store("Could not remove resolved dead letter", error)
            })?;
    }
    transaction.commit().await.map_err(|error| {
        DeadLetterOperatorError::store("Could not commit dead-letter cleanup", error)
    })?;
    Ok(DeadLetterCleanupResult {
        protocol: DEAD_LETTER_CLEANUP_RESULT_PROTOCOL,
        deleted_dead_letter_ids: plan.dead_letter_ids.clone(),
        preserved_state: preserved_cleanup_state(),
        outcome: "completed".to_owned(),
        next_actions: vec!["inspect_dead_letters".to_owned()],
    })
}

fn preserved_cleanup_state() -> Vec<String> {
    vec![
        "service_event_inbox".to_owned(),
        "service_event_delivery_evidence".to_owned(),
        "service_event_replays".to_owned(),
    ]
}
