use anyhow::Context as _;
use axum::Router;
use axum::body::Body;
use axum::http::{HeaderName, HeaderValue, Method, header};
use axum::middleware;
use axum::response::Html;
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto::Builder as HyperConnectionBuilder,
};
use platform_core::{
    AppConfig, AppContext, LoggingEventPublisher, PostgresRuntimeConfigProvider,
    RuntimeConfigRegistry, Shutdown, connect_pool, connect_redis, telemetry,
};
use platform_http::request_context_middleware;
use spiffe_rustls::{LocalOnly, authorizer, mtls_server};
use spiffe_rustls_tokio::TlsAcceptor;
use std::convert::Infallible;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::watch;
use tower::ServiceExt as _;
use tower_http::cors::CorsLayer;
use tracing::info;

mod local_system_plane;

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

    let mut app = try_build_router_with_composition(ctx.clone(), &composition)
        .context("failed to build API router")?;
    if let Some(local_system_plane) = local_system_plane::router_from_env(&ctx.config)
        .context("failed to configure the local System Plane")?
    {
        app = app.merge(local_system_plane);
        info!("enabled loopback-only local System Plane");
    }
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
    let (router, mut document) =
        openapi::api_router_for_context_with_composition(&ctx, composition)?.split_for_parts();
    openapi::normalize_error_response_content_types(&mut document);
    let document = Arc::new(document);

    Ok(router
        .route("/docs", axum::routing::get(scalar_docs))
        .route("/openapi.json", axum::routing::get(serve_openapi))
        .layer(axum::Extension(document))
        .layer(axum::Extension(host_wiring.auth_session_policy()))
        .layer(middleware::from_fn_with_state(
            ctx.clone(),
            request_context_middleware,
        ))
        .layer(cors_layer(&ctx))
        .with_state(ctx))
}

/// Builds the independent System Plane Router. Callers must serve it on a
/// listener that injects a transport binding authenticated by mTLS.
pub fn try_build_router_with_composition_and_system_plane(
    _ctx: AppContext,
    _composition: &lenso_bootstrap::HostComposition,
    runtime: &lenso_bootstrap::HostSystemPlaneRuntime,
) -> platform_core::AppResult<Router> {
    let core = Some(Arc::clone(&runtime.core));
    let installations = Some(Arc::clone(&runtime.service_installations));
    let observability = runtime.runtime_observability.clone();
    let operations = runtime.runtime_operations.clone();
    let (router, _document) = platform_system_plane::router(core.clone())
        .merge(platform_module_management::system_plane_router(
            installations,
        ))
        .merge(platform_runtime_observability::router(observability))
        .merge(platform_runtime_operations::router(operations))
        .layer(axum::Extension(core))
        .split_for_parts();
    Ok(router)
}

/// Serves a System Plane-only Router over rotating SPIFFE X.509-SVID mTLS.
/// The verified peer SPIFFE ID is injected as the transport binding; request
/// headers cannot manufacture or replace it.
pub async fn run_production_system_plane<F>(
    listener: tokio::net::TcpListener,
    router: Router,
    identity: Arc<lenso_service::SpiffeWorkloadIdentityProvider>,
    allowed_peer_spiffe_ids: impl IntoIterator<Item = String>,
    shutdown: F,
) -> anyhow::Result<()>
where
    F: Future<Output = ()> + Send,
{
    let allowed_peer_spiffe_ids = allowed_peer_spiffe_ids.into_iter().collect::<Vec<_>>();
    if allowed_peer_spiffe_ids.is_empty() {
        anyhow::bail!("production System Plane requires at least one allowed peer SPIFFE ID");
    }
    let tls = mtls_server(identity.x509_source())
        .authorize(
            authorizer::exact(allowed_peer_spiffe_ids)
                .context("invalid System Plane peer SPIFFE allow list")?,
        )
        .trust_domain_policy(LocalOnly(identity.config().trust_domain().clone()))
        .with_alpn_protocols([b"http/1.1"])
        .build()
        .context("failed to build System Plane mTLS configuration")?;
    let acceptor = TlsAcceptor::new(Arc::new(tls));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let mut connections = tokio::task::JoinSet::new();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            () = &mut shutdown => break,
            accepted = listener.accept() => {
                let (stream, peer_address) = accepted.context("System Plane listener failed")?;
                let acceptor = acceptor.clone();
                let router = router.clone();
                let mut connection_shutdown = shutdown_rx.clone();
                connections.spawn(async move {
                    let (tls, peer) = match acceptor.accept(stream).await {
                        Ok(result) => result,
                        Err(error) => {
                            tracing::warn!(%peer_address, %error, "rejected System Plane mTLS connection");
                            return;
                        }
                    };
                    let Some(peer_spiffe_id) = peer.spiffe_id() else {
                        tracing::warn!(%peer_address, "rejected System Plane peer without a SPIFFE ID");
                        return;
                    };
                    let binding =
                        lenso_service::SpiffeWorkloadIdentityProvider::authenticated_transport_binding(
                            peer_spiffe_id,
                        );
                    let service = service_fn(move |request: hyper::Request<Incoming>| {
                        let router = router.clone();
                        let binding = binding.clone();
                        async move {
                            let (mut parts, incoming) = request.into_parts();
                            parts.extensions.insert(binding);
                            let request = hyper::Request::from_parts(parts, Body::new(incoming));
                            let response = router.oneshot(request).await?;
                            Ok::<_, Infallible>(response)
                        }
                    });
                    let connection_builder = HyperConnectionBuilder::new(TokioExecutor::new());
                    let connection =
                        connection_builder.serve_connection(TokioIo::new(tls), service);
                    tokio::pin!(connection);
                    tokio::select! {
                        result = &mut connection => {
                            if let Err(error) = result {
                                tracing::warn!(%peer_address, %error, "System Plane connection failed");
                            }
                        }
                        changed = connection_shutdown.changed() => {
                            if changed.is_ok() {
                                connection.as_mut().graceful_shutdown();
                                if let Err(error) = connection.await {
                                    tracing::warn!(%peer_address, %error, "System Plane graceful shutdown failed");
                                }
                            }
                        }
                    }
                });
            }
        }
    }

    let _ = shutdown_tx.send(true);
    while let Some(result) = connections.join_next().await {
        if let Err(error) = result {
            tracing::warn!(%error, "System Plane connection task failed");
        }
    }
    identity.shutdown().await;
    Ok(())
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
