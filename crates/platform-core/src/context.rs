use crate::clock::{Clock, SystemClock};
use crate::config::AppConfig;
use crate::db::DbPool;
use crate::events::EventPublisher;
use crate::health::HealthRegistry;
use crate::ids::{IdGenerator, UuidGenerator};
use crate::shutdown::Shutdown;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct CorrelationId(pub String);

impl CorrelationId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct RequestId(pub String);

impl RequestId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct TenantId(pub String);

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct TraceContext {
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub baggage: Vec<(String, String)>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActorContext {
    Anonymous,
    User {
        user_id: String,
        scopes: Vec<String>,
    },
    Service {
        service_id: String,
        scopes: Vec<String>,
    },
    System,
}

impl Default for ActorContext {
    fn default() -> Self {
        Self::Anonymous
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RequestContext {
    pub request_id: RequestId,
    pub correlation_id: CorrelationId,
    pub trace: TraceContext,
    pub actor: ActorContext,
    pub tenant_id: Option<TenantId>,
    pub causation_id: Option<String>,
}

impl RequestContext {
    pub fn new(request_id: RequestId, correlation_id: CorrelationId) -> Self {
        Self {
            request_id,
            correlation_id,
            trace: TraceContext::default(),
            actor: ActorContext::Anonymous,
            tenant_id: None,
            causation_id: None,
        }
    }
}

#[derive(Clone)]
pub struct AppContext {
    pub config: Arc<AppConfig>,
    pub db: DbPool,
    pub clock: Arc<dyn Clock>,
    pub ids: Arc<dyn IdGenerator>,
    pub events: Arc<dyn EventPublisher>,
    pub health: HealthRegistry,
    pub shutdown: Shutdown,
}

impl Debug for AppContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AppContext")
            .field("config", &self.config)
            .field("db", &"<pool>")
            .field("health", &self.health)
            .field("shutdown", &self.shutdown)
            .finish_non_exhaustive()
    }
}

impl AppContext {
    pub fn new(config: AppConfig, db: DbPool, events: Arc<dyn EventPublisher>) -> Self {
        Self {
            config: Arc::new(config),
            db,
            clock: Arc::new(SystemClock),
            ids: Arc::new(UuidGenerator),
            events,
            health: HealthRegistry::default(),
            shutdown: Shutdown::new(),
        }
    }
}
