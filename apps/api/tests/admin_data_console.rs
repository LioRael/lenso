use app_api::build_router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use platform_admin_data::{
    AdminModule, AdminModuleMetadata, install_admin_module_metadata,
    install_admin_module_metadata_refresh_fn, install_admin_module_refresh_fn,
    install_admin_modules,
};
use platform_core::{AppConfig, AppContext, LoggingEventPublisher};
use platform_module::{
    AdminDataSource, AdminListQuery, AdminPage, AdminSchema, AdminSurface, EntitySchema,
    FieldSchema, FieldType, ModuleLoadStatus, ModuleSource,
};
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::Mutex;
use tower::ServiceExt;

static ADMIN_DATA_CONSOLE_TEST_LOCK: Mutex<()> = Mutex::const_new(());

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
        source: ModuleSource::Linked,
        load_status: ModuleLoadStatus::Loaded,
        schema: stub_schema(),
        listed_in_schema: true,
        data_source: Some(Arc::new(StubUsers)),
    }]);
    install_admin_module_metadata(vec![AdminModuleMetadata {
        module_name: "identity".to_owned(),
        source: ModuleSource::Linked,
        load_status: ModuleLoadStatus::Loaded,
        admin: Some(AdminSurface::Schema(stub_schema())),
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

fn admin_post(path: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("authorization", "Bearer dev-service:admin")
        .body(Body::empty())
        .expect("request builds")
}

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    serde_json::from_slice(&bytes).expect("json body")
}

#[tokio::test]
async fn schema_endpoint_requires_auth() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
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
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let response = app()
        .oneshot(admin_get("/admin/data/schema"))
        .await
        .expect("request completes");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let json: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(json["modules"][0]["module_name"], "identity");
    assert_eq!(json["modules"][0]["source"], "linked");
    assert_eq!(json["modules"][0]["status"], "loaded");
    assert_eq!(json["modules"][0]["error"], Value::Null);
    assert_eq!(json["modules"][0]["schema"]["entities"][0]["name"], "users");
}

#[tokio::test]
async fn modules_endpoint_lists_admin_surface_metadata() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let response = app()
        .oneshot(admin_get("/admin/data/modules"))
        .await
        .expect("request completes");
    assert_eq!(response.status(), StatusCode::OK);
    let json = json_body(response).await;
    assert_eq!(json["modules"][0]["module_name"], "identity");
    assert_eq!(json["modules"][0]["source"], "linked");
    assert_eq!(json["modules"][0]["status"], "loaded");
    assert_eq!(json["modules"][0]["error"], Value::Null);
    assert_eq!(json["modules"][0]["admin"]["kind"], "schema");
    assert_eq!(json["modules"][0]["admin"]["entities"][0]["name"], "users");
}

#[tokio::test]
async fn list_records_returns_stub_data() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let response = app()
        .oneshot(admin_get("/admin/data/identity/users"))
        .await
        .expect("request completes");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let json: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(json["data"][0]["id"], "usr_1");
    assert_eq!(json["data"][0]["email"], "a@example.com");
}

#[tokio::test]
async fn unlisted_modules_can_read_records_without_schema_discovery() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    install_admin_modules(vec![
        AdminModule {
            module_name: "identity".to_owned(),
            source: ModuleSource::Linked,
            load_status: ModuleLoadStatus::Loaded,
            schema: stub_schema(),
            listed_in_schema: true,
            data_source: Some(Arc::new(StubUsers)),
        },
        AdminModule {
            module_name: "identity-declarative".to_owned(),
            source: ModuleSource::Linked,
            load_status: ModuleLoadStatus::Loaded,
            schema: stub_schema(),
            listed_in_schema: false,
            data_source: Some(Arc::new(StubUsers)),
        },
    ]);
    install_admin_module_metadata(vec![]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test").expect("lazy pool"),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let schema_response = app
        .clone()
        .oneshot(admin_get("/admin/data/schema"))
        .await
        .expect("schema request completes");
    let schema = json_body(schema_response).await;
    assert!(
        !schema["modules"]
            .as_array()
            .expect("modules array")
            .iter()
            .any(|module| module["module_name"] == "identity-declarative")
    );

    let list_response = app
        .oneshot(admin_get("/admin/data/identity-declarative/users"))
        .await
        .expect("list request completes");
    assert_eq!(list_response.status(), StatusCode::OK);
    let list = json_body(list_response).await;
    assert_eq!(list["data"][0]["id"], "usr_1");
}

