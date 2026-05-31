pub mod clock;
pub mod config;
pub mod context;
pub mod db;
pub mod error;
pub mod events;
pub mod execution;
pub mod health;
pub mod ids;
pub mod migrations;
pub mod outbox;
pub mod shutdown;
pub mod telemetry;

pub use clock::{Clock, SystemClock};
pub use config::{
    AppConfig, AuthConfig, DatabaseConfig, HttpConfig, ModuleConfig, RuntimeConfig, ServiceConfig,
    TelemetryConfig,
};
pub use context::{
    ActorContext, AppContext, CorrelationId, RequestContext, RequestId, TenantId, TraceContext,
};
pub use db::{connect_pool, DbPool};
pub use error::{AppError, AppResult, ErrorCode};
pub use events::{EventEnvelope, EventPayload, EventPublisher, LoggingEventPublisher};
pub use execution::{ExecutionContext, ExecutionId};
pub use health::{HealthRegistry, HealthStatus};
pub use ids::{IdGenerator, UuidGenerator};
pub use migrations::{apply_migrations, Migration, PLATFORM_MIGRATIONS};
pub use outbox::{
    ClaimedOutboxEvent, EventDispatcher, EventHandler, EventHandlerRegistry,
    LoggingEventDispatcher, OutboxEvent, OutboxPublisher, OutboxRelay, OutboxStatus,
};
pub use shutdown::Shutdown;
