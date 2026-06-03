use app_api::build_router;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use platform_admin_data::install_admin_modules;
use platform_core::{
    AppConfig, AppContext, AuthConfig, DatabaseConfig, DbPool, HttpConfig, LoggingEventPublisher,
    ModuleSourcesConfig, RemoteModuleSourceConfig, ServiceConfig, TelemetryConfig,
};
use serde_json::Value;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower::ServiceExt;

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
    let config = AppConfig {
        service: ServiceConfig::default(),
        database: DatabaseConfig {
            url: "postgres://localhost/lenso_test".to_owned(),
            max_connections: 1,
        },
        http: HttpConfig::default(),
        telemetry: TelemetryConfig::default(),
        auth: AuthConfig::default(),
        module_sources: ModuleSourcesConfig {
            remote: vec![RemoteModuleSourceConfig {
                name: "remote-crm".to_owned(),
                base_url,
                auth_token_env: None,
                timeout_ms: 5_000,
            }],
        },
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

    install_admin_modules(admin_modules);
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
