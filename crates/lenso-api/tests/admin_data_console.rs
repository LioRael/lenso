use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use lenso_api::build_router;
use platform_admin_data::{
    AdminModule, AdminModuleMetadata, AdminModuleSourceDiagnostics, AdminRemoteModuleDiagnostics,
    install_admin_module_metadata, install_admin_module_metadata_refresh_fn,
    install_admin_module_refresh_fn, install_admin_modules,
};
use platform_core::{
    AppConfig, AppContext, LoggingEventPublisher, ModuleSourcesConfig, PLATFORM_MIGRATIONS,
    RemoteModuleSourceConfig, StoryDisplayDescriptor, StoryDisplaySource, apply_migrations,
};
use platform_module::{
    AdminAction, AdminActionConfirmation, AdminActionDangerLevel, AdminActionInputField,
    AdminActionInputSchema, AdminActionSource, AdminDataSource, AdminDeclarativeComponent,
    AdminDeclarativePage, AdminDeclarativeSection, AdminDeclarativeSurface, AdminListQuery,
    AdminPage, AdminQuerySource, AdminSchema, AdminSurface, EntitySchema, EventSurface,
    FieldSchema, FieldType, ModuleHttpRoute, ModuleLoadStatus, ModuleSource, RuntimeSurface,
};
use platform_runtime::RUNTIME_MIGRATIONS;
use platform_testing::TestDatabase;
use serde_json::Value;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::Mutex;
use tower::ServiceExt;

static ADMIN_DATA_CONSOLE_TEST_LOCK: Mutex<()> = Mutex::const_new(());

fn lazy_failing_db() -> platform_core::DbPool {
    // ponytail: no-DB tests only need optional persistence to fail fast.
    PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_millis(50))
        .connect_lazy_with(
            PgConnectOptions::new()
                .host("127.0.0.1")
                .port(1)
                .username("postgres")
                .database("lenso_test"),
        )
}

fn app_config_with_default_modules() -> AppConfig {
    let mut config = AppConfig::from_env();
    // ponytail: these metadata tests assert built-in demo modules, not local .env toggles.
    config.module_sources = ModuleSourcesConfig::default();
    config.modules.clear();
    config
}

fn remove_module_catalog_fixture() {
    let _ = fs::remove_file(Path::new(".lenso/module-catalog.json"));
}

struct FileFixture {
    original: Option<Vec<u8>>,
    path: PathBuf,
}

impl FileFixture {
    fn write(path: impl Into<PathBuf>, contents: impl AsRef<[u8]>) -> Self {
        let path = path.into();
        let fixture = Self {
            original: fs::read(&path).ok(),
            path,
        };
        if let Some(parent) = fixture.path.parent() {
            fs::create_dir_all(parent).expect("create fixture parent");
        }
        fs::write(&fixture.path, contents).expect("write fixture");
        fixture
    }

    fn remove(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let fixture = Self {
            original: fs::read(&path).ok(),
            path,
        };
        let _ = fs::remove_file(&fixture.path);
        fixture
    }
}

