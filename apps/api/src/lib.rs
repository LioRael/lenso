use axum::Router;
use axum::http::{HeaderValue, Method, header};
use axum::middleware;
use platform_core::AppContext;
use platform_http::request_context_middleware;
use platform_http::routes::base_router;
use tower_http::cors::CorsLayer;

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
        .layer(cors_layer())
        .with_state(ctx)
}

async fn openapi_placeholder() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::to_value(openapi_document()).expect("OpenAPI document should serialize"))
}

fn cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin([
            HeaderValue::from_static("http://localhost:5173"),
            HeaderValue::from_static("http://localhost:5174"),
            HeaderValue::from_static("http://localhost:5175"),
            HeaderValue::from_static("http://localhost:5176"),
            HeaderValue::from_static("http://localhost:5177"),
        ])
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::ACCEPT, header::AUTHORIZATION, header::CONTENT_TYPE])
}
