use app_api::build_router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use platform_admin_data::{AdminModule, install_admin_modules};
use platform_core::{AppConfig, AppContext, LoggingEventPublisher};
use platform_module::{
    AdminDataSource, AdminListQuery, AdminPage, AdminSchema, EntitySchema, FieldSchema, FieldType,
};
use serde_json::Value;
use std::sync::Arc;
use tower::ServiceExt;

#[derive(Debug)]
struct StubUsers;

#[async_trait::async_trait]
impl AdminDataSource for StubUsers {
    async fn list(&self, entity: &str, _q: &AdminListQuery) -> platform_core::AppResult<AdminPage> {
        assert_eq!(entity, "users");
        Ok(AdminPage {
            records: vec![serde_json::json!({"id": "usr_1", "email": "a@example.com"})],
            next_cursor: None,
        })
    }
    async fn get(&self, _entity: &str, id: &str) -> platform_core::AppResult<Option<Value>> {
        Ok((id == "usr_1").then(|| serde_json::json!({"id": "usr_1", "email": "a@example.com"})))
    }
}

fn stub_schema() -> AdminSchema {
    AdminSchema {
        entities: vec![EntitySchema {
            name: "users".to_owned(),
            label: "Users".to_owned(),
            read_capability: "identity.users.read".to_owned(),
            fields: vec![FieldSchema {
                name: "email".into(),
                label: "Email".into(),
                field_type: FieldType::String,
                nullable: false,
            }],
        }],
    }
}

fn app() -> axum::Router {
    install_admin_modules(vec![AdminModule {
        module_name: "identity".to_owned(),
        schema: stub_schema(),
        data_source: Arc::new(StubUsers),
    }]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test").expect("lazy pool"),
        Arc::new(LoggingEventPublisher),
    );
    build_router(ctx)
}

fn admin_get(path: &str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .header("authorization", "Bearer dev-service:admin")
        .body(Body::empty())
        .expect("request builds")
}

#[tokio::test]
async fn schema_endpoint_requires_auth() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/admin/data/schema")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("request completes");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn schema_endpoint_lists_installed_modules() {
    let response = app()
        .oneshot(admin_get("/admin/data/schema"))
        .await
        .expect("request completes");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.expect("body");
    let json: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(json["modules"][0]["module_name"], "identity");
    assert_eq!(json["modules"][0]["schema"]["entities"][0]["name"], "users");
}

#[tokio::test]
async fn list_records_returns_stub_data() {
    let response = app()
        .oneshot(admin_get("/admin/data/identity/users"))
        .await
        .expect("request completes");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.expect("body");
    let json: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(json["data"][0]["id"], "usr_1");
    assert_eq!(json["data"][0]["email"], "a@example.com");
}

#[tokio::test]
async fn unknown_module_returns_404() {
    let response = app()
        .oneshot(admin_get("/admin/data/widgets/things"))
        .await
        .expect("request completes");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
