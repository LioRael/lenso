use auth::session_policy::{
    AuthHostExtension, AuthSessionPolicy, SessionCreateDecision, SessionCreateInput,
};
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use lenso_api::try_build_router_with_composition;
use platform_core::{AppConfig, AppContext, LoggingEventPublisher, Migration, apply_migrations};
use platform_module::{HostLinkedModule, ModuleManifest};
use platform_testing::TestDatabase;
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

#[tokio::test]
async fn api_projects_host_wiring_auth_session_policy_into_password_routes() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let mut config = AppConfig::from_env();
    config.database.url = db.url.clone();
    let composition =
        lenso_bootstrap::HostComposition::new().with_linked_module(policy_linked_module());
    let migrations = lenso_bootstrap::migrations_for_config_with_composition(&config, &composition)
        .expect("migrations should compose");
    apply_migrations(&db.pool, &migrations)
        .await
        .expect("migrations apply");

    let ctx = AppContext::new(config, db.pool.clone(), Arc::new(LoggingEventPublisher));
    let app = try_build_router_with_composition(ctx, &composition).expect("router should build");
    let response = app
        .oneshot(json_request(
            "/v1/auth/password/register",
            json!({
                "identifier": "host-wiring@example.com",
                "password": "correct-password",
                "device_id": "browser_device"
            }),
        ))
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let json: Value = serde_json::from_slice(&body).expect("json response");
    let session_id = json["session_id"].as_str().expect("session id");
    let device_id = sqlx::query_scalar::<_, Option<String>>(
        "select device_id from auth.sessions where id = $1",
    )
    .bind(session_id)
    .fetch_one(&db.pool)
    .await
    .expect("session row should exist");

    assert_eq!(device_id.as_deref(), Some("device_from_host_wiring"));

    db.cleanup().await;
}

fn json_request(path: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&body).expect("serialize body"),
        ))
        .expect("request builds")
}

const POLICY_MIGRATIONS: &[Migration] = &[];

fn policy_manifest() -> ModuleManifest {
    ModuleManifest::builder("test-policy").build()
}

fn policy_linked_module() -> HostLinkedModule {
    HostLinkedModule::manifest_only("test-policy", policy_manifest, POLICY_MIGRATIONS)
        .with_contribution(AuthHostExtension::session_policy(policy_factory))
}

fn policy_factory(_ctx: &AppContext) -> Arc<dyn AuthSessionPolicy> {
    Arc::new(TestPolicy)
}

#[derive(Debug)]
struct TestPolicy;

#[async_trait::async_trait]
impl AuthSessionPolicy for TestPolicy {
    async fn before_session_create(
        &self,
        input: &SessionCreateInput,
    ) -> platform_core::AppResult<SessionCreateDecision> {
        assert_eq!(input.proposed_device_id.as_deref(), Some("browser_device"));
        Ok(SessionCreateDecision {
            device_id: Some("device_from_host_wiring".to_owned()),
        })
    }
}
