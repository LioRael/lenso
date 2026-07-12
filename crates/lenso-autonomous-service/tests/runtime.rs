use axum::{body::Body, routing::post};
use http::{Request, StatusCode};
use http_body_util::BodyExt as _;
use lenso_autonomous_service::{ServiceRuntimeConfig, prepare_runtime, service_router};
use lenso_service::{
    AutonomousServiceContract, AutonomousServiceStore, AutonomousServiceWorkload,
    ServiceTenancyMode, WorkloadRole,
};
use platform_testing::TestDatabase;
use tower::ServiceExt as _;
use utoipa_axum::router::OpenApiRouter;

fn service() -> AutonomousServiceContract {
    service_named("support")
}

fn service_named(service_id: &str) -> AutonomousServiceContract {
    let mut service = AutonomousServiceContract::new(
        service_id,
        vec![
            AutonomousServiceWorkload::new(
                format!("{service_id}-api"),
                service_id,
                WorkloadRole::API,
            ),
            AutonomousServiceWorkload::new(
                format!("{service_id}-migrate"),
                service_id,
                WorkloadRole::MIGRATION,
            ),
        ],
        ServiceTenancyMode::None,
        vec!["local".to_owned()],
    );
    service.stores = vec![AutonomousServiceStore::new("primary", service_id)];
    service
}

#[tokio::test]
async fn store_owner_is_rejected_before_business_migrations_run() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    prepare_runtime(
        &service_named("support"),
        &ServiceRuntimeConfig::new("support", "primary", "support"),
        db.pool.clone(),
        platform_core::PLATFORM_MIGRATIONS,
    )
    .await
    .expect("first Service should claim Store");
    let forbidden_migration = platform_core::Migration {
        name: "billing/0001_forbidden",
        sql: "create schema billing_should_not_exist;",
    };

    let error = prepare_runtime(
        &service_named("billing"),
        &ServiceRuntimeConfig::new("billing", "primary", "billing"),
        db.pool.clone(),
        &[forbidden_migration],
    )
    .await
    .expect_err("another Service must not reuse the Store");

    assert_eq!(
        error.code,
        lenso_autonomous_service::RuntimeErrorCode::StoreAlreadyOwned
    );
    let mutated: bool = sqlx::query_scalar(
        "select exists (select 1 from information_schema.schemata where schema_name = 'billing_should_not_exist')",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert!(!mutated);

    db.cleanup().await;
}

#[tokio::test]
async fn api_operation_persists_service_local_story_segment() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let state = prepare_runtime(
        &service(),
        &ServiceRuntimeConfig::new("support", "primary", "support"),
        db.pool.clone(),
        platform_core::PLATFORM_MIGRATIONS,
    )
    .await
    .expect("Service migrations should apply");
    let app = service_router(
        OpenApiRouter::new().route("/tickets", post(|| async { StatusCode::CREATED })),
        state,
    );

    let operation = app
        .clone()
        .oneshot(Request::post("/tickets").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(operation.status(), StatusCode::CREATED);

    let evidence = app
        .oneshot(
            Request::get("/runtime/story-segments")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(evidence.status(), StatusCode::OK);
    let body = evidence.into_body().collect().await.unwrap().to_bytes();
    let segments: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(segments[0]["serviceId"], "support");
    assert_eq!(segments[0]["workloadId"], "support-api");
    assert_eq!(segments[0]["operation"], "POST /tickets");
    assert_eq!(segments[0]["status"], "succeeded");

    db.cleanup().await;
}
