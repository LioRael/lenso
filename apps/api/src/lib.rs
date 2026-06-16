use anyhow::Context as _;
use axum::Router;
use axum::http::{HeaderValue, Method, header};
use axum::middleware;
use axum::response::Html;
use platform_core::{
    AppConfig, AppContext, LoggingEventPublisher, PostgresRuntimeConfigProvider,
    RuntimeConfigRegistry, Shutdown, connect_pool, telemetry,
};
use platform_http::request_context_middleware;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing::info;

pub mod openapi;

pub use openapi::openapi_document;

pub async fn run_from_env() -> anyhow::Result<()> {
    run_from_env_with_composition(app_bootstrap::HostComposition::default()).await
}

pub async fn run_from_env_with_composition(
    composition: app_bootstrap::HostComposition,
) -> anyhow::Result<()> {
    let config = AppConfig::try_from_env().context("invalid application configuration")?;
    telemetry::init(&config.telemetry)?;

    let db = connect_pool(&config.database).await?;
    let mut ctx = AppContext::new(config, db, Arc::new(LoggingEventPublisher));

    let descriptors =
        app_bootstrap::runtime_config_descriptors_with_composition(&ctx, &composition)
            .context("failed to collect runtime-config descriptors")?;
    let groups =
        app_bootstrap::runtime_config_group_descriptors_with_composition(&ctx, &composition)
            .context("failed to collect runtime-config groups")?;
    let registry = RuntimeConfigRegistry::try_new_with_groups(descriptors, groups)
        .context("duplicate runtime-config descriptor registered")?;
    platform_admin::install_runtime_config_registry(registry.clone());
    let runtime_config =
        PostgresRuntimeConfigProvider::connect(ctx.db.clone(), Arc::new(registry), "api")
            .await
            .context("failed to load runtime-config snapshot")?;
    runtime_config.spawn_listener();
    ctx = ctx.with_runtime_config_provider(runtime_config);

    let admin_modules = app_bootstrap::load_admin_modules_with_composition(&ctx, &composition)
        .await
        .context("failed to load admin modules")?;
    platform_admin_data::install_admin_modules(admin_modules);
    let admin_module_metadata =
        app_bootstrap::load_admin_module_metadata_with_composition(&ctx, &composition)
            .await
            .context("failed to load admin module metadata")?;
    install_admin_module_metadata(admin_module_metadata);
    let remote_http_proxy_registry = app_bootstrap::load_remote_http_proxy_registry(&ctx)
        .await
        .context("failed to load remote HTTP proxy registry")?;
    platform_module_remote::install_remote_http_proxy_registry(remote_http_proxy_registry);

    let admin_refresh_ctx = ctx.clone();
    let admin_refresh_composition = composition.clone();
    platform_admin_data::install_admin_module_refresh_fn(move || {
        let ctx = admin_refresh_ctx.clone();
        let composition = admin_refresh_composition.clone();
        async move { app_bootstrap::load_admin_modules_with_composition(&ctx, &composition).await }
    });
    let admin_metadata_refresh_ctx = ctx.clone();
    let admin_metadata_refresh_composition = composition.clone();
    platform_admin_data::install_admin_module_metadata_refresh_fn(move || {
        let ctx = admin_metadata_refresh_ctx.clone();
        let composition = admin_metadata_refresh_composition.clone();
        async move {
            let metadata =
                app_bootstrap::load_admin_module_metadata_with_composition(&ctx, &composition)
                    .await?;
            install_platform_admin_catalogs(&metadata);
            Ok(metadata)
        }
    });

    let app = try_build_router_with_composition(ctx.clone(), &composition)
        .context("failed to build API router")?;
    let address: SocketAddr = format!("{}:{}", ctx.config.http.host, ctx.config.http.port)
        .parse()
        .context("invalid HTTP bind address")?;

    info!(%address, "starting API server");
    let listener = tokio::net::TcpListener::bind(address).await?;

    let shutdown = ctx.shutdown.clone();
    axum::serve(listener, app)
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
    try_build_router_with_composition(ctx, &app_bootstrap::HostComposition::default())
}

pub fn try_build_router_with_composition(
    mut ctx: AppContext,
    composition: &app_bootstrap::HostComposition,
) -> platform_core::AppResult<Router> {
    if let Some(actor_resolver) =
        app_bootstrap::auth_actor_resolver_for_context_with_composition(&ctx, composition)?
    {
        ctx = ctx.with_actor_resolver(actor_resolver);
    }
    install_default_platform_admin_catalogs(&ctx, composition)?;
    let (router, document) =
        openapi::api_router_for_context_with_composition(&ctx, composition)?.split_for_parts();
    let document = Arc::new(document);

    Ok(router
        .route("/docs", axum::routing::get(scalar_docs))
        .route("/openapi.json", axum::routing::get(serve_openapi))
        .layer(axum::Extension(document))
        .layer(middleware::from_fn_with_state(
            ctx.clone(),
            request_context_middleware,
        ))
        .layer(cors_layer(&ctx))
        .with_state(ctx))
}

fn install_default_platform_admin_catalogs(
    ctx: &AppContext,
    composition: &app_bootstrap::HostComposition,
) -> platform_core::AppResult<()> {
    app_bootstrap::install_default_story_display_catalog_with_composition(ctx, composition)?;
    platform_admin::install_default_runtime_function_declarations(
        platform_admin::runtime_function_declarations_from_modules(
            app_bootstrap::linked_runtime_function_declaration_sources_for_context_with_composition(
                ctx,
                composition,
            )?,
        ),
    );
    Ok(())
}

fn install_admin_module_metadata(metadata: Vec<platform_admin_data::AdminModuleMetadata>) {
    install_platform_admin_catalogs(&metadata);
    platform_admin_data::install_admin_module_metadata(metadata);
}

fn install_platform_admin_catalogs(metadata: &[platform_admin_data::AdminModuleMetadata]) {
    app_bootstrap::install_story_display_catalog(metadata);
    platform_admin::install_runtime_function_declarations(
        platform_admin::runtime_function_declarations_from_modules(
            app_bootstrap::runtime_function_declaration_sources_from_metadata(metadata),
        ),
    );
}

async fn scalar_docs() -> Html<&'static str> {
    Html(SCALAR_DOCS_HTML)
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

const SCALAR_DOCS_HTML: &str = r##"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Lenso API Docs</title>
    <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script>
    <style>
      body {
        margin: 0;
      }
    </style>
  </head>
  <body>
    <div id="app"></div>
    <script>
      Scalar.createApiReference("#app", {
        url: "/openapi.json",
        theme: "default",
      });
    </script>
  </body>
</html>
"##;
