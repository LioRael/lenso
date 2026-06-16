use crate::clock::{Clock, SystemClock};
use crate::config::{AppConfig, is_local_development_environment};
use crate::db::DbPool;
use crate::events::EventPublisher;
use crate::execution_logs::{ExecutionLogProvider, PostgresExecutionLogProvider};
use crate::health::HealthRegistry;
use crate::ids::{IdGenerator, UuidGenerator};
use crate::runtime_config::{RuntimeConfigProvider, StaticRuntimeConfigProvider};
use crate::shutdown::Shutdown;
use crate::telemetry_query::{NoopTelemetrySpanProvider, TelemetrySpanProvider};
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

#[derive(Debug, Clone, Default)]
pub struct ActorResolutionRequest {
    pub authorization: Option<String>,
    pub cookie: Option<String>,
}

#[async_trait::async_trait]
pub trait ActorResolver: Debug + Send + Sync {
    async fn resolve_actor(&self, request: ActorResolutionRequest) -> ActorContext;
}

#[derive(Debug, Clone)]
pub struct DevActorResolver {
    environment: String,
}

impl DevActorResolver {
    #[must_use]
    pub fn new(environment: impl Into<String>) -> Self {
        Self {
            environment: environment.into(),
        }
    }
}

#[async_trait::async_trait]
impl ActorResolver for DevActorResolver {
    async fn resolve_actor(&self, request: ActorResolutionRequest) -> ActorContext {
        let _ = request.cookie;
        request
            .authorization
            .and_then(|value| parse_dev_bearer_actor(&value, &self.environment))
            .unwrap_or_default()
    }
}

fn parse_dev_bearer_actor(value: &str, environment: &str) -> Option<ActorContext> {
    if !is_local_development_environment(environment) {
        return None;
    }

    let token = value.strip_prefix("Bearer ")?;

    if let Some(user_id) = token.strip_prefix("dev-user:") {
        return Some(ActorContext::User {
            user_id: user_id.to_owned(),
            scopes: Vec::new(),
        });
    }

    if let Some(service_token) = token.strip_prefix("dev-service:") {
        let (service_id, scopes) = parse_dev_actor_scopes(service_token);
        return Some(ActorContext::Service { service_id, scopes });
    }

    None
}

fn parse_dev_actor_scopes(value: &str) -> (String, Vec<String>) {
    let Some((id, raw_scopes)) = value.split_once(':') else {
        return (value.to_owned(), Vec::new());
    };
    let scopes = raw_scopes
        .split(',')
        .filter(|scope| !scope.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    (id.to_owned(), scopes)
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
    pub actor_resolver: Arc<dyn ActorResolver>,
    pub clock: Arc<dyn Clock>,
    pub ids: Arc<dyn IdGenerator>,
    pub events: Arc<dyn EventPublisher>,
    pub telemetry_spans: Arc<dyn TelemetrySpanProvider>,
    pub execution_logs: Arc<dyn ExecutionLogProvider>,
    pub runtime_config: Arc<dyn RuntimeConfigProvider>,
    pub health: HealthRegistry,
    pub shutdown: Shutdown,
}

impl Debug for AppContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AppContext")
            .field("config", &self.config)
            .field("db", &"<pool>")
            .field("actor_resolver", &self.actor_resolver)
            .field("telemetry_spans", &self.telemetry_spans)
            .field("execution_logs", &self.execution_logs)
            .field("runtime_config", &self.runtime_config)
            .field("health", &self.health)
            .field("shutdown", &self.shutdown)
            .finish_non_exhaustive()
    }
}

impl AppContext {
    pub fn new(config: AppConfig, db: DbPool, events: Arc<dyn EventPublisher>) -> Self {
        let execution_logs = Arc::new(PostgresExecutionLogProvider::new(db.clone()));
        let actor_resolver = Arc::new(DevActorResolver::new(config.service.environment.clone()));
        Self {
            config: Arc::new(config),
            db,
            actor_resolver,
            clock: Arc::new(SystemClock),
            ids: Arc::new(UuidGenerator),
            events,
            telemetry_spans: Arc::new(NoopTelemetrySpanProvider),
            execution_logs,
            runtime_config: Arc::new(StaticRuntimeConfigProvider::empty()),
            health: HealthRegistry::default(),
            shutdown: Shutdown::new(),
        }
    }

    pub fn with_actor_resolver(mut self, actor_resolver: Arc<dyn ActorResolver>) -> Self {
        self.actor_resolver = actor_resolver;
        self
    }

    pub fn with_telemetry_span_provider(
        mut self,
        telemetry_spans: Arc<dyn TelemetrySpanProvider>,
    ) -> Self {
        self.telemetry_spans = telemetry_spans;
        self
    }

    pub fn with_execution_log_provider(
        mut self,
        execution_logs: Arc<dyn ExecutionLogProvider>,
    ) -> Self {
        self.execution_logs = execution_logs;
        self
    }

    pub fn with_runtime_config_provider(
        mut self,
        runtime_config: Arc<dyn RuntimeConfigProvider>,
    ) -> Self {
        self.runtime_config = runtime_config;
        self
    }
}
