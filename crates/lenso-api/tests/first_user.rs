use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use lenso_api::try_build_router_with_composition;
use platform_core::{AppConfig, AppContext, LoggingEventPublisher, apply_migrations};
use platform_testing::TestDatabase;
use serde_json::Value;
use std::sync::Arc;
use tower::ServiceExt as _;

#[tokio::test]
async fn first_user_host_exposes_health_and_data_plane_contracts() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };

    let mut config = AppConfig::from_env();
    config.database.url = db.url.clone();
    let composition = lenso_bootstrap::HostComposition::default();
    let migrations = lenso_bootstrap::migrations_for_config_with_composition(&config, &composition)
        .expect("host migrations should compose");
    apply_migrations(&db.pool, &migrations)
        .await
        .expect("host migrations should apply");

    let ctx = AppContext::new(config, db.pool.clone(), Arc::new(LoggingEventPublisher));
    let app = try_build_router_with_composition(ctx, &composition).expect("router should build");

    let livez = app
        .clone()
        .oneshot(
            Request::get("/livez")
                .body(Body::empty())
                .expect("livez request builds"),
        )
        .await
        .expect("livez request should complete");
    assert_eq!(livez.status(), StatusCode::OK);

    let openapi = app
        .oneshot(
            Request::get("/openapi.json")
                .body(Body::empty())
                .expect("OpenAPI request builds"),
        )
        .await
        .expect("OpenAPI request should complete");
    assert_eq!(openapi.status(), StatusCode::OK);
    let body = to_bytes(openapi.into_body(), usize::MAX)
        .await
        .expect("OpenAPI body should be readable");
    let document: Value = serde_json::from_slice(&body).expect("OpenAPI should be JSON");
    let paths = document["paths"]
        .as_object()
        .expect("OpenAPI paths should exist");
    assert!(paths.keys().any(|path| path.starts_with("/v1/auth/")));
    assert!(!paths.keys().any(|path| path.starts_with("/admin")));

    db.cleanup().await;
}
