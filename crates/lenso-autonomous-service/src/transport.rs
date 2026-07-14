use crate::ServiceRuntimeState;
use async_trait::async_trait;
use lenso_service::EventEnvelope;
use platform_core::{AppError, ErrorCode, Migration, apply_migrations};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use std::fmt::Debug;
use thiserror::Error;
use utoipa::ToSchema;
use uuid::Uuid;

const LOCAL_TRANSPORT_MIGRATIONS: &[Migration] = &[
    Migration {
        name: "autonomous-service/0004_create_local_transport",
        sql: include_str!("../migrations/0004_create_local_transport.sql"),
    },
    Migration {
        name: "autonomous-service/0007_schedule_local_transport_retry",
        sql: include_str!("../migrations/0007_schedule_local_transport_retry.sql"),
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportErrorCode {
    StoreUnavailable,
    InvalidEnvelope,
    DeliveryFailed,
    HandlerFailed,
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct TransportError {
    pub code: TransportErrorCode,
    pub message: String,
    #[source]
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl TransportError {
    #[must_use]
    pub fn new(code: TransportErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            source: None,
        }
    }

    fn store(message: impl Into<String>, error: sqlx::Error) -> Self {
        Self {
            code: TransportErrorCode::StoreUnavailable,
            message: message.into(),
            source: Some(Box::new(error)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportPublication {
    pub consumer_id: String,
    pub envelope: EventEnvelope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportPublicationReceipt {
    pub delivery_id: String,
    pub event_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportDelivery {
    pub delivery_id: String,
    pub consumer_id: String,
    pub envelope: EventEnvelope,
    pub attempt: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportNegativeAcknowledgement {
    pub reason: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportFailureDisposition {
    pub failure_reason: DeliveryFailureReason,
    pub reason_code: String,
    pub diagnostic: String,
    pub retry_at: Option<chrono::DateTime<chrono::Utc>>,
    pub terminal_outcome: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportHealthStatus {
    Ready,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportDeploymentClass {
    LocalSandbox,
    Production,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportHealth {
    pub adapter: String,
    pub status: TransportHealthStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TransportDiagnostic {
    pub delivery_id: String,
    pub event_id: String,
    pub outcome: String,
    pub detail: serde_json::Value,
    pub recorded_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServiceEventEvidence {
    pub evidence_id: String,
    pub stage: String,
    pub outcome: String,
    pub event_id: String,
    pub delivery_id: Option<String>,
    pub detail: serde_json::Value,
    pub recorded_at: chrono::DateTime<chrono::Utc>,
}

#[utoipa::path(
    get,
    path = "/runtime/event-deliveries",
    responses(
        (status = 200, body = [ServiceEventEvidence]),
        (status = 503, body = platform_http::ErrorResponse, content_type = "application/problem+json"),
        (status = 500, body = platform_http::ErrorResponse, content_type = "application/problem+json")
    ),
    tag = "service-runtime"
)]
#[allow(clippy::result_large_err)]
pub(crate) async fn event_delivery_evidence(
    axum::extract::State(state): axum::extract::State<ServiceRuntimeState>,
) -> Result<axum::Json<Vec<ServiceEventEvidence>>, platform_http::ApiErrorResponse> {
    let pool = state
        .store()
        .map_err(platform_http::ApiErrorResponse::from)?;
    sqlx::query_as::<_, ServiceEventEvidence>(
        r"
        select evidence_id, stage, outcome, event_id, delivery_id, detail, recorded_at
        from platform.service_event_delivery_evidence
        order by recorded_at desc, evidence_id
        limit 100
        ",
    )
    .fetch_all(pool)
    .await
    .map(axum::Json)
    .map_err(|error| {
        platform_http::ApiErrorResponse::from(
            AppError::new(
                ErrorCode::Internal,
                "Could not read local event delivery evidence",
            )
            .with_source(error),
        )
    })
}

#[async_trait]
pub trait TransportAdapter: Debug + Send + Sync {
    fn deployment_class(&self) -> TransportDeploymentClass {
        TransportDeploymentClass::LocalSandbox
    }

    async fn publish(
        &self,
        publication: TransportPublication,
    ) -> Result<TransportPublicationReceipt, TransportError>;

    /// Publishes a replay using a delivery identity durably allocated by the
    /// Service before the adapter is called. Adapters must either publish with
    /// exactly this identity or return before making the delivery visible.
    async fn publish_replay(
        &self,
        _publication: TransportPublication,
        _delivery_id: &str,
    ) -> Result<TransportPublicationReceipt, TransportError> {
        Err(TransportError::new(
            TransportErrorCode::DeliveryFailed,
            "Transport Adapter does not support durable replay publication",
        ))
    }

    async fn receive(
        &self,
        consumer_id: &str,
        limit: i64,
    ) -> Result<Vec<TransportDelivery>, TransportError>;

    async fn receive_at(
        &self,
        consumer_id: &str,
        limit: i64,
        _now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<TransportDelivery>, TransportError> {
        self.receive(consumer_id, limit).await
    }

    async fn acknowledge(&self, delivery: &TransportDelivery) -> Result<(), TransportError>;

    async fn negative_acknowledge(
        &self,
        delivery: &TransportDelivery,
        acknowledgement: TransportNegativeAcknowledgement,
    ) -> Result<(), TransportError>;

    async fn record_failure(
        &self,
        delivery: &TransportDelivery,
        disposition: TransportFailureDisposition,
    ) -> Result<(), TransportError> {
        self.negative_acknowledge(
            delivery,
            TransportNegativeAcknowledgement {
                reason: disposition.diagnostic,
                retryable: disposition.retry_at.is_some(),
            },
        )
        .await
    }

    async fn health(&self) -> Result<TransportHealth, TransportError>;

    async fn diagnostics(&self) -> Result<Vec<TransportDiagnostic>, TransportError>;
}

#[derive(Debug, Clone)]
pub struct LocalTransportAdapter {
    pool: PgPool,
}

impl LocalTransportAdapter {
    pub async fn prepare(pool: PgPool) -> Result<Self, TransportError> {
        apply_migrations(&pool, LOCAL_TRANSPORT_MIGRATIONS)
            .await
            .map_err(|error| TransportError {
                code: TransportErrorCode::StoreUnavailable,
                message: "Local Transport Adapter Store migration failed".to_owned(),
                source: Some(Box::new(error)),
            })?;
        let adapter = Self { pool };
        adapter.recover_unacknowledged().await?;
        Ok(adapter)
    }

    async fn publish_with_delivery_id(
        &self,
        publication: TransportPublication,
        delivery_id: String,
    ) -> Result<TransportPublicationReceipt, TransportError> {
        let envelope =
            serde_json::to_value(&publication.envelope).map_err(|error| TransportError {
                code: TransportErrorCode::InvalidEnvelope,
                message: "Event Envelope could not be serialized".to_owned(),
                source: Some(Box::new(error)),
            })?;
        let mut transaction = self.pool.begin().await.map_err(|error| {
            TransportError::store("Could not begin local transport publication", error)
        })?;
        sqlx::query(
            r"
            insert into platform.local_transport_deliveries (
                delivery_id, consumer_id, event_id, envelope, status
            ) values ($1, $2, $3, $4, 'available')
            ",
        )
        .bind(&delivery_id)
        .bind(&publication.consumer_id)
        .bind(&publication.envelope.event_id)
        .bind(envelope)
        .execute(&mut *transaction)
        .await
        .map_err(|error| TransportError::store("Could not publish Event Envelope", error))?;
        insert_transport_diagnostic(
            &mut transaction,
            &delivery_id,
            &publication.envelope.event_id,
            "published",
            json!({"consumerId": publication.consumer_id}),
        )
        .await?;
        transaction.commit().await.map_err(|error| {
            TransportError::store("Could not commit local transport publication", error)
        })?;
        Ok(TransportPublicationReceipt {
            delivery_id,
            event_id: publication.envelope.event_id,
        })
    }

    async fn recover_unacknowledged(&self) -> Result<(), TransportError> {
        let mut transaction = self.pool.begin().await.map_err(|error| {
            TransportError::store("Could not begin local transport recovery", error)
        })?;
        let recovered = sqlx::query_as::<_, RecoveredLocalDeliveryRow>(
            r"
            update platform.local_transport_deliveries
            set status = 'available', updated_at = now()
            where status = 'received'
            returning delivery_id, event_id, attempts
            ",
        )
        .fetch_all(&mut *transaction)
        .await
        .map_err(|error| {
            TransportError::store(
                "Could not recover unacknowledged transport deliveries",
                error,
            )
        })?;
        for delivery in recovered {
            insert_transport_diagnostic(
                &mut transaction,
                &delivery.delivery_id,
                &delivery.event_id,
                "recovered_unacknowledged",
                json!({"previousAttempt": delivery.attempts}),
            )
            .await?;
        }
        transaction.commit().await.map_err(|error| {
            TransportError::store("Could not commit local transport recovery", error)
        })
    }

    async fn record_diagnostic(
        &self,
        delivery_id: &str,
        event_id: &str,
        outcome: &str,
        detail: serde_json::Value,
    ) -> Result<(), TransportError> {
        let mut transaction = self.pool.begin().await.map_err(|error| {
            TransportError::store("Could not begin transport diagnostic", error)
        })?;
        insert_transport_diagnostic(&mut transaction, delivery_id, event_id, outcome, detail)
            .await?;
        transaction
            .commit()
            .await
            .map_err(|error| TransportError::store("Could not commit transport diagnostic", error))
    }
}

#[derive(Debug, FromRow)]
struct LocalDeliveryRow {
    delivery_id: String,
    consumer_id: String,
    envelope: serde_json::Value,
    attempts: i32,
}

#[derive(Debug, FromRow)]
struct RecoveredLocalDeliveryRow {
    delivery_id: String,
    event_id: String,
    attempts: i32,
}

#[async_trait]
impl TransportAdapter for LocalTransportAdapter {
    async fn publish(
        &self,
        publication: TransportPublication,
    ) -> Result<TransportPublicationReceipt, TransportError> {
        self.publish_with_delivery_id(publication, Uuid::new_v4().to_string())
            .await
    }

    async fn publish_replay(
        &self,
        publication: TransportPublication,
        delivery_id: &str,
    ) -> Result<TransportPublicationReceipt, TransportError> {
        self.publish_with_delivery_id(publication, delivery_id.to_owned())
            .await
    }

    async fn receive(
        &self,
        consumer_id: &str,
        limit: i64,
    ) -> Result<Vec<TransportDelivery>, TransportError> {
        self.receive_at(consumer_id, limit, chrono::Utc::now())
            .await
    }

    async fn receive_at(
        &self,
        consumer_id: &str,
        limit: i64,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<TransportDelivery>, TransportError> {
        let rows = sqlx::query_as::<_, LocalDeliveryRow>(
            r"
            with claimed as (
                select delivery_id
                from platform.local_transport_deliveries
                where consumer_id = $1 and status = 'available' and available_at <= $3
                order by created_at, delivery_id
                limit $2
                for update skip locked
            )
            update platform.local_transport_deliveries delivery
            set status = 'received', attempts = attempts + 1, updated_at = now()
            from claimed
            where delivery.delivery_id = claimed.delivery_id
            returning delivery.delivery_id, delivery.consumer_id, delivery.envelope, delivery.attempts
            ",
        )
        .bind(consumer_id)
        .bind(limit)
        .bind(now)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| TransportError::store("Could not receive Event Envelopes", error))?;

        rows.into_iter()
            .map(|row| {
                let envelope =
                    serde_json::from_value(row.envelope).map_err(|error| TransportError {
                        code: TransportErrorCode::InvalidEnvelope,
                        message: "Stored Event Envelope could not be decoded".to_owned(),
                        source: Some(Box::new(error)),
                    })?;
                Ok(TransportDelivery {
                    delivery_id: row.delivery_id,
                    consumer_id: row.consumer_id,
                    envelope,
                    attempt: u32::try_from(row.attempts).unwrap_or_default(),
                })
            })
            .collect()
    }

    async fn acknowledge(&self, delivery: &TransportDelivery) -> Result<(), TransportError> {
        sqlx::query(
            "update platform.local_transport_deliveries set status = 'acknowledged', updated_at = now() where delivery_id = $1",
        )
        .bind(&delivery.delivery_id)
        .execute(&self.pool)
        .await
        .map_err(|error| TransportError::store("Could not acknowledge transport delivery", error))?;
        self.record_diagnostic(
            &delivery.delivery_id,
            &delivery.envelope.event_id,
            "acknowledged",
            json!({"attempt": delivery.attempt}),
        )
        .await
    }

    async fn negative_acknowledge(
        &self,
        delivery: &TransportDelivery,
        acknowledgement: TransportNegativeAcknowledgement,
    ) -> Result<(), TransportError> {
        let status = if acknowledgement.retryable {
            "available"
        } else {
            "rejected"
        };
        sqlx::query(
            "update platform.local_transport_deliveries set status = $2, last_error = $3, updated_at = now() where delivery_id = $1",
        )
        .bind(&delivery.delivery_id)
        .bind(status)
        .bind(&acknowledgement.reason)
        .execute(&self.pool)
        .await
        .map_err(|error| {
            TransportError::store("Could not negatively acknowledge transport delivery", error)
        })?;
        self.record_diagnostic(
            &delivery.delivery_id,
            &delivery.envelope.event_id,
            "negative_acknowledged",
            json!({
                "reason": acknowledgement.reason,
                "retryable": acknowledgement.retryable,
            }),
        )
        .await
    }

    async fn record_failure(
        &self,
        delivery: &TransportDelivery,
        disposition: TransportFailureDisposition,
    ) -> Result<(), TransportError> {
        let retryable = disposition.retry_at.is_some();
        let status = if retryable { "available" } else { "rejected" };
        let available_at = disposition.retry_at.unwrap_or_else(chrono::Utc::now);
        sqlx::query(
            r"
            update platform.local_transport_deliveries
            set status = $2, last_error = $3, available_at = $4,
                failure_reason = $5, reason_code = $6, terminal_outcome = $7,
                updated_at = now()
            where delivery_id = $1
            ",
        )
        .bind(&delivery.delivery_id)
        .bind(status)
        .bind(&disposition.diagnostic)
        .bind(available_at)
        .bind(disposition.failure_reason.as_str())
        .bind(&disposition.reason_code)
        .bind(&disposition.terminal_outcome)
        .execute(&self.pool)
        .await
        .map_err(|error| TransportError::store("Could not persist transport failure", error))?;
        self.record_diagnostic(
            &delivery.delivery_id,
            &delivery.envelope.event_id,
            "failure_recorded",
            json!({
                "failureReason": disposition.failure_reason,
                "reasonCode": disposition.reason_code,
                "diagnostic": disposition.diagnostic,
                "retryable": retryable,
                "nextAttemptAt": disposition.retry_at,
                "terminalOutcome": disposition.terminal_outcome,
            }),
        )
        .await
    }

    async fn health(&self) -> Result<TransportHealth, TransportError> {
        sqlx::query_scalar::<_, i32>("select 1")
            .fetch_one(&self.pool)
            .await
            .map_err(|error| {
                TransportError::store("Local transport Store is unavailable", error)
            })?;
        Ok(TransportHealth {
            adapter: "local".to_owned(),
            status: TransportHealthStatus::Ready,
        })
    }

    async fn diagnostics(&self) -> Result<Vec<TransportDiagnostic>, TransportError> {
        sqlx::query_as(
            r"
            select delivery_id, event_id, outcome, detail, recorded_at
            from platform.local_transport_diagnostics
            order by recorded_at, diagnostic_id
            ",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| TransportError::store("Could not inspect transport diagnostics", error))
    }
}

async fn insert_transport_diagnostic(
    transaction: &mut Transaction<'_, Postgres>,
    delivery_id: &str,
    event_id: &str,
    outcome: &str,
    detail: serde_json::Value,
) -> Result<(), TransportError> {
    sqlx::query(
        r"
        insert into platform.local_transport_diagnostics (
            diagnostic_id, delivery_id, event_id, outcome, detail
        ) values ($1, $2, $3, $4, $5)
        ",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(delivery_id)
    .bind(event_id)
    .bind(outcome)
    .bind(detail)
    .execute(&mut **transaction)
    .await
    .map_err(|error| TransportError::store("Could not persist transport diagnostic", error))?;
    Ok(())
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ServiceEventPublisher;

impl ServiceEventPublisher {
    pub async fn publish_in_tx(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        consumer_id: &str,
        envelope: &EventEnvelope,
    ) -> Result<(), TransportError> {
        let envelope_json = serde_json::to_value(envelope).map_err(|error| TransportError {
            code: TransportErrorCode::InvalidEnvelope,
            message: "Event Envelope could not be serialized for the Service Outbox".to_owned(),
            source: Some(Box::new(error)),
        })?;
        sqlx::query(
            r"
            insert into platform.service_event_outbox (
                event_id, consumer_id, envelope, status
            ) values ($1, $2, $3, 'pending')
            ",
        )
        .bind(&envelope.event_id)
        .bind(consumer_id)
        .bind(envelope_json)
        .execute(&mut **transaction)
        .await
        .map_err(|error| {
            TransportError::store("Could not record Service Outbox publication intent", error)
        })?;
        record_service_evidence(
            transaction,
            "outbox",
            "pending",
            &envelope.event_id,
            None,
            json!({"consumerId": consumer_id}),
        )
        .await?;
        Ok(())
    }
}

#[derive(Debug, FromRow)]
struct ServiceOutboxRow {
    event_id: String,
    consumer_id: String,
    envelope: serde_json::Value,
}

#[derive(Debug, FromRow)]
struct ExistingInboxRow {
    delivery_id: String,
    status: String,
    next_attempt_at: Option<chrono::DateTime<chrono::Utc>>,
    failure_reason: Option<String>,
    reason_code: Option<String>,
    last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InboxStatus {
    Received,
    Retryable,
    Completed,
    Rejected,
    Failed,
    DeadLettered,
}

impl InboxStatus {
    fn parse(value: &str) -> Result<Self, TransportError> {
        match value {
            "received" => Ok(Self::Received),
            "retryable" => Ok(Self::Retryable),
            "completed" => Ok(Self::Completed),
            "rejected" => Ok(Self::Rejected),
            "failed" => Ok(Self::Failed),
            "dead_lettered" => Ok(Self::DeadLettered),
            _ => Err(TransportError::new(
                TransportErrorCode::DeliveryFailed,
                format!("Service Inbox contains unsupported status `{value}`"),
            )),
        }
    }

    const fn can_retry(self) -> bool {
        matches!(self, Self::Received | Self::Retryable)
    }
}

pub async fn relay_service_events_once(
    state: &ServiceRuntimeState,
    adapter: &dyn TransportAdapter,
    limit: i64,
) -> Result<usize, TransportError> {
    let pool = state.transport_store()?;
    let rows = sqlx::query_as::<_, ServiceOutboxRow>(
        r"
        select event_id, consumer_id, envelope
        from platform.service_event_outbox
        where status in ('pending', 'failed')
        order by created_at, event_id
        limit $1
        ",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|error| TransportError::store("Could not inspect Service Outbox", error))?;

    let mut delivered = 0;
    for row in rows {
        let envelope: EventEnvelope =
            serde_json::from_value(row.envelope).map_err(|error| TransportError {
                code: TransportErrorCode::InvalidEnvelope,
                message: format!(
                    "Service Outbox event `{}` could not be decoded",
                    row.event_id
                ),
                source: Some(Box::new(error)),
            })?;
        match adapter
            .publish(TransportPublication {
                consumer_id: row.consumer_id,
                envelope,
            })
            .await
        {
            Ok(receipt) => {
                let mut transaction = pool.begin().await.map_err(|error| {
                    TransportError::store("Could not update Service Outbox", error)
                })?;
                sqlx::query(
                    r"
                    update platform.service_event_outbox
                    set status = 'published', attempts = attempts + 1,
                        transport_message_id = $2, last_error = null,
                        published_at = now(), updated_at = now()
                    where event_id = $1
                    ",
                )
                .bind(&row.event_id)
                .bind(&receipt.delivery_id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| {
                    TransportError::store("Could not complete Service Outbox", error)
                })?;
                record_service_evidence(
                    &mut transaction,
                    "outbox",
                    "published",
                    &row.event_id,
                    Some(&receipt.delivery_id),
                    json!({}),
                )
                .await?;
                transaction.commit().await.map_err(|error| {
                    TransportError::store("Could not commit Service Outbox outcome", error)
                })?;
                delivered += 1;
            }
            Err(error) => {
                sqlx::query(
                    r"
                    update platform.service_event_outbox
                    set status = 'failed', attempts = attempts + 1,
                        last_error = $2, updated_at = now()
                    where event_id = $1
                    ",
                )
                .bind(&row.event_id)
                .bind(&error.message)
                .execute(pool)
                .await
                .map_err(|store_error| {
                    TransportError::store("Could not persist Service Outbox failure", store_error)
                })?;
                return Err(error);
            }
        }
    }
    Ok(delivered)
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct ServiceEventHandlerError {
    pub code: ServiceEventHandlerErrorCode,
    pub failure_reason: DeliveryFailureReason,
    pub reason_code: String,
    pub message: String,
    #[source]
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceEventHandlerErrorCode {
    Retryable,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryFailureReason {
    Retryable,
    NonRetryable,
    Expired,
    Unauthorized,
    Incompatible,
    Poison,
    Exhausted,
}

impl DeliveryFailureReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Retryable => "retryable",
            Self::NonRetryable => "non_retryable",
            Self::Expired => "expired",
            Self::Unauthorized => "unauthorized",
            Self::Incompatible => "incompatible",
            Self::Poison => "poison",
            Self::Exhausted => "exhausted",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "retryable" => Some(Self::Retryable),
            "non_retryable" => Some(Self::NonRetryable),
            "expired" => Some(Self::Expired),
            "unauthorized" => Some(Self::Unauthorized),
            "incompatible" => Some(Self::Incompatible),
            "poison" => Some(Self::Poison),
            "exhausted" => Some(Self::Exhausted),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceEventRetryPolicy {
    max_attempts: u32,
    retry_delays: Vec<chrono::Duration>,
}

impl ServiceEventRetryPolicy {
    #[must_use]
    pub fn new(max_attempts: u32, retry_delays: Vec<chrono::Duration>) -> Self {
        assert!(
            max_attempts > 0,
            "event retry policy requires at least one attempt"
        );
        assert!(
            retry_delays
                .iter()
                .all(|delay| *delay >= chrono::Duration::zero()),
            "event retry delays must not be negative"
        );
        Self {
            max_attempts,
            retry_delays,
        }
    }

    fn is_exhausted(&self, attempt: u32) -> bool {
        attempt >= self.max_attempts
    }

    fn persisted_schedule(&self) -> serde_json::Value {
        json!(
            self.retry_delays
                .iter()
                .map(chrono::Duration::num_milliseconds)
                .collect::<Vec<_>>()
        )
    }

    fn from_persisted(max_attempts: i32, schedule: &serde_json::Value) -> Option<Self> {
        let max_attempts = u32::try_from(max_attempts).ok()?;
        let retry_delays = schedule
            .as_array()?
            .iter()
            .map(|delay| delay.as_i64().map(chrono::Duration::milliseconds))
            .collect::<Option<Vec<_>>>()?;
        (max_attempts > 0
            && retry_delays
                .iter()
                .all(|delay| *delay >= chrono::Duration::zero()))
        .then_some(Self {
            max_attempts,
            retry_delays,
        })
    }

    fn retry_at(
        &self,
        attempt: u32,
        now: chrono::DateTime<chrono::Utc>,
    ) -> chrono::DateTime<chrono::Utc> {
        let delay_index = usize::try_from(attempt.saturating_sub(1)).unwrap_or(usize::MAX);
        let delay = self
            .retry_delays
            .get(delay_index)
            .or_else(|| self.retry_delays.last())
            .copied()
            .unwrap_or_default();
        now + delay
    }
}

impl Default for ServiceEventRetryPolicy {
    fn default() -> Self {
        Self::new(
            3,
            vec![chrono::Duration::seconds(1), chrono::Duration::seconds(5)],
        )
    }
}

impl ServiceEventHandlerError {
    pub fn store(error: sqlx::Error) -> Self {
        Self {
            code: ServiceEventHandlerErrorCode::Retryable,
            failure_reason: DeliveryFailureReason::Retryable,
            reason_code: "business_effect_store_unavailable".to_owned(),
            message: "Module-owned event behavior could not persist its business effect".to_owned(),
            source: Some(Box::new(error)),
        }
    }

    #[must_use]
    pub fn retryable(reason_code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: ServiceEventHandlerErrorCode::Retryable,
            failure_reason: DeliveryFailureReason::Retryable,
            reason_code: reason_code.into(),
            message: message.into(),
            source: None,
        }
    }

    #[must_use]
    pub fn rejected(message: impl Into<String>) -> Self {
        Self::rejected_with_code("event_rejected", message)
    }

    #[must_use]
    pub fn rejected_with_code(reason_code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::terminal(DeliveryFailureReason::NonRetryable, reason_code, message)
    }

    #[must_use]
    pub fn expired(reason_code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::terminal(DeliveryFailureReason::Expired, reason_code, message)
    }

    #[must_use]
    pub fn unauthorized(reason_code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::terminal(DeliveryFailureReason::Unauthorized, reason_code, message)
    }

    #[must_use]
    pub fn incompatible(reason_code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::terminal(DeliveryFailureReason::Incompatible, reason_code, message)
    }

    #[must_use]
    pub fn poison(reason_code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::terminal(DeliveryFailureReason::Poison, reason_code, message)
    }

    fn terminal(
        failure_reason: DeliveryFailureReason,
        reason_code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code: ServiceEventHandlerErrorCode::Rejected,
            failure_reason,
            reason_code: reason_code.into(),
            message: message.into(),
            source: None,
        }
    }
}

#[async_trait]
pub trait ServiceEventHandler: Debug + Send + Sync {
    async fn handle(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        envelope: &EventEnvelope,
    ) -> Result<(), ServiceEventHandlerError>;
}

pub async fn consume_service_events_once(
    state: &ServiceRuntimeState,
    adapter: &dyn TransportAdapter,
    consumer_id: &str,
    handler: &dyn ServiceEventHandler,
    limit: i64,
) -> Result<usize, TransportError> {
    consume_service_events_once_at(
        state,
        adapter,
        consumer_id,
        handler,
        limit,
        chrono::Utc::now(),
        &ServiceEventRetryPolicy::default(),
    )
    .await
}

pub async fn consume_service_events_once_at(
    state: &ServiceRuntimeState,
    adapter: &dyn TransportAdapter,
    consumer_id: &str,
    handler: &dyn ServiceEventHandler,
    limit: i64,
    now: chrono::DateTime<chrono::Utc>,
    retry_policy: &ServiceEventRetryPolicy,
) -> Result<usize, TransportError> {
    let pool = state.transport_store()?;
    let deliveries = adapter.receive_at(consumer_id, limit, now).await?;
    let mut completed = 0;
    let mut first_handler_failure = None;
    for delivery in deliveries {
        let envelope_json =
            serde_json::to_value(&delivery.envelope).map_err(|error| TransportError {
                code: TransportErrorCode::InvalidEnvelope,
                message: "Received Event Envelope could not be serialized".to_owned(),
                source: Some(Box::new(error)),
            })?;
        let mut transaction = pool.begin().await.map_err(|error| {
            TransportError::store("Could not begin Service Inbox transaction", error)
        })?;
        let inserted = sqlx::query_scalar::<_, String>(
            r"
            insert into platform.service_event_inbox (
                delivery_id, consumer_id, event_id, envelope, status
            ) values ($1, $2, $3, $4, 'received')
            on conflict (consumer_id, event_id) do nothing
            returning delivery_id
            ",
        )
        .bind(&delivery.delivery_id)
        .bind(consumer_id)
        .bind(&delivery.envelope.event_id)
        .bind(envelope_json.clone())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| TransportError::store("Could not record Service Inbox receipt", error))?;

        if inserted.is_none() {
            let existing = sqlx::query_as::<_, ExistingInboxRow>(
                r"
                select delivery_id, status, next_attempt_at, failure_reason,
                       reason_code, last_error
                from platform.service_event_inbox
                where consumer_id = $1 and event_id = $2
                for update
                ",
            )
            .bind(consumer_id)
            .bind(&delivery.envelope.event_id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(|error| {
                TransportError::store("Could not inspect duplicate Service Inbox event", error)
            })?;
            let existing_status = InboxStatus::parse(&existing.status)?;
            if existing_status.can_retry() {
                if let Some(next_attempt_at) = existing.next_attempt_at
                    && next_attempt_at > now
                {
                    record_service_evidence(
                        &mut transaction,
                        "inbox",
                        "retry_deferred",
                        &delivery.envelope.event_id,
                        Some(&delivery.delivery_id),
                        json!({
                            "nextAttemptAt": next_attempt_at,
                            "receivedAt": now,
                        }),
                    )
                    .await?;
                    transaction.commit().await.map_err(|error| {
                        TransportError::store("Could not commit deferred retry evidence", error)
                    })?;
                    adapter
                        .record_failure(
                            &delivery,
                            TransportFailureDisposition {
                                failure_reason: existing
                                    .failure_reason
                                    .as_deref()
                                    .and_then(DeliveryFailureReason::parse)
                                    .unwrap_or(DeliveryFailureReason::Retryable),
                                reason_code: existing
                                    .reason_code
                                    .unwrap_or_else(|| "retry_not_due".to_owned()),
                                diagnostic: existing
                                    .last_error
                                    .unwrap_or_else(|| "Scheduled retry is not due".to_owned()),
                                retry_at: Some(next_attempt_at),
                                terminal_outcome: None,
                            },
                        )
                        .await?;
                    continue;
                }
                let retried = sqlx::query(
                    r"
                    update platform.service_event_inbox
                    set delivery_id = $3, envelope = $4, status = 'received',
                        last_error = null, received_at = now(), completed_at = null
                    where consumer_id = $1 and event_id = $2
                      and status in ('received', 'retryable')
                    ",
                )
                .bind(consumer_id)
                .bind(&delivery.envelope.event_id)
                .bind(&delivery.delivery_id)
                .bind(envelope_json)
                .execute(&mut *transaction)
                .await
                .map_err(|error| {
                    TransportError::store("Could not retry Service Inbox event", error)
                })?;
                if retried.rows_affected() != 1 {
                    return Err(TransportError::new(
                        TransportErrorCode::DeliveryFailed,
                        "Service Inbox retry lost its state transition",
                    ));
                }
                record_service_evidence(
                    &mut transaction,
                    "inbox",
                    "retrying",
                    &delivery.envelope.event_id,
                    Some(&delivery.delivery_id),
                    json!({
                        "attempt": delivery.attempt,
                        "previousDeliveryId": existing.delivery_id,
                    }),
                )
                .await?;
            } else {
                record_service_evidence(
                    &mut transaction,
                    "inbox",
                    "duplicate",
                    &delivery.envelope.event_id,
                    Some(&delivery.delivery_id),
                    json!({
                        "attempt": delivery.attempt,
                        "originalDeliveryId": existing.delivery_id,
                        "originalStatus": existing.status,
                    }),
                )
                .await?;
                if existing_status == InboxStatus::Completed {
                    complete_active_replay(
                        &mut transaction,
                        consumer_id,
                        &delivery.envelope.event_id,
                        &delivery.delivery_id,
                        now,
                        true,
                    )
                    .await?;
                }
                transaction.commit().await.map_err(|error| {
                    TransportError::store(
                        "Could not commit duplicate Service Inbox evidence",
                        error,
                    )
                })?;
                acknowledge_service_delivery(
                    pool,
                    adapter,
                    &delivery,
                    ServiceDeliveryAcknowledgement::Duplicate,
                )
                .await?;
                continue;
            }
        }
        record_service_evidence(
            &mut transaction,
            "inbox",
            "received",
            &delivery.envelope.event_id,
            Some(&delivery.delivery_id),
            json!({"attempt": delivery.attempt}),
        )
        .await?;

        if let Err(handler_error) = handler.handle(&mut transaction, &delivery.envelope).await {
            transaction.rollback().await.map_err(|error| {
                TransportError::store("Could not roll back failed Service Inbox handling", error)
            })?;
            let handler_outcome_persistence = persist_handler_outcome(
                pool,
                &delivery,
                consumer_id,
                &handler_error,
                now,
                retry_policy,
            )
            .await?;
            let HandlerOutcomePersistence::Persisted(disposition) = handler_outcome_persistence
            else {
                acknowledge_service_delivery(
                    pool,
                    adapter,
                    &delivery,
                    ServiceDeliveryAcknowledgement::Duplicate,
                )
                .await?;
                continue;
            };
            adapter
                .record_failure(
                    &delivery,
                    TransportFailureDisposition {
                        failure_reason: disposition.failure_reason,
                        reason_code: handler_error.reason_code.clone(),
                        diagnostic: handler_error.message.clone(),
                        retry_at: disposition.next_attempt_at,
                        terminal_outcome: disposition.terminal_outcome.map(str::to_owned),
                    },
                )
                .await?;
            if disposition.terminal_outcome.is_some() {
                continue;
            }
            if first_handler_failure.is_none() {
                first_handler_failure = Some(TransportError {
                    code: TransportErrorCode::HandlerFailed,
                    message: handler_error.message,
                    source: handler_error.source,
                });
            }
            continue;
        }

        let completed_inbox = sqlx::query(
            r"
            update platform.service_event_inbox
            set status = 'completed', completed_at = $4, attempt_count = attempt_count + 1,
                next_attempt_at = null, failure_reason = null, reason_code = null,
                terminal_outcome = 'completed',
                delivery_history = delivery_history || jsonb_build_array(jsonb_build_object(
                    'attempt', attempt_count + 1,
                    'deliveryId', $1::text,
                    'outcome', 'completed',
                    'recordedAt', $4::timestamptz
                ))
            where delivery_id = $1 and consumer_id = $2 and event_id = $3
              and status = 'received'
            ",
        )
        .bind(&delivery.delivery_id)
        .bind(consumer_id)
        .bind(&delivery.envelope.event_id)
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(|error| TransportError::store("Could not complete Service Inbox event", error))?;
        if completed_inbox.rows_affected() != 1 {
            return Err(TransportError::new(
                TransportErrorCode::DeliveryFailed,
                "Service Inbox completion lost its state transition",
            ));
        }
        complete_active_replay(
            &mut transaction,
            consumer_id,
            &delivery.envelope.event_id,
            &delivery.delivery_id,
            now,
            false,
        )
        .await?;
        transaction.commit().await.map_err(|error| {
            TransportError::store("Could not commit Service Inbox business effect", error)
        })?;
        acknowledge_service_delivery(
            pool,
            adapter,
            &delivery,
            ServiceDeliveryAcknowledgement::Processed,
        )
        .await?;
        completed += 1;
    }
    first_handler_failure.map_or(Ok(completed), Err)
}

async fn complete_active_replay(
    transaction: &mut Transaction<'_, Postgres>,
    consumer_id: &str,
    event_id: &str,
    delivery_id: &str,
    completed_at: chrono::DateTime<chrono::Utc>,
    duplicate: bool,
) -> Result<(), TransportError> {
    let resolved = sqlx::query(
        r#"
        update platform.service_event_dead_letters dead_letter
        set status = 'resolved', resolved_at = $3
        where consumer_id = $1 and event_id = $2 and status = 'replay_active'
          and exists (
              select 1
              from platform.service_event_replays replay
              where replay.dead_letter_id = dead_letter.dead_letter_id
                and replay.replay_delivery_id = $4
                and replay.status in ('preparing', 'published')
          )
        "#,
    )
    .bind(consumer_id)
    .bind(event_id)
    .bind(completed_at)
    .bind(delivery_id)
    .execute(&mut **transaction)
    .await
    .map_err(|error| TransportError::store("Could not resolve replayed dead letter", error))?;
    if resolved.rows_affected() == 0 {
        return Ok(());
    }
    let replay_status = if duplicate {
        "duplicate_completed"
    } else {
        "completed"
    };
    sqlx::query(
        r#"
        update platform.service_event_replays
        set status = $3, completed_at = $4
        where consumer_id = $1 and event_id = $2
          and replay_delivery_id = $5
          and status in ('preparing', 'published')
        "#,
    )
    .bind(consumer_id)
    .bind(event_id)
    .bind(replay_status)
    .bind(completed_at)
    .bind(delivery_id)
    .execute(&mut **transaction)
    .await
    .map_err(|error| TransportError::store("Could not complete replay audit", error))?;
    record_service_evidence(
        transaction,
        "replay",
        replay_status,
        event_id,
        None,
        json!({"duplicate": duplicate}),
    )
    .await
}

async fn acknowledge_service_delivery(
    pool: &PgPool,
    adapter: &dyn TransportAdapter,
    delivery: &TransportDelivery,
    acknowledgement: ServiceDeliveryAcknowledgement,
) -> Result<(), TransportError> {
    if let Err(error) = adapter.acknowledge(delivery).await {
        record_service_evidence_in_store(
            pool,
            "delivery",
            "acknowledgement_failed",
            &delivery.envelope.event_id,
            Some(&delivery.delivery_id),
            json!({
                "reason": error.message,
                "duplicate": acknowledgement.is_duplicate(),
            }),
        )
        .await?;
        return Err(error);
    }
    record_service_evidence_in_store(
        pool,
        "delivery",
        "acknowledged",
        &delivery.envelope.event_id,
        Some(&delivery.delivery_id),
        json!({"duplicate": acknowledgement.is_duplicate()}),
    )
    .await
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServiceDeliveryAcknowledgement {
    Processed,
    Duplicate,
}

impl ServiceDeliveryAcknowledgement {
    const fn is_duplicate(self) -> bool {
        matches!(self, Self::Duplicate)
    }
}

async fn persist_handler_outcome(
    pool: &PgPool,
    delivery: &TransportDelivery,
    consumer_id: &str,
    error: &ServiceEventHandlerError,
    now: chrono::DateTime<chrono::Utc>,
    retry_policy: &ServiceEventRetryPolicy,
) -> Result<HandlerOutcomePersistence, TransportError> {
    let mut transaction = pool.begin().await.map_err(|error| {
        TransportError::store("Could not persist Service Inbox handler outcome", error)
    })?;
    let existing = sqlx::query_as::<_, ExistingHandlerOutcome>(
        r"
        select status, attempt_count, delivery_history, original_envelope,
               max_attempts, retry_schedule
        from platform.service_event_inbox
        where consumer_id = $1 and event_id = $2
        for update
        ",
    )
    .bind(consumer_id)
    .bind(&delivery.envelope.event_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| TransportError::store("Could not inspect failed Inbox event", error))?;
    if existing
        .as_ref()
        .is_some_and(|row| !InboxStatus::parse(&row.status).is_ok_and(InboxStatus::can_retry))
    {
        record_service_evidence(
            &mut transaction,
            "inbox",
            "superseded_handler_outcome",
            &delivery.envelope.event_id,
            Some(&delivery.delivery_id),
            json!({
                "failureReason": error.failure_reason,
                "reasonCode": error.reason_code,
                "diagnostic": error.message,
            }),
        )
        .await?;
        transaction.commit().await.map_err(|error| {
            TransportError::store("Could not commit superseded Inbox handler outcome", error)
        })?;
        return Ok(HandlerOutcomePersistence::Superseded);
    }

    let effective_policy = existing
        .as_ref()
        .and_then(|row| {
            ServiceEventRetryPolicy::from_persisted(row.max_attempts?, row.retry_schedule.as_ref()?)
        })
        .unwrap_or_else(|| retry_policy.clone());
    let attempt = existing.as_ref().map_or(1, |row| row.attempt_count + 1);
    let exhausted = error.failure_reason == DeliveryFailureReason::Retryable
        && effective_policy.is_exhausted(u32::try_from(attempt).unwrap_or(u32::MAX));
    let failure_reason = if exhausted {
        DeliveryFailureReason::Exhausted
    } else {
        error.failure_reason
    };
    let dead_lettered = matches!(
        failure_reason,
        DeliveryFailureReason::Poison | DeliveryFailureReason::Exhausted
    );
    let retryable = failure_reason == DeliveryFailureReason::Retryable;
    let (status, outcome, terminal_outcome, next_attempt_at) = if retryable {
        (
            "retryable",
            "retry_scheduled",
            None,
            Some(effective_policy.retry_at(u32::try_from(attempt).unwrap_or(u32::MAX), now)),
        )
    } else if dead_lettered {
        (
            "dead_lettered",
            "dead_lettered",
            Some("dead_lettered"),
            None,
        )
    } else {
        ("rejected", "rejected", Some("rejected"), None)
    };
    let envelope = serde_json::to_value(&delivery.envelope).map_err(|error| TransportError {
        code: TransportErrorCode::InvalidEnvelope,
        message: "Failed Event Envelope could not be serialized".to_owned(),
        source: Some(Box::new(error)),
    })?;
    let original_envelope = existing
        .as_ref()
        .and_then(|row| row.original_envelope.clone())
        .unwrap_or_else(|| envelope.clone());
    let original_contract_id = original_envelope["contractId"]
        .as_str()
        .unwrap_or(&delivery.envelope.contract_id);
    let original_contract_version = original_envelope["contractVersion"]
        .as_str()
        .unwrap_or(&delivery.envelope.contract_version);
    let mut delivery_history = existing
        .as_ref()
        .map(|row| row.delivery_history.clone())
        .unwrap_or_else(|| json!([]));
    delivery_history
        .as_array_mut()
        .expect("Inbox delivery history migration guarantees a JSON array")
        .push(json!({
            "attempt": attempt,
            "deliveryId": delivery.delivery_id,
            "outcome": outcome,
            "failureReason": failure_reason,
            "reasonCode": error.reason_code,
            "diagnostic": error.message,
            "recordedAt": now,
            "nextAttemptAt": next_attempt_at,
        }));
    let persisted = sqlx::query(
        r"
        insert into platform.service_event_inbox (
            delivery_id, consumer_id, event_id, envelope, status, last_error,
            attempt_count, next_attempt_at, failure_reason, reason_code,
            terminal_outcome, delivery_history, received_at, original_envelope,
            max_attempts, retry_schedule
        ) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                  $14, $15, $16)
        on conflict (consumer_id, event_id) do update
        set delivery_id = excluded.delivery_id,
            envelope = excluded.envelope,
            status = excluded.status,
            last_error = excluded.last_error,
            attempt_count = excluded.attempt_count,
            next_attempt_at = excluded.next_attempt_at,
            failure_reason = excluded.failure_reason,
            reason_code = excluded.reason_code,
            terminal_outcome = excluded.terminal_outcome,
            delivery_history = excluded.delivery_history,
            received_at = excluded.received_at,
            completed_at = case when excluded.terminal_outcome is null then null else $13 end
        where platform.service_event_inbox.status in ('received', 'retryable')
        ",
    )
    .bind(&delivery.delivery_id)
    .bind(consumer_id)
    .bind(&delivery.envelope.event_id)
    .bind(&envelope)
    .bind(status)
    .bind(&error.message)
    .bind(attempt)
    .bind(next_attempt_at)
    .bind(failure_reason.as_str())
    .bind(&error.reason_code)
    .bind(terminal_outcome)
    .bind(&delivery_history)
    .bind(now)
    .bind(&original_envelope)
    .bind(i32::try_from(effective_policy.max_attempts).unwrap_or(i32::MAX))
    .bind(effective_policy.persisted_schedule())
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        TransportError::store("Could not persist Service Inbox handler outcome", error)
    })?;
    debug_assert_eq!(persisted.rows_affected(), 1);

    if dead_lettered {
        let next_actions = match failure_reason {
            DeliveryFailureReason::Poison => {
                json!(["inspect_payload", "correct_producer", "replay_event"])
            }
            DeliveryFailureReason::Exhausted => {
                json!(["inspect_dependency", "restore_service", "replay_event"])
            }
            _ => unreachable!("only poison and exhausted events enter dead-letter state"),
        };
        sqlx::query(
            r"
            insert into platform.service_event_dead_letters (
                dead_letter_id, consumer_id, event_id, delivery_id, envelope,
                contract_id, contract_version, failure_reason, reason_code,
                diagnostic, attempt_count, terminal_outcome, delivery_history,
                max_attempts, retry_schedule, next_actions, dead_lettered_at
            ) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11,
                      'dead_lettered', $12, $13, $14, $15, $16)
            on conflict (consumer_id, event_id) do nothing
            ",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(consumer_id)
        .bind(&delivery.envelope.event_id)
        .bind(&delivery.delivery_id)
        .bind(&original_envelope)
        .bind(original_contract_id)
        .bind(original_contract_version)
        .bind(failure_reason.as_str())
        .bind(&error.reason_code)
        .bind(&error.message)
        .bind(attempt)
        .bind(&delivery_history)
        .bind(i32::try_from(effective_policy.max_attempts).unwrap_or(i32::MAX))
        .bind(effective_policy.persisted_schedule())
        .bind(next_actions)
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(|error| TransportError::store("Could not persist Service dead letter", error))?;
    }
    record_service_evidence(
        &mut transaction,
        "inbox",
        outcome,
        &delivery.envelope.event_id,
        Some(&delivery.delivery_id),
        json!({
            "attempt": attempt,
            "failureReason": failure_reason,
            "reasonCode": error.reason_code,
            "diagnostic": error.message,
            "retryable": retryable,
            "nextAttemptAt": next_attempt_at,
            "terminalOutcome": terminal_outcome,
            "maxAttempts": effective_policy.max_attempts,
            "retrySchedule": effective_policy.persisted_schedule(),
        }),
    )
    .await?;
    transaction.commit().await.map_err(|error| {
        TransportError::store("Could not commit Service Inbox handler outcome", error)
    })?;
    Ok(HandlerOutcomePersistence::Persisted(
        HandlerFailureDisposition {
            failure_reason,
            next_attempt_at,
            terminal_outcome,
        },
    ))
}

#[derive(Debug, FromRow)]
struct ExistingHandlerOutcome {
    status: String,
    attempt_count: i32,
    delivery_history: serde_json::Value,
    original_envelope: Option<serde_json::Value>,
    max_attempts: Option<i32>,
    retry_schedule: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HandlerFailureDisposition {
    failure_reason: DeliveryFailureReason,
    next_attempt_at: Option<chrono::DateTime<chrono::Utc>>,
    terminal_outcome: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HandlerOutcomePersistence {
    Persisted(HandlerFailureDisposition),
    Superseded,
}

async fn record_service_evidence(
    transaction: &mut Transaction<'_, Postgres>,
    stage: &str,
    outcome: &str,
    event_id: &str,
    delivery_id: Option<&str>,
    detail: serde_json::Value,
) -> Result<(), TransportError> {
    sqlx::query(
        r"
        insert into platform.service_event_delivery_evidence (
            evidence_id, stage, outcome, event_id, delivery_id, detail
        ) values ($1, $2, $3, $4, $5, $6)
        ",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(stage)
    .bind(outcome)
    .bind(event_id)
    .bind(delivery_id)
    .bind(detail)
    .execute(&mut **transaction)
    .await
    .map_err(|error| TransportError::store("Could not persist Service event evidence", error))?;
    Ok(())
}

async fn record_service_evidence_in_store(
    pool: &PgPool,
    stage: &str,
    outcome: &str,
    event_id: &str,
    delivery_id: Option<&str>,
    detail: serde_json::Value,
) -> Result<(), TransportError> {
    let mut transaction = pool
        .begin()
        .await
        .map_err(|error| TransportError::store("Could not begin Service event evidence", error))?;
    record_service_evidence(
        &mut transaction,
        stage,
        outcome,
        event_id,
        delivery_id,
        detail,
    )
    .await?;
    transaction
        .commit()
        .await
        .map_err(|error| TransportError::store("Could not commit Service event evidence", error))
}

impl ServiceRuntimeState {
    fn transport_store(&self) -> Result<&PgPool, TransportError> {
        self.store().map_err(|error: AppError| TransportError {
            code: TransportErrorCode::StoreUnavailable,
            message: error.public_message,
            source: None,
        })
    }
}

impl From<TransportError> for AppError {
    fn from(error: TransportError) -> Self {
        AppError::new(ErrorCode::ExternalDependency, error.message)
    }
}
