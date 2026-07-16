use crate::{
    TransportAdapter, TransportDelivery, TransportDeploymentClass, TransportDiagnostic,
    TransportError, TransportErrorCode, TransportFailureDisposition, TransportHealth,
    TransportHealthStatus, TransportNegativeAcknowledgement, TransportPublication,
    TransportPublicationReceipt,
};
use async_nats::{
    Client,
    jetstream::{self, AckKind, consumer::PullConsumer, message::Acker, stream::Stream},
};
use async_trait::async_trait;
use futures::StreamExt as _;
use lenso_service::{AuthenticatedTransportBinding, EventEnvelope};
use platform_core::{Migration, apply_migrations};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use std::{collections::BTreeMap, fmt, sync::Mutex, time::Duration};
use uuid::Uuid;

const NATS_JETSTREAM_MIGRATIONS: &[Migration] = &[Migration {
    name: "autonomous-service/0009_create_nats_jetstream_transport_diagnostics",
    sql: include_str!("../migrations/0009_create_nats_jetstream_transport_diagnostics.sql"),
}];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NatsJetStreamConsumerBinding {
    pub subject: String,
    pub durable_consumer_name: String,
}

/// Operator-supplied topology for an already provisioned JetStream stream and
/// durable consumers. Credential resolution and infrastructure provisioning
/// intentionally remain outside the adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NatsJetStreamTransportConfig {
    pub stream_name: String,
    pub consumers: BTreeMap<String, NatsJetStreamConsumerBinding>,
    pub authenticated_transport_binding: AuthenticatedTransportBinding,
    pub receive_timeout: Duration,
}

impl NatsJetStreamTransportConfig {
    #[must_use]
    pub fn new(
        stream_name: impl Into<String>,
        authenticated_transport_binding: AuthenticatedTransportBinding,
    ) -> Self {
        Self {
            stream_name: stream_name.into(),
            consumers: BTreeMap::new(),
            authenticated_transport_binding,
            receive_timeout: Duration::from_millis(250),
        }
    }

    #[must_use]
    pub fn with_consumer(
        mut self,
        consumer_id: impl Into<String>,
        binding: NatsJetStreamConsumerBinding,
    ) -> Self {
        self.consumers.insert(consumer_id.into(), binding);
        self
    }

    #[must_use]
    pub const fn with_receive_timeout(mut self, receive_timeout: Duration) -> Self {
        self.receive_timeout = receive_timeout;
        self
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NatsJetStreamEnvelope {
    delivery_id: String,
    consumer_id: String,
    envelope: EventEnvelope,
}

struct PendingAcknowledgement {
    event_id: String,
    acker: Acker,
}

/// Production Transport Adapter backed by an operator-provisioned NATS
/// JetStream stream and durable pull consumers.
pub struct NatsJetStreamTransportAdapter {
    client: Client,
    stream: Stream,
    diagnostic_pool: PgPool,
    config: NatsJetStreamTransportConfig,
    pending_acknowledgements: Mutex<BTreeMap<String, PendingAcknowledgement>>,
}

impl fmt::Debug for NatsJetStreamTransportAdapter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NatsJetStreamTransportAdapter")
            .field("stream_name", &self.config.stream_name)
            .field("consumers", &self.config.consumers)
            .finish_non_exhaustive()
    }
}

impl NatsJetStreamTransportAdapter {
    pub const DEPLOYMENT_CLASS: TransportDeploymentClass = TransportDeploymentClass::Production;

    /// Binds to topology that an operator has already provisioned. This method
    /// validates the stream and every durable consumer but never creates,
    /// updates, or deletes JetStream infrastructure.
    pub async fn bind(
        client: Client,
        diagnostic_pool: PgPool,
        config: NatsJetStreamTransportConfig,
    ) -> Result<Self, TransportError> {
        apply_migrations(&diagnostic_pool, NATS_JETSTREAM_MIGRATIONS)
            .await
            .map_err(|error| {
                transport_error(
                    TransportErrorCode::StoreUnavailable,
                    "NATS JetStream diagnostic Store migration failed",
                    error,
                )
            })?;
        let context = jetstream::new(client.clone());
        let stream = context
            .get_stream(&config.stream_name)
            .await
            .map_err(|error| {
                transport_error(
                    TransportErrorCode::StoreUnavailable,
                    "Could not bind the provisioned NATS JetStream stream",
                    error,
                )
            })?;
        for (consumer_id, binding) in &config.consumers {
            let consumer: PullConsumer = stream
                .get_consumer(&binding.durable_consumer_name)
                .await
                .map_err(|error| {
                    transport_error(
                        TransportErrorCode::StoreUnavailable,
                        format!(
                            "Could not bind provisioned NATS JetStream consumer for {consumer_id}"
                        ),
                        error,
                    )
                })?;
            if consumer.cached_info().config.filter_subject != binding.subject {
                return Err(TransportError::new(
                    TransportErrorCode::DeliveryFailed,
                    format!(
                        "NATS JetStream consumer {} does not filter subject {}",
                        binding.durable_consumer_name, binding.subject
                    ),
                ));
            }
        }
        Ok(Self {
            client,
            stream,
            diagnostic_pool,
            config,
            pending_acknowledgements: Mutex::new(BTreeMap::new()),
        })
    }