impl Drop for FileFixture {
    fn drop(&mut self) {
        match &self.original {
            Some(original) => {
                let _ = fs::write(&self.path, original);
            }
            None => {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}

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

#[derive(Debug)]
struct StubActions;

#[async_trait::async_trait]
impl AdminActionSource for StubActions {
    async fn invoke(&self, action: &str, input: Value) -> platform_core::AppResult<Value> {
        Ok(serde_json::json!({
            "action": action,
            "input": input,
        }))
    }
}

#[derive(Debug)]
struct StubQueries;

#[async_trait::async_trait]
impl AdminQuerySource for StubQueries {
    async fn query(&self, query: &str) -> platform_core::AppResult<Value> {
        Ok(serde_json::json!({
            "query": query,
            "metrics": {
                "contacts": 2,
                "healthy": true
            }
        }))
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

fn stub_declarative_surface() -> AdminSurface {
    AdminSurface::DeclarativeCustom(AdminDeclarativeSurface {
        pages: vec![AdminDeclarativePage {
            name: "overview".to_owned(),
            label: "Overview".to_owned(),
            sections: vec![
                AdminDeclarativeSection {
                    name: "health".to_owned(),
                    label: "Health".to_owned(),
                    component: AdminDeclarativeComponent::QueryValue {
                        query: "health".to_owned(),
                        capability: "remote_crm.health.read".to_owned(),
                        value_path: "metrics.contacts".to_owned(),
                    },
                },
                AdminDeclarativeSection {
                    name: "contacts".to_owned(),
                    label: "Contacts".to_owned(),
                    component: AdminDeclarativeComponent::EntityTable {
                        entity: "users".to_owned(),
                    },
                },
            ],
        }],
        actions: vec![
            AdminAction {
                name: "sync_contacts".to_owned(),
                label: "Sync contacts".to_owned(),
                capability: "remote_crm.contacts.sync".to_owned(),
                input_schema: Some(AdminActionInputSchema {
                    fields: vec![AdminActionInputField {
                        name: "dry_run".to_owned(),
                        label: "Dry run".to_owned(),
                        field_type: FieldType::Boolean,
                        required: false,
                        description: Some(
                            "Preview the sync without writing remote data".to_owned(),
                        ),
                    }],
                }),
                confirmation: None,
                danger_level: AdminActionDangerLevel::Low,
                operation: None,
            },
            AdminAction {
                name: "danger_sync".to_owned(),
                label: "Danger sync".to_owned(),
                capability: "remote_crm.contacts.sync".to_owned(),
                input_schema: None,
                confirmation: Some(AdminActionConfirmation {
                    message: "This action writes remote contact data.".to_owned(),
                    required_phrase: Some("SYNC".to_owned()),
                }),
                danger_level: AdminActionDangerLevel::High,
                operation: None,
            },
            AdminAction {
                name: "validated_sync".to_owned(),
                label: "Validated sync".to_owned(),
                capability: "remote_crm.contacts.sync".to_owned(),
                input_schema: Some(AdminActionInputSchema {
                    fields: vec![
                        AdminActionInputField {
                            name: "limit".to_owned(),
                            label: "Limit".to_owned(),
                            field_type: FieldType::Integer,
                            required: true,
                            description: Some("Maximum contacts to sync".to_owned()),
                        },
                        AdminActionInputField {
                            name: "filter".to_owned(),
                            label: "Filter".to_owned(),
                            field_type: FieldType::Json,
                            required: false,
                            description: Some("Optional JSON filter".to_owned()),
                        },
                    ],
                }),
                confirmation: None,
                danger_level: AdminActionDangerLevel::Low,
                operation: None,
            },
        ],
        fallback_schema: Some(stub_schema()),
    })
}

fn app() -> axum::Router {
    install_admin_modules(vec![AdminModule {
        module_name: "identity".to_owned(),
        source: ModuleSource::Linked,
        load_status: ModuleLoadStatus::Loaded,
        schema: stub_schema(),
        admin: Some(AdminSurface::Schema(stub_schema())),
        listed_in_schema: true,
        data_source: Some(Arc::new(StubUsers)),
        action_source: None,
        query_source: None,
    }]);
    install_admin_module_metadata(vec![AdminModuleMetadata {
        module_name: "identity".to_owned(),
        source: ModuleSource::Linked,
        load_status: ModuleLoadStatus::Loaded,
        http_routes: vec![],
        runtime: None,
        events: None,
        lifecycle: None,
        console: vec![],
        console_slots: Vec::new(),
        console_contributions: Vec::new(),
        story_display: vec![],
        capabilities: vec![],
        dependencies: vec![],
        admin: Some(AdminSurface::Schema(stub_schema())),
        source_diagnostics: None,
    }]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    build_router(ctx)
}

async fn app_with_test_db(db: &TestDatabase) -> axum::Router {
    let migrations = PLATFORM_MIGRATIONS
        .iter()
        .chain(RUNTIME_MIGRATIONS)
        .copied()
        .collect::<Vec<_>>();
    apply_migrations(&db.pool, &migrations)
        .await
        .expect("platform and runtime migrations apply");
    let mut config = AppConfig::from_env();
    config.database.url = db.url.clone();
    let ctx = AppContext::new(config, db.pool.clone(), Arc::new(LoggingEventPublisher));
    build_router(ctx)
}

fn admin_get(path: &str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .header("authorization", "Bearer dev-service:admin")
        .body(Body::empty())
        .expect("request builds")
}

fn admin_get_with_token(path: &str, token: &str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .header("authorization", format!("Bearer {token}"))
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

fn admin_delete(path: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(path)
        .header("authorization", "Bearer dev-service:admin")
        .body(Body::empty())
        .expect("request builds")
}

fn admin_post_json(path: &str, body: &'static str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("authorization", "Bearer dev-service:admin")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .expect("request builds")
}

fn admin_post_json_with_token(path: &str, body: &'static str, token: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body))
        .expect("request builds")
}

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    serde_json::from_slice(&bytes).expect("json body")
}

trait RequestExt {
    fn with_header(self, name: &'static str, value: &'static str) -> Self;
}

impl RequestExt for Request<Body> {
    fn with_header(mut self, name: &'static str, value: &'static str) -> Self {
        self.headers_mut()
            .insert(name, value.parse().expect("header value"));
        self
    }
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
async fn modules_endpoint_lists_registry_metadata() {
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
    assert_eq!(json["modules"][0]["http_routes"], serde_json::json!([]));
    assert_eq!(json["modules"][0]["runtime"], Value::Null);
    assert_eq!(json["modules"][0]["story_display"], serde_json::json!([]));
    assert_eq!(json["modules"][0]["capabilities"], serde_json::json!([]));
    assert_eq!(json["modules"][0]["admin"]["kind"], "schema");
    assert_eq!(json["modules"][0]["admin"]["entities"][0]["name"], "users");
    assert!(json["refreshed_at"].as_str().is_some());
    assert_eq!(json["refresh_error"], Value::Null);
}

#[tokio::test]
async fn modules_endpoint_lists_linked_module_http_routes() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let ctx = AppContext::new(
        app_config_with_default_modules(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    install_admin_modules(lenso_bootstrap::admin_modules(&ctx));
    install_admin_module_metadata(
        lenso_bootstrap::load_admin_module_metadata(&ctx)
            .await
            .expect("admin module metadata loads"),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_get("/admin/data/modules"))
        .await
        .expect("request completes");
    assert_eq!(response.status(), StatusCode::OK);
    let json = json_body(response).await;
    let auth = json["modules"]
        .as_array()
        .expect("modules array")
        .iter()
        .find(|module| module["module_name"] == "auth")
        .expect("auth module metadata");
    assert_eq!(auth["source"], "linked");
    assert_eq!(auth["http_routes"][0]["method"], "POST");
    assert_eq!(auth["http_routes"][0]["path"], "/v1/auth/dev/sessions");
    assert_eq!(
        auth["http_routes"][0]["display_name"],
        "Create Development Session"
    );
    assert_eq!(
        auth["http_routes"][0]["story_title"],
        "Development Auth Session"
    );
    assert_eq!(auth["http_routes"][1]["method"], "POST");
    assert_eq!(auth["http_routes"][1]["path"], "/v1/auth/sessions/revoke");
    assert_eq!(auth["http_routes"][1]["display_name"], "Revoke Session");
    assert_eq!(auth["console"].as_array().expect("console array").len(), 2);
    assert_eq!(auth["console"][0]["name"], "sessions");
    assert_eq!(auth["console"][0]["label"], "Sessions");
    assert_eq!(auth["console"][0]["area"], "data");
    assert_eq!(auth["console"][0]["route"], "/data/auth/sessions");
    assert_eq!(auth["console"][0]["package"]["name"], "@lenso/auth-console");
    assert_eq!(auth["console"][0]["package"]["export"], "authConsoleModule");
    assert_eq!(
        auth["console"][0]["required_capabilities"],
        serde_json::json!(["auth.users.read"])
    );
    assert_eq!(auth["console"][1]["name"], "users");
    assert_eq!(auth["console"][1]["label"], "Users");
    assert_eq!(auth["console"][1]["route"], "/data/auth/users");
}

#[tokio::test]
async fn modules_endpoint_lists_linked_modules_without_admin_surfaces() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let ctx = AppContext::new(
        app_config_with_default_modules(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    install_admin_modules(lenso_bootstrap::admin_modules(&ctx));
    install_admin_module_metadata(
        lenso_bootstrap::load_admin_module_metadata(&ctx)
            .await
            .expect("admin module metadata loads"),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_get("/admin/data/modules"))
        .await
        .expect("request completes");
    assert_eq!(response.status(), StatusCode::OK);
    let json = json_body(response).await;
    let auth_password = json["modules"]
        .as_array()
        .expect("modules array")
        .iter()
        .find(|module| module["module_name"] == "auth-password")
        .expect("auth-password module metadata");
    assert_eq!(auth_password["source"], "linked");
    assert_eq!(auth_password["status"], "loaded");
    assert_eq!(auth_password["error"], Value::Null);
    assert_eq!(auth_password["runtime"], Value::Null);
    assert_eq!(auth_password["dependencies"], serde_json::json!(["auth"]));
    assert_eq!(auth_password["admin"], Value::Null);
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
async fn admin_action_invocation_calls_declared_source() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    install_admin_modules(vec![AdminModule {
        module_name: "remote-crm".to_owned(),
        source: ModuleSource::Remote,
        load_status: ModuleLoadStatus::Loaded,
        schema: stub_schema(),
        admin: Some(stub_declarative_surface()),
        listed_in_schema: false,
        data_source: Some(Arc::new(StubUsers)),
        action_source: Some(Arc::new(StubActions)),
        query_source: None,
    }]);
    install_admin_module_metadata(vec![AdminModuleMetadata {
        module_name: "remote-crm".to_owned(),
        source: ModuleSource::Remote,
        load_status: ModuleLoadStatus::Loaded,
        http_routes: vec![],
        runtime: None,
        events: None,
        lifecycle: None,
        console: vec![],
        console_slots: Vec::new(),
        console_contributions: Vec::new(),
        story_display: vec![],
        capabilities: vec!["remote_crm.contacts.sync".to_owned()],
        dependencies: vec![],
        admin: Some(stub_declarative_surface()),
        source_diagnostics: None,
    }]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_post_json_with_token(
            "/admin/data/remote-crm/actions/sync_contacts",
            r#"{"input":{"dry_run":true}}"#,
            "dev-service:admin:remote_crm.contacts.sync",
        ))
        .await
        .expect("request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let json = json_body(response).await;
    assert_eq!(json["data"]["action"], "sync_contacts");
    assert_eq!(json["data"]["input"]["dry_run"], true);
}

#[tokio::test]
async fn admin_query_invocation_calls_declared_source() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    install_admin_modules(vec![AdminModule {
        module_name: "remote-crm".to_owned(),
        source: ModuleSource::Remote,
        load_status: ModuleLoadStatus::Loaded,
        schema: stub_schema(),
        admin: Some(stub_declarative_surface()),
        listed_in_schema: false,
        data_source: Some(Arc::new(StubUsers)),
        action_source: Some(Arc::new(StubActions)),
        query_source: Some(Arc::new(StubQueries)),
    }]);
    install_admin_module_metadata(vec![AdminModuleMetadata {
        module_name: "remote-crm".to_owned(),
        source: ModuleSource::Remote,
        load_status: ModuleLoadStatus::Loaded,
        http_routes: vec![],
        runtime: None,
        events: None,
        lifecycle: None,
        console: vec![],
        console_slots: Vec::new(),
        console_contributions: Vec::new(),
        story_display: vec![],
        capabilities: vec!["remote_crm.health.read".to_owned()],
        dependencies: vec![],
        admin: Some(stub_declarative_surface()),
        source_diagnostics: None,
    }]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_get_with_token(
            "/admin/data/remote-crm/queries/health",
            "dev-service:admin:remote_crm.health.read",
        ))
        .await
        .expect("request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let json = json_body(response).await;
    assert_eq!(json["data"]["query"], "health");
    assert_eq!(json["data"]["metrics"]["contacts"], 2);
}

#[tokio::test]
async fn available_modules_returns_remote_install_rows() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    remove_module_catalog_fixture();
    install_admin_modules(vec![]);
    install_admin_module_metadata(vec![AdminModuleMetadata {
        module_name: "billing".to_owned(),
        source: ModuleSource::Remote,
        load_status: ModuleLoadStatus::Loaded,
        http_routes: vec![],
        runtime: None,
        events: None,
        lifecycle: None,
        console: vec![],
        console_slots: Vec::new(),
        console_contributions: Vec::new(),
        story_display: vec![],
        capabilities: vec!["billing.read".to_owned()],
        dependencies: vec![],
        admin: None,
        source_diagnostics: Some(AdminModuleSourceDiagnostics::Remote(
            AdminRemoteModuleDiagnostics {
                transport: "http_json".to_owned(),
                base_url: "https://example.com/lenso/module/v1".to_owned(),
                manifest_url: "https://example.com/lenso/module/v1/manifest".to_owned(),
                timeout_ms: 1000,
                auth_configured: false,
                load_duration_ms: Some(42),
                last_checked_at: Some("2026-06-07T00:00:00Z".to_owned()),
                last_load_error: None,
            },
        )),
    }]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .clone()
        .oneshot(admin_get("/admin/data/available-modules"))
        .await
        .expect("available modules request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["version"], 1);
    assert_eq!(body["status"], "passed");
    assert_eq!(body["catalog"]["modules"], 1);
    assert_eq!(
        body["catalog"]["registryFile"],
        "host-admin-module-metadata"
    );
    assert_eq!(body["modules"][0]["name"], "billing");
    assert_eq!(body["modules"][0]["source"], "remote");
    assert_eq!(body["modules"][0]["catalogVersion"], "unknown");
    assert_eq!(
        body["modules"][0]["manifestReference"],
        "https://example.com/lenso/module/v1/manifest"
    );
    assert_eq!(
        body["modules"][0]["baseUrl"],
        "https://example.com/lenso/module/v1"
    );
    assert_eq!(body["modules"][0]["capabilities"][0], "billing.read");
    assert_eq!(
        body["modules"][0]["hostCompatibility"]["consolePackageApi"],
        "1"
    );
    assert_eq!(
        body["modules"][0]["hostCompatibility"]["lensoVersion"],
        "0.1.7"
    );
    assert_eq!(body["modules"][0]["manifestStatus"], "ok");
    assert_eq!(body["modules"][0]["status"], "ready");

    let legacy_response = app
        .oneshot(admin_get("/admin/data/module-registry/snapshot"))
        .await
        .expect("legacy snapshot request completes");
    assert_eq!(legacy_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn service_modules_returns_empty_when_none_configured() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let _env = FileFixture::remove(".env");
    let _ledger = FileFixture::remove(".lenso/module-installs.json");
    let _services = FileFixture::remove(".lenso/module-services.json");
    install_admin_module_metadata(vec![]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_get("/admin/data/service-modules"))
        .await
        .expect("service modules request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["status"], "empty");
    assert_eq!(body["modules"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn service_modules_marks_restart_pending_from_env_source() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let _env = FileFixture::write(".env", "REMOTE_MODULES=billing=grpc://example.com:50051\n");
    let _ledger = FileFixture::remove(".lenso/module-installs.json");
    let _services = FileFixture::remove(".lenso/module-services.json");
    install_admin_module_metadata(vec![]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_get("/admin/data/service-modules"))
        .await
        .expect("service modules request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["status"], "needs_attention");
    assert_eq!(body["modules"][0]["moduleName"], "billing");
    assert_eq!(body["modules"][0]["status"], "restart_pending");
    assert_eq!(body["modules"][0]["manifestStatus"], "skipped");
    assert_eq!(body["modules"][0]["configured"], true);
    assert_eq!(body["modules"][0]["loaded"], false);
}

#[tokio::test]
async fn service_modules_include_service_release_history() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let _env = FileFixture::write(".env", "REMOTE_MODULES=billing=grpc://example.com:50051\n");
    let _ledger = FileFixture::remove(".lenso/module-installs.json");
    let _services = FileFixture::remove(".lenso/module-services.json");
    let _release_ledger = FileFixture::write(
        ".lenso/service-releases.json",
        serde_json::json!({
            "version": 1,
            "releases": [
                {
                    "id": "rel_old",
                    "serviceName": "billing",
                    "appliedAtUnixMs": 100,
                    "risk": "safe",
                    "current": {
                        "version": "0.1.0",
                        "manifestReference": "./billing/v1/lenso.service.json"
                    },
                    "candidate": {
                        "version": "0.2.0",
                        "manifestReference": "./billing/v2/lenso.service.json",
                        "packageReference": "./billing/v2/lenso.service-package.json"
                    },
                    "rollbackTarget": "./billing/v1/lenso.service.json"
                },
                {
                    "id": "rel_new",
                    "serviceName": "billing",
                    "appliedAtUnixMs": 200,
                    "risk": "breaking",
                    "current": {
                        "version": "0.2.0",
                        "manifestReference": "./billing/v2/lenso.service.json"
                    },
                    "candidate": {
                        "version": "0.3.0",
                        "manifestReference": "./billing/v3/lenso.service.json"
                    },
                    "rollbackTarget": "./billing/v2/lenso.service.json"
                }
            ]
        })
        .to_string(),
    );
    install_admin_module_metadata(vec![]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_get("/admin/data/service-modules"))
        .await
        .expect("service modules request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let module = &body["modules"][0];
    assert_eq!(module["moduleName"], "billing");
    assert_eq!(module["latestRelease"]["id"], "rel_new");
    assert_eq!(module["latestRelease"]["risk"], "breaking");
    assert_eq!(module["latestRelease"]["candidateVersion"], "0.3.0");
    assert_eq!(module["releaseHistory"].as_array().unwrap().len(), 2);
    assert_eq!(module["releaseHistory"][1]["id"], "rel_old");
    assert_eq!(
        module["releaseHistory"][1]["candidatePackageReference"],
        "./billing/v2/lenso.service-package.json"
    );
}

#[tokio::test]
async fn service_modules_include_service_environment_and_deployment_state() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let _env = FileFixture::write(".env", "REMOTE_MODULES=billing=grpc://example.com:50051\n");
    let _ledger = FileFixture::remove(".lenso/module-installs.json");
    let _services = FileFixture::remove(".lenso/module-services.json");
    let _release_ledger = FileFixture::write(
        ".lenso/service-releases.json",
        serde_json::json!({
            "version": 1,
            "releases": [{
                "id": "rel_staging",
                "serviceName": "billing",
                "appliedAtUnixMs": 200,
                "risk": "safe",
                "environment": {
                    "name": "staging",
                    "target": "kubernetes",
                    "namespace": "lenso-staging",
                    "image": "ghcr.io/acme/billing:0.3.0"
                },
                "current": { "version": "0.2.0" },
                "candidate": { "version": "0.3.0" }
            }]
        })
        .to_string(),
    );
    let _environments = FileFixture::write(
        ".lenso/service-environments.json",
        serde_json::json!({
            "version": 1,
            "environments": [{
                "name": "staging",
                "serviceName": "billing",
                "target": "kubernetes",
                "namespace": "lenso-staging",
                "image": "ghcr.io/acme/billing:0.3.0",
                "manifestReference": "https://billing.example.com/lenso/service/v1/manifest"
            }]
        })
        .to_string(),
    );
    let _deployments = FileFixture::write(
        ".lenso/service-deployments.json",
        serde_json::json!({
            "version": 2,
            "observations": [{
                "serviceName": "billing",
                "environment": "staging",
                "target": "kubernetes",
                "observedAtUnixMs": 300,
                "state": "ready",
                "drift": "in_sync",
                "cluster": {
                    "namespace": "lenso-staging",
                    "deployment": "billing",
                    "readyReplicas": 2,
                    "desiredReplicas": 2,
                    "image": "ghcr.io/acme/billing:0.3.0",
                    "releaseId": "rel_staging"
                },
                "host": {
                    "releaseId": "rel_staging",
                    "candidateVersion": "0.3.0"
                },
                "checks": [{
                    "name": "deployment_rollout",
                    "status": "ok",
                    "detail": "2/2 replicas ready"
                }],
                "nextAction": "monitor rollout and Remote Calls"
            }],
            "history": [{
                "serviceName": "billing",
                "environment": "staging",
                "target": "kubernetes",
                "observedAtUnixMs": 100,
                "state": "progressing",
                "drift": "host_ahead",
                "nextAction": "wait for rollout or inspect Kubernetes deployment"
            }, {
                "serviceName": "billing",
                "environment": "staging",
                "target": "kubernetes",
                "observedAtUnixMs": 300,
                "state": "ready",
                "drift": "in_sync",
                "nextAction": "monitor rollout and Remote Calls"
            }]
        })
        .to_string(),
    );
    install_admin_module_metadata(vec![]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_get("/admin/data/service-modules"))
        .await
        .expect("service modules request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let module = &body["modules"][0];
    assert_eq!(module["moduleName"], "billing");
    assert_eq!(module["latestRelease"]["environment"], "staging");
    assert_eq!(module["environments"][0]["target"], "kubernetes");
    assert_eq!(module["environments"][0]["namespace"], "lenso-staging");
    assert_eq!(module["deployments"][0]["state"], "ready");
    assert_eq!(module["deployments"][0]["cluster"]["readyReplicas"], 2);
    assert_eq!(module["deploymentHistory"][0]["state"], "ready");
    assert_eq!(module["deploymentHistory"][1]["state"], "progressing");
    assert_eq!(module["deploymentDrift"], "in_sync");
    assert_eq!(
        module["deploymentNextAction"],
        "monitor rollout and Remote Calls"
    );
}

#[tokio::test]
async fn service_modules_include_operator_managed_deployment_state() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let _env = FileFixture::write(".env", "REMOTE_MODULES=billing=grpc://example.com:50051\n");
    let _ledger = FileFixture::remove(".lenso/module-installs.json");
    let _services = FileFixture::remove(".lenso/module-services.json");
    let _release_ledger = FileFixture::write(
        ".lenso/service-releases.json",
        serde_json::json!({
            "version": 1,
            "releases": [{
                "id": "rel_staging",
                "serviceName": "billing",
                "appliedAtUnixMs": 200,
                "risk": "safe",
                "environment": {
                    "name": "staging",
                    "target": "operator",
                    "namespace": "lenso-staging",
                    "image": "ghcr.io/acme/billing:0.3.0"
                },
                "candidate": { "version": "0.3.0" }
            }]
        })
        .to_string(),
    );
    let _environments = FileFixture::write(
        ".lenso/service-environments.json",
        serde_json::json!({
            "version": 1,
            "environments": [{
                "name": "staging",
                "serviceName": "billing",
                "target": "operator",
                "namespace": "lenso-staging",
                "image": "ghcr.io/acme/billing:0.3.0",
                "manifestReference": "https://billing.example.com/lenso/service/v1/manifest"
            }]
        })
        .to_string(),
    );
    let _deployments = FileFixture::write(
        ".lenso/service-deployments.json",
        serde_json::json!({
            "version": 1,
            "observations": [{
                "serviceName": "billing",
                "environment": "staging",
                "target": "operator",
                "observedAtUnixMs": 300,
                "state": "ready",
                "drift": "in_sync",
                "operator": {
                    "resource": "billing",
                    "namespace": "lenso-staging",
                    "observedGeneration": 3,
                    "conditions": [{
                        "type": "Ready",
                        "status": "True",
                        "reason": "DeploymentAvailable",
                        "message": "2/2 replicas are ready.",
                        "lastTransitionTime": "2026-06-29T00:00:00Z"
                    }]
                },
                "cluster": {
                    "namespace": "lenso-staging",
                    "deployment": "billing",
                    "readyReplicas": 2,
                    "desiredReplicas": 2,
                    "availableReplicas": 2,
                    "image": "ghcr.io/acme/billing:0.3.0",
                    "releaseId": "rel_staging"
                },
                "host": {
                    "releaseId": "rel_staging",
                    "candidateVersion": "0.3.0"
                },
                "checks": [{
                    "name": "operator_reconcile",
                    "status": "ok",
                    "detail": "LensoServiceProvider/billing is ready"
                }],
                "nextAction": "monitor operator conditions, Remote Calls, and Runtime Story"
            }]
        })
        .to_string(),
    );
    install_admin_module_metadata(vec![]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_get("/admin/data/service-modules"))
        .await
        .expect("service modules request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let module = &body["modules"][0];
    assert_eq!(module["environments"][0]["target"], "operator");
    assert_eq!(module["deployments"][0]["target"], "operator");
    assert_eq!(module["deployments"][0]["operator"]["resource"], "billing");
    assert_eq!(
        module["deployments"][0]["operator"]["observedGeneration"],
        3
    );
    assert_eq!(
        module["deployments"][0]["operator"]["conditions"][0]["reason"],
        "DeploymentAvailable"
    );
    assert_eq!(module["deployments"][0]["cluster"]["availableReplicas"], 2);
}

#[tokio::test]
async fn service_modules_marks_stale_state_from_lock_file() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let _env = FileFixture::write(".env", "REMOTE_MODULES=billing=grpc://example.com:50051\n");
    let _ledger = FileFixture::remove(".lenso/module-installs.json");
    let _services = FileFixture::write(
        ".lenso/module-services.json",
        serde_json::json!({
            "version": 1,
            "modules": [{
                "moduleName": "billing",
                "services": [{
                    "name": "api",
                    "command": "pnpm dev",
                    "readyUrl": "http://127.0.0.1:9/readyz",
                    "autoStart": true
                }]
            }]
        })
        .to_string(),
    );
    let _lock = FileFixture::write(".lenso/remote-billing-api.lock", "owner_pid=123\n");
    install_admin_module_metadata(vec![]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_get("/admin/data/service-modules"))
        .await
        .expect("service modules request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["modules"][0]["moduleName"], "billing");
    assert_eq!(body["modules"][0]["status"], "stale_state");
    assert_eq!(body["modules"][0]["services"][0]["ready"], false);
    assert_eq!(
        body["modules"][0]["services"][0]["lockFile"],
        ".lenso/remote-billing-api.lock"
    );
}

#[tokio::test]
async fn service_modules_marks_missing_config_for_host_started_service() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let _env = FileFixture::write(".env", "REMOTE_MODULES=billing=grpc://example.com:50051\n");
    let _ledger = FileFixture::write(
        ".lenso/module-installs.json",
        serde_json::json!({
            "version": 1,
            "modules": [{
                "moduleName": "billing",
                "source": "remote",
                "service": {
                    "name": "billing",
                    "requiredEnv": ["BILLING_API_KEY"]
                }
            }]
        })
        .to_string(),
    );
    let _services = FileFixture::write(
        ".lenso/module-services.json",
        serde_json::json!({
            "version": 1,
            "modules": [{
                "moduleName": "billing",
                "services": [{
                    "name": "api",
                    "command": "pnpm dev",
                    "readyUrl": "http://127.0.0.1:9/readyz",
                    "autoStart": true
                }]
            }]
        })
        .to_string(),
    );
    install_admin_module_metadata(vec![]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_get("/admin/data/service-modules"))
        .await
        .expect("service modules request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let module = &body["modules"][0];
    assert_eq!(module["moduleName"], "billing");
    assert_eq!(module["status"], "missing_config");
    assert_eq!(
        module["config"]["requiredEnv"],
        serde_json::json!(["BILLING_API_KEY"])
    );
    assert_eq!(
        module["config"]["missingEnv"],
        serde_json::json!(["BILLING_API_KEY"])
    );
}

#[tokio::test]
async fn service_modules_marks_loaded_remote_ready() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let _env = FileFixture::write(
        ".env",
        "REMOTE_MODULES=billing=https://example.com/billing\n",
    );
    let _ledger = FileFixture::remove(".lenso/module-installs.json");
    let _services = FileFixture::remove(".lenso/module-services.json");
    install_admin_module_metadata(vec![AdminModuleMetadata {
        module_name: "billing".to_owned(),
        source: ModuleSource::Remote,
        load_status: ModuleLoadStatus::Loaded,
        http_routes: vec![],
        runtime: None,
        events: None,
        lifecycle: None,
        console: vec![],
        console_slots: Vec::new(),
        console_contributions: Vec::new(),
        story_display: vec![],
        capabilities: vec![],
        dependencies: vec![],
        admin: None,
        source_diagnostics: Some(AdminModuleSourceDiagnostics::Remote(
            AdminRemoteModuleDiagnostics {
                transport: "http".to_owned(),
                base_url: "https://example.com/billing".to_owned(),
                manifest_url: "https://example.com/billing/manifest".to_owned(),
                timeout_ms: 5000,
                auth_configured: false,
                load_duration_ms: Some(10),
                last_checked_at: None,
                last_load_error: None,
            },
        )),
    }]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_get("/admin/data/service-modules"))
        .await
        .expect("service modules request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["status"], "ready");
    assert_eq!(body["modules"][0]["moduleName"], "billing");
    assert_eq!(body["modules"][0]["status"], "ready");
    assert_eq!(body["modules"][0]["loaded"], true);
    assert_eq!(body["modules"][0]["manifestStatus"], "reachable");
}

#[tokio::test]
async fn service_modules_merges_service_provider_source_into_provided_module() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let _env = FileFixture::write(
        ".env",
        "REMOTE_MODULES=support-service=http://127.0.0.1:4110/lenso/service/v1\n",
    );
    let _ledger = FileFixture::write(
        ".lenso/module-installs.json",
        serde_json::json!({
            "version": 1,
            "modules": [{
                "moduleName": "support-ticket",
                "source": "remote",
                "service": {
                    "name": "support-service",
                    "statusPath": "/lenso/service/v1/status"
                },
                "moduleRelease": {
                    "manifestReference": "../support/lenso.module-release.json",
                    "manifestSnapshot": {
                        "protocol": "lenso.module-release.v1",
                        "name": "support-ticket",
                        "version": "0.2.0",
                        "provider": {
                            "name": "support-service",
                            "servicePackage": "../support/lenso.service-package.json"
                        }
                    }
                }
            }]
        })
        .to_string(),
    );
    let _health = FileFixture::remove(".lenso/service-health.json");
    let _services = FileFixture::remove(".lenso/module-services.json");
    install_admin_module_metadata(vec![AdminModuleMetadata {
        module_name: "support-ticket".to_owned(),
        source: ModuleSource::Remote,
        load_status: ModuleLoadStatus::Loaded,
        http_routes: vec![],
        runtime: None,
        events: None,
        lifecycle: None,
        console: vec![],
        console_slots: Vec::new(),
        console_contributions: Vec::new(),
        story_display: vec![],
        capabilities: vec![],
        dependencies: vec![],
        admin: None,
        source_diagnostics: Some(AdminModuleSourceDiagnostics::Remote(
            AdminRemoteModuleDiagnostics {
                transport: "http".to_owned(),
                base_url: "http://127.0.0.1:4110/lenso/service/v1/modules/support-ticket"
                    .to_owned(),
                manifest_url:
                    "http://127.0.0.1:4110/lenso/service/v1/modules/support-ticket/manifest"
                        .to_owned(),
                timeout_ms: 5000,
                auth_configured: false,
                load_duration_ms: Some(10),
                last_checked_at: None,
                last_load_error: None,
            },
        )),
    }]);
    let mut config = AppConfig::from_env();
    config.module_sources.remote = vec![RemoteModuleSourceConfig {
        name: "support-service".to_owned(),
        base_url: "http://127.0.0.1:4110/lenso/service/v1".to_owned(),
        auth_token_env: None,
        timeout_ms: 5000,
    }];
    let ctx = AppContext::new(config, lazy_failing_db(), Arc::new(LoggingEventPublisher));
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_get("/admin/data/service-modules"))
        .await
        .expect("service modules request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["status"], "ready");
    assert_eq!(body["modules"].as_array().unwrap().len(), 1);
    assert_eq!(body["modules"][0]["moduleName"], "support-ticket");
    assert_eq!(body["modules"][0]["providerName"], "support-service");
    assert_eq!(body["modules"][0]["status"], "ready");
    assert_eq!(body["modules"][0]["configured"], true);
    assert_eq!(body["modules"][0]["loaded"], true);
    assert_eq!(
        body["modules"][0]["baseUrl"],
        "http://127.0.0.1:4110/lenso/service/v1"
    );
    assert_eq!(
        body["modules"][0]["moduleRelease"]["manifestReference"],
        "../support/lenso.module-release.json"
    );
    assert_eq!(
        body["modules"][0]["moduleRelease"]["providerName"],
        "support-service"
    );
    assert_eq!(
        body["modules"][0]["moduleRelease"]["servicePackage"],
        "../support/lenso.service-package.json"
    );
}

#[tokio::test]
async fn service_modules_exposes_operations_for_provider_modules() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let _env = FileFixture::write(
        ".env",
        "REMOTE_MODULES=support-suite-provider=http://127.0.0.1:4110/lenso/service/v1\n",
    );
    let _ledger = FileFixture::write(
        ".lenso/module-installs.json",
        serde_json::json!({
            "version": 1,
            "modules": [{
                "moduleName": "support-ticket",
                "source": "remote",
                "service": {
                    "name": "support-suite-provider"
                }
            }]
        })
        .to_string(),
    );
    let _health = FileFixture::remove(".lenso/service-health.json");
    let _services = FileFixture::remove(".lenso/module-services.json");
    let http_route: ModuleHttpRoute = serde_json::from_value(serde_json::json!({
        "method": "GET",
        "path": "/tickets",
        "capability": "support_ticket.tickets.read",
        "display_name": "List tickets",
        "story_title": "Tickets listed",
        "operation": {
            "operationId": "support-ticket/http/list",
            "summary": "List tickets through the service",
            "safeProbe": {
                "method": "GET",
                "path": "/tickets",
                "expectStatus": 200
            }
        }
    }))
    .expect("http route metadata");
    let runtime: RuntimeSurface = serde_json::from_value(serde_json::json!({
        "functions": [{
            "name": "support-ticket.reindex.v1",
            "version": 1,
            "queue": "support-ticket"
        }]
    }))
    .expect("runtime metadata");
    let events: EventSurface = serde_json::from_value(serde_json::json!({
        "handlers": [{
            "name": "ticket-created-handler",
            "event_name": "support_ticket.ticket_created.v1"
        }]
    }))
    .expect("event metadata");
    let action: AdminAction = serde_json::from_value(serde_json::json!({
        "name": "assign_ticket",
        "label": "Assign ticket",
        "capability": "support_ticket.tickets.write"
    }))
    .expect("admin action metadata");
    install_admin_module_metadata(vec![AdminModuleMetadata {
        module_name: "support-ticket".to_owned(),
        source: ModuleSource::Remote,
        load_status: ModuleLoadStatus::Loaded,
        http_routes: vec![http_route],
        runtime: Some(runtime),
        events: Some(events),
        lifecycle: None,
        console: vec![],
        console_slots: Vec::new(),
        console_contributions: Vec::new(),
        story_display: vec![],
        capabilities: vec![],
        dependencies: vec![],
        admin: Some(AdminSurface::DeclarativeCustom(AdminDeclarativeSurface {
            pages: vec![],
            actions: vec![action],
            fallback_schema: None,
        })),
        source_diagnostics: Some(AdminModuleSourceDiagnostics::Remote(
            AdminRemoteModuleDiagnostics {
                transport: "http".to_owned(),
                base_url: "http://127.0.0.1:4110/lenso/service/v1/modules/support-ticket"
                    .to_owned(),
                manifest_url:
                    "http://127.0.0.1:4110/lenso/service/v1/modules/support-ticket/manifest"
                        .to_owned(),
                timeout_ms: 5000,
                auth_configured: false,
                load_duration_ms: Some(10),
                last_checked_at: None,
                last_load_error: None,
            },
        )),
    }]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_get("/admin/data/service-modules"))
        .await
        .expect("service modules request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let module = body["modules"]
        .as_array()
        .expect("modules array")
        .iter()
        .find(|module| module["moduleName"] == "support-ticket")
        .expect("support ticket module");
    let operations = module["operations"].as_array().expect("operations array");
    assert_eq!(operations.len(), 4);

    let http = operations
        .iter()
        .find(|operation| operation["kind"] == "http_route")
        .expect("http operation");
    assert_eq!(http["operationId"], "support-ticket/http/list");
    assert_eq!(http["capability"], "support_ticket.tickets.read");
    assert_eq!(http["safeProbe"], true);
    assert_eq!(
        http["links"]["remoteCalls"],
        "/operations/remote-calls?module=support-ticket"
    );
    assert_eq!(http["links"]["story"], "/?q=support-suite-provider");
    assert_eq!(
        http["links"]["technicalOperations"],
        "/operations?q=support-suite-provider"
    );
    assert_eq!(
        http["nextAction"],
        "run lenso service verify for this operation"
    );

    let runtime = operations
        .iter()
        .find(|operation| operation["kind"] == "runtime_function")
        .expect("runtime operation");
    assert_eq!(
        runtime["operationId"],
        "support-ticket/runtime/support-ticket.reindex.v1"
    );
    assert_eq!(runtime["capability"], Value::Null);
    assert_eq!(runtime["safeProbe"], false);

    let event = operations
        .iter()
        .find(|operation| operation["kind"] == "event_handler")
        .expect("event operation");
    assert_eq!(
        event["operationId"],
        "support-ticket/event/ticket-created-handler"
    );
    assert_eq!(event["capability"], Value::Null);
    assert_eq!(event["safeProbe"], false);

    let action = operations
        .iter()
        .find(|operation| operation["kind"] == "admin_action")
        .expect("admin action operation");
    assert_eq!(action["operationId"], "support-ticket/action/assign_ticket");
    assert_eq!(action["capability"], "support_ticket.tickets.write");
    assert_eq!(action["safeProbe"], false);
    assert_eq!(
        action["nextAction"],
        "add safeProbe metadata before active checks"
    );
}

#[tokio::test]
async fn service_modules_exposes_release_status_and_deployment_metadata() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let _env = FileFixture::write(
        ".env",
        "REMOTE_MODULES=support-ticket=http://127.0.0.1:9/lenso/module/v1\n",
    );
    let _health = FileFixture::remove(".lenso/service-health.json");
    let _ledger = FileFixture::write(
        ".lenso/module-installs.json",
        serde_json::json!({
            "version": 1,
            "modules": [{
                "moduleName": "support-ticket",
                "source": "remote",
                "compatibility": {
                    "consolePackageApi": "1",
                    "remoteProtocolVersion": "1",
                    "requiredHostFeatures": ["service.status"]
                },
                "deployment": {
                    "target": "container-paas",
                    "commands": ["pnpm --dir examples/support-ticket start"]
                },
                "service": {
                    "name": "api",
                    "requiredEnv": ["SUPPORT_API_KEY"],
                    "statusUrl": "http://127.0.0.1:9/lenso/module/v1/status",
                    "transports": ["http"],
                    "version": "0.1.0"
                }
            }]
        })
        .to_string(),
    );
    let _services = FileFixture::remove(".lenso/module-services.json");
    install_admin_module_metadata(vec![]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_get("/admin/data/service-modules"))
        .await
        .expect("service modules request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let module = &body["modules"][0];
    assert_eq!(module["moduleName"], "support-ticket");
    assert_eq!(
        module["statusUrl"],
        "http://127.0.0.1:9/lenso/module/v1/status"
    );
    assert_eq!(module["serviceStatus"]["checked"], true);
    assert_eq!(module["serviceStatus"]["state"], "unreachable");
    assert_eq!(module["healthHistory"][0]["state"], "unreachable");
    assert_eq!(module["compatibility"]["state"], "compatible");
    assert_eq!(
        module["config"]["requiredEnv"],
        serde_json::json!(["SUPPORT_API_KEY"])
    );
    assert_eq!(
        module["config"]["missingEnv"],
        serde_json::json!(["SUPPORT_API_KEY"])
    );
    assert_eq!(module["deployment"]["target"], "container-paas");
}

#[tokio::test]
async fn available_modules_reads_official_catalog_when_no_local_catalog_exists() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    remove_module_catalog_fixture();
    let _env = FileFixture::write(
        ".env",
        "REMOTE_MODULES=support-suite-provider=http://127.0.0.1:4110/lenso/service/v1\n",
    );
    install_admin_module_metadata(vec![AdminModuleMetadata {
        module_name: "auth".to_owned(),
        source: ModuleSource::Linked,
        load_status: ModuleLoadStatus::Loaded,
        http_routes: vec![],
        runtime: None,
        events: None,
        lifecycle: None,
        console: vec![],
        console_slots: Vec::new(),
        console_contributions: Vec::new(),
        story_display: vec![],
        capabilities: vec![],
        dependencies: vec![],
        admin: None,
        source_diagnostics: None,
    }]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_get("/admin/data/available-modules"))
        .await
        .expect("available modules request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["status"], "passed");
    assert_eq!(
        body["catalog"]["registryFile"],
        "builtin:lenso-official-module-catalog"
    );
    assert_eq!(body["catalog"]["modules"], 9);
    assert_eq!(body["modules"][0]["name"], "auth");
    assert_eq!(body["modules"][0]["source"], "linked");
    assert_eq!(body["modules"][0]["catalogVersion"], "0.1.4");
    assert_eq!(body["modules"][0]["consolePackageHints"], 1);
    assert_eq!(body["modules"][1]["name"], "auth-oauth");
    assert_eq!(body["modules"][1]["source"], "linked");
    assert_eq!(body["modules"][1]["consolePackageHints"], 1);
    assert_eq!(body["modules"][2]["name"], "auth-anonymous");
    assert_eq!(body["modules"][2]["source"], "linked");
    assert_eq!(body["modules"][3]["name"], "auth-password");
    assert_eq!(body["modules"][3]["source"], "linked");
    assert_eq!(body["modules"][4]["name"], "auth-device");
    assert_eq!(body["modules"][4]["source"], "linked");
    assert_eq!(body["modules"][4]["consolePackageHints"], 1);
    assert_eq!(body["modules"][5]["name"], "auth-github");
    assert_eq!(body["modules"][5]["source"], "linked");
    assert_eq!(body["modules"][5]["consolePackageHints"], 1);
    assert_eq!(body["modules"][6]["name"], "auth-google");
    assert_eq!(body["modules"][6]["source"], "linked");
    assert_eq!(body["modules"][6]["consolePackageHints"], 1);
    assert_eq!(body["modules"][7]["name"], "auth-oidc");
    assert_eq!(body["modules"][7]["source"], "linked");
    assert_eq!(body["modules"][7]["consolePackageHints"], 1);
    assert_eq!(body["modules"][8]["name"], "support-ticket");
    assert_eq!(body["modules"][8]["source"], "remote");
    assert_eq!(body["modules"][8]["providedBy"], "support-suite-provider");
    let support_ticket = body["modules"]
        .as_array()
        .expect("available modules is an array")
        .iter()
        .find(|module| module["name"] == "support-ticket")
        .expect("official catalog includes support-ticket");
    assert_eq!(
        support_ticket["serviceManifest"],
        "http://127.0.0.1:4110/lenso/service/v1/manifest"
    );
    assert_eq!(
        support_ticket["manifestReference"],
        "http://127.0.0.1:4110/lenso/service/v1/manifest"
    );
    assert_eq!(
        support_ticket["summary"],
        "Ticket intake, triage, and operations"
    );
    assert_eq!(support_ticket["consolePackageHints"], 0);
    assert_eq!(
        support_ticket["installState"]["remoteSource"]["desiredBaseUrl"],
        "http://127.0.0.1:4110/lenso/service/v1"
    );
    assert_eq!(
        support_ticket["installState"]["remoteSource"]["configured"],
        true
    );
    assert_eq!(support_ticket["status"], "ready");
}

#[tokio::test]
async fn available_modules_reads_local_module_catalog() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    fs::create_dir_all(".lenso").expect("create catalog dir");
    fs::write(
        ".lenso/module-catalog.json",
        serde_json::json!({
            "version": 1,
            "modules": [{
                "name": "billing",
                "version": "0.2.0",
                "source": "remote",
                "manifestReference": "https://example.com/billing/manifest",
                "baseUrl": "https://example.com/billing",
                "summary": "Billing workspace and operations",
                "capabilities": ["billing.read", "billing.write"],
                "compatibility": {
                    "consolePackageApi": "1",
                    "lenso": {
                        "minVersion": "0.1.0",
                        "maxVersion": "0.1.7"
                    }
                },
                "consolePackages": [{
                    "packageName": "@vendor/lenso-billing-console",
                    "exportName": "billingConsoleModule",
                    "route": "/data/billing"
                }]
            }]
        })
        .to_string(),
    )
    .expect("write catalog fixture");
    install_admin_module_metadata(vec![AdminModuleMetadata {
        module_name: "billing".to_owned(),
        source: ModuleSource::Remote,
        load_status: ModuleLoadStatus::Loaded,
        http_routes: vec![],
        runtime: None,
        events: None,
        lifecycle: None,
        console: vec![],
        console_slots: Vec::new(),
        console_contributions: Vec::new(),
        story_display: vec![],
        capabilities: vec![],
        dependencies: vec![],
        admin: None,
        source_diagnostics: Some(AdminModuleSourceDiagnostics::Remote(
            AdminRemoteModuleDiagnostics {
                transport: "http_json".to_owned(),
                base_url: "https://example.com/billing".to_owned(),
                manifest_url: "https://example.com/billing/manifest".to_owned(),
                timeout_ms: 1000,
                auth_configured: false,
                load_duration_ms: None,
                last_checked_at: None,
                last_load_error: None,
            },
        )),
    }]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_get("/admin/data/available-modules"))
        .await
        .expect("available modules request completes");

    remove_module_catalog_fixture();

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["status"], "passed");
    assert_eq!(
        body["catalog"]["registryFile"],
        ".lenso/module-catalog.json"
    );
    assert_eq!(body["catalog"]["modules"], 1);
    assert_eq!(body["modules"][0]["name"], "billing");
    assert_eq!(body["modules"][0]["catalogVersion"], "0.2.0");
    assert_eq!(
        body["modules"][0]["summary"],
        "Billing workspace and operations"
    );
    assert_eq!(body["modules"][0]["capabilities"][0], "billing.read");
    assert_eq!(body["modules"][0]["capabilities"][1], "billing.write");
    assert_eq!(
        body["modules"][0]["compatibility"]["lenso"]["minVersion"],
        "0.1.0"
    );
    assert_eq!(
        body["modules"][0]["manifestReference"],
        "https://example.com/billing/manifest"
    );
    assert_eq!(body["modules"][0]["consolePackageHints"], 1);
    assert!(body["modules"][0].get("installPolicy").is_none());
}

#[tokio::test]
async fn available_modules_exposes_module_release_catalog_metadata() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let _catalog = FileFixture::write(
        ".lenso/module-catalog.json",
        serde_json::json!({
            "version": 1,
            "modules": [{
                "protocol": "lenso.module-release.v1",
                "name": "support-ticket",
                "version": "0.2.0",
                "source": "service",
                "manifestReference": "../support/lenso.module-release.json",
                "summary": "Ticket intake, triage, and operations",
                "provider": {
                    "name": "support-suite-provider",
                    "servicePackage": "../support/lenso.service-package.json"
                }
            }]
        })
        .to_string(),
    );
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_get("/admin/data/available-modules"))
        .await
        .expect("available modules request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let module = &body["modules"][0];
    assert_eq!(module["name"], "support-ticket");
    assert_eq!(
        module["moduleRelease"]["manifestReference"],
        "../support/lenso.module-release.json"
    );
    assert_eq!(
        module["moduleRelease"]["providerName"],
        "support-suite-provider"
    );
    assert_eq!(
        module["moduleRelease"]["servicePackage"],
        "../support/lenso.service-package.json"
    );
}

#[tokio::test]
async fn available_modules_reconciles_service_provider_source_after_restart() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let _catalog = FileFixture::write(
        ".lenso/module-catalog.json",
        serde_json::json!({
            "version": 1,
            "modules": [{
                "name": "support-ticket",
                "version": "0.1.0",
                "source": "service",
                "providedBy": "support-suite-provider",
                "serviceManifest": "http://127.0.0.1:4110/lenso/service/v1/manifest",
                "manifestReference": "http://127.0.0.1:4110/lenso/service/v1/manifest",
                "baseUrl": "http://127.0.0.1:4110/lenso/service/v1",
                "summary": "Ticket intake, triage, and operations"
            }]
        })
        .to_string(),
    );
    let _env = FileFixture::write(
        ".env",
        "REMOTE_MODULES=support-suite-provider=http://127.0.0.1:4110/lenso/service/v1\n",
    );
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);
    install_admin_module_metadata(vec![AdminModuleMetadata {
        module_name: "support-ticket".to_owned(),
        source: ModuleSource::Remote,
        load_status: ModuleLoadStatus::Loaded,
        http_routes: vec![],
        runtime: None,
        events: None,
        lifecycle: None,
        console: vec![],
        console_slots: Vec::new(),
        console_contributions: Vec::new(),
        story_display: vec![],
        capabilities: vec![],
        dependencies: vec![],
        admin: None,
        source_diagnostics: Some(AdminModuleSourceDiagnostics::Remote(
            AdminRemoteModuleDiagnostics {
                transport: "http".to_owned(),
                base_url: "http://127.0.0.1:4110/lenso/service/v1/modules/support-ticket"
                    .to_owned(),
                manifest_url:
                    "http://127.0.0.1:4110/lenso/service/v1/modules/support-ticket/manifest"
                        .to_owned(),
                timeout_ms: 5000,
                auth_configured: false,
                load_duration_ms: Some(10),
                last_checked_at: None,
                last_load_error: None,
            },
        )),
    }]);

    let response = app
        .oneshot(admin_get("/admin/data/available-modules"))
        .await
        .expect("available modules request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let module = &body["modules"][0];
    assert_eq!(module["name"], "support-ticket");
    assert_eq!(
        module["installState"]["remoteSource"]["desiredBaseUrl"],
        "http://127.0.0.1:4110/lenso/service/v1"
    );
    assert_eq!(
        module["installState"]["remoteSource"]["runningBaseUrl"],
        "http://127.0.0.1:4110/lenso/service/v1"
    );
    assert_eq!(
        module["installState"]["remoteSource"]["restartPending"],
        false
    );
}

#[tokio::test]
async fn available_modules_reports_local_install_state() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let _catalog = FileFixture::write(
        ".lenso/module-catalog.json",
        serde_json::json!({
            "version": 1,
            "modules": [{
                "name": "billing",
                "version": "0.2.0",
                "source": "remote",
                "manifestReference": "https://example.com/billing/manifest",
                "baseUrl": "https://example.com/billing",
                "summary": "Billing workspace and operations",
                "consolePackages": [{
                    "packageName": "@vendor/lenso-billing-console",
                    "exportName": "billingConsoleModule",
                    "route": "/data/billing"
                }]
            }]
        })
        .to_string(),
    );
    let _console_registry = FileFixture::write(
        ".lenso/console/extensions/registry.json",
        serde_json::json!({
            "version": 1,
            "bundles": [{
                "entry": "/console/extensions/billing/billing-console.js",
                "exportName": "billingConsoleModule",
                "hostApi": "1",
                "moduleName": "billing",
                "packageName": "@vendor/lenso-billing-console"
            }]
        })
        .to_string(),
    );
    let _env = FileFixture::write(
        ".env",
        "REMOTE_MODULES=billing=https://example.com/billing/\n",
    );
    install_admin_module_metadata(vec![AdminModuleMetadata {
        module_name: "auth".to_owned(),
        source: ModuleSource::Linked,
        load_status: ModuleLoadStatus::Loaded,
        http_routes: vec![],
        runtime: None,
        events: None,
        lifecycle: None,
        console: vec![],
        console_slots: Vec::new(),
        console_contributions: Vec::new(),
        story_display: vec![],
        capabilities: vec![],
        dependencies: vec![],
        admin: None,
        source_diagnostics: None,
    }]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_get("/admin/data/available-modules"))
        .await
        .expect("available modules request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let install_state = &body["modules"][0]["installState"];
    assert_eq!(install_state["moduleRegistered"], false);
    assert_eq!(install_state["linkedSource"], Value::Null);
    assert_eq!(install_state["remoteSource"]["envFile"], ".env");
    assert_eq!(install_state["remoteSource"]["configured"], true);
    assert_eq!(
        install_state["remoteSource"]["desiredBaseUrl"],
        "https://example.com/billing"
    );
    assert_eq!(install_state["remoteSource"]["runningBaseUrl"], Value::Null);
    assert_eq!(install_state["remoteSource"]["restartPending"], true);
    assert_eq!(
        install_state["remoteSource"]["restartReason"],
        "service provider source configured in .env but not loaded"
    );
    assert_eq!(
        install_state["consolePlan"]["planFile"],
        ".lenso/console/extensions/registry.json"
    );
    assert_eq!(install_state["consolePlan"]["exists"], true);
    assert_eq!(install_state["consolePlan"]["readable"], true);
    assert_eq!(install_state["consolePlan"]["moduleEntryPresent"], true);
    assert_eq!(install_state["consolePlan"]["packageCount"], 1);
    assert_eq!(install_state["consolePlan"]["restartRequired"], true);
    assert_eq!(
        install_state["consolePlan"]["packages"][0]["key"],
        "@vendor/lenso-billing-console#billingConsoleModule"
    );
    assert_eq!(
        install_state["consolePlan"]["packages"][0]["status"],
        "installed"
    );
}

#[tokio::test]
async fn available_module_install_writes_remote_source_and_console_extension() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let bundle_path = std::env::current_dir()
        .expect("current dir")
        .join(".lenso/fixtures/billing-console.js");
    let _bundle = FileFixture::write(
        &bundle_path,
        b"export const billingConsoleModule = { id: 'billing', surfaces: [] };\n",
    );
    let style_path = std::env::current_dir()
        .expect("current dir")
        .join(".lenso/fixtures/billing-console.css");
    let _style = FileFixture::write(&style_path, b".billing-console{display:grid}\n");
    let _catalog = FileFixture::write(
        ".lenso/module-catalog.json",
        serde_json::json!({
            "version": 1,
            "modules": [{
                "name": "billing",
                "version": "0.2.0",
                "source": "remote",
                "manifestReference": "https://example.com/billing/manifest",
                "baseUrl": "https://example.com/billing/",
                "summary": "Billing workspace and operations",
                "consolePackages": [{
                    "packageName": "@vendor/lenso-billing-console",
                    "exportName": "billingConsoleModule",
                    "bundleUrl": format!("file://{}", bundle_path.display()),
                    "styles": [format!("file://{}", style_path.display())],
                    "route": "/data/billing"
                }]
            }]
        })
        .to_string(),
    );
    let _env = FileFixture::write(
        ".env",
        "DATABASE_URL=postgres://localhost/lenso\nREMOTE_MODULES=crm=https://example.com/crm\n",
    );
    let _install_plan = FileFixture::remove(".lenso/console-package-install-plan.json");
    let _console_registry = FileFixture::remove(".lenso/console/extensions/registry.json");
    let _copied_bundle =
        FileFixture::remove(".lenso/console/extensions/billing/billing-console.js");
    let _copied_style =
        FileFixture::remove(".lenso/console/extensions/billing/billing-console.css");
    install_admin_module_metadata(vec![AdminModuleMetadata {
        module_name: "auth".to_owned(),
        source: ModuleSource::Linked,
        load_status: ModuleLoadStatus::Loaded,
        http_routes: vec![],
        runtime: None,
        events: None,
        lifecycle: None,
        console: vec![],
        console_slots: Vec::new(),
        console_contributions: Vec::new(),
        story_display: vec![],
        capabilities: vec![],
        dependencies: vec![],
        admin: None,
        source_diagnostics: None,
    }]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_post_json(
            "/admin/data/available-modules/billing/install",
            "{}",
        ))
        .await
        .expect("install request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["moduleName"], "billing");
    assert_eq!(body["linkedSource"], Value::Null);
    assert_eq!(body["remoteSource"]["envFile"], ".env");
    assert_eq!(
        body["remoteSource"]["desiredBaseUrl"],
        "https://example.com/billing"
    );
    assert_eq!(body["remoteSource"]["restartPending"], true);
    assert_eq!(body["consolePlan"]["packageCount"], 1);
    assert_eq!(body["consolePlan"]["restartRequired"], true);

    let env_file = fs::read_to_string(".env").expect("read env file");
    assert!(env_file.contains("DATABASE_URL=postgres://localhost/lenso\n"));
    assert!(
        env_file.contains(
            "REMOTE_MODULES=crm=https://example.com/crm,billing=https://example.com/billing\n"
        ),
        "{env_file}"
    );

    let console_registry =
        fs::read_to_string(".lenso/console/extensions/registry.json").expect("read registry");
    let console_registry_json: Value =
        serde_json::from_str(&console_registry).expect("registry is json");
    assert_eq!(console_registry_json["version"], 1);
    assert_eq!(
        console_registry_json["bundles"][0]["entry"],
        "/console/extensions/billing/billing-console.js"
    );
    assert_eq!(console_registry_json["bundles"][0]["moduleName"], "billing");
    assert_eq!(
        console_registry_json["bundles"][0]["packageName"],
        "@vendor/lenso-billing-console"
    );
    assert_eq!(
        console_registry_json["bundles"][0]["styles"],
        serde_json::json!(["/console/extensions/billing/billing-console.css"])
    );
    assert_eq!(
        fs::read_to_string(".lenso/console/extensions/billing/billing-console.js")
            .expect("read copied bundle"),
        "export const billingConsoleModule = { id: 'billing', surfaces: [] };\n"
    );
    assert_eq!(
        fs::read_to_string(".lenso/console/extensions/billing/billing-console.css")
            .expect("read copied style"),
        ".billing-console{display:grid}\n"
    );
}

#[tokio::test]
async fn available_service_module_install_writes_provider_remote_source() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let _catalog = FileFixture::write(
        ".lenso/module-catalog.json",
        serde_json::json!({
            "version": 1,
            "modules": [{
                "name": "support-ticket",
                "version": "0.1.0",
                "source": "service",
                "providedBy": "support-suite-provider",
                "serviceManifest": "http://127.0.0.1:4110/lenso/service/v1/manifest",
                "manifestReference": "http://127.0.0.1:4110/lenso/service/v1/manifest",
                "baseUrl": "http://127.0.0.1:4110/lenso/service/v1",
                "summary": "Ticket intake, triage, and operations"
            }]
        })
        .to_string(),
    );
    let _env = FileFixture::write(".env", "REMOTE_MODULES=crm=https://example.com/crm\n");
    let _console_registry = FileFixture::remove(".lenso/console/extensions/registry.json");
    install_admin_module_metadata(vec![]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_post_json(
            "/admin/data/available-modules/support-ticket/install",
            "{}",
        ))
        .await
        .expect("install request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["moduleName"], "support-ticket");
    assert_eq!(body["moduleRelease"], Value::Null);
    assert_eq!(body["remoteSource"]["configured"], true);
    assert_eq!(
        body["remoteSource"]["desiredBaseUrl"],
        "http://127.0.0.1:4110/lenso/service/v1"
    );
    let env_file = fs::read_to_string(".env").expect("read env file");
    assert!(
        env_file.contains(
            "REMOTE_MODULES=crm=https://example.com/crm,support-suite-provider=http://127.0.0.1:4110/lenso/service/v1\n"
        ),
        "{env_file}"
    );
    assert!(!env_file.contains("support-ticket=http://127.0.0.1:4110"));
}

#[tokio::test]
async fn available_module_release_install_writes_provider_receipt() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let _catalog = FileFixture::write(
        ".lenso/module-catalog.json",
        serde_json::json!({
            "version": 1,
            "modules": [{
                "name": "support-ticket",
                "version": "0.1.0",
                "protocol": "lenso.module-release.v1",
                "source": "service",
                "manifestReference": "dist/lenso-service/support-suite-provider/modules/support-ticket/lenso.module-release.json",
                "baseUrl": "http://127.0.0.1:4110/lenso/service/v1",
                "provider": {
                    "name": "support-suite-provider",
                    "serviceManifest": "http://127.0.0.1:4110/lenso/service/v1/manifest"
                },
                "capabilities": ["support_ticket.tickets.read"]
            }]
        })
        .to_string(),
    );
    let _env = FileFixture::write(".env", "REMOTE_MODULES=crm=https://example.com/crm\n");
    let _ledger = FileFixture::remove(".lenso/module-installs.json");
    let _console_registry = FileFixture::remove(".lenso/console/extensions/registry.json");
    install_admin_module_metadata(vec![]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_post_json(
            "/admin/data/available-modules/support-ticket/install",
            "{}",
        ))
        .await
        .expect("install request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["moduleName"], "support-ticket");
    assert_eq!(
        body["moduleRelease"]["manifestReference"],
        "dist/lenso-service/support-suite-provider/modules/support-ticket/lenso.module-release.json"
    );
    assert_eq!(
        body["moduleRelease"]["providerName"],
        "support-suite-provider"
    );
    assert_eq!(body["remoteSource"]["configured"], true);
    assert_eq!(
        body["remoteSource"]["desiredBaseUrl"],
        "http://127.0.0.1:4110/lenso/service/v1"
    );
    let env_file = fs::read_to_string(".env").expect("read env file");
    assert!(
        env_file.contains(
            "REMOTE_MODULES=crm=https://example.com/crm,support-suite-provider=http://127.0.0.1:4110/lenso/service/v1\n"
        ),
        "{env_file}"
    );
    assert!(!env_file.contains("support-ticket=http://127.0.0.1:4110"));

    let ledger_source =
        fs::read_to_string(".lenso/module-installs.json").expect("read module install ledger");
    let ledger: Value = serde_json::from_str(&ledger_source).expect("ledger is json");
    assert_eq!(ledger["modules"][0]["moduleName"], "support-ticket");
    assert_eq!(
        ledger["modules"][0]["service"]["name"],
        "support-suite-provider"
    );
    assert_eq!(
        ledger["modules"][0]["service"]["manifestReference"],
        "http://127.0.0.1:4110/lenso/service/v1/manifest"
    );
    assert_eq!(
        ledger["modules"][0]["moduleRelease"]["manifestReference"],
        "dist/lenso-service/support-suite-provider/modules/support-ticket/lenso.module-release.json"
    );
    assert_eq!(
        ledger["modules"][0]["moduleRelease"]["manifestSnapshot"]["provider"]["name"],
        "support-suite-provider"
    );
    assert_eq!(
        ledger["modules"][0]["moduleRelease"]["manifestSnapshot"]["capabilities"],
        serde_json::json!(["support_ticket.tickets.read"])
    );
}

#[tokio::test]
async fn available_module_install_rejects_catalog_preflight_blockers() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let _catalog = FileFixture::write(
        ".lenso/module-catalog.json",
        serde_json::json!({
            "version": 1,
            "modules": [{
                "name": "local-crm",
                "version": "0.1.0",
                "source": "remote",
                "manifestReference": "./lenso.module.json"
            }]
        })
        .to_string(),
    );
    let _env = FileFixture::remove(".env");
    let _install_plan = FileFixture::remove(".lenso/console-package-install-plan.json");
    let _console_registry = FileFixture::remove(".lenso/console/extensions/registry.json");
    install_admin_module_metadata(vec![]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_post_json(
            "/admin/data/available-modules/local-crm/install",
            "{}",
        ))
        .await
        .expect("install request completes");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"]["message"], "local-crm baseUrl is missing");
    assert!(!Path::new(".env").exists());
    assert!(!Path::new(".lenso/console-package-install-plan.json").exists());
    assert!(!Path::new(".lenso/console/extensions/registry.json").exists());
}

#[tokio::test]
async fn available_linked_module_install_sets_demo_composition_profile() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let bundle_path = std::env::current_dir()
        .expect("current dir")
        .join(".lenso/fixtures/auth-console.js");
    let _bundle = FileFixture::write(
        &bundle_path,
        b"export const authConsoleModule = { id: 'auth', surfaces: [] };\n",
    );
    let style_path = std::env::current_dir()
        .expect("current dir")
        .join(".lenso/fixtures/auth-console.css");
    let _style = FileFixture::write(&style_path, b".auth-console{display:grid}\n");
    let _catalog = FileFixture::write(
        ".lenso/module-catalog.json",
        serde_json::json!({
            "version": 1,
            "modules": [{
                "name": "auth",
                "version": "0.1.3",
                "source": "linked",
                "manifestReference": "builtin:auth",
                "summary": "Local auth fixture",
                "capabilities": ["auth.users.read"],
                "consolePackages": [{
                    "packageName": "@lenso/auth-console",
                    "exportName": "authConsoleModule",
                    "bundleUrl": format!("file://{}", bundle_path.display()),
                    "entry": "/console/extensions/auth/auth-console.js",
                    "hostApi": "1",
                    "route": "/data/auth/sessions",
                    "requiredCapabilities": ["auth.users.read"],
                    "styles": [format!("file://{}", style_path.display())],
                    "version": "0.1.1"
                }]
            }]
        })
        .to_string(),
    );
    let _env = FileFixture::write(".env", "DATABASE_URL=postgres://localhost/lenso\n");
    let _console_registry = FileFixture::remove(".lenso/console/extensions/registry.json");
    let _copied_bundle = FileFixture::remove(".lenso/console/extensions/auth/auth-console.js");
    let _copied_style = FileFixture::remove(".lenso/console/extensions/auth/auth-console.css");
    install_admin_module_metadata(vec![]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_post_json(
            "/admin/data/available-modules/auth/install",
            "{}",
        ))
        .await
        .expect("install request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["moduleName"], "auth");
    assert_eq!(body["restartRequired"], true);
    assert_eq!(body["remoteSource"], Value::Null);
    assert_eq!(body["linkedSource"]["configured"], true);
    assert_eq!(body["linkedSource"]["desiredEnabled"], true);
    assert_eq!(body["linkedSource"]["runningEnabled"], false);
    assert_eq!(body["consolePlan"]["packageCount"], 1);
    assert_eq!(
        body["consolePlan"]["packages"][0]["key"],
        "@lenso/auth-console#authConsoleModule"
    );
    assert_eq!(body["consolePlan"]["packages"][0]["status"], "installed");
    assert_eq!(
        body["linkedSource"]["restartReason"],
        "linked module enabled by env override; restart API and worker"
    );
    let env_file = fs::read_to_string(".env").expect("read env file");
    assert!(env_file.contains("DATABASE_URL=postgres://localhost/lenso\n"));
    assert!(env_file.contains("LENSO_COMPOSITION_PROFILE=demo\n"));
    assert!(env_file.contains("LENSO_MODULE_AUTH_ENABLED=true\n"));
    assert!(!env_file.contains("REMOTE_MODULES=auth"));
    let console_registry =
        fs::read_to_string(".lenso/console/extensions/registry.json").expect("read registry");
    let console_registry_json: Value =
        serde_json::from_str(&console_registry).expect("registry is json");
    assert_eq!(console_registry_json["version"], 1);
    assert_eq!(
        console_registry_json["bundles"][0]["entry"],
        "/console/extensions/auth/auth-console.js"
    );
    assert_eq!(console_registry_json["bundles"][0]["moduleName"], "auth");
    assert_eq!(
        console_registry_json["bundles"][0]["packageName"],
        "@lenso/auth-console"
    );
    assert_eq!(
        console_registry_json["bundles"][0]["requiredCapabilities"],
        serde_json::json!(["auth.users.read"])
    );
    assert_eq!(
        console_registry_json["bundles"][0]["styles"],
        serde_json::json!(["/console/extensions/auth/auth-console.css"])
    );
    assert_eq!(
        fs::read_to_string(".lenso/console/extensions/auth/auth-console.js")
            .expect("read copied bundle"),
        "export const authConsoleModule = { id: 'auth', surfaces: [] };\n"
    );
    assert_eq!(
        fs::read_to_string(".lenso/console/extensions/auth/auth-console.css")
            .expect("read copied style"),
        ".auth-console{display:grid}\n"
    );

    let status_response = build_router(AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    ))
    .oneshot(admin_get("/admin/data/available-modules"))
    .await
    .expect("available modules request completes");
    let status_body = json_body(status_response).await;
    let auth = status_body["modules"]
        .as_array()
        .expect("modules array")
        .iter()
        .find(|module| module["name"] == "auth")
        .expect("auth available module");
    assert_eq!(auth["installState"]["remoteSource"], Value::Null);
    assert_eq!(auth["installState"]["linkedSource"]["restartPending"], true);
    assert_eq!(auth["installState"]["consolePlan"]["packageCount"], 1);
    assert_eq!(
        auth["installState"]["linkedSource"]["restartReason"],
        "linked module enabled by env override; restart API and worker"
    );
}

#[tokio::test]
async fn available_remote_module_uninstall_removes_source_and_console_extension() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let _catalog = FileFixture::write(
        ".lenso/module-catalog.json",
        serde_json::json!({
            "version": 1,
            "modules": [{
                "name": "billing",
                "version": "0.2.0",
                "source": "remote",
                "manifestReference": "https://example.com/billing/manifest",
                "baseUrl": "https://example.com/billing",
                "consolePackages": [{
                    "packageName": "@vendor/lenso-billing-console",
                    "exportName": "billingConsoleModule",
                    "bundleUrl": "https://example.com/billing/billing-console.js",
                    "route": "/data/billing"
                }]
            }]
        })
        .to_string(),
    );
    let _env = FileFixture::write(
        ".env",
        "REMOTE_MODULES=crm=https://example.com/crm,billing=https://example.com/billing\n",
    );
    let _install_plan = FileFixture::write(
        ".lenso/console-package-install-plan.json",
        serde_json::json!({
            "version": 1,
            "modules": [
                {
                    "moduleName": "billing",
                    "baseUrl": "https://example.com/billing",
                    "manifestReference": "https://example.com/billing/manifest",
                    "restartRequired": true,
                    "consolePackages": [{
                        "packageName": "@vendor/lenso-billing-console",
                        "exportName": "billingConsoleModule"
                    }]
                },
                {
                    "moduleName": "crm",
                    "baseUrl": "https://example.com/crm",
                    "manifestReference": "https://example.com/crm/manifest",
                    "restartRequired": true,
                    "consolePackages": []
                }
            ]
        })
        .to_string(),
    );
    let _console_registry = FileFixture::write(
        ".lenso/console/extensions/registry.json",
        serde_json::json!({
            "version": 1,
            "bundles": [
                {
                    "entry": "/console/extensions/billing/billing-console.js",
                    "exportName": "billingConsoleModule",
                    "hostApi": "1",
                    "moduleName": "billing",
                    "packageName": "@vendor/lenso-billing-console"
                },
                {
                    "entry": "/console/extensions/crm/crm-console.js",
                    "exportName": "crmConsoleModule",
                    "hostApi": "1",
                    "moduleName": "crm",
                    "packageName": "@vendor/lenso-crm-console"
                }
            ]
        })
        .to_string(),
    );
    let _copied_bundle = FileFixture::write(
        ".lenso/console/extensions/billing/billing-console.js",
        b"export const billingConsoleModule = { id: 'billing', surfaces: [] };\n",
    );
    install_admin_module_metadata(vec![AdminModuleMetadata {
        module_name: "billing".to_owned(),
        source: ModuleSource::Remote,
        load_status: ModuleLoadStatus::Loaded,
        http_routes: vec![],
        runtime: None,
        events: None,
        lifecycle: None,
        console: vec![],
        console_slots: Vec::new(),
        console_contributions: Vec::new(),
        story_display: vec![],
        capabilities: vec![],
        dependencies: vec![],
        admin: None,
        source_diagnostics: Some(AdminModuleSourceDiagnostics::Remote(
            AdminRemoteModuleDiagnostics {
                transport: "http".to_owned(),
                base_url: "https://example.com/billing".to_owned(),
                manifest_url: "https://example.com/billing/manifest".to_owned(),
                timeout_ms: 5000,
                auth_configured: false,
                load_duration_ms: Some(10),
                last_checked_at: None,
                last_load_error: None,
            },
        )),
    }]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_delete(
            "/admin/data/available-modules/billing/install",
        ))
        .await
        .expect("uninstall request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["moduleName"], "billing");
    assert_eq!(body["restartRequired"], true);
    assert_eq!(body["linkedSource"], Value::Null);
    assert_eq!(body["remoteSource"]["configured"], false);
    assert_eq!(
        body["remoteSource"]["runningBaseUrl"],
        "https://example.com/billing"
    );
    assert_eq!(body["remoteSource"]["restartPending"], true);
    assert_eq!(
        body["remoteSource"]["restartReason"],
        "service provider source removed from .env but still loaded"
    );
    let env_file = fs::read_to_string(".env").expect("read env file");
    assert_eq!(env_file, "REMOTE_MODULES=crm=https://example.com/crm\n");
    assert!(!Path::new(".lenso/console/extensions/billing/billing-console.js").exists());
    let console_registry =
        fs::read_to_string(".lenso/console/extensions/registry.json").expect("read registry");
    let console_registry_json: Value =
        serde_json::from_str(&console_registry).expect("registry is json");
    assert_eq!(
        console_registry_json["bundles"].as_array().unwrap().len(),
        1
    );
    assert_eq!(console_registry_json["bundles"][0]["moduleName"], "crm");
    let install_plan =
        fs::read_to_string(".lenso/console-package-install-plan.json").expect("read install plan");
    let install_plan_json: Value =
        serde_json::from_str(&install_plan).expect("install plan is json");
    assert_eq!(install_plan_json["modules"].as_array().unwrap().len(), 1);
    assert_eq!(install_plan_json["modules"][0]["moduleName"], "crm");
}

#[tokio::test]
async fn available_service_module_uninstall_removes_provider_remote_source() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let _catalog = FileFixture::write(
        ".lenso/module-catalog.json",
        serde_json::json!({
            "version": 1,
            "modules": [{
                "name": "support-ticket",
                "version": "0.1.0",
                "source": "service",
                "providedBy": "support-suite-provider",
                "serviceManifest": "http://127.0.0.1:4110/lenso/service/v1/manifest",
                "manifestReference": "http://127.0.0.1:4110/lenso/service/v1/manifest",
                "baseUrl": "http://127.0.0.1:4110/lenso/service/v1"
            }]
        })
        .to_string(),
    );
    let _env = FileFixture::write(
        ".env",
        "REMOTE_MODULES=crm=https://example.com/crm,support-suite-provider=http://127.0.0.1:4110/lenso/service/v1\n",
    );
    let _console_registry = FileFixture::remove(".lenso/console/extensions/registry.json");
    let _install_plan = FileFixture::remove(".lenso/console-package-install-plan.json");
    let _ledger = FileFixture::write(
        ".lenso/module-installs.json",
        serde_json::json!({
            "version": 1,
            "modules": [
                {
                    "moduleName": "crm",
                    "source": "remote"
                },
                {
                    "moduleName": "support-ticket",
                    "source": "remote",
                    "service": {
                        "name": "support-suite-provider"
                    },
                    "moduleRelease": {
                        "manifestReference": "dist/lenso-service/support-suite-provider/modules/support-ticket/lenso.module-release.json",
                        "manifestSnapshot": {
                            "protocol": "lenso.module-release.v1",
                            "name": "support-ticket",
                            "version": "0.1.0",
                            "source": "service",
                            "provider": {
                                "name": "support-suite-provider"
                            }
                        }
                    }
                }
            ]
        })
        .to_string(),
    );
    install_admin_module_metadata(vec![]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_delete(
            "/admin/data/available-modules/support-ticket/install",
        ))
        .await
        .expect("uninstall request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["moduleName"], "support-ticket");
    assert_eq!(body["remoteSource"]["configured"], false);
    let env_file = fs::read_to_string(".env").expect("read env file");
    assert_eq!(env_file, "REMOTE_MODULES=crm=https://example.com/crm\n");
    let ledger_source =
        fs::read_to_string(".lenso/module-installs.json").expect("read module install ledger");
    let ledger: Value = serde_json::from_str(&ledger_source).expect("ledger is json");
    assert_eq!(ledger["modules"].as_array().unwrap().len(), 1);
    assert_eq!(ledger["modules"][0]["moduleName"], "crm");
}

#[tokio::test]
async fn available_linked_module_uninstall_disables_module_env_override() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    remove_module_catalog_fixture();
    let _env = FileFixture::write(
        ".env",
        "LENSO_COMPOSITION_PROFILE=demo\nLENSO_MODULE_AUTH_ENABLED=true\n",
    );
    let _console_registry = FileFixture::write(
        ".lenso/console/extensions/registry.json",
        serde_json::json!({
            "version": 1,
            "bundles": [
                {
                    "entry": "/console/extensions/auth/auth-console.js",
                    "exportName": "authConsoleModule",
                    "hostApi": "1",
                    "moduleName": "auth",
                    "packageName": "@lenso/auth-console"
                },
                {
                    "entry": "/console/extensions/crm/crm-console.js",
                    "exportName": "crmConsoleModule",
                    "hostApi": "1",
                    "moduleName": "crm",
                    "packageName": "@vendor/crm-console"
                }
            ]
        })
        .to_string(),
    );
    install_admin_module_metadata(vec![AdminModuleMetadata {
        module_name: "auth".to_owned(),
        source: ModuleSource::Linked,
        load_status: ModuleLoadStatus::Loaded,
        http_routes: vec![],
        runtime: None,
        events: None,
        lifecycle: None,
        console: vec![],
        console_slots: Vec::new(),
        console_contributions: Vec::new(),
        story_display: vec![],
        capabilities: vec![],
        dependencies: vec![],
        admin: None,
        source_diagnostics: None,
    }]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_delete("/admin/data/available-modules/auth/install"))
        .await
        .expect("uninstall request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["moduleName"], "auth");
    assert_eq!(body["remoteSource"], Value::Null);
    assert_eq!(body["linkedSource"]["configured"], true);
    assert_eq!(body["linkedSource"]["desiredEnabled"], false);
    assert_eq!(body["linkedSource"]["runningEnabled"], true);
    assert_eq!(
        body["linkedSource"]["restartReason"],
        "linked module disabled by env override; restart API and worker"
    );
    let env_file = fs::read_to_string(".env").expect("read env file");
    assert!(env_file.contains("LENSO_COMPOSITION_PROFILE=demo\n"));
    assert!(env_file.contains("LENSO_MODULE_AUTH_ENABLED=false\n"));
    let console_registry =
        fs::read_to_string(".lenso/console/extensions/registry.json").expect("read registry");
    let console_registry_json: Value =
        serde_json::from_str(&console_registry).expect("registry is json");
    assert_eq!(
        console_registry_json["bundles"].as_array().unwrap().len(),
        1
    );
    assert_eq!(console_registry_json["bundles"][0]["moduleName"], "crm");

    let status_response = build_router(AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    ))
    .oneshot(admin_get("/admin/data/available-modules"))
    .await
    .expect("available modules request completes");
    let status_body = json_body(status_response).await;
    let auth = status_body["modules"]
        .as_array()
        .expect("modules array")
        .iter()
        .find(|module| module["name"] == "auth")
        .expect("auth available module");
    assert_eq!(auth["installState"]["remoteSource"], Value::Null);
    assert_eq!(auth["installState"]["linkedSource"]["restartPending"], true);
    assert_eq!(auth["installState"]["consolePlan"]["packageCount"], 0);
    assert_eq!(
        auth["installState"]["linkedSource"]["restartReason"],
        "linked module disabled by env override; restart API and worker"
    );
}

#[tokio::test]
async fn available_modules_marks_catalog_preflight_issues() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    fs::create_dir_all(".lenso").expect("create catalog dir");
    fs::write(
        ".lenso/module-catalog.json",
        serde_json::json!({
            "version": 1,
            "modules": [
                {
                    "name": "billing",
                    "version": "0.2.0",
                    "source": "remote",
                    "manifestReference": "https://example.com/billing/manifest",
                    "baseUrl": "https://example.com/billing",
                    "compatibility": {
                        "lenso": {
                            "minVersion": "0.2.0"
                        }
                    }
                },
                {
                    "name": "local-crm",
                    "version": "0.1.0",
                    "source": "remote",
                    "manifestReference": "./lenso.module.json"
                },
                {
                    "name": "ts-service",
                    "version": "0.1.0",
                    "source": "service",
                    "manifestReference": "https://example.com/ts-service/manifest",
                    "baseUrl": "https://example.com/ts-service",
                    "compatibility": {
                        "remote_protocol_version": "99",
                        "required_host_features": ["service.status"]
                    }
                },
                {
                    "name": "old-billing",
                    "version": "0.1.0",
                    "source": "remote",
                    "manifestReference": "https://example.com/old-billing/manifest",
                    "baseUrl": "https://example.com/old-billing",
                    "archivedAt": "2026-06-07T12:00:00.000Z",
                    "archiveReason": "replaced by billing-v2"
                }
            ]
        })
        .to_string(),
    )
    .expect("write catalog fixture");
    install_admin_module_metadata(vec![]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_get("/admin/data/available-modules"))
        .await
        .expect("available modules request completes");

    remove_module_catalog_fixture();

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["status"], "failed");
    assert_eq!(body["catalog"]["modules"], 4);
    assert_eq!(body["issues"].as_array().expect("issues array").len(), 3);
    assert_eq!(body["issues"][0]["group"], "Compatibility");
    assert_eq!(
        body["issues"][0]["message"],
        "billing requires Lenso >= 0.2.0; host is 0.1.7"
    );
    assert_eq!(body["issues"][1]["group"], "Catalog");
    assert_eq!(body["issues"][1]["message"], "local-crm baseUrl is missing");
    assert_eq!(body["issues"][2]["group"], "Compatibility");
    assert_eq!(
        body["issues"][2]["message"],
        "ts-service requires remote protocol 99; host supports 1"
    );
    assert_eq!(body["modules"][0]["status"], "needs_attention");
    assert_eq!(body["modules"][1]["status"], "needs_attention");
    assert_eq!(body["modules"][2]["status"], "needs_attention");
    assert_eq!(body["modules"][3]["status"], "archived");
    assert_eq!(body["modules"][3]["manifestStatus"], "archived");
    assert_eq!(
        body["modules"][3]["archiveReason"],
        "replaced by billing-v2"
    );
}

#[tokio::test]
async fn admin_action_invocation_requires_confirmation_phrase_when_declared() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    install_admin_modules(vec![AdminModule {
        module_name: "remote-crm".to_owned(),
        source: ModuleSource::Remote,
        load_status: ModuleLoadStatus::Loaded,
        schema: stub_schema(),
        admin: Some(stub_declarative_surface()),
        listed_in_schema: false,
        data_source: Some(Arc::new(StubUsers)),
        action_source: Some(Arc::new(StubActions)),
        query_source: None,
    }]);
    install_admin_module_metadata(vec![AdminModuleMetadata {
        module_name: "remote-crm".to_owned(),
        source: ModuleSource::Remote,
        load_status: ModuleLoadStatus::Loaded,
        http_routes: vec![],
        runtime: None,
        events: None,
        lifecycle: None,
        console: vec![],
        console_slots: Vec::new(),
        console_contributions: Vec::new(),
        story_display: vec![],
        capabilities: vec!["remote_crm.contacts.sync".to_owned()],
        dependencies: vec![],
        admin: Some(stub_declarative_surface()),
        source_diagnostics: None,
    }]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let rejected = app
        .clone()
        .oneshot(admin_post_json_with_token(
            "/admin/data/remote-crm/actions/danger_sync",
            r#"{"input":{}}"#,
            "dev-service:admin:remote_crm.contacts.sync",
        ))
        .await
        .expect("request completes");
    assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);

    let accepted = app
        .oneshot(admin_post_json_with_token(
            "/admin/data/remote-crm/actions/danger_sync",
            r#"{"input":{},"confirmation_phrase":"SYNC"}"#,
            "dev-service:admin:remote_crm.contacts.sync",
        ))
        .await
        .expect("request completes");
    assert_eq!(accepted.status(), StatusCode::OK);
    let json = json_body(accepted).await;
    assert_eq!(json["data"]["action"], "danger_sync");
}

#[tokio::test]
async fn admin_action_invocation_validates_declared_input_schema() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    install_admin_modules(vec![AdminModule {
        module_name: "remote-crm".to_owned(),
        source: ModuleSource::Remote,
        load_status: ModuleLoadStatus::Loaded,
        schema: stub_schema(),
        admin: Some(stub_declarative_surface()),
        listed_in_schema: false,
        data_source: Some(Arc::new(StubUsers)),
        action_source: Some(Arc::new(StubActions)),
        query_source: None,
    }]);
    install_admin_module_metadata(vec![AdminModuleMetadata {
        module_name: "remote-crm".to_owned(),
        source: ModuleSource::Remote,
        load_status: ModuleLoadStatus::Loaded,
        http_routes: vec![],
        runtime: None,
        events: None,
        lifecycle: None,
        console: vec![],
        console_slots: Vec::new(),
        console_contributions: Vec::new(),
        story_display: vec![],
        capabilities: vec!["remote_crm.contacts.sync".to_owned()],
        dependencies: vec![],
        admin: Some(stub_declarative_surface()),
        source_diagnostics: None,
    }]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let missing_required = app
        .clone()
        .oneshot(admin_post_json_with_token(
            "/admin/data/remote-crm/actions/validated_sync",
            r#"{"input":{"filter":{"active":true}}}"#,
            "dev-service:admin:remote_crm.contacts.sync",
        ))
        .await
        .expect("request completes");
    assert_eq!(missing_required.status(), StatusCode::BAD_REQUEST);
    let missing_body = json_body(missing_required).await;
    assert_eq!(missing_body["error"]["code"], "validation_failed");
    assert_eq!(
        missing_body["error"]["message"],
        "admin action input field `limit` is required"
    );

    let wrong_type = app
        .clone()
        .oneshot(admin_post_json_with_token(
            "/admin/data/remote-crm/actions/validated_sync",
            r#"{"input":{"limit":2.5}}"#,
            "dev-service:admin:remote_crm.contacts.sync",
        ))
        .await
        .expect("request completes");
    assert_eq!(wrong_type.status(), StatusCode::BAD_REQUEST);
    let wrong_type_body = json_body(wrong_type).await;
    assert_eq!(
        wrong_type_body["error"]["message"],
        "admin action input field `limit` must be an integer"
    );

    let undeclared_field = app
        .clone()
        .oneshot(admin_post_json_with_token(
            "/admin/data/remote-crm/actions/validated_sync",
            r#"{"input":{"limit":25,"unexpected":true}}"#,
            "dev-service:admin:remote_crm.contacts.sync",
        ))
        .await
        .expect("request completes");
    assert_eq!(undeclared_field.status(), StatusCode::BAD_REQUEST);
    let undeclared_field_body = json_body(undeclared_field).await;
    assert_eq!(
        undeclared_field_body["error"]["message"],
        "admin action input field `unexpected` is not declared"
    );

    let accepted = app
        .oneshot(admin_post_json_with_token(
            "/admin/data/remote-crm/actions/validated_sync",
            r#"{"input":{"limit":25,"filter":{"active":true}}}"#,
            "dev-service:admin:remote_crm.contacts.sync",
        ))
        .await
        .expect("request completes");
    assert_eq!(accepted.status(), StatusCode::OK);
    let json = json_body(accepted).await;
    assert_eq!(json["data"]["action"], "validated_sync");
    assert_eq!(json["data"]["input"]["limit"], 25);
    assert_eq!(json["data"]["input"]["filter"]["active"], true);
}

#[tokio::test]
async fn admin_action_invocation_records_story_and_technical_operation() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    install_admin_modules(vec![AdminModule {
        module_name: "remote-crm".to_owned(),
        source: ModuleSource::Remote,
        load_status: ModuleLoadStatus::Loaded,
        schema: stub_schema(),
        admin: Some(stub_declarative_surface()),
        listed_in_schema: false,
        data_source: Some(Arc::new(StubUsers)),
        action_source: Some(Arc::new(StubActions)),
        query_source: None,
    }]);
    install_admin_module_metadata(vec![AdminModuleMetadata {
        module_name: "remote-crm".to_owned(),
        source: ModuleSource::Remote,
        load_status: ModuleLoadStatus::Loaded,
        http_routes: vec![],
        runtime: None,
        events: None,
        lifecycle: None,
        console: vec![],
        console_slots: Vec::new(),
        console_contributions: Vec::new(),
        story_display: vec![],
        capabilities: vec!["remote_crm.contacts.sync".to_owned()],
        dependencies: vec![],
        admin: Some(stub_declarative_surface()),
        source_diagnostics: None,
    }]);
    let app = app_with_test_db(&db).await;

    let response = app
        .clone()
        .oneshot(
            admin_post_json_with_token(
                "/admin/data/remote-crm/actions/sync_contacts",
                r#"{"input":{"dry_run":true}}"#,
                "dev-service:admin:remote_crm.contacts.sync",
            )
            .with_header("x-request-id", "req_admin_action_story")
            .with_header("x-correlation-id", "corr_admin_action_story")
            .with_header(
                "traceparent",
                "00-00000000000000000000000000000031-0000000000000031-01",
            ),
        )
        .await
        .expect("request completes");
    assert_eq!(response.status(), StatusCode::OK);
    let response_json = json_body(response).await;
    assert_eq!(
        response_json["invocation"]["request_id"],
        "req_admin_action_story"
    );
    assert_eq!(
        response_json["invocation"]["correlation_id"],
        "corr_admin_action_story"
    );
    assert_eq!(
        response_json["invocation"]["story_node_id"],
        "adminaction_req_admin_action_story"
    );

    let story_response = app
        .clone()
        .oneshot(admin_get("/admin/runtime/stories/corr_admin_action_story"))
        .await
        .expect("story request completes");
    assert_eq!(story_response.status(), StatusCode::OK);
    let story = json_body(story_response).await;
    let nodes = story["data"]["nodes"].as_array().expect("nodes array");
    let action_node = nodes
        .iter()
        .find(|node| node["type"] == "admin_action")
        .expect("admin action story node");
    assert_eq!(action_node["id"], "adminaction_req_admin_action_story");
    assert_eq!(action_node["name"], "Sync contacts");
    assert_eq!(action_node["status"], "completed");
    assert_eq!(action_node["service"], "remote-crm");
    assert_eq!(
        action_node["metadata"]["source_metadata"]["action_name"],
        "sync_contacts"
    );
    assert_eq!(
        action_node["metadata"]["source_metadata"]["capability"],
        "remote_crm.contacts.sync"
    );
    assert_eq!(
        action_node["metadata"]["source_metadata"]["input_summary"],
        "dry_run: true"
    );
    assert_eq!(
        action_node["metadata"]["source_metadata"]["result_summary"],
        "action: sync_contacts / input: {...}"
    );
    assert_eq!(
        action_node["metadata"]["source_metadata"]["trace_id"],
        "00000000000000000000000000000031"
    );
    assert_eq!(
        action_node["metadata"]["source_metadata"]["span_id"],
        "0000000000000031"
    );

    let operations_response = app
        .clone()
        .oneshot(admin_get(
            "/admin/runtime/stories/corr_admin_action_story/technical-operations",
        ))
        .await
        .expect("technical operations request completes");
    assert_eq!(operations_response.status(), StatusCode::OK);
    let operations = json_body(operations_response).await;
    let operation = operations["data"]
        .as_array()
        .expect("operations array")
        .iter()
        .find(|operation| operation["source"] == "admin_action")
        .expect("admin action technical operation");
    assert_eq!(operation["category"], "admin");
    assert_eq!(operation["status"], "ok");
    assert_eq!(
        operation["related_node_id"],
        "adminaction_req_admin_action_story"
    );
    assert_eq!(
        operation["attributes"]["request_id"],
        "req_admin_action_story"
    );
    assert_eq!(operation["attributes"]["module_name"], "remote-crm");

    let list_response = app
        .oneshot(admin_get(
            "/admin/runtime/admin-actions?module_name=remote-crm&action_name=sync_contacts&correlation_id=corr_admin_action_story&success=true",
        ))
        .await
        .expect("admin action list request completes");
    assert_eq!(list_response.status(), StatusCode::OK);
    let list = json_body(list_response).await;
    assert_eq!(list["data"][0]["id"], "adminaction_req_admin_action_story");
    assert_eq!(list["data"][0]["module_name"], "remote-crm");
    assert_eq!(list["data"][0]["action_name"], "sync_contacts");
    assert_eq!(list["data"][0]["label"], "Sync contacts");
    assert_eq!(list["data"][0]["success"], true);
    assert_eq!(list["data"][0]["capability"], "remote_crm.contacts.sync");
    assert_eq!(list["data"][0]["request_id"], "req_admin_action_story");
    assert_eq!(list["data"][0]["input_summary"], "dry_run: true");
    assert_eq!(
        list["data"][0]["result_summary"],
        "action: sync_contacts / input: {...}"
    );

    db.cleanup().await;
}

#[tokio::test]
async fn admin_action_invocation_requires_declared_capability_scope() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    install_admin_modules(vec![AdminModule {
        module_name: "remote-crm".to_owned(),
        source: ModuleSource::Remote,
        load_status: ModuleLoadStatus::Loaded,
        schema: stub_schema(),
        admin: Some(stub_declarative_surface()),
        listed_in_schema: false,
        data_source: Some(Arc::new(StubUsers)),
        action_source: Some(Arc::new(StubActions)),
        query_source: None,
    }]);
    install_admin_module_metadata(vec![AdminModuleMetadata {
        module_name: "remote-crm".to_owned(),
        source: ModuleSource::Remote,
        load_status: ModuleLoadStatus::Loaded,
        http_routes: vec![],
        runtime: None,
        events: None,
        lifecycle: None,
        console: vec![],
        console_slots: Vec::new(),
        console_contributions: Vec::new(),
        story_display: vec![],
        capabilities: vec!["remote_crm.contacts.sync".to_owned()],
        dependencies: vec![],
        admin: Some(stub_declarative_surface()),
        source_diagnostics: None,
    }]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_post_json(
            "/admin/data/remote-crm/actions/sync_contacts",
            r#"{"input":{"dry_run":true}}"#,
        ))
        .await
        .expect("request completes");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_action_invocation_rejects_unknown_action() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    install_admin_modules(vec![AdminModule {
        module_name: "remote-crm".to_owned(),
        source: ModuleSource::Remote,
        load_status: ModuleLoadStatus::Loaded,
        schema: stub_schema(),
        admin: Some(stub_declarative_surface()),
        listed_in_schema: false,
        data_source: Some(Arc::new(StubUsers)),
        action_source: Some(Arc::new(StubActions)),
        query_source: None,
    }]);
    install_admin_module_metadata(vec![AdminModuleMetadata {
        module_name: "remote-crm".to_owned(),
        source: ModuleSource::Remote,
        load_status: ModuleLoadStatus::Loaded,
        http_routes: vec![],
        runtime: None,
        events: None,
        lifecycle: None,
        console: vec![],
        console_slots: Vec::new(),
        console_contributions: Vec::new(),
        story_display: vec![],
        capabilities: vec!["remote_crm.contacts.sync".to_owned()],
        dependencies: vec![],
        admin: Some(stub_declarative_surface()),
        source_diagnostics: None,
    }]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_post_json(
            "/admin/data/remote-crm/actions/missing_action",
            r#"{"input":{}}"#,
        ))
        .await
        .expect("request completes");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
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
            admin: Some(AdminSurface::Schema(stub_schema())),
            listed_in_schema: true,
            data_source: Some(Arc::new(StubUsers)),
            action_source: None,
            query_source: None,
        },
        AdminModule {
            module_name: "identity-declarative".to_owned(),
            source: ModuleSource::Linked,
            load_status: ModuleLoadStatus::Loaded,
            schema: stub_schema(),
            admin: Some(AdminSurface::Schema(stub_schema())),
            listed_in_schema: false,
            data_source: Some(Arc::new(StubUsers)),
            action_source: None,
            query_source: None,
        },
    ]);
    install_admin_module_metadata(vec![]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
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
        admin: Some(AdminSurface::Schema(stub_schema())),
        listed_in_schema: true,
        data_source: Some(Arc::new(StubUsers)),
        action_source: None,
        query_source: None,
    }]);
    install_admin_module_refresh_fn(|| async {
        REFRESH_COUNT.fetch_add(1, Ordering::SeqCst);
        Ok(vec![
            AdminModule {
                module_name: "identity".to_owned(),
                source: ModuleSource::Linked,
                load_status: ModuleLoadStatus::Loaded,
                schema: stub_schema(),
                admin: Some(AdminSurface::Schema(stub_schema())),
                listed_in_schema: true,
                data_source: Some(Arc::new(StubUsers)),
                action_source: None,
                query_source: None,
            },
            AdminModule {
                module_name: "remote-crm".to_owned(),
                source: ModuleSource::Remote,
                load_status: ModuleLoadStatus::Error {
                    message: "remote manifest request failed".to_owned(),
                },
                schema: AdminSchema { entities: vec![] },
                admin: None,
                listed_in_schema: true,
                data_source: None,
                action_source: None,
                query_source: None,
            },
        ])
    });
    install_admin_module_metadata_refresh_fn(|| async {
        Ok(vec![
            AdminModuleMetadata {
                module_name: "identity".to_owned(),
                source: ModuleSource::Linked,
                load_status: ModuleLoadStatus::Loaded,
                http_routes: vec![],
                runtime: None,
                events: None,
                lifecycle: None,
                console: vec![],
                console_slots: Vec::new(),
                console_contributions: Vec::new(),
                story_display: vec![StoryDisplayDescriptor {
                    source: StoryDisplaySource::ExecutionName {
                        name: "identity.create_user".to_owned(),
                    },
                    display_name: "Create User".to_owned(),
                    story_title: Some("User Registration".to_owned()),
                }],
                capabilities: vec!["identity.users.read".to_owned()],
                dependencies: vec![],
                admin: Some(AdminSurface::Schema(stub_schema())),
                source_diagnostics: None,
            },
            AdminModuleMetadata {
                module_name: "remote-crm".to_owned(),
                source: ModuleSource::Remote,
                load_status: ModuleLoadStatus::Error {
                    message: "remote manifest request failed".to_owned(),
                },
                http_routes: vec![],
                runtime: None,
                events: None,
                lifecycle: None,
                console: vec![],
                console_slots: Vec::new(),
                console_contributions: Vec::new(),
                story_display: vec![],
                capabilities: vec![],
                dependencies: vec![],
                admin: None,
                source_diagnostics: None,
            },
        ])
    });
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
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
    let refreshed_identity_metadata = modules_body["modules"]
        .as_array()
        .expect("modules array")
        .iter()
        .find(|module| module["module_name"] == "identity")
        .expect("identity metadata was refreshed");
    assert_eq!(
        refreshed_identity_metadata["capabilities"],
        serde_json::json!(["identity.users.read"])
    );
    assert_eq!(
        refreshed_identity_metadata["story_display"][0]["story_title"],
        "User Registration"
    );

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

