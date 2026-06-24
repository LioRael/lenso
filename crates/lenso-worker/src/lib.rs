use anyhow::Context as _;
use platform_core::{
    AppConfig, AppContext, EventHandlerRegistry, LoggingEventPublisher, OutboxRelay,
    PostgresRuntimeConfigProvider, RuntimeConfigRegistry, Shutdown, WorkerRuntimeConfig,
    connect_pool, telemetry,
};
use platform_runtime::{
    FunctionRegistry, RuntimeScheduler, RuntimeWorker, ScheduledFunctionDefinition,
};
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

pub async fn run_from_env() -> anyhow::Result<()> {
    run_from_env_with_composition(lenso_bootstrap::HostComposition::default()).await
}

pub async fn run_from_env_with_composition(
    composition: lenso_bootstrap::HostComposition,
) -> anyhow::Result<()> {
    let config = AppConfig::try_from_env().context("invalid application configuration")?;
    telemetry::init(&config.telemetry)?;

    let db = connect_pool(&config.database).await?;
    let ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));

    let descriptors =
        lenso_bootstrap::runtime_config_descriptors_with_composition(&ctx, &composition)
            .context("failed to collect runtime-config descriptors")?;
    let groups =
        lenso_bootstrap::runtime_config_group_descriptors_with_composition(&ctx, &composition)
            .context("failed to collect runtime-config groups")?;
    let runtime_config_registry = RuntimeConfigRegistry::try_new_with_groups(descriptors, groups)
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

    let _remote_services = lenso_bootstrap::start_installed_remote_module_services(&ctx)
        .await
        .context("failed to start remote module services")?;

    let modules = lenso_bootstrap::load_modules_with_composition(&ctx, &composition)
        .await
        .context("failed to load modules")?;
    let registry = Arc::new(lenso_bootstrap::function_registry(&modules));
    let activation_run_ids =
        lenso_bootstrap::enqueue_lifecycle_activation_jobs(&ctx, &modules, &registry)
            .await
            .context("failed to enqueue module lifecycle activation jobs")?;
    let schedules = lenso_bootstrap::scheduled_functions(&modules, registry.as_ref())
        .context("failed to collect scheduled runtime functions")?;
    let event_handlers =
        lenso_bootstrap::event_handlers_with_runtime_actions(&ctx, &modules, registry.clone());

    info!(
        functions = registry.all().count(),
        lifecycle_activation_jobs = activation_run_ids.len(),
        scheduled_functions = schedules.len(),
        "starting worker"
    );

    run_worker_loop(ctx.clone(), event_handlers, registry, schedules).await;
    Ok(())
}

async fn run_worker_loop(
    ctx: AppContext,
    dispatcher: EventHandlerRegistry,
    registry: Arc<FunctionRegistry>,
    schedules: Vec<ScheduledFunctionDefinition>,
) {
    let shutdown = ctx.shutdown.clone();
    let mut shutdown_rx = shutdown.subscribe();
    let relay = OutboxRelay::new(ctx.db.clone(), "worker-local");
    let scheduler = RuntimeScheduler::new(ctx.db.clone(), "worker-local");
    let runtime_worker = RuntimeWorker::new(ctx.db.clone(), registry, "worker-local");
    loop {
        let cfg: WorkerRuntimeConfig = ctx
            .runtime_config
            .snapshot()
            .get("worker")
            .unwrap_or_default();
        // batch_size is descriptor-capped at 1000, so the u64->i64 cast is lossless.
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
                match scheduler.enqueue_due(&schedules).await {
                    Ok(run_ids) => {
                        if !run_ids.is_empty() {
                            tracing::debug!(
                                scheduled_function_runs = run_ids.len(),
                                "runtime scheduler tick"
                            );
                        }
                    }
                    Err(error) => {
                        tracing::warn!(error = ?error, "runtime scheduler tick failed");
                    }
                }
                match relay.relay_once(&dispatcher, batch_size).await {
                    Ok(count) => {
                        tracing::debug!(claimed_outbox_events = count, "outbox relay tick");
                    }
                    Err(error) => {
                        tracing::warn!(error = ?error, "outbox relay tick failed");
                    }
                }
                match runtime_worker.claim_and_run_batch(batch_size).await {
                    Ok(count) => {
                        tracing::debug!(claimed_function_runs = count, "runtime worker tick");
                    }
                    Err(error) => {
                        tracing::warn!(error = ?error, "runtime worker tick failed");
                    }
                }
            }
        }
    }
}
