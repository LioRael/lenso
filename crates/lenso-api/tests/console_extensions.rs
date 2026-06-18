use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use lenso_api::build_router;
use platform_core::{AppConfig, AppContext, LoggingEventPublisher};
use serde_json::json;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tower::ServiceExt;

#[tokio::test]
async fn serves_console_extension_registry_from_configured_directory() {
    let root = unique_console_root("extensions");
    tokio::fs::create_dir_all(&root)
        .await
        .expect("extension dir should be created");
    tokio::fs::write(
        root.join("registry.json"),
        serde_json::to_vec(&json!({
            "version": 1,
            "bundles": [
                {
                    "packageName": "@vendor/crm-console",
                    "exportName": "crmConsoleModule",
                    "entry": "/console/extensions/crm/entry.js",
                    "hostApi": "1"
                }
            ]
        }))
        .expect("registry should serialize"),
    )
    .await
    .expect("registry should be written");

    let mut config = AppConfig::from_env();
    config.console.extensions_dir = root.to_string_lossy().into_owned();
    let app = app_with_config(config);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/console/extensions/registry.json")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body should read");
    let body: serde_json::Value = serde_json::from_slice(&bytes).expect("registry should be json");
    assert_eq!(body["bundles"][0]["packageName"], "@vendor/crm-console");

    let _ = tokio::fs::remove_dir_all(root).await;
}

#[tokio::test]
async fn serves_console_dist_with_index_fallback() {
    let root = unique_console_root("dist");
    tokio::fs::create_dir_all(&root)
        .await
        .expect("console dist dir should be created");
    tokio::fs::write(root.join("index.html"), "<div id=\"root\"></div>")
        .await
        .expect("console index should be written");

    let mut config = AppConfig::from_env();
    config.console.dist_dir = root.to_string_lossy().into_owned();
    let app = app_with_config(config);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/console/runtime/stories")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body should read");
    let body = String::from_utf8(bytes.to_vec()).expect("body should be utf-8");
    assert!(body.contains("root"));

    let _ = tokio::fs::remove_dir_all(root).await;
}

fn app_with_config(config: AppConfig) -> axum::Router {
    build_router(AppContext::new(
        config,
        platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build"),
        Arc::new(LoggingEventPublisher),
    ))
}

fn unique_console_root(kind: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("lenso-console-{kind}-{nanos}"))
}