    fn binding(&self, consumer_id: &str) -> Result<&NatsJetStreamConsumerBinding, TransportError> {
        self.config.consumers.get(consumer_id).ok_or_else(|| {
            TransportError::new(
                TransportErrorCode::DeliveryFailed,
                format!("No NATS JetStream topology binding exists for {consumer_id}"),
            )
        })
    }

    async fn publish_with_delivery_id(
        &self,
        publication: TransportPublication,
        delivery_id: String,
    ) -> Result<TransportPublicationReceipt, TransportError> {
        let subject = self.binding(&publication.consumer_id)?.subject.clone();
        let event_id = publication.envelope.event_id.clone();
        let payload = serde_json::to_vec(&NatsJetStreamEnvelope {
            delivery_id: delivery_id.clone(),
            consumer_id: publication.consumer_id.clone(),
            envelope: publication.envelope,
        })
        .map_err(|error| {
            transport_error(
                TransportErrorCode::InvalidEnvelope,
                "Event Envelope could not be serialized for NATS JetStream",
                error,
            )
        })?;
        let acknowledgement = jetstream::new(self.client.clone())
            .send_publish(
                subject,
                jetstream::message::PublishMessage::build()
                    .payload(payload.into())
                    .message_id(delivery_id.clone()),
            )
            .await
            .map_err(|error| {
                transport_error(
                    TransportErrorCode::DeliveryFailed,
                    "Could not publish Event Envelope to NATS JetStream",
                    error,
                )
            })?;
        acknowledgement.await.map_err(|error| {
            transport_error(
                TransportErrorCode::DeliveryFailed,
                "NATS JetStream did not acknowledge Event Envelope publication",
                error,
            )
        })?;
        self.record_diagnostic(
            &delivery_id,
            &event_id,
            "published",
            json!({"consumerId": publication.consumer_id}),
        )
        .await?;
        Ok(TransportPublicationReceipt {
            delivery_id,
            event_id,
        })
    }

    fn take_acknowledgement(
        &self,
        delivery_id: &str,
    ) -> Result<PendingAcknowledgement, TransportError> {
        self.pending_acknowledgements
            .lock()
            .expect("NATS JetStream acknowledgement lock poisoned")
            .remove(delivery_id)
            .ok_or_else(|| {
                TransportError::new(
                    TransportErrorCode::DeliveryFailed,
                    format!("No pending NATS JetStream delivery exists for {delivery_id}"),
                )
            })
    }

    async fn record_diagnostic(
        &self,
        delivery_id: &str,
        event_id: &str,
        outcome: &str,
        detail: serde_json::Value,
    ) -> Result<(), TransportError> {
        sqlx::query(
            r"
            insert into platform.nats_jetstream_transport_diagnostics (
                diagnostic_id, delivery_id, event_id, outcome, detail
            ) values ($1, $2, $3, $4, $5)
            ",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(delivery_id)
        .bind(event_id)
        .bind(outcome)
        .bind(detail)
        .execute(&self.diagnostic_pool)
        .await
        .map_err(|error| {
            transport_error(
                TransportErrorCode::StoreUnavailable,
                "Could not retain NATS JetStream transport diagnostic",
                error,
            )
        })?;
        Ok(())
    }
}

#[async_trait]
impl TransportAdapter for NatsJetStreamTransportAdapter {
    fn deployment_class(&self) -> TransportDeploymentClass {
        Self::DEPLOYMENT_CLASS
    }

