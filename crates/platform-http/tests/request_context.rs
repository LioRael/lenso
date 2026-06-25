use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use platform_core::config::ConsoleConfig;
use platform_core::{
    ActorContext, ActorResolutionRequest, ActorResolver, AppConfig, AppContext, AuthConfig,
    DatabaseConfig, HttpConfig, LoggingEventPublisher, ModuleSourcesConfig, RedisConfig,
    ServiceConfig, TelemetryConfig,
};
use platform_http::{AdminActor, CONSOLE_ADMIN_SCOPE, HttpRequestContext, JsonBody};
use serde::Deserialize;
use serde_json::Value;
use std::net::SocketAddr;
use std::sync::Arc;
use tower::ServiceExt;

#[tokio::test]
async fn missing_headers_generate_request_context_ids() {
    let response = router()
        .oneshot(
            Request::builder()
                .uri("/context")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should complete");

    let request_id = response.headers().get("x-request-id").cloned();
    let correlation_id = response.headers().get("x-correlation-id").cloned();
    let body = json_body(response).await;

    assert!(request_id.is_some());
    assert!(correlation_id.is_some());
    assert!(body["request_id"].as_str().unwrap().starts_with("req_"));
    assert!(
        body["correlation_id"]
            .as_str()
            .unwrap()
            .starts_with("corr_")
    );
}

#[tokio::test]
async fn provided_headers_are_preserved_in_request_context() {
    let response = router()
        .oneshot(
            Request::builder()
                .uri("/context")
                .header("x-request-id", "req-provided")
                .header("x-correlation-id", "corr-provided")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should complete");

    assert_eq!(
        response.headers().get("x-request-id").unwrap(),
        "req-provided"
    );
    assert_eq!(
        response.headers().get("x-correlation-id").unwrap(),
        "corr-provided"
    );

    let body = json_body(response).await;
    assert_eq!(body["request_id"], "req-provided");
    assert_eq!(body["correlation_id"], "corr-provided");
}

#[tokio::test]
async fn client_metadata_is_preserved_in_request_context() {
    let mut request = Request::builder()
        .uri("/context")
        .header("user-agent", "LensoTest/1.0")
        .body(Body::empty())
        .expect("request should build");
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([203, 0, 113, 7], 4242))));

    let response = router()
        .oneshot(request)
        .await
        .expect("request should complete");
    let body = json_body(response).await;

    assert_eq!(body["client"]["ip"], "203.0.113.7");
    assert_eq!(body["client"]["user_agent"], "LensoTest/1.0");
}

