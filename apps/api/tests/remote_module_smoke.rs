use app_api::build_router;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use platform_admin_data::{
    AdminModuleMetadata, install_admin_module_metadata, install_admin_modules,
};
use platform_core::{
    ActorContext, AppConfig, AppContext, AuthConfig, CorrelationId, DatabaseConfig, DbPool,
    HttpConfig, LoggingEventPublisher, ModuleSourcesConfig, PLATFORM_MIGRATIONS,
    RemoteModuleSourceConfig, ServiceConfig, TelemetryConfig, TraceContext, apply_migrations,
};
use platform_runtime::{EnqueueFunctionRequest, RUNTIME_MIGRATIONS, RuntimeClient, RuntimeWorker};
use platform_testing::TestDatabase;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
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
    let db = DbPool::connect_lazy("postgres://localhost/lenso_test").expect("lazy pool");
    app_with_remote_modules_and_db(remote, db, "postgres://localhost/lenso_test".to_owned()).await
}

async fn app_with_remote_modules_and_db(
    remote: Vec<RemoteModuleSourceConfig>,
    db: DbPool,
    database_url: String,
) -> axum::Router {
    let ctx = app_context_with_remote_modules_and_db(remote, db, database_url);
    let admin_modules = app_bootstrap::load_admin_modules(&ctx)
        .await
        .expect("remote admin modules load");
    let admin_module_metadata = app_bootstrap::load_admin_module_metadata(&ctx)
        .await
        .expect("remote admin module metadata loads");
    let remote_http_proxy_registry = app_bootstrap::load_remote_http_proxy_registry(&ctx)
        .await
        .expect("remote HTTP proxy registry loads");

    install_admin_modules(admin_modules);
    install_module_metadata(admin_module_metadata);
    platform_module_remote::install_remote_http_proxy_registry(remote_http_proxy_registry);
    build_router(ctx)
}

fn app_context_with_remote_modules_and_db(
    remote: Vec<RemoteModuleSourceConfig>,
    db: DbPool,
    database_url: String,
) -> AppContext {
    let config = AppConfig {
        service: ServiceConfig::default(),
        database: DatabaseConfig {
            url: database_url,
            max_connections: 1,
        },
        http: HttpConfig::default(),
        telemetry: TelemetryConfig::default(),
        auth: AuthConfig::default(),
        module_sources: ModuleSourcesConfig {
            remote,
            ..ModuleSourcesConfig::default()
        },
        modules: Default::default(),
    };
    AppContext::new(config, db, Arc::new(LoggingEventPublisher))
}

fn install_module_metadata(metadata: Vec<AdminModuleMetadata>) {
    platform_admin::install_runtime_function_declarations(
        platform_admin::runtime_function_declarations_from_modules(
            app_bootstrap::runtime_function_declaration_sources_from_metadata(&metadata),
        ),
    );
    install_admin_module_metadata(metadata);
}

async fn app_with_remote_module_and_test_db(base_url: String, db: &TestDatabase) -> axum::Router {
    let migrations = PLATFORM_MIGRATIONS
        .iter()
        .chain(RUNTIME_MIGRATIONS)
        .copied()
        .collect::<Vec<_>>();
    apply_migrations(&db.pool, &migrations)
        .await
        .expect("platform and runtime migrations apply");
    app_with_remote_modules_and_db(
        vec![RemoteModuleSourceConfig {
            name: "remote-crm".to_owned(),
            base_url,
            auth_token_env: None,
            timeout_ms: 5_000,
        }],
        db.pool.clone(),
        db.url.clone(),
    )
    .await
}

fn admin_get(path: &str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .header("authorization", "Bearer dev-service:admin")
        .body(Body::empty())
        .expect("request builds")
}

fn service_get(path: &str, token: &str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .expect("request builds")
}

fn service_post(
    path: &str,
    token: &str,
    content_type: &str,
    body: impl Into<Body>,
) -> Request<Body> {
    service_json_method("POST", path, token, content_type, body)
}

fn service_json_method(
    method: &str,
    path: &str,
    token: &str,
    content_type: &str,
    body: impl Into<Body>,
) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", content_type)
        .body(body.into())
        .expect("request builds")
}

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    serde_json::from_slice(&bytes).expect("json body")
}

fn error_detail_reason<'a>(body: &'a Value, field: &str) -> Option<&'a str> {
    body["error"]["details"]
        .as_array()?
        .iter()
        .find(|detail| detail["field"] == field)
        .and_then(|detail| detail["reason"].as_str())
}

#[derive(Debug)]
struct RemoteProxyCallRow {
    id: String,
    module_name: String,
    method: String,
    declared_path: String,
    remote_path: String,
    remote_status: Option<i32>,
    duration_ms: i64,
    success: bool,
    error_code: Option<String>,
    request_id: String,
    correlation_id: String,
    trace_id: Option<String>,
    span_id: Option<String>,
    path_params: Value,
    error_details: Value,
}

type RemoteProxyCallTuple = (
    String,
    String,
    String,
    String,
    String,
    Option<i32>,
    i64,
    bool,
    Option<String>,
    String,
    String,
    Option<String>,
    Option<String>,
    Value,
    Value,
);

impl From<RemoteProxyCallTuple> for RemoteProxyCallRow {
    fn from(row: RemoteProxyCallTuple) -> Self {
        Self {
            module_name: row.0,
            method: row.1,
            declared_path: row.2,
            remote_path: row.3,
            id: row.4,
            remote_status: row.5,
            duration_ms: row.6,
            success: row.7,
            error_code: row.8,
            request_id: row.9,
            correlation_id: row.10,
            trace_id: row.11,
            span_id: row.12,
            path_params: row.13,
            error_details: row.14,
        }
    }
}