#[tokio::test]
async fn unknown_module_returns_404() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let response = app()
        .oneshot(admin_get("/admin/data/widgets/things"))
        .await
        .expect("request completes");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn refresh_schema_replaces_installed_modules() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    static REFRESH_COUNT: AtomicUsize = AtomicUsize::new(0);

    install_admin_modules(vec![AdminModule {
        module_name: "identity".to_owned(),
        source: ModuleSource::Linked,
        load_status: ModuleLoadStatus::Loaded,
        schema: stub_schema(),
        listed_in_schema: true,
        data_source: Some(Arc::new(StubUsers)),
    }]);
    install_admin_module_refresh_fn(|| async {
        REFRESH_COUNT.fetch_add(1, Ordering::SeqCst);
        Ok(vec![
            AdminModule {
                module_name: "identity".to_owned(),
                source: ModuleSource::Linked,
                load_status: ModuleLoadStatus::Loaded,
                schema: stub_schema(),
                listed_in_schema: true,
                data_source: Some(Arc::new(StubUsers)),
            },
            AdminModule {
                module_name: "remote-crm".to_owned(),
                source: ModuleSource::Remote,
                load_status: ModuleLoadStatus::Error {
                    message: "remote manifest request failed".to_owned(),
                },
                schema: AdminSchema { entities: vec![] },
                listed_in_schema: true,
                data_source: None,
            },
        ])
    });
    install_admin_module_metadata_refresh_fn(|| async {
        Ok(vec![
            AdminModuleMetadata {
                module_name: "identity".to_owned(),
                source: ModuleSource::Linked,
                load_status: ModuleLoadStatus::Loaded,
                admin: Some(AdminSurface::Schema(stub_schema())),
            },
            AdminModuleMetadata {
                module_name: "remote-crm".to_owned(),
                source: ModuleSource::Remote,
                load_status: ModuleLoadStatus::Error {
                    message: "remote manifest request failed".to_owned(),
                },
                admin: None,
            },
        ])
    });
    let ctx = AppContext::new(
        AppConfig::from_env(),
        platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test").expect("lazy pool"),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let refresh_response = app
        .clone()
        .oneshot(admin_post("/admin/data/schema/refresh"))
        .await
        .expect("refresh request completes");
    assert_eq!(refresh_response.status(), StatusCode::OK);
    let refresh_body = json_body(refresh_response).await;
    let refreshed_remote = refresh_body["modules"]
        .as_array()
        .expect("modules array")
        .iter()
        .find(|module| module["module_name"] == "remote-crm")
        .expect("remote-crm was refreshed");
    assert_eq!(refreshed_remote["status"], "error");
    assert_eq!(REFRESH_COUNT.load(Ordering::SeqCst), 1);

    let modules_response = app
        .clone()
        .oneshot(admin_get("/admin/data/modules"))
        .await
        .expect("modules request completes");
    let modules_body = json_body(modules_response).await;
    let refreshed_remote_metadata = modules_body["modules"]
        .as_array()
        .expect("modules array")
        .iter()
        .find(|module| module["module_name"] == "remote-crm")
        .expect("remote-crm metadata was refreshed");
    assert_eq!(refreshed_remote_metadata["status"], "error");
    assert_eq!(refreshed_remote_metadata["admin"], Value::Null);

    let schema_response = app
        .oneshot(admin_get("/admin/data/schema"))
        .await
        .expect("schema request completes");
    let schema_body = json_body(schema_response).await;
    assert!(
        schema_body["modules"]
            .as_array()
            .expect("modules array")
            .iter()
            .any(|module| module["module_name"] == "remote-crm")
    );
}
