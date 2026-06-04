use app_api::build_router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use platform_core::{AppConfig, AppContext, LoggingEventPublisher};
use std::sync::Arc;
use tower::ServiceExt;

fn app() -> axum::Router {
    let ctx = AppContext::new(
        AppConfig::from_env(),
        platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build"),
        Arc::new(LoggingEventPublisher),
    );
    build_router(ctx)
}

/// A browser preflight for the config console's PUT must be allowed: the
/// `Access-Control-Allow-Methods` response header has to advertise PUT, or the
/// browser blocks the request before it ever reaches a handler.
#[tokio::test]
async fn preflight_allows_put_for_config_write() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/admin/config/*/identity.password_reset_ttl_minutes")
                .header("origin", "http://localhost:5173")
                .header("access-control-request-method", "PUT")
                .header(
                    "access-control-request-headers",
                    "authorization,content-type",
                )
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let allow_methods = response
        .headers()
        .get("access-control-allow-methods")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_uppercase();
    assert!(
        allow_methods.contains("PUT"),
        "preflight must advertise PUT, got: {allow_methods:?}"
    );
}

#[tokio::test]
async fn preflight_allows_patch_for_remote_proxy() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/modules/remote-crm/http/contacts/contact_1")
                .header("origin", "http://localhost:5173")
                .header("access-control-request-method", "PATCH")
                .header(
                    "access-control-request-headers",
                    "authorization,content-type",
                )
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let allow_methods = response
        .headers()
        .get("access-control-allow-methods")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_uppercase();
    assert!(
        allow_methods.contains("PATCH"),
        "preflight must advertise PATCH, got: {allow_methods:?}"
    );
}

/// The config console's DELETE (reset-to-default) must likewise be preflight-allowed.
#[tokio::test]
async fn preflight_allows_delete_for_config_reset() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/admin/config/*/identity.password_reset_ttl_minutes")
                .header("origin", "http://localhost:5173")
                .header("access-control-request-method", "DELETE")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let allow_methods = response
        .headers()
        .get("access-control-allow-methods")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_uppercase();
    assert!(
        allow_methods.contains("DELETE"),
        "preflight must advertise DELETE, got: {allow_methods:?}"
    );
}
