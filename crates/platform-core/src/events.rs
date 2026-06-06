use crate::context::{ActorContext, CorrelationId, TenantId, TraceContext};
use crate::error::AppResult;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::Debug;

pub type EventPayload = Value;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EventEnvelope {
    pub event_id: String,
    pub event_name: String,
    pub event_version: u16,
    pub source_module: String,
    pub subject: String,
    pub tenant_id: Option<TenantId>,
    pub actor: ActorContext,
    pub occurred_at: DateTime<Utc>,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<String>,
    pub trace: TraceContext,
    pub payload: EventPayload,
    pub schema_ref: String,
}

#[async_trait]
pub trait EventPublisher: Debug + Send + Sync {
    async fn publish(&self, event: EventEnvelope) -> AppResult<()>;
}

#[derive(Debug, Default)]
pub struct NoopEventPublisher;

#[async_trait]
impl EventPublisher for NoopEventPublisher {
    async fn publish(&self, _event: EventEnvelope) -> AppResult<()> {
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct LoggingEventPublisher;

#[async_trait]
impl EventPublisher for LoggingEventPublisher {
    async fn publish(&self, event: EventEnvelope) -> AppResult<()> {
        tracing::info!(
            event_id = %event.event_id,
            event_name = %event.event_name,
            source_module = %event.source_module,
            subject = %event.subject,
            correlation_id = %event.correlation_id.0,
            "module event published"
        );
        Ok(())
    }
}