#[tokio::test]
async fn refresh_modules_replaces_module_registry_metadata() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    static METADATA_REFRESH_COUNT: AtomicUsize = AtomicUsize::new(0);

    install_admin_modules(vec![AdminModule {
        module_name: "identity".to_owned(),
        source: ModuleSource::Linked,
        load_status: ModuleLoadStatus::Loaded,
        schema: stub_schema(),
        admin: Some(AdminSurface::Schema(stub_schema())),
        listed_in_schema: true,
        data_source: Some(Arc::new(StubUsers)),
        action_source: None,
        query_source: None,
    }]);
    install_admin_module_metadata(vec![AdminModuleMetadata {
        module_name: "identity".to_owned(),
        source: ModuleSource::Linked,
        load_status: ModuleLoadStatus::Loaded,
        http_routes: vec![],
        runtime: None,
        events: None,
        lifecycle: None,
        console: vec![],
        console_slots: Vec::new(),
        console_contributions: Vec::new(),
        story_display: vec![],
        capabilities: vec![],
        dependencies: vec![],
        admin: Some(AdminSurface::Schema(stub_schema())),
        source_diagnostics: None,
    }]);
    install_admin_module_metadata_refresh_fn(|| async {
        METADATA_REFRESH_COUNT.fetch_add(1, Ordering::SeqCst);
        Ok(vec![AdminModuleMetadata {
            module_name: "notifications".to_owned(),
            source: ModuleSource::Linked,
            load_status: ModuleLoadStatus::Loaded,
            http_routes: vec![],
            runtime: None,
            events: None,
            lifecycle: None,
            console: vec![],
            console_slots: Vec::new(),
            console_contributions: Vec::new(),
            story_display: vec![StoryDisplayDescriptor {
                source: StoryDisplaySource::ExecutionName {
                    name: "notifications.send_welcome_email.v1".to_owned(),
                },
                display_name: "Send Welcome Email".to_owned(),
                story_title: None,
            }],
            capabilities: vec!["notifications.email.send".to_owned()],
            dependencies: vec![],
            admin: None,
            source_diagnostics: None,
        }])
    });
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let refresh_response = app
        .clone()
        .oneshot(admin_post("/admin/data/modules/refresh"))
        .await
        .expect("refresh request completes");
    assert_eq!(refresh_response.status(), StatusCode::OK);
    let refresh_body = json_body(refresh_response).await;
    assert_eq!(METADATA_REFRESH_COUNT.load(Ordering::SeqCst), 1);
    assert_eq!(refresh_body["modules"][0]["module_name"], "notifications");
    assert!(refresh_body["refreshed_at"].as_str().is_some());
    assert_eq!(refresh_body["refresh_error"], Value::Null);
    assert_eq!(refresh_body["refresh_history"][0]["status"], "success");
    assert_eq!(refresh_body["refresh_history"][0]["module_count"], 1);
    assert!(
        refresh_body["refresh_history"][0]["duration_ms"]
            .as_u64()
            .is_some()
    );
    assert_eq!(refresh_body["refresh_history"][0]["error"], Value::Null);
    assert_eq!(
        refresh_body["refresh_history"][0]["module_results"][0]["module_name"],
        "notifications"
    );
    assert_eq!(
        refresh_body["refresh_history"][0]["module_results"][0]["status"],
        "loaded"
    );
    assert_eq!(
        refresh_body["modules"][0]["capabilities"],
        serde_json::json!(["notifications.email.send"])
    );
    assert_eq!(
        refresh_body["modules"][0]["story_display"][0]["display_name"],
        "Send Welcome Email"
    );

    let modules_response = app
        .oneshot(admin_get("/admin/data/modules"))
        .await
        .expect("modules request completes");
    let modules_body = json_body(modules_response).await;
    assert!(
        modules_body["modules"]
            .as_array()
            .expect("modules array")
            .iter()
            .any(|module| module["module_name"] == "notifications")
    );
    assert_eq!(modules_body["refresh_error"], Value::Null);
}

