use anyhow::Context as _;
use platform_core::{
    AppConfig, AppContext, EventHandlerRegistry, LoggingEventPublisher, OutboxRelay,
    PostgresRuntimeConfigProvider, RuntimeConfigRegistry, Shutdown, WorkerRuntimeConfig,
    connect_pool, telemetry,
};
use platform_runtime::{FunctionRegistry, RuntimeWorker};
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = AppConfig::try_from_env().context("invalid application configuration")?;
    telemetry::init(&config.telemetry)?;

    let db = connect_pool(&config.database).await?;
    let ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));

    let descriptors = app_bootstrap::runtime_config_descriptors(&ctx)
        .context("failed to collect runtime-config descriptors")?;
    let runtime_config_registry = RuntimeConfigRegistry::try_new(descriptors)
        .context("duplicate runtime-config descriptor registered")?;
    let runtime_config = PostgresRuntimeConfigProvider::connect(
        ctx.db.clone(),
        Arc::new(runtime_config_registry),
        "worker",
    )
    .await
    .context("failed to load runtime-config snapshot")?;
    runtime_config.spawn_listener();
    let ctx = ctx.with_runtime_config_provider(runtime_config);

    let modules = app_bootstrap::load_modules(&ctx)
        .await
        .context("failed to load modules")?;
    let registry = Arc::new(app_bootstrap::function_registry(&modules));
    let activation_run_ids =
        app_bootstrap::enqueue_lifecycle_activation_jobs(&ctx, &modules, &registry)
            .await
            .context("failed to enqueue module lifecycle activation jobs")?;
    let event_handlers =
        app_bootstrap::event_handlers_with_runtime_actions(&ctx, &modules, registry.clone());

    info!(
        functions = registry.all().count(),
        lifecycle_activation_jobs = activation_run_ids.len(),
        "starting Lenso starter worker"
    );

    run_worker_loop(ctx.clone(), event_handlers, registry).await;
    Ok(())
}

async fn run_worker_loop(
    ctx: AppContext,
    dispatcher: EventHandlerRegistry,
    registry: Arc<FunctionRegistry>,
) {
    let shutdown = ctx.shutdown.clone();
    let mut shutdown_rx = shutdown.subscribe();
    let relay = OutboxRelay::new(ctx.db.clone(), "worker-local");
    let runtime_worker = RuntimeWorker::new(ctx.db.clone(), registry, "worker-local");

    loop {
        let cfg: WorkerRuntimeConfig = ctx
            .runtime_config
            .snapshot()
            .get("worker")
            .unwrap_or_default();
        let batch_size = cfg.batch_size as i64;

        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_ok() && *shutdown_rx.borrow() {
                    break;
                }
            }
            () = Shutdown::wait_for_signal() => {
                shutdown.signal();
            }
            () = tokio::time::sleep(Duration::from_millis(cfg.poll_interval_ms)) => {
                if let Err(error) = relay.relay_once(&dispatcher, batch_size).await {
                    tracing::warn!(error = ?error, "outbox relay tick failed");
                }
                if let Err(error) = runtime_worker.claim_and_run_batch(batch_size).await {
                    tracing::warn!(error = ?error, "runtime worker tick failed");
                }
            }
        }
    }
}