#[tokio::test]
async fn dev_user_bearer_token_sets_user_actor_context() {
    let response = router()
        .oneshot(
            Request::builder()
                .uri("/context")
                .header("authorization", "Bearer dev-user:user_123")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should complete");

    let body = json_body(response).await;
    assert_eq!(body["actor"]["kind"], "user");
    assert_eq!(body["actor"]["user_id"], "user_123");
}

#[tokio::test]
async fn dev_service_bearer_token_sets_service_actor_context() {
    let response = router()
        .oneshot(
            Request::builder()
                .uri("/context")
                .header("authorization", "Bearer dev-service:worker")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should complete");

    let body = json_body(response).await;
    assert_eq!(body["actor"]["kind"], "service");
    assert_eq!(body["actor"]["service_id"], "worker");
    assert_eq!(body["actor"]["scopes"], serde_json::json!([]));
}

#[tokio::test]
async fn dev_service_bearer_token_can_set_scopes() {
    let response = router()
        .oneshot(
            Request::builder()
                .uri("/context")
                .header(
                    "authorization",
                    "Bearer dev-service:admin:remote_crm.contacts.read,other.scope",
                )
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should complete");

    let body = json_body(response).await;
    assert_eq!(body["actor"]["kind"], "service");
    assert_eq!(body["actor"]["service_id"], "admin");
    assert_eq!(
        body["actor"]["scopes"],
        serde_json::json!(["remote_crm.contacts.read", "other.scope"])
    );
}

#[tokio::test]
async fn dev_bearer_token_is_ignored_outside_local_environment() {
    let response = router_for_environment("production")
        .oneshot(
            Request::builder()
                .uri("/context")
                .header(
                    "authorization",
                    "Bearer dev-service:admin:remote_crm.contacts.read",
                )
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should complete");

    let body = json_body(response).await;
    assert_eq!(body["actor"]["kind"], "anonymous");
}

#[tokio::test]
async fn custom_actor_resolver_can_set_actor_context() {
    let response = router_with_actor_resolver(Arc::new(StaticActorResolver))
        .oneshot(
            Request::builder()
                .uri("/context")
                .header("authorization", "Bearer real-token")
                .header("cookie", "lenso_session=session_123")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should complete");

    let body = json_body(response).await;
    assert_eq!(body["actor"]["kind"], "user");
    assert_eq!(body["actor"]["user_id"], "user_from_resolver");
    assert_eq!(body["actor"]["scopes"], serde_json::json!(["auth.test"]));
}

#[tokio::test]
async fn admin_actor_accepts_user_with_console_admin_scope() {
    let response = router_with_actor_resolver(Arc::new(FixedActorResolver {
        actor: ActorContext::User {
            user_id: "usr_admin".to_owned(),
            scopes: vec![CONSOLE_ADMIN_SCOPE.to_owned(), "auth.users.read".to_owned()],
        },
    }))
    .oneshot(
        Request::builder()
            .uri("/admin-context")
            .body(Body::empty())
            .expect("request should build"),
    )
    .await
    .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["actor"], "user:usr_admin");
}

#[tokio::test]
async fn admin_actor_rejects_user_without_console_admin_scope() {
    let response = router_with_actor_resolver(Arc::new(FixedActorResolver {
        actor: ActorContext::User {
            user_id: "usr_regular".to_owned(),
            scopes: vec!["auth.users.read".to_owned()],
        },
    }))
    .oneshot(
        Request::builder()
            .uri("/admin-context")
            .body(Body::empty())
            .expect("request should build"),
    )
    .await
    .expect("request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = json_body(response).await;
    assert_eq!(body["error"]["message"], "Console admin scope is required");
}

#[tokio::test]
async fn malformed_json_returns_standard_error_shape_with_request_context() {
    let response = router()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/json")
                .header("content-type", "application/json")
                .header("x-request-id", "req-json")
                .header("x-correlation-id", "corr-json")
                .body(Body::from(r#"{"name":"Ada""#))
                .expect("request should build"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = json_body(response).await;
    assert_eq!(body["error"]["code"], "validation_failed");
    assert_eq!(body["error"]["message"], "Request validation failed");
    assert_eq!(body["error"]["request_id"], "req-json");
    assert_eq!(body["error"]["correlation_id"], "corr-json");
    assert!(!body["error"]["details"].as_array().unwrap().is_empty());
}

fn router() -> Router {
    router_for_environment("local")
}

fn router_for_environment(environment: &str) -> Router {
    router_for_environment_with_actor_resolver(environment, None)
}

fn router_with_actor_resolver(actor_resolver: Arc<dyn ActorResolver>) -> Router {
    router_for_environment_with_actor_resolver("production", Some(actor_resolver))
}

fn router_for_environment_with_actor_resolver(
    environment: &str,
    actor_resolver: Option<Arc<dyn ActorResolver>>,
) -> Router {
    let config = AppConfig {
        service: ServiceConfig {
            name: "lenso-test".to_owned(),
            environment: environment.to_owned(),
        },
        database: DatabaseConfig {
            url: "postgres://localhost/lenso_test".to_owned(),
            max_connections: 1,
        },
        redis: RedisConfig::default(),
        http: HttpConfig {
            host: "127.0.0.1".to_owned(),
            port: 0,
            cors_allowed_origins: Vec::new(),
        },
        telemetry: TelemetryConfig::default(),
        auth: AuthConfig::default(),
        console: ConsoleConfig::default(),
        module_sources: ModuleSourcesConfig::default(),
        modules: Default::default(),
    };
    let mut ctx = AppContext::new(
        config,
        platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy db pool should construct"),
        Arc::new(LoggingEventPublisher),
    );
    if let Some(actor_resolver) = actor_resolver {
        ctx = ctx.with_actor_resolver(actor_resolver);
    }

    Router::new()
        .route("/context", get(context_handler))
        .route("/admin-context", get(admin_context_handler))
        .route("/json", post(json_handler))
        .layer(middleware::from_fn_with_state(
            ctx,
            platform_http::request_context_middleware,
        ))
}

async fn context_handler(HttpRequestContext(ctx): HttpRequestContext) -> impl IntoResponse {
    axum::Json(serde_json::json!({
        "request_id": ctx.request_id.0,
        "correlation_id": ctx.correlation_id.0,
        "actor": ctx.actor,
        "client": ctx.client,
    }))
}

async fn admin_context_handler(admin: AdminActor) -> impl IntoResponse {
    let actor = match admin {
        AdminActor::Service { service_id, .. } => format!("service:{service_id}"),
        AdminActor::User { user_id, .. } => format!("user:{user_id}"),
        AdminActor::System => "system".to_owned(),
    };
    axum::Json(serde_json::json!({ "actor": actor }))
}

async fn json_handler(JsonBody(input): JsonBody<JsonInput>) -> impl IntoResponse {
    axum::Json(serde_json::json!({ "name": input.name }))
}

#[derive(Debug, Deserialize)]
struct JsonInput {
    name: String,
}

#[derive(Debug)]
struct StaticActorResolver;

#[async_trait::async_trait]
impl ActorResolver for StaticActorResolver {
    async fn resolve_actor(&self, request: ActorResolutionRequest) -> ActorContext {
        assert_eq!(request.authorization.as_deref(), Some("Bearer real-token"));
        assert_eq!(request.cookie.as_deref(), Some("lenso_session=session_123"));
        ActorContext::User {
            user_id: "user_from_resolver".to_owned(),
            scopes: vec!["auth.test".to_owned()],
        }
    }
}

#[derive(Debug)]
struct FixedActorResolver {
    actor: ActorContext,
}

#[async_trait::async_trait]
impl ActorResolver for FixedActorResolver {
    async fn resolve_actor(&self, _request: ActorResolutionRequest) -> ActorContext {
        self.actor.clone()
    }
}

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body should read");
    serde_json::from_slice(&bytes).expect("body should be json")
}