#[tokio::test]
async fn refresh_modules_records_error_without_dropping_snapshot() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    install_admin_modules(vec![]);
    install_admin_module_metadata(vec![AdminModuleMetadata {
        module_name: "identity".to_owned(),
        source: ModuleSource::Linked,
        load_status: ModuleLoadStatus::Loaded,
        http_routes: vec![],
        runtime: None,
        events: None,
        lifecycle: None,
        console: vec![],
        console_slots: Vec::new(),
        console_contributions: Vec::new(),
        story_display: vec![],
        capabilities: vec![],
        dependencies: vec![],
        admin: Some(AdminSurface::Schema(stub_schema())),
        source_diagnostics: None,
    }]);
    install_admin_module_metadata_refresh_fn(|| async {
        Err(platform_core::AppError::new(
            platform_core::ErrorCode::ExternalDependency,
            "remote manifest request failed",
        )
        .retryable())
    });
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_post("/admin/data/modules/refresh"))
        .await
        .expect("request completes");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["modules"][0]["module_name"], "identity");
    assert!(body["refreshed_at"].as_str().is_some());
    assert_eq!(body["refresh_error"], "remote manifest request failed");
    assert_eq!(body["refresh_history"][0]["status"], "error");
    assert_eq!(body["refresh_history"][0]["module_count"], 1);
    assert_eq!(
        body["refresh_history"][0]["error"],
        "remote manifest request failed"
    );
    assert_eq!(
        body["refresh_history"][0]["module_results"]
            .as_array()
            .expect("top-level refresh failure has no module results")
            .len(),
        0
    );
}
