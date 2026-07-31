use anyhow::Context as _;
use axum::Router;
use axum::http::{HeaderName, HeaderValue, Method, header};
use axum::middleware;
use axum::response::Html;
use platform_core::{
    AppConfig, AppContext, LoggingEventPublisher, PostgresRuntimeConfigProvider,
    RuntimeConfigRegistry, Shutdown, connect_pool, connect_redis, telemetry,
};
use platform_http::request_context_middleware;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing::info;

mod console_bridge;
pub mod openapi;

pub use openapi::openapi_document;

pub async fn run_from_env() -> anyhow::Result<()> {
    run_from_env_with_composition(lenso_bootstrap::HostComposition::default()).await
}

pub async fn run_from_env_with_composition(
    composition: lenso_bootstrap::HostComposition,
) -> anyhow::Result<()> {
    let config = AppConfig::try_from_env().context("invalid application configuration")?;
    telemetry::init(&config.telemetry)?;

    let db = connect_pool(&config.database).await?;
    let redis = connect_redis(&config.redis).await?;
    let mut ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher)).with_redis(redis);

    let descriptors =
        lenso_bootstrap::runtime_config_descriptors_with_composition(&ctx, &composition)
            .context("failed to collect runtime-config descriptors")?;
    let groups =
        lenso_bootstrap::runtime_config_group_descriptors_with_composition(&ctx, &composition)
            .context("failed to collect runtime-config groups")?;
    let registry = RuntimeConfigRegistry::try_new_with_groups(descriptors, groups)
        .context("duplicate runtime-config descriptor registered")?;
    let runtime_config =
        PostgresRuntimeConfigProvider::connect(ctx.db.clone(), Arc::new(registry), "api")
            .await
            .context("failed to load runtime-config snapshot")?;
    runtime_config.spawn_listener();
    ctx = ctx.with_runtime_config_provider(runtime_config);

    let provider_plan = lenso_bootstrap::provider_runtime_plan_from_workspace(".")
        .context("failed to compile Provider Runtime Plan")?;
    if let Some(provider_runtime) = lenso_bootstrap::load_provider_runtime_with_composition(
        &ctx,
        &composition,
        provider_plan.as_ref(),
    )
    .await
    .context("failed to load locked Provider Runtime")?
    {
        platform_provider::install_provider_http_proxy_registry(provider_runtime.proxy_registry());
    }

    let app = try_build_router_with_composition(ctx.clone(), &composition)
        .context("failed to build API router")?;
    let address: SocketAddr = format!("{}:{}", ctx.config.http.host, ctx.config.http.port)
        .parse()
        .context("invalid HTTP bind address")?;

    info!(%address, "starting API server");
    let listener = tokio::net::TcpListener::bind(address).await?;
    let shutdown = ctx.shutdown.clone();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        let mut shutdown_rx = shutdown.subscribe();
        tokio::select! {
            () = Shutdown::wait_for_signal() => {},
            changed = shutdown_rx.changed() => {
                let _ = changed;
            },
        }
    })
    .await?;

    Ok(())
}

pub fn build_router(ctx: AppContext) -> Router {
    try_build_router(ctx).expect("Runtime API router should build with a valid composition profile")
}

pub fn try_build_router(ctx: AppContext) -> platform_core::AppResult<Router> {
    try_build_router_with_composition(ctx, &lenso_bootstrap::HostComposition::default())
}

pub fn try_build_router_with_composition(
    mut ctx: AppContext,
    composition: &lenso_bootstrap::HostComposition,
) -> platform_core::AppResult<Router> {
    if let Some(actor_resolver) =
        lenso_bootstrap::auth_actor_resolver_for_context_with_composition(&ctx, composition)?
    {
        ctx = ctx.with_actor_resolver(actor_resolver);
    }
    let host_wiring = lenso_bootstrap::host_wiring_for_context_with_composition(&ctx, composition)?;
    let console_bridge = console_bridge::ConsoleBridgeRegistry::from_modules(
        lenso_bootstrap::modules_for_config_with_composition(&ctx, composition)?,
    );
    let (router, mut document) =
        openapi::api_router_for_context_with_composition(&ctx, composition)?.split_for_parts();
    openapi::normalize_error_response_content_types(&mut document);
    let document = Arc::new(document);

    Ok(router
        .route("/docs", axum::routing::get(scalar_docs))
        .route("/openapi.json", axum::routing::get(serve_openapi))
        .layer(axum::Extension(document))
        .layer(axum::Extension(console_bridge))
        .layer(axum::Extension(host_wiring.auth_session_policy()))
        .layer(middleware::from_fn_with_state(
            ctx.clone(),
            request_context_middleware,
        ))
        .layer(cors_layer(&ctx))
        .with_state(ctx))
}

async fn scalar_docs() -> ([(HeaderName, HeaderValue); 3], Html<&'static str>) {
    (
        [
            (
                HeaderName::from_static("content-security-policy"),
                HeaderValue::from_static(SCALAR_DOCS_CSP),
            ),
            (
                HeaderName::from_static("referrer-policy"),
                HeaderValue::from_static("no-referrer"),
            ),
            (
                HeaderName::from_static("x-content-type-options"),
                HeaderValue::from_static("nosniff"),
            ),
        ],
        Html(SCALAR_DOCS_HTML),
    )
}

async fn serve_openapi(
    axum::Extension(document): axum::Extension<Arc<utoipa::openapi::OpenApi>>,
) -> axum::Json<utoipa::openapi::OpenApi> {
    axum::Json((*document).clone())
}

fn cors_layer(ctx: &AppContext) -> CorsLayer {
    let origins: Vec<HeaderValue> = ctx
        .config
        .http
        .cors_allowed_origins
        .iter()
        .filter_map(|origin| origin.parse().ok())
        .collect();

    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::ACCEPT, header::AUTHORIZATION, header::CONTENT_TYPE])
}

const SCALAR_DOCS_CSP: &str = "default-src 'none'; script-src https://cdn.jsdelivr.net 'sha256-wT12sSim/cr/4i3SfCUXmSC76WSRp+uWevWj0uNZ/vU='; style-src 'unsafe-inline'; connect-src 'self'; img-src 'self' data: https:; font-src 'self' data: https:; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

const SCALAR_DOCS_HTML: &str = r##"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Lenso API Docs</title>
    <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference@1.62.5" integrity="sha384-jVBCKhcCfx34USN27x4iQK1SBNdL/HxKq3KuBAxTS4WPaP5w80K4fjpwB+DezJL5" crossorigin="anonymous"></script>
    <style>
      body {
        margin: 0;
      }
    </style>
  </head>
  <body>
    <div id="app"></div>
    <script>Scalar.createApiReference("#app",{url:"/openapi.json",theme:"default"});</script>
  </body>
</html>
"##;
