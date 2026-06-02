use axum::Router;
use axum::http::{HeaderValue, Method, header};
use axum::middleware;
use axum::response::Html;
use platform_core::AppContext;
use platform_http::request_context_middleware;
use std::sync::Arc;
use tower_http::cors::CorsLayer;

pub mod admin_runtime;
pub mod openapi;

pub use openapi::openapi_document;

pub fn build_router(ctx: AppContext) -> Router {
    let (router, document) = openapi::api_router().split_for_parts();
    let document = Arc::new(document);

    router
        .route("/docs", axum::routing::get(scalar_docs))
        .route("/openapi.json", axum::routing::get(serve_openapi))
        .layer(axum::Extension(document))
        .layer(middleware::from_fn_with_state(
            ctx.clone(),
            request_context_middleware,
        ))
        .layer(cors_layer(&ctx))
        .with_state(ctx)
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
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
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