async fn wait_for_story_event(pool: &platform_core::DbPool, source_id: &str) {
    for _ in 0..100 {
        let exists = sqlx::query_scalar::<_, bool>(
            r#"
            select exists (
                select 1
                from platform.story_events
                where source_type = 'http_request'
                    and source_id = $1
            )
            "#,
        )
        .bind(source_id)
        .fetch_one(pool)
        .await
        .expect("story event existence query should succeed");

        if exists {
            return;
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    panic!("story event {source_id} should be projected");
}

#[tokio::test]
async fn remote_module_fixture_is_visible_through_admin_data_api() {
    let _guard = REMOTE_SMOKE_TEST_LOCK.lock().await;
    let base_url = spawn_remote_module(remote_module_example::router()).await;
    let app = app_with_remote_module(base_url.clone()).await;

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
    assert_eq!(remote_module["source_diagnostics"]["kind"], "remote");
    assert_eq!(remote_module["source_diagnostics"]["base_url"], base_url);
    assert_eq!(
        remote_module["source_diagnostics"]["manifest_url"],
        format!("{base_url}/manifest")
    );
    assert_eq!(remote_module["source_diagnostics"]["timeout_ms"], 5_000);
    assert!(
        remote_module["source_diagnostics"]["load_duration_ms"]
            .as_u64()
            .is_some()
    );
    assert_eq!(
        remote_module["source_diagnostics"]["auth_configured"],
        false
    );
    assert!(
        remote_module["source_diagnostics"]["last_checked_at"]
            .as_str()
            .is_some()
    );
    assert_eq!(
        remote_module["source_diagnostics"]["last_load_error"],
        Value::Null
    );
    assert_eq!(remote_module["http_routes"][0]["method"], "GET");
    assert_eq!(remote_module["http_routes"][0]["path"], "/contacts");
    assert_eq!(
        remote_module["http_routes"][0]["capability"],
        "remote_crm.contacts.read"
    );
    assert_eq!(
        remote_module["http_routes"][0]["display_name"],
        "List Contacts"
    );
    assert_eq!(
        remote_module["http_routes"][0]["story_title"],
        "List Contacts"
    );
    assert_eq!(
        remote_module["runtime"]["functions"][0]["name"],
        "remote_crm.sync_contact.v1"
    );
    assert_eq!(remote_module["runtime"]["functions"][0]["version"], 1);
    assert_eq!(
        remote_module["runtime"]["functions"][0]["queue"],
        "remote-crm"
    );
    assert_eq!(
        remote_module["runtime"]["functions"][0]["input_schema"],
        "remote_crm.sync_contact.v1"
    );
    assert_eq!(
        remote_module["runtime"]["functions"][0]["retry_policy"]["max_attempts"],
        3
    );
    assert_eq!(
        remote_module["lifecycle"]["startup_checks"][0]["kind"],
        "function_registered"
    );
    assert_eq!(
        remote_module["lifecycle"]["startup_checks"][0]["function_name"],
        "remote_crm.sync_contact.v1"
    );
    assert_eq!(
        remote_module["lifecycle"]["startup_checks"][0]["required"],
        true
    );
    assert_eq!(
        remote_module["lifecycle"]["activation_jobs"][0]["name"],
        "sync contacts on startup"
    );
    assert_eq!(
        remote_module["lifecycle"]["activation_jobs"][0]["function_name"],
        "remote_crm.sync_contact.v1"
    );
    assert_eq!(
        remote_module["lifecycle"]["activation_jobs"][0]["run_policy"],
        "every_startup"
    );
    assert_eq!(
        remote_module["lifecycle"]["activation_jobs"][0]["input"]["reason"],
        "worker_startup"
    );
    assert_eq!(
        remote_module["lifecycle"]["activation_jobs"][0]["required"],
        true
    );
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
async fn remote_runtime_function_runs_include_module_declaration() {
    let _guard = REMOTE_SMOKE_TEST_LOCK.lock().await;
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let base_url = spawn_remote_module(remote_module_example::router()).await;
    let app = app_with_remote_module_and_test_db(base_url, &db).await;
    insert_remote_function_run(&db.pool).await;

    let response = app
        .oneshot(admin_get(
            "/admin/runtime/functions?function_name=remote_crm.sync_contact.v1",
        ))
        .await
        .expect("function runs request completes");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"].as_array().expect("function runs").len(), 1);
    assert_eq!(
        body["data"][0]["function_name"],
        "remote_crm.sync_contact.v1"
    );
    let declaration = &body["data"][0]["runtime_declaration"];
    assert_eq!(declaration["module_name"], "remote-crm");
    assert_eq!(declaration["module_source"], "remote");
    assert_eq!(declaration["name"], "remote_crm.sync_contact.v1");
    assert_eq!(declaration["version"], 1);
    assert_eq!(declaration["queue"], "remote-crm");
    assert_eq!(declaration["input_schema"], "remote_crm.sync_contact.v1");
    assert_eq!(declaration["retry_policy"]["max_attempts"], 3);
    assert_eq!(declaration["retry_policy"]["initial_delay_ms"], 1000);

    db.cleanup().await;
}

#[tokio::test]
async fn remote_http_proxy_forwards_declared_get_routes() {
    let _guard = REMOTE_SMOKE_TEST_LOCK.lock().await;
    let base_url = spawn_remote_module(remote_module_example::router()).await;
    let app = app_with_remote_module(base_url).await;

    let unauthenticated = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/modules/remote-crm/http/contacts/contact_1")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("unauthenticated request completes");
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

    let missing_capability = app
        .clone()
        .oneshot(service_get(
            "/modules/remote-crm/http/contacts/contact_1",
            "dev-service:admin",
        ))
        .await
        .expect("missing capability request completes");
    assert_eq!(missing_capability.status(), StatusCode::FORBIDDEN);

    let matched_response = app
        .clone()
        .oneshot(service_get(
            "/modules/remote-crm/http/contacts/contact_1",
            "dev-service:admin:remote_crm.contacts.read",
        ))
        .await
        .expect("matched request completes");
    assert_eq!(matched_response.status(), StatusCode::OK);
    let matched = json_body(matched_response).await;
    assert_eq!(matched["status"], "forwarded");
    assert_eq!(matched["module_name"], "remote-crm");
    assert_eq!(matched["method"], "GET");
    assert_eq!(matched["declared_path"], "/contacts/{id}");
    assert_eq!(matched["remote_path"], "/contacts/contact_1");
    assert_eq!(matched["capability"], "remote_crm.contacts.read");
    assert_eq!(matched["path_params"]["id"], "contact_1");
    assert_eq!(matched["data"]["id"], "contact_1");
    assert_eq!(matched["data"]["email"], "ada@example.com");

    let missing_route = app
        .clone()
        .oneshot(service_get(
            "/modules/remote-crm/http/accounts/account_1",
            "dev-service:admin:remote_crm.contacts.read",
        ))
        .await
        .expect("missing route request completes");
    assert_eq!(missing_route.status(), StatusCode::NOT_FOUND);

    let remote_missing = app
        .oneshot(service_get(
            "/modules/remote-crm/http/contacts/contact_404",
            "dev-service:admin:remote_crm.contacts.read",
        ))
        .await
        .expect("remote missing request completes");
    assert_eq!(remote_missing.status(), StatusCode::NOT_FOUND);
    let remote_missing = json_body(remote_missing).await;
    assert_eq!(remote_missing["error"]["code"], "not_found");
    assert_eq!(
        remote_missing["error"]["message"],
        "contact contact_404 was not found"
    );
    assert_eq!(
        error_detail_reason(&remote_missing, "remote_module"),
        Some("remote-crm")
    );
    assert_eq!(
        error_detail_reason(&remote_missing, "remote_method"),
        Some("GET")
    );
    assert_eq!(
        error_detail_reason(&remote_missing, "declared_path"),
        Some("/contacts/{id}")
    );
    assert_eq!(
        error_detail_reason(&remote_missing, "remote_path"),
        Some("/contacts/contact_404")
    );
    assert_eq!(
        error_detail_reason(&remote_missing, "remote_status"),
        Some("404")
    );
}

#[tokio::test]
async fn installed_remote_module_runs_through_api_proxy_and_worker_runtime() {
    let _guard = REMOTE_SMOKE_TEST_LOCK.lock().await;
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let migrations = PLATFORM_MIGRATIONS
        .iter()
        .chain(RUNTIME_MIGRATIONS)
        .copied()
        .collect::<Vec<_>>();
    apply_migrations(&db.pool, &migrations)
        .await
        .expect("platform and runtime migrations apply");

    let base_url = spawn_remote_module(remote_module_example::router()).await;
    let remote_sources = vec![RemoteModuleSourceConfig {
        name: "remote-crm".to_owned(),
        base_url: base_url.clone(),
        auth_token_env: None,
        timeout_ms: 5_000,
    }];
    let ctx = app_context_with_remote_modules_and_db(
        remote_sources.clone(),
        db.pool.clone(),
        db.url.clone(),
    );
    let modules = app_bootstrap::load_modules(&ctx)
        .await
        .expect("installed remote module loads");
    let registry = app_bootstrap::function_registry(&modules);
    assert!(registry.get("remote_crm.sync_contact.v1").is_some());

    let app = app_with_remote_modules_and_db(remote_sources, db.pool.clone(), db.url.clone()).await;
    let proxy_response = app
        .oneshot(service_get(
            "/modules/remote-crm/http/contacts/contact_1",
            "dev-service:admin:remote_crm.contacts.read",
        ))
        .await
        .expect("remote proxy request completes");
    assert_eq!(proxy_response.status(), StatusCode::OK);
    let proxied = json_body(proxy_response).await;
    assert_eq!(proxied["data"]["email"], "ada@example.com");
    assert_eq!(proxied["remote_path"], "/contacts/contact_1");

    RuntimeClient::new(db.pool.clone())
        .enqueue_function(EnqueueFunctionRequest {
            function_name: "remote_crm.sync_contact.v1".to_owned(),
            input_json: serde_json::json!({ "contact_id": "contact_1" }),
            correlation_id: CorrelationId::new("corr_install_to_run"),
            actor: ActorContext::Service {
                service_id: "worker".to_owned(),
                scopes: vec!["runtime.functions.enqueue".to_owned()],
            },
            trace: TraceContext {
                trace_id: Some("trace_install_to_run".to_owned()),
                span_id: Some("span_install_to_run".to_owned()),
                baggage: Vec::new(),
            },
            causation_id: Some("remote_module_install".to_owned()),
            max_attempts: Some(3),
        })
        .await
        .expect("remote runtime function should enqueue");

    let worker = RuntimeWorker::new(db.pool.clone(), Arc::new(registry), "worker-install-run");
    assert_eq!(
        worker
            .claim_and_run_batch(10)
            .await
            .expect("worker should run remote function"),
        1
    );
    let status: String =
        sqlx::query_scalar("select status from runtime.function_runs where function_name = $1")
            .bind("remote_crm.sync_contact.v1")
            .fetch_one(&db.pool)
            .await
            .expect("function run status should query");
    assert_eq!(status, "completed");
    let remote_runtime_operation: Value = sqlx::query_scalar(
        r#"
        select log.attributes
        from platform.execution_logs log
        where log.execution_name = $1
            and log.attributes ->> 'source' = 'remote_runtime'
        order by log.occurred_at asc
        limit 1
        "#,
    )
    .bind("remote_crm.sync_contact.v1")
    .fetch_one(&db.pool)
    .await
    .expect("remote runtime operation should query");
    assert_eq!(remote_runtime_operation["module_name"], "remote-crm");
    assert_eq!(
        remote_runtime_operation["remote_path"],
        "/runtime/functions/remote_crm.sync_contact.v1/invoke"
    );
    assert_eq!(remote_runtime_operation["success"], true);

    db.cleanup().await;
}

#[tokio::test]
async fn remote_http_proxy_persists_call_history_and_story_operations() {
    let _guard = REMOTE_SMOKE_TEST_LOCK.lock().await;
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let base_url = spawn_remote_module(remote_module_example::router()).await;
    let app = app_with_remote_module_and_test_db(base_url, &db).await;

    let success_response = app
        .clone()
        .oneshot(
            service_get(
                "/modules/remote-crm/http/contacts/contact_1",
                "dev-service:admin:remote_crm.contacts.read",
            )
            .with_header("x-request-id", "req_proxy_success")
            .with_header("x-correlation-id", "corr_proxy_history")
            .with_header(
                "traceparent",
                "00-00000000000000000000000000000021-0000000000000021-01",
            ),
        )
        .await
        .expect("success proxy request completes");
    assert_eq!(success_response.status(), StatusCode::OK);

    let failure_response = app
        .clone()
        .oneshot(
            service_get(
                "/modules/remote-crm/http/proxy-fixtures/text",
                "dev-service:admin:remote_crm.contacts.read",
            )
            .with_header("x-request-id", "req_proxy_failure")
            .with_header("x-correlation-id", "corr_proxy_history")
            .with_header(
                "traceparent",
                "00-00000000000000000000000000000022-0000000000000022-01",
            ),
        )
        .await
        .expect("failure proxy request completes");
    assert_eq!(failure_response.status(), StatusCode::BAD_GATEWAY);
    wait_for_story_event(&db.pool, "req_proxy_success").await;
    wait_for_story_event(&db.pool, "req_proxy_failure").await;

    let rows = sqlx::query_as::<_, RemoteProxyCallTuple>(
        r#"
        select
            module_name,
            method,
            declared_path,
            remote_path,
            id,
            remote_status,
            duration_ms,
            success,
            error_code,
            request_id,
            correlation_id,
            trace_id,
            span_id,
            path_params,
            error_details
        from platform.remote_http_proxy_calls
        where correlation_id = $1
        order by occurred_at, id
        "#,
    )
    .bind("corr_proxy_history")
    .fetch_all(&db.pool)
    .await
    .expect("proxy call history should query")
    .into_iter()
    .map(RemoteProxyCallRow::from)
    .collect::<Vec<_>>();

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].module_name, "remote-crm");
    assert_eq!(rows[0].method, "GET");
    assert_eq!(rows[0].declared_path, "/contacts/{id}");
    assert_eq!(rows[0].remote_path, "/contacts/contact_1");
    assert_eq!(rows[0].remote_status, Some(200));
    assert!(rows[0].success);
    assert_eq!(rows[0].error_code, None);
    assert_eq!(rows[0].request_id, "req_proxy_success");
    assert_eq!(rows[0].correlation_id, "corr_proxy_history");
    assert_eq!(
        rows[0].trace_id.as_deref(),
        Some("00000000000000000000000000000021")
    );
    assert_eq!(rows[0].span_id.as_deref(), Some("0000000000000021"));
    assert_eq!(rows[0].path_params["id"], "contact_1");
    assert_eq!(
        rows[0]
            .error_details
            .as_array()
            .expect("error details array"),
        &Vec::<Value>::new()
    );

    assert_eq!(rows[1].module_name, "remote-crm");
    assert_eq!(rows[1].method, "GET");
    assert_eq!(rows[1].declared_path, "/proxy-fixtures/text");
    assert_eq!(rows[1].remote_path, "/proxy-fixtures/text");
    assert_eq!(rows[1].remote_status, Some(200));
    assert!(!rows[1].success);
    assert_eq!(
        rows[1].error_code.as_deref(),
        Some("external_dependency_failure")
    );
    assert_eq!(rows[1].request_id, "req_proxy_failure");
    assert_eq!(rows[1].correlation_id, "corr_proxy_history");
    assert_eq!(
        rows[1].trace_id.as_deref(),
        Some("00000000000000000000000000000022")
    );
    assert_eq!(rows[1].span_id.as_deref(), Some("0000000000000022"));
    assert!(
        rows[1]
            .error_details
            .as_array()
            .expect("error details array")
            .iter()
            .any(|detail| detail["field"] == "remote_module" && detail["reason"] == "remote-crm")
    );

    let story_detail_response = app
        .clone()
        .oneshot(
            admin_get("/admin/runtime/stories/corr_proxy_history")
                .with_header("x-request-id", "req_admin_story_detail"),
        )
        .await
        .expect("story detail request completes");
    let story_detail_status = story_detail_response.status();
    let story_detail = json_body(story_detail_response).await;
    assert_eq!(
        story_detail_status,
        StatusCode::OK,
        "story detail body: {story_detail}"
    );
    assert_eq!(story_detail["data"]["summary"]["title"], "Fetch Contact");
    let story_nodes = story_detail["data"]["nodes"]
        .as_array()
        .expect("story nodes array");
    let story_root = story_nodes
        .iter()
        .find(|node| node["id"] == "httpreq_req_proxy_success")
        .expect("successful remote proxy request should create a story root");
    assert_eq!(
        story_root["name"],
        "GET /modules/remote-crm/http/contacts/contact_1"
    );
    assert_eq!(story_root["type"], "http_request");
    assert_eq!(story_root["status"], "completed");
    assert_eq!(
        story_root["metadata"]["source_metadata"]["request_id"],
        "req_proxy_success"
    );
    assert_eq!(
        story_root["metadata"]["source_metadata"]["path"],
        "/modules/remote-crm/http/contacts/contact_1"
    );
    let remote_success_node_id = format!("remoteproxy_{}", rows[0].id);
    let remote_success_node = story_nodes
        .iter()
        .find(|node| node["id"] == remote_success_node_id)
        .expect("successful remote proxy call should create a story node");
    assert_eq!(remote_success_node["type"], "remote_proxy_call");
    assert_eq!(remote_success_node["name"], "Fetch Contact");
    assert_eq!(remote_success_node["status"], "completed");
    assert_eq!(remote_success_node["service"], "remote-crm");
    assert_eq!(remote_success_node["display_name"], "Fetch Contact");
    assert_eq!(
        remote_success_node["metadata"]["source_metadata"]["request_id"],
        "req_proxy_success"
    );
    assert_eq!(
        remote_success_node["metadata"]["source_metadata"]["remote_proxy_call_id"],
        rows[0].id
    );
    assert_eq!(
        remote_success_node["metadata"]["source_metadata"]["module_name"],
        "remote-crm"
    );
    assert_eq!(
        remote_success_node["metadata"]["source_metadata"]["method"],
        "GET"
    );
    assert_eq!(
        remote_success_node["metadata"]["source_metadata"]["declared_path"],
        "/contacts/{id}"
    );
    assert_eq!(
        remote_success_node["metadata"]["source_metadata"]["remote_path"],
        "/contacts/contact_1"
    );
    assert_eq!(
        remote_success_node["metadata"]["source_metadata"]["duration_ms"],
        rows[0].duration_ms
    );
    assert_eq!(
        remote_success_node["metadata"]["source_metadata"]["trace_id"],
        "00000000000000000000000000000021"
    );
    assert_eq!(
        remote_success_node["metadata"]["source_metadata"]["span_id"],
        "0000000000000021"
    );
    assert_eq!(
        remote_success_node["metadata"]["source_metadata"]["story_title"],
        "Fetch Contact"
    );
    assert_eq!(
        remote_success_node["metadata"]["source_metadata"]["remote_status"],
        200
    );
    assert_eq!(
        remote_success_node["metadata"]["source_metadata"]["path_params"]["id"],
        "contact_1"
    );
    assert_eq!(
        remote_success_node["metadata"]["source_metadata"]["error_details"],
        Value::Array(vec![])
    );

    let remote_failure_node_id = format!("remoteproxy_{}", rows[1].id);
    let remote_failure_node = story_nodes
        .iter()
        .find(|node| node["id"] == remote_failure_node_id)
        .expect("failed remote proxy call should create a story node");
    assert_eq!(remote_failure_node["type"], "remote_proxy_call");
    assert_eq!(remote_failure_node["name"], "Fetch Text Fixture");
    assert_eq!(remote_failure_node["status"], "failed");
    assert_eq!(
        remote_failure_node["metadata"]["source_metadata"]["error_code"],
        "external_dependency_failure"
    );
    assert_eq!(
        remote_failure_node["metadata"]["source_metadata"]["module_name"],
        "remote-crm"
    );
    assert_eq!(
        remote_failure_node["metadata"]["source_metadata"]["method"],
        "GET"
    );
    assert_eq!(
        remote_failure_node["metadata"]["source_metadata"]["declared_path"],
        "/proxy-fixtures/text"
    );
    assert_eq!(
        remote_failure_node["metadata"]["source_metadata"]["remote_path"],
        "/proxy-fixtures/text"
    );
    assert_eq!(
        remote_failure_node["metadata"]["source_metadata"]["remote_status"],
        200
    );
    assert_eq!(
        remote_failure_node["metadata"]["source_metadata"]["duration_ms"],
        rows[1].duration_ms
    );
    assert_eq!(
        remote_failure_node["metadata"]["source_metadata"]["request_id"],
        "req_proxy_failure"
    );
    assert_eq!(
        remote_failure_node["metadata"]["source_metadata"]["trace_id"],
        "00000000000000000000000000000022"
    );
    assert_eq!(
        remote_failure_node["metadata"]["source_metadata"]["span_id"],
        "0000000000000022"
    );
    assert!(
        remote_failure_node["metadata"]["source_metadata"]["error_details"]
            .as_array()
            .expect("story node error details array")
            .iter()
            .any(|detail| detail["field"] == "remote_module" && detail["reason"] == "remote-crm")
    );

    let story_edges = story_detail["data"]["edges"]
        .as_array()
        .expect("story edges array");
    assert!(story_edges.iter().any(|edge| {
        edge["source"] == "httpreq_req_proxy_success" && edge["target"] == remote_success_node_id
    }));
    assert!(story_edges.iter().any(|edge| {
        edge["source"] == "httpreq_req_proxy_failure" && edge["target"] == remote_failure_node_id
    }));
    let story_timeline = story_detail["data"]["timeline_items"]
        .as_array()
        .expect("story timeline array");
    assert!(story_timeline.iter().any(|item| {
        item["id"] == remote_success_node_id && item["type"] == "remote_proxy_call"
    }));

    let story_ops_response = app
        .oneshot(
            admin_get("/admin/runtime/stories/corr_proxy_history/technical-operations")
                .with_header("x-request-id", "req_admin_story_ops"),
        )
        .await
        .expect("story technical operations request completes");
    let story_ops_status = story_ops_response.status();
    let story_ops = json_body(story_ops_response).await;
    assert_eq!(
        story_ops_status,
        StatusCode::OK,
        "story technical operations body: {story_ops}"
    );
    let operations = story_ops["data"].as_array().expect("operations array");
    let remote_success = operations
        .iter()
        .find(|operation| {
            operation["source"] == "remote_proxy"
                && operation["attributes"]["request_id"] == "req_proxy_success"
        })
        .expect("successful remote proxy operation should be present");
    assert_eq!(remote_success["story_id"], "corr_proxy_history");
    assert_eq!(remote_success["correlation_id"], "corr_proxy_history");
    assert_eq!(remote_success["category"], "external");
    assert_eq!(remote_success["related_node_id"], remote_success_node_id);
    assert_eq!(remote_success["status"], "ok");
    assert_eq!(remote_success["attributes"]["module_name"], "remote-crm");
    assert_eq!(
        remote_success["attributes"]["trace_id"],
        "00000000000000000000000000000021"
    );
    assert_eq!(remote_success["attributes"]["span_id"], "0000000000000021");

    let remote_failure = operations
        .iter()
        .find(|operation| {
            operation["source"] == "remote_proxy"
                && operation["attributes"]["request_id"] == "req_proxy_failure"
        })
        .expect("failed remote proxy operation should be present");
    assert_eq!(remote_failure["story_id"], "corr_proxy_history");
    assert_eq!(remote_failure["correlation_id"], "corr_proxy_history");
    assert_eq!(remote_failure["category"], "external");
    assert_eq!(remote_failure["related_node_id"], remote_failure_node_id);
    assert_eq!(remote_failure["status"], "error");
    assert_eq!(
        remote_failure["attributes"]["error_code"],
        "external_dependency_failure"
    );
    assert_eq!(
        remote_failure["attributes"]["trace_id"],
        "00000000000000000000000000000022"
    );
    assert_eq!(remote_failure["attributes"]["span_id"], "0000000000000022");

    db.cleanup().await;
}

#[tokio::test]
async fn remote_http_proxy_rejects_unsafe_get_responses() {
    let _guard = REMOTE_SMOKE_TEST_LOCK.lock().await;
    let base_url = spawn_remote_module(remote_module_example::router()).await;
    let app = app_with_remote_module(base_url).await;

    let text_response = app
        .clone()
        .oneshot(service_get(
            "/modules/remote-crm/http/proxy-fixtures/text",
            "dev-service:admin:remote_crm.contacts.read",
        ))
        .await
        .expect("text fixture request completes");
    assert_eq!(text_response.status(), StatusCode::BAD_GATEWAY);
    let text_error = json_body(text_response).await;
    assert_eq!(text_error["error"]["code"], "external_dependency_failure");
    assert!(
        text_error["error"]["message"]
            .as_str()
            .expect("error message")
            .contains("content-type was not JSON")
    );
    assert_eq!(
        error_detail_reason(&text_error, "remote_module"),
        Some("remote-crm")
    );
    assert_eq!(
        error_detail_reason(&text_error, "remote_method"),
        Some("GET")
    );
    assert_eq!(
        error_detail_reason(&text_error, "declared_path"),
        Some("/proxy-fixtures/text")
    );
    assert_eq!(
        error_detail_reason(&text_error, "remote_path"),
        Some("/proxy-fixtures/text")
    );
    assert_eq!(
        error_detail_reason(&text_error, "remote_status"),
        Some("200")
    );

    let oversized_response = app
        .oneshot(service_get(
            "/modules/remote-crm/http/proxy-fixtures/oversized",
            "dev-service:admin:remote_crm.contacts.read",
        ))
        .await
        .expect("oversized fixture request completes");
    assert_eq!(oversized_response.status(), StatusCode::BAD_GATEWAY);
    let oversized_error = json_body(oversized_response).await;
    assert_eq!(
        oversized_error["error"]["code"],
        "external_dependency_failure"
    );
    assert!(
        oversized_error["error"]["message"]
            .as_str()
            .expect("error message")
            .contains("response body exceeded")
    );
}

#[tokio::test]
async fn remote_http_proxy_uses_configured_remote_timeout() {
    let _guard = REMOTE_SMOKE_TEST_LOCK.lock().await;
    let base_url = spawn_remote_module(remote_module_example::router()).await;
    let app = app_with_remote_modules(vec![RemoteModuleSourceConfig {
        name: "remote-crm".to_owned(),
        base_url,
        auth_token_env: None,
        timeout_ms: 50,
    }])
    .await;

    let response = app
        .oneshot(service_get(
            "/modules/remote-crm/http/proxy-fixtures/slow",
            "dev-service:admin:remote_crm.contacts.read",
        ))
        .await
        .expect("slow fixture request completes");
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let body = json_body(response).await;
    assert_eq!(body["error"]["code"], "external_dependency_failure");
    assert!(
        body["error"]["message"]
            .as_str()
            .expect("error message")
            .contains("remote HTTP proxy request failed")
    );
    assert_eq!(
        error_detail_reason(&body, "remote_module"),
        Some("remote-crm")
    );
    assert_eq!(error_detail_reason(&body, "remote_method"), Some("GET"));
    assert_eq!(
        error_detail_reason(&body, "declared_path"),
        Some("/proxy-fixtures/slow")
    );
    assert_eq!(
        error_detail_reason(&body, "remote_path"),
        Some("/proxy-fixtures/slow")
    );
    assert_eq!(error_detail_reason(&body, "remote_status"), None);
}

#[tokio::test]
async fn remote_http_proxy_forwards_declared_post_routes() {
    let _guard = REMOTE_SMOKE_TEST_LOCK.lock().await;
    let base_url = spawn_remote_module(remote_module_example::router()).await;
    let app = app_with_remote_module(base_url).await;

    let matched_response = app
        .clone()
        .oneshot(service_post(
            "/modules/remote-crm/http/contacts",
            "dev-service:admin:remote_crm.contacts.read",
            "application/json",
            r#"{"id":"contact_new","email":"new@example.com"}"#,
        ))
        .await
        .expect("matched post request completes");
    assert_eq!(matched_response.status(), StatusCode::OK);
    let matched = json_body(matched_response).await;
    assert_eq!(matched["status"], "forwarded");
    assert_eq!(matched["module_name"], "remote-crm");
    assert_eq!(matched["method"], "POST");
    assert_eq!(matched["declared_path"], "/contacts");
    assert_eq!(matched["remote_path"], "/contacts");
    assert_eq!(matched["capability"], "remote_crm.contacts.read");
    assert_eq!(matched["data"]["id"], "contact_new");
    assert_eq!(matched["data"]["email"], "new@example.com");
    assert_eq!(matched["data"]["operation"], "created");
    assert_eq!(matched["data"]["input"]["email"], "new@example.com");

    let missing_route = app
        .clone()
        .oneshot(service_post(
            "/modules/remote-crm/http/contacts/contact_1",
            "dev-service:admin:remote_crm.contacts.read",
            "application/json",
            r#"{"email":"new@example.com"}"#,
        ))
        .await
        .expect("missing route request completes");
    assert_eq!(missing_route.status(), StatusCode::NOT_FOUND);

    let non_json = app
        .clone()
        .oneshot(service_post(
            "/modules/remote-crm/http/contacts",
            "dev-service:admin:remote_crm.contacts.read",
            "text/plain",
            "not json",
        ))
        .await
        .expect("non-json request completes");
    assert_eq!(non_json.status(), StatusCode::BAD_REQUEST);
    let non_json_body = json_body(non_json).await;
    assert_eq!(non_json_body["error"]["code"], "validation_failed");
    assert!(
        non_json_body["error"]["message"]
            .as_str()
            .expect("error message")
            .contains("request content-type was not JSON")
    );

    let oversized = app
        .oneshot(service_post(
            "/modules/remote-crm/http/contacts",
            "dev-service:admin:remote_crm.contacts.read",
            "application/json",
            format!(r#"{{"payload":"{}"}}"#, "x".repeat((1024 * 1024) + 1)),
        ))
        .await
        .expect("oversized request completes");
    assert_eq!(oversized.status(), StatusCode::BAD_REQUEST);
    let oversized_body = json_body(oversized).await;
    assert_eq!(oversized_body["error"]["code"], "validation_failed");
    assert!(
        oversized_body["error"]["message"]
            .as_str()
            .expect("error message")
            .contains("request body exceeded")
    );
}

#[tokio::test]
async fn remote_http_proxy_forwards_declared_put_and_patch_routes() {
    let _guard = REMOTE_SMOKE_TEST_LOCK.lock().await;
    let base_url = spawn_remote_module(remote_module_example::router()).await;
    let app = app_with_remote_module(base_url).await;

    let put_response = app
        .clone()
        .oneshot(service_json_method(
            "PUT",
            "/modules/remote-crm/http/contacts/contact_1",
            "dev-service:admin:remote_crm.contacts.read",
            "application/json",
            r#"{"email":"updated@example.com"}"#,
        ))
        .await
        .expect("put request completes");
    assert_eq!(put_response.status(), StatusCode::OK);
    let put = json_body(put_response).await;
    assert_eq!(put["status"], "forwarded");
    assert_eq!(put["method"], "PUT");
    assert_eq!(put["declared_path"], "/contacts/{id}");
    assert_eq!(put["remote_path"], "/contacts/contact_1");
    assert_eq!(put["path_params"]["id"], "contact_1");
    assert_eq!(put["data"]["id"], "contact_1");
    assert_eq!(put["data"]["email"], "updated@example.com");
    assert_eq!(put["data"]["operation"], "replaced");

    let patch_response = app
        .clone()
        .oneshot(service_json_method(
            "PATCH",
            "/modules/remote-crm/http/contacts/contact_2",
            "dev-service:admin:remote_crm.contacts.read",
            "application/json",
            r#"{"email":"patched@example.com"}"#,
        ))
        .await
        .expect("patch request completes");
    assert_eq!(patch_response.status(), StatusCode::OK);
    let patch = json_body(patch_response).await;
    assert_eq!(patch["status"], "forwarded");
    assert_eq!(patch["method"], "PATCH");
    assert_eq!(patch["declared_path"], "/contacts/{id}");
    assert_eq!(patch["remote_path"], "/contacts/contact_2");
    assert_eq!(patch["path_params"]["id"], "contact_2");
    assert_eq!(patch["data"]["id"], "contact_2");
    assert_eq!(patch["data"]["email"], "patched@example.com");
    assert_eq!(patch["data"]["operation"], "patched");

    let non_json = app
        .clone()
        .oneshot(service_json_method(
            "PUT",
            "/modules/remote-crm/http/contacts/contact_1",
            "dev-service:admin:remote_crm.contacts.read",
            "text/plain",
            "not json",
        ))
        .await
        .expect("non-json put request completes");
    assert_eq!(non_json.status(), StatusCode::BAD_REQUEST);
    let non_json_body = json_body(non_json).await;
    assert_eq!(non_json_body["error"]["code"], "validation_failed");
    assert!(
        non_json_body["error"]["message"]
            .as_str()
            .expect("error message")
            .contains("request content-type was not JSON")
    );
}

#[tokio::test]
async fn remote_http_proxy_forwards_declared_delete_routes() {
    let _guard = REMOTE_SMOKE_TEST_LOCK.lock().await;
    let base_url = spawn_remote_module(remote_module_example::router()).await;
    let app = app_with_remote_module(base_url).await;

    let deleted_response = app
        .clone()
        .oneshot(service_json_method(
            "DELETE",
            "/modules/remote-crm/http/contacts/contact_1",
            "dev-service:admin:remote_crm.contacts.read",
            "application/json",
            Body::empty(),
        ))
        .await
        .expect("delete request completes");
    assert_eq!(deleted_response.status(), StatusCode::OK);
    let deleted = json_body(deleted_response).await;
    assert_eq!(deleted["status"], "forwarded");
    assert_eq!(deleted["method"], "DELETE");
    assert_eq!(deleted["declared_path"], "/contacts/{id}");
    assert_eq!(deleted["remote_path"], "/contacts/contact_1");
    assert_eq!(deleted["path_params"]["id"], "contact_1");
    assert_eq!(deleted["data"]["id"], "contact_1");
    assert_eq!(deleted["data"]["deleted"], true);

    let purged_response = app
        .clone()
        .oneshot(service_json_method(
            "DELETE",
            "/modules/remote-crm/http/contacts/contact_1/purge",
            "dev-service:admin:remote_crm.contacts.read",
            "application/json",
            Body::empty(),
        ))
        .await
        .expect("delete 204 request completes");
    assert_eq!(purged_response.status(), StatusCode::OK);
    let purged = json_body(purged_response).await;
    assert_eq!(purged["status"], "forwarded");
    assert_eq!(purged["method"], "DELETE");
    assert_eq!(purged["declared_path"], "/contacts/{id}/purge");
    assert_eq!(purged["remote_path"], "/contacts/contact_1/purge");
    assert_eq!(purged["path_params"]["id"], "contact_1");
    assert_eq!(purged["data"], Value::Null);

    let body_rejected = app
        .oneshot(service_json_method(
            "DELETE",
            "/modules/remote-crm/http/contacts/contact_1",
            "dev-service:admin:remote_crm.contacts.read",
            "application/json",
            r#"{}"#,
        ))
        .await
        .expect("delete body request completes");
    assert_eq!(body_rejected.status(), StatusCode::BAD_REQUEST);
    let body_rejected = json_body(body_rejected).await;
    assert_eq!(body_rejected["error"]["code"], "validation_failed");
    assert!(
        body_rejected["error"]["message"]
            .as_str()
            .expect("error message")
            .contains("DELETE request body must be empty")
    );
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
        .expect("failed remote module metadata");
    assert_eq!(remote_module["source_diagnostics"]["kind"], "remote");
    assert_eq!(
        remote_module["source_diagnostics"]["base_url"],
        "http://127.0.0.1:9/lenso/module/v1"
    );
    assert_eq!(remote_module["source_diagnostics"]["timeout_ms"], 50);
    assert!(
        remote_module["source_diagnostics"]["load_duration_ms"]
            .as_u64()
            .is_some()
    );
    assert!(
        remote_module["source_diagnostics"]["last_load_error"]
            .as_str()
            .expect("last load error")
            .contains("remote manifest request failed")
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
    assert_eq!(declarative_module["http_routes"][0]["path"], "/contacts");
    assert_eq!(
        declarative_module["http_routes"][0]["display_name"],
        "List Contacts"
    );
    assert_eq!(declarative_module["admin"]["kind"], "declarative_custom");
    assert_eq!(declarative_module["admin"]["pages"][0]["name"], "overview");
    assert_eq!(
        declarative_module["admin"]["pages"][0]["sections"][0]["component"]["kind"],
        "metric_strip"
    );
    assert_eq!(
        declarative_module["admin"]["pages"][0]["sections"][2]["component"]["kind"],
        "entity_detail"
    );
    assert_eq!(
        declarative_module["admin"]["pages"][0]["sections"][2]["component"]["entity"],
        "contacts"
    );
    assert_eq!(
        declarative_module["admin"]["fallback_schema"]["entities"][0]["name"],
        "contacts"
    );
    assert_eq!(
        declarative_module["admin"]["actions"][0]["input_schema"]["fields"][0]["name"],
        "dry_run"
    );
    assert_eq!(
        declarative_module["admin"]["actions"][0]["confirmation"]["required_phrase"],
        "SYNC"
    );
    assert_eq!(
        declarative_module["admin"]["actions"][0]["danger_level"],
        "medium"
    );

    let declarative_list_response = app
        .clone()
        .oneshot(admin_get(
            "/admin/data/remote-crm-declarative/contacts?limit=2",
        ))
        .await
        .expect("declarative list request completes");
    assert_eq!(declarative_list_response.status(), StatusCode::OK);
    let declarative_list = json_body(declarative_list_response).await;
    assert_eq!(declarative_list["data"][0]["id"], "contact_1");
    assert_eq!(declarative_list["data"][0]["email"], "ada@example.com");
    assert_eq!(declarative_list["page"]["next_cursor"], "contact_2");

    let action_response = app
        .clone()
        .oneshot(service_post(
            "/admin/data/remote-crm-declarative/actions/sync_contacts",
            "dev-service:admin:remote_crm.contacts.sync",
            "application/json",
            r#"{"input":{"dry_run":true},"confirmation_phrase":"SYNC"}"#,
        ))
        .await
        .expect("declarative action request completes");
    assert_eq!(action_response.status(), StatusCode::OK);
    let action = json_body(action_response).await;
    assert_eq!(action["data"]["synced"], true);
    assert_eq!(action["data"]["dry_run"], true);
    assert_eq!(action["data"]["contacts"], 3);

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

async fn insert_remote_function_run(pool: &DbPool) {
    sqlx::query(
        r#"
        insert into runtime.function_runs (
            id,
            function_name,
            input_json,
            correlation_id,
            actor
        )
        values (
            'fnrun_remote_sync',
            'remote_crm.sync_contact.v1',
            $1,
            'corr_remote_sync',
            '{"kind":"system"}'::jsonb
        )
        "#,
    )
    .bind(serde_json::json!({ "contact_id": "contact_1" }))
    .execute(pool)
    .await
    .expect("remote runtime function run should insert");
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
