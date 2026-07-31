use axum::body::Body;
use axum::http::{Request, StatusCode};
use lenso_api::build_router;
use platform_core::{AppConfig, AppContext, LoggingEventPublisher};
use std::sync::Arc;
use tower::ServiceExt;

#[tokio::test]
async fn embedded_console_and_extension_routes_are_not_mounted() {
    let app = app_with_config(AppConfig::from_env());

    for path in [
        "/console",
        "/console/runtime/stories",
        "/console/extensions/registry.json",
        "/console/extensions/vendor/entry.js",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("request should complete");
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "path: {path}");
    }
}

fn app_with_config(config: AppConfig) -> axum::Router {
    build_router(AppContext::new(
        config,
        platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build"),
        Arc::new(LoggingEventPublisher),
    ))
}
