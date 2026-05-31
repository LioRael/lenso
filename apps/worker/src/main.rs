use platform_core::{
    connect_pool, telemetry, AppConfig, AppContext, EventHandlerRegistry, LoggingEventPublisher,
    OutboxRelay, Shutdown,
};
use platform_runtime::FunctionRegistry;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = AppConfig::from_env();
    telemetry::init(&config.telemetry)?;

    let db = connect_pool(&config.database).await?;
    let ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));

    let identity_domain = identity::domain();
    let notifications_domain = notifications::module::domain();

    let mut registry = FunctionRegistry::default();
    identity_domain.runtime.register_into(&mut registry);
    notifications_domain.runtime.register_into(&mut registry);

    let mut event_handlers = EventHandlerRegistry::new();
    event_handlers.register_all(identity_domain.event_handlers);
    event_handlers.register_all(notifications_domain.event_handlers);

    info!(
        functions = registry.all().count(),
        user_registered_handlers = event_handlers.handler_count("identity.user_registered.v1"),
        "starting worker placeholder"
    );

    run_worker_loop(ctx.clone(), event_handlers).await;
    Ok(())
}

async fn run_worker_loop(ctx: AppContext, dispatcher: EventHandlerRegistry) {
    let shutdown = ctx.shutdown.clone();
    let mut shutdown_rx = shutdown.subscribe();
    let relay = OutboxRelay::new(ctx.db.clone(), "worker-local", 25);
    loop {
        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_ok() && *shutdown_rx.borrow() {
                    break;
                }
            }
            () = Shutdown::wait_for_signal() => {
                shutdown.signal();
            }
            () = tokio::time::sleep(Duration::from_millis(500)) => {
                match relay.relay_once(&dispatcher).await {
                    Ok(count) => {
                        tracing::debug!(claimed_outbox_events = count, "outbox relay tick");
                    }
                    Err(error) => {
                        tracing::warn!(error = ?error, "outbox relay tick failed");
                    }
                }
            }
        }
    }
}
