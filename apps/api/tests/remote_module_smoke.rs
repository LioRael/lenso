use app_api::build_router;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use platform_admin_data::{install_admin_module_metadata, install_admin_modules};
use platform_core::{
    AppConfig, AppContext, AuthConfig, DatabaseConfig, DbPool, HttpConfig, LoggingEventPublisher,
    ModuleSourcesConfig, RemoteModuleSourceConfig, ServiceConfig, TelemetryConfig,
};
use serde_json::Value;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tower::ServiceExt;

static REMOTE_SMOKE_TEST_LOCK: Mutex<()> = Mutex::const_new(());

async fn spawn_remote_module(router: Router) -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind remote module fixture");
    let address = listener.local_addr().expect("fixture address");

    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("fixture server");
    });

    format!("http://{address}/lenso/module/v1")
}

async fn app_with_remote_module(base_url: String) -> axum::Router {
    app_with_remote_modules(vec![RemoteModuleSourceConfig {
        name: "remote-crm".to_owned(),
        base_url,
        auth_token_env: None,
        timeout_ms: 5_000,
    }])
    .await
}

async fn app_with_remote_modules(remote: Vec<RemoteModuleSourceConfig>) -> axum::Router {
    let config = AppConfig {
        service: ServiceConfig::default(),
        database: DatabaseConfig {
            url: "postgres://localhost/lenso_test".to_owned(),
            max_connections: 1,
        },
        http: HttpConfig::default(),
        telemetry: TelemetryConfig::default(),
        auth: AuthConfig::default(),
        module_sources: ModuleSourcesConfig { remote },
        modules: Default::default(),
    };
    let ctx = AppContext::new(
        config,
        DbPool::connect_lazy("postgres://localhost/lenso_test").expect("lazy pool"),
        Arc::new(LoggingEventPublisher),
    );
    let admin_modules = app_bootstrap::load_admin_modules(&ctx)
        .await
        .expect("remote admin modules load");
    let admin_module_metadata = app_bootstrap::load_admin_module_metadata(&ctx)
        .await
        .expect("remote admin module metadata loads");

    install_admin_modules(admin_modules);
    install_admin_module_metadata(admin_module_metadata);
    build_router(ctx)
}

