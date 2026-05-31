use axum::middleware;
use axum::Router;
use platform_core::AppContext;
use platform_http::request_context_middleware;
use platform_http::routes::base_router;

pub mod admin_runtime;
pub mod openapi;

pub use openapi::openapi_document;

pub fn build_router(ctx: AppContext) -> Router {
    let identity_http = identity::module::http(ctx.clone());

    base_router(ctx.clone())
        .merge(identity_http.router)
        .merge(admin_runtime::router())
        .route("/openapi.json", axum::routing::get(openapi_placeholder))
        .layer(middleware::from_fn(request_context_middleware))
        .with_state(ctx)
}

async fn openapi_placeholder() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::to_value(openapi_document()).expect("OpenAPI document should serialize"))
}
