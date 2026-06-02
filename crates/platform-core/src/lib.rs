pub mod clock;
pub mod config;
pub mod context;
pub mod db;
pub mod error;
pub mod events;
pub mod execution;
pub mod execution_logs;
pub mod health;
pub mod ids;
pub mod migrations;
pub mod outbox;
pub mod settings;
pub mod shutdown;
pub mod story_display;
pub mod story_events;
pub mod telemetry;
pub mod telemetry_attrs;
pub mod telemetry_query;

pub use clock::{Clock, SystemClock};
pub use config::{
    AppConfig, AuthConfig, DatabaseConfig, HttpConfig, LogFormat, ModuleConfig, ServiceConfig,
    TelemetryConfig, WorkerConfig, parse_cors_allowed_origins,
};
pub use context::{
    ActorContext, AppContext, CorrelationId, RequestContext, RequestId, TenantId, TraceContext,
};
pub use db::{DbPool, connect_pool};
pub use error::{AppError, AppResult, ErrorCode};
pub use events::{EventEnvelope, EventPayload, EventPublisher, LoggingEventPublisher};
pub use execution::{ExecutionContext, ExecutionId};
pub use execution_logs::{
    ExecutionLogProvider, ExecutionLogQuery, ExecutionLogRow, PostgresExecutionLogProvider,
};
pub use health::{HealthRegistry, HealthStatus};
pub use ids::{IdGenerator, UuidGenerator};
pub use migrations::{Migration, PLATFORM_MIGRATIONS, apply_migrations};
pub use outbox::{
    ClaimedOutboxEvent, EventDispatcher, EventHandler, EventHandlerRegistry,
    LoggingEventDispatcher, OutboxEvent, OutboxPublisher, OutboxRelay, OutboxStatus,
};
pub use settings::{
    CONFIG_NOTIFY_CHANNEL, PostgresSettingsProvider, SettingAuditEntry, SettingDescriptor,
    SettingScope, SettingSource, SettingType, SettingsProvider, SettingsRegistry, SettingsSnapshot,
    SnapshotCell, StaticSettingsProvider, StoredSetting,
};
pub use shutdown::Shutdown;
pub use story_display::{StoryDisplayDescriptor, StoryDisplaySource};
pub use telemetry_attrs::{
    RuntimeSpanAttributes, generate_trace_context, record_runtime_span_attributes,
    trace_context_from_headers, trace_context_from_traceparent, trace_headers,
};
pub use telemetry_query::{
    InMemoryTelemetrySpanProvider, NoopTelemetrySpanProvider, TelemetrySpan, TelemetrySpanProvider,
    TelemetrySpanQuery,
};