fn admin_get(path: &str) -> Request<Body> {
    Request::builder()
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
async fn remote_module_fixture_is_visible_through_admin_data_api() {
    let _guard = REMOTE_SMOKE_TEST_LOCK.lock().await;
    let base_url = spawn_remote_module(remote_module_example::router()).await;
    let app = app_with_remote_module(base_url).await;

    let schema_response = app
        .clone()
        .oneshot(admin_get("/admin/data/schema"))
        .await
        .expect("schema request completes");
    assert_eq!(schema_response.status(), StatusCode::OK);
    let schema = json_body(schema_response).await;
    let remote_schema = schema["modules"]
        .as_array()
        .expect("modules array")
        .iter()
        .find(|module| module["module_name"] == "remote-crm")
        .expect("remote-crm schema is installed");
    assert_eq!(remote_schema["source"], "remote");
    assert_eq!(remote_schema["status"], "loaded");
    assert_eq!(remote_schema["error"], Value::Null);
    assert_eq!(remote_schema["schema"]["entities"][0]["name"], "contacts");

    let modules_response = app
        .clone()
        .oneshot(admin_get("/admin/data/modules"))
        .await
        .expect("modules request completes");
    assert_eq!(modules_response.status(), StatusCode::OK);
    let modules = json_body(modules_response).await;
    let remote_module = modules["modules"]
        .as_array()
        .expect("modules array")
        .iter()
        .find(|module| module["module_name"] == "remote-crm")
        .expect("remote-crm metadata is installed");
    assert_eq!(remote_module["source"], "remote");
    assert_eq!(remote_module["status"], "loaded");
    assert_eq!(remote_module["admin"]["kind"], "schema");

    let list_response = app
        .clone()
        .oneshot(admin_get("/admin/data/remote-crm/contacts?limit=2"))
        .await
        .expect("list request completes");
    assert_eq!(list_response.status(), StatusCode::OK);
    let list = json_body(list_response).await;
    assert_eq!(list["data"][0]["id"], "contact_1");
    assert_eq!(list["data"][0]["email"], "ada@example.com");
    assert_eq!(list["page"]["next_cursor"], "contact_2");

    let detail_response = app
        .oneshot(admin_get("/admin/data/remote-crm/contacts/contact_2"))
        .await
        .expect("detail request completes");
    assert_eq!(detail_response.status(), StatusCode::OK);
    let detail = json_body(detail_response).await;
    assert_eq!(detail["data"]["id"], "contact_2");
    assert_eq!(detail["data"]["company"], "Compiler Systems");
}

#[tokio::test]
async fn failed_remote_module_load_is_reported_in_schema() {
    let _guard = REMOTE_SMOKE_TEST_LOCK.lock().await;
    let app = app_with_remote_modules(vec![RemoteModuleSourceConfig {
        name: "remote-crm".to_owned(),
        base_url: "http://127.0.0.1:9/lenso/module/v1".to_owned(),
        auth_token_env: None,
        timeout_ms: 50,
    }])
    .await;

    let schema_response = app
        .clone()
        .oneshot(admin_get("/admin/data/schema"))
        .await
        .expect("schema request completes");
    assert_eq!(schema_response.status(), StatusCode::OK);
    let schema = json_body(schema_response).await;
    let remote_schema = schema["modules"]
        .as_array()
        .expect("modules array")
        .iter()
        .find(|module| module["module_name"] == "remote-crm")
        .expect("failed remote is reported");

    assert_eq!(remote_schema["source"], "remote");
    assert_eq!(remote_schema["status"], "error");
    assert!(
        remote_schema["error"]
            .as_str()
            .expect("error message")
            .contains("remote manifest request failed")
    );
    assert_eq!(
        remote_schema["schema"]["entities"]
            .as_array()
            .expect("empty schema"),
        &Vec::<Value>::new()
    );

    let list_response = app
        .oneshot(admin_get("/admin/data/remote-crm/contacts"))
        .await
        .expect("list request completes");
    assert_eq!(list_response.status(), StatusCode::BAD_GATEWAY);
    let body = json_body(list_response).await;
    assert_eq!(body["error"]["message"], "module remote-crm is not loaded");
}

#[tokio::test]
async fn custom_remote_modules_are_visible_through_metadata_api() {
    let _guard = REMOTE_SMOKE_TEST_LOCK.lock().await;
    let fixture_base = spawn_remote_module(remote_module_example::router()).await;
    let iframe_origin = fixture_base.trim_end_matches("/lenso/module/v1").to_owned();
    let app = app_with_remote_modules(vec![
        RemoteModuleSourceConfig {
            name: "remote-crm".to_owned(),
            base_url: spawn_remote_module(remote_module_example::router()).await,
            auth_token_env: None,
            timeout_ms: 5_000,
        },
        RemoteModuleSourceConfig {
            name: "remote-crm-embedded".to_owned(),
            base_url: format!("{fixture_base}/embedded"),
            auth_token_env: None,
            timeout_ms: 5_000,
        },
        RemoteModuleSourceConfig {
            name: "remote-crm-declarative".to_owned(),
            base_url: format!("{fixture_base}/declarative"),
            auth_token_env: None,
            timeout_ms: 5_000,
        },
    ])
    .await;

    let modules_response = app
        .clone()
        .oneshot(admin_get("/admin/data/modules"))
        .await
        .expect("modules request completes");
    assert_eq!(modules_response.status(), StatusCode::OK);
    let modules = json_body(modules_response).await;
    let embedded_module = modules["modules"]
        .as_array()
        .expect("modules array")
        .iter()
        .find(|module| module["module_name"] == "remote-crm-embedded")
        .expect("remote-crm-embedded metadata is installed");
    assert_eq!(embedded_module["source"], "remote");
    assert_eq!(embedded_module["status"], "loaded");
    assert_eq!(embedded_module["admin"]["kind"], "embedded_custom");
    assert_eq!(embedded_module["admin"]["runtime"], "iframe");
    assert_eq!(
        embedded_module["admin"]["entry"]["url"],
        format!("{fixture_base}/embedded/admin")
    );
    assert_eq!(
        embedded_module["admin"]["entry"]["allowed_origins"],
        serde_json::json!([iframe_origin])
    );
    assert_eq!(
        embedded_module["admin"]["fallback_schema"]["entities"][0]["name"],
        "contacts"
    );

    let declarative_module = modules["modules"]
        .as_array()
        .expect("modules array")
        .iter()
        .find(|module| module["module_name"] == "remote-crm-declarative")
        .expect("remote-crm-declarative metadata is installed");
    assert_eq!(declarative_module["source"], "remote");
    assert_eq!(declarative_module["status"], "loaded");
    assert_eq!(declarative_module["admin"]["kind"], "declarative_custom");
    assert_eq!(declarative_module["admin"]["pages"][0]["name"], "overview");
    assert_eq!(
        declarative_module["admin"]["pages"][0]["sections"][0]["component"]["kind"],
        "metric_strip"
    );
    assert_eq!(
        declarative_module["admin"]["fallback_schema"]["entities"][0]["name"],
        "contacts"
    );

    let schema_response = app
        .oneshot(admin_get("/admin/data/schema"))
        .await
        .expect("schema request completes");
    assert_eq!(schema_response.status(), StatusCode::OK);
    let schema = json_body(schema_response).await;
    assert!(
        !schema["modules"]
            .as_array()
            .expect("modules array")
            .iter()
            .any(|module| module["module_name"] == "remote-crm-embedded")
    );
    assert!(
        !schema["modules"]
            .as_array()
            .expect("modules array")
            .iter()
            .any(|module| module["module_name"] == "remote-crm-declarative")
    );
}