    async fn publish(
        &self,
        publication: TransportPublication,
    ) -> Result<TransportPublicationReceipt, TransportError> {
        self.publish_with_delivery_id(publication, Uuid::now_v7().to_string())
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
        if limit <= 0 {
            return Ok(Vec::new());
        }
        let binding = self.binding(consumer_id)?;
        let consumer: PullConsumer = self
            .stream
            .get_consumer(&binding.durable_consumer_name)
            .await
            .map_err(|error| {
                transport_error(
                    TransportErrorCode::StoreUnavailable,
                    "Could not load NATS JetStream durable consumer",
                    error,
                )
            })?;
        let batch = usize::try_from(limit).unwrap_or(usize::MAX);
        let mut messages = consumer
            .fetch()
            .max_messages(batch)
            .expires(self.config.receive_timeout)
            .messages()
            .await
            .map_err(|error| {
                transport_error(
                    TransportErrorCode::DeliveryFailed,
                    "Could not request NATS JetStream deliveries",
                    error,
                )
            })?;
        let mut deliveries = Vec::new();
        while let Some(message) = messages.next().await {
            let message = message.map_err(|error| {
                transport_error(
                    TransportErrorCode::DeliveryFailed,
                    "Could not receive NATS JetStream delivery",
                    error,
                )
            })?;
            let attempt = u32::try_from(
                message
                    .info()
                    .map_err(|error| {
                        transport_error(
                            TransportErrorCode::DeliveryFailed,
                            "NATS JetStream delivery metadata was invalid",
                            error,
                        )
                    })?
                    .delivered,
            )
            .unwrap_or(u32::MAX);
            let (message, acker) = message.split();
            let wire: NatsJetStreamEnvelope =
                serde_json::from_slice(&message.payload).map_err(|error| {
                    transport_error(
                        TransportErrorCode::InvalidEnvelope,
                        "NATS JetStream delivery did not contain a valid Event Envelope",
                        error,
                    )
                })?;
            if wire.consumer_id != consumer_id {
                return Err(TransportError::new(
                    TransportErrorCode::InvalidEnvelope,
                    "NATS JetStream delivery consumer did not match its topology binding",
                ));
            }
            self.record_diagnostic(
                &wire.delivery_id,
                &wire.envelope.event_id,
                "received",
                json!({"attempt": attempt, "consumerId": consumer_id}),
            )
            .await?;
            self.pending_acknowledgements
                .lock()
                .expect("NATS JetStream acknowledgement lock poisoned")
                .insert(
                    wire.delivery_id.clone(),
                    PendingAcknowledgement {
                        event_id: wire.envelope.event_id.clone(),
                        acker,
                    },
                );
            deliveries.push(TransportDelivery {
                delivery_id: wire.delivery_id,
                consumer_id: wire.consumer_id,
                envelope: wire.envelope,
                attempt,
                authenticated_transport_binding: self
                    .config
                    .authenticated_transport_binding
                    .clone(),
            });
        }
        Ok(deliveries)
    }

    async fn acknowledge(&self, delivery: &TransportDelivery) -> Result<(), TransportError> {
        let pending = self.take_acknowledgement(&delivery.delivery_id)?;
        pending.acker.double_ack().await.map_err(|error| {
            transport_error(
                TransportErrorCode::DeliveryFailed,
                "Could not acknowledge NATS JetStream delivery",
                error,
            )
        })?;
        self.record_diagnostic(
            &delivery.delivery_id,
            &pending.event_id,
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
        let pending = self.take_acknowledgement(&delivery.delivery_id)?;
        let ack_kind = if acknowledgement.retryable {
            AckKind::Nak(None)
        } else {
            AckKind::Term
        };
        pending.acker.ack_with(ack_kind).await.map_err(|error| {
            transport_error(
                TransportErrorCode::DeliveryFailed,
                "Could not negatively acknowledge NATS JetStream delivery",
                error,
            )
        })?;
        self.record_diagnostic(
            &delivery.delivery_id,
            &pending.event_id,
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
        let pending = self.take_acknowledgement(&delivery.delivery_id)?;
        let acknowledgement = disposition.retry_at.map_or(AckKind::Term, |retry_at| {
            let delay = (retry_at - chrono::Utc::now())
                .to_std()
                .unwrap_or(Duration::ZERO);
            AckKind::Nak(Some(delay))
        });
        pending
            .acker
            .ack_with(acknowledgement)
            .await
            .map_err(|error| {
                transport_error(
                    TransportErrorCode::DeliveryFailed,
                    "Could not record NATS JetStream delivery failure",
                    error,
                )
            })?;
        self.record_diagnostic(
            &delivery.delivery_id,
            &pending.event_id,
            "failure_recorded",
            json!({
                "failureReason": disposition.failure_reason,
                "reasonCode": disposition.reason_code,
                "diagnostic": disposition.diagnostic,
                "nextAttemptAt": disposition.retry_at,
                "terminalOutcome": disposition.terminal_outcome,
            }),
        )
        .await
    }

    async fn health(&self) -> Result<TransportHealth, TransportError> {
        let status = if self.client.connection_state() == async_nats::connection::State::Connected {
            match jetstream::new(self.client.clone())
                .get_stream(&self.config.stream_name)
                .await
            {
                Ok(_) => TransportHealthStatus::Ready,
                Err(_) => TransportHealthStatus::Unavailable,
            }
        } else {
            TransportHealthStatus::Unavailable
        };
        Ok(TransportHealth {
            adapter: "nats_jetstream".to_owned(),
            status,
        })
    }

    async fn diagnostics(&self) -> Result<Vec<TransportDiagnostic>, TransportError> {
        sqlx::query_as(
            r"
            select delivery_id, event_id, outcome, detail, recorded_at
            from platform.nats_jetstream_transport_diagnostics
            order by recorded_at, diagnostic_id
            ",
        )
        .fetch_all(&self.diagnostic_pool)
        .await
        .map_err(|error| {
            transport_error(
                TransportErrorCode::StoreUnavailable,
                "Could not inspect retained NATS JetStream diagnostics",
                error,
            )
        })
    }
}

fn transport_error(
    code: TransportErrorCode,
    context: impl fmt::Display,
    error: impl fmt::Display,
) -> TransportError {
    TransportError::new(code, format!("{context}: {error}"))
}
