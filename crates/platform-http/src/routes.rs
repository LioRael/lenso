use crate::health::{livez, readyz};
use axum::Router;
use axum::routing::get;
use platform_core::AppContext;
use utoipa_axum::router::OpenApiRouter;

/// Axum router specialized to the shared [`AppContext`] state.
pub type ApiRouter = Router<AppContext>;

/// `OpenAPI`-aware router specialized to the shared [`AppContext`] state.
///
/// Domains and apps build these so that HTTP routes and their `OpenAPI`
/// documentation come from a single `#[utoipa::path]`-annotated handler.
pub type ApiOpenApiRouter = OpenApiRouter<AppContext>;

/// Base router with liveness/readiness probes.
///
/// These probes are intentionally excluded from the `OpenAPI` contract, so they
/// are registered as plain routes rather than documented paths.
pub fn base_router() -> ApiOpenApiRouter {
    OpenApiRouter::new()
        .route("/livez", get(livez))
        .route("/readyz", get(readyz))
}
