use app_api::build_router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use platform_admin_data::{
    AdminModule, AdminModuleMetadata, AdminModuleSourceDiagnostics, AdminRemoteModuleDiagnostics,
    install_admin_module_metadata, install_admin_module_metadata_refresh_fn,
    install_admin_module_refresh_fn, install_admin_modules,
};
use platform_core::{
    AppConfig, AppContext, LoggingEventPublisher, PLATFORM_MIGRATIONS, StoryDisplayDescriptor,
    StoryDisplaySource, apply_migrations,
};
use platform_module::{
    AdminAction, AdminActionConfirmation, AdminActionDangerLevel, AdminActionInputField,
    AdminActionInputSchema, AdminActionSource, AdminDataSource, AdminDeclarativeComponent,
    AdminDeclarativePage, AdminDeclarativeSection, AdminDeclarativeSurface, AdminListQuery,
    AdminPage, AdminSchema, AdminSurface, EntitySchema, FieldSchema, FieldType, ModuleLoadStatus,
    ModuleSource,
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
            sections: vec![AdminDeclarativeSection {
                name: "contacts".to_owned(),
                label: "Contacts".to_owned(),
                component: AdminDeclarativeComponent::EntityTable {
                    entity: "users".to_owned(),
                },
            }],
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

fn admin_post(path: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
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
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    install_admin_modules(app_bootstrap::admin_modules(&ctx));
    install_admin_module_metadata(
        app_bootstrap::load_admin_module_metadata(&ctx)
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
    let identity = json["modules"]
        .as_array()
        .expect("modules array")
        .iter()
        .find(|module| module["module_name"] == "identity")
        .expect("identity module metadata");
    assert_eq!(identity["source"], "linked");
    assert_eq!(identity["http_routes"][0]["method"], "POST");
    assert_eq!(identity["http_routes"][0]["path"], "/v1/identity/users");
    assert_eq!(
        identity["http_routes"][0]["display_name"],
        "Create User Request"
    );
    assert_eq!(
        identity["http_routes"][0]["story_title"],
        "User Registration"
    );
    assert_eq!(identity["http_routes"][1]["method"], "GET");
    assert_eq!(identity["http_routes"][1]["path"], "/v1/identity/me");
    assert_eq!(
        identity["http_routes"][1]["display_name"],
        "Fetch Current User"
    );
}

#[tokio::test]
async fn modules_endpoint_lists_linked_modules_without_admin_surfaces() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    install_admin_modules(app_bootstrap::admin_modules(&ctx));
    install_admin_module_metadata(
        app_bootstrap::load_admin_module_metadata(&ctx)
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
    let notifications = json["modules"]
        .as_array()
        .expect("modules array")
        .iter()
        .find(|module| module["module_name"] == "notifications")
        .expect("notifications module metadata");
    assert_eq!(notifications["source"], "linked");
    assert_eq!(notifications["status"], "loaded");
    assert_eq!(notifications["error"], Value::Null);
    assert_eq!(notifications["http_routes"], serde_json::json!([]));
    assert_eq!(
        notifications["runtime"]["functions"][0]["name"],
        "notifications.send_welcome_email.v1"
    );
    assert_eq!(
        notifications["runtime"]["functions"][0]["queue"],
        "notifications"
    );
    assert_eq!(notifications["capabilities"], serde_json::json!([]));
    assert!(
        notifications["story_display"]
            .as_array()
            .expect("story display array")
            .iter()
            .any(|descriptor| {
                descriptor["display_name"] == "Send Welcome Email"
                    && descriptor["source"]["kind"] == "execution_name"
                    && descriptor["source"]["name"] == "notifications.send_welcome_email.v1"
            })
    );
    assert_eq!(notifications["admin"], Value::Null);
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
        story_display: vec![],
        capabilities: vec!["billing.read".to_owned()],
        dependencies: vec![],
        admin: None,
        source_diagnostics: Some(AdminModuleSourceDiagnostics::Remote(
            AdminRemoteModuleDiagnostics {
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
        "0.1.0"
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
                        "maxVersion": "0.1.0"
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
    let _install_plan = FileFixture::write(
        ".lenso/console-package-install-plan.json",
        serde_json::json!({
            "version": 1,
            "modules": [{
                "baseUrl": "https://example.com/billing",
                "consolePackages": [{
                    "command": "pnpm add @vendor/lenso-billing-console",
                    "exportName": "billingConsoleModule",
                    "key": "@vendor/lenso-billing-console#billingConsoleModule",
                    "packageName": "@vendor/lenso-billing-console",
                    "requestedByModule": "billing",
                    "route": "/data/billing",
                    "status": "requires_manual_install"
                }],
                "manifestReference": "https://example.com/billing/manifest",
                "moduleName": "billing",
                "restartRequired": true
            }]
        })
        .to_string(),
    );
    let _env = FileFixture::write(
        ".env",
        "REMOTE_MODULES=billing=https://example.com/billing/\n",
    );
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

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let install_state = &body["modules"][0]["installState"];
    assert_eq!(install_state["moduleRegistered"], false);
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
        "remote source configured in .env but not loaded"
    );
    assert_eq!(
        install_state["consolePlan"]["planFile"],
        ".lenso/console-package-install-plan.json"
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
        install_state["consolePlan"]["packages"][0]["command"],
        "pnpm add @vendor/lenso-billing-console"
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
    assert_eq!(body["catalog"]["modules"], 3);
    assert_eq!(body["issues"].as_array().expect("issues array").len(), 2);
    assert_eq!(body["issues"][0]["group"], "Compatibility");
    assert_eq!(
        body["issues"][0]["message"],
        "billing requires Lenso >= 0.2.0; host is 0.1.0"
    );
    assert_eq!(body["issues"][1]["group"], "Catalog");
    assert_eq!(body["issues"][1]["message"], "local-crm baseUrl is missing");
    assert_eq!(body["modules"][0]["status"], "needs_attention");
    assert_eq!(body["modules"][1]["status"], "needs_attention");
    assert_eq!(body["modules"][2]["status"], "archived");
    assert_eq!(body["modules"][2]["manifestStatus"], "archived");
    assert_eq!(
        body["modules"][2]["archiveReason"],
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
