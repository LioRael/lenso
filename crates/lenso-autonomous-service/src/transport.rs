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

const LOCAL_TRANSPORT_MIGRATIONS: &[Migration] = &[Migration {
    name: "autonomous-service/0004_create_local_transport",
    sql: include_str!("../migrations/0004_create_local_transport.sql"),
}];

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportHealthStatus {
    Ready,
    Unavailable,
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
    async fn publish(
        &self,
        publication: TransportPublication,
    ) -> Result<TransportPublicationReceipt, TransportError>;

    async fn receive(
        &self,
        consumer_id: &str,
        limit: i64,
    ) -> Result<Vec<TransportDelivery>, TransportError>;

    async fn acknowledge(&self, delivery: &TransportDelivery) -> Result<(), TransportError>;

    async fn negative_acknowledge(
        &self,
        delivery: &TransportDelivery,
        acknowledgement: TransportNegativeAcknowledgement,
    ) -> Result<(), TransportError>;

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
        let delivery_id = Uuid::new_v4().to_string();
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

    async fn receive(
        &self,
        consumer_id: &str,
        limit: i64,
    ) -> Result<Vec<TransportDelivery>, TransportError> {
        let rows = sqlx::query_as::<_, LocalDeliveryRow>(
            r"
            with claimed as (
                select delivery_id
                from platform.local_transport_deliveries
                where consumer_id = $1 and status = 'available'
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

impl ServiceEventHandlerError {
    pub fn store(error: sqlx::Error) -> Self {
        Self {
            code: ServiceEventHandlerErrorCode::Retryable,
            reason_code: "business_effect_store_unavailable".to_owned(),
            message: "Module-owned event behavior could not persist its business effect".to_owned(),
            source: Some(Box::new(error)),
        }
    }

    #[must_use]
    pub fn rejected(message: impl Into<String>) -> Self {
        Self::rejected_with_code("event_rejected", message)
    }

    #[must_use]
    pub fn rejected_with_code(reason_code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: ServiceEventHandlerErrorCode::Rejected,
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
    let pool = state.transport_store()?;
    let deliveries = adapter.receive(consumer_id, limit).await?;
    let mut completed = 0;
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
                select delivery_id, status
                from platform.service_event_inbox
                where consumer_id = $1 and event_id = $2
                ",
            )
            .bind(consumer_id)
            .bind(&delivery.envelope.event_id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(|error| {
                TransportError::store("Could not inspect duplicate Service Inbox event", error)
            })?;
            if existing.status == "retryable" {
                sqlx::query(
                    r"
                    update platform.service_event_inbox
                    set delivery_id = $3, envelope = $4, status = 'received',
                        last_error = null, received_at = now(), completed_at = null
                    where consumer_id = $1 and event_id = $2
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
                        "completedDeliveryId": existing.delivery_id,
                        "completedStatus": existing.status,
                    }),
                )
                .await?;
                transaction.commit().await.map_err(|error| {
                    TransportError::store(
                        "Could not commit duplicate Service Inbox evidence",
                        error,
                    )
                })?;
                acknowledge_service_delivery(pool, adapter, &delivery, true).await?;
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
            persist_handler_outcome(pool, &delivery, consumer_id, &handler_error).await?;
            adapter
                .negative_acknowledge(
                    &delivery,
                    TransportNegativeAcknowledgement {
                        reason: handler_error.message.clone(),
                        retryable: handler_error.code == ServiceEventHandlerErrorCode::Retryable,
                    },
                )
                .await?;
            return Err(TransportError {
                code: TransportErrorCode::HandlerFailed,
                message: handler_error.message,
                source: handler_error.source,
            });
        }

        sqlx::query(
            r"
            update platform.service_event_inbox
            set status = 'completed', completed_at = now()
            where delivery_id = $1
            ",
        )
        .bind(&delivery.delivery_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| TransportError::store("Could not complete Service Inbox event", error))?;
        transaction.commit().await.map_err(|error| {
            TransportError::store("Could not commit Service Inbox business effect", error)
        })?;
        acknowledge_service_delivery(pool, adapter, &delivery, false).await?;
        completed += 1;
    }
    Ok(completed)
}

async fn acknowledge_service_delivery(
    pool: &PgPool,
    adapter: &dyn TransportAdapter,
    delivery: &TransportDelivery,
    duplicate: bool,
) -> Result<(), TransportError> {
    if let Err(error) = adapter.acknowledge(delivery).await {
        record_service_evidence_in_store(
            pool,
            "delivery",
            "acknowledgement_failed",
            &delivery.envelope.event_id,
            Some(&delivery.delivery_id),
            json!({"reason": error.message, "duplicate": duplicate}),
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
        json!({"duplicate": duplicate}),
    )
    .await
}

async fn persist_handler_outcome(
    pool: &PgPool,
    delivery: &TransportDelivery,
    consumer_id: &str,
    error: &ServiceEventHandlerError,
) -> Result<(), TransportError> {
    let (status, outcome) = match error.code {
        ServiceEventHandlerErrorCode::Retryable => ("retryable", "retryable_failed"),
        ServiceEventHandlerErrorCode::Rejected => ("rejected", "rejected"),
    };
    let mut transaction = pool.begin().await.map_err(|error| {
        TransportError::store("Could not persist Service Inbox handler outcome", error)
    })?;
    sqlx::query(
        r"
        insert into platform.service_event_inbox (
            delivery_id, consumer_id, event_id, envelope, status, last_error
        ) values ($1, $2, $3, $4, $5, $6)
        on conflict (consumer_id, event_id) do update
        set delivery_id = excluded.delivery_id,
            envelope = excluded.envelope,
            status = excluded.status,
            last_error = excluded.last_error,
            received_at = now(),
            completed_at = null
        ",
    )
    .bind(&delivery.delivery_id)
    .bind(consumer_id)
    .bind(&delivery.envelope.event_id)
    .bind(
        serde_json::to_value(&delivery.envelope).map_err(|error| TransportError {
            code: TransportErrorCode::InvalidEnvelope,
            message: "Failed Event Envelope could not be serialized".to_owned(),
            source: Some(Box::new(error)),
        })?,
    )
    .bind(status)
    .bind(&error.message)
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        TransportError::store("Could not persist Service Inbox handler outcome", error)
    })?;
    record_service_evidence(
        &mut transaction,
        "inbox",
        outcome,
        &delivery.envelope.event_id,
        Some(&delivery.delivery_id),
        json!({
            "reasonCode": error.reason_code,
            "reason": error.message,
            "retryable": error.code == ServiceEventHandlerErrorCode::Retryable,
        }),
    )
    .await?;
    transaction.commit().await.map_err(|error| {
        TransportError::store("Could not commit Service Inbox handler outcome", error)
    })?;
    Ok(())
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
