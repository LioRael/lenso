use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use lenso_api::build_router;
use platform_core::{AppConfig, AppContext, DbPool, LoggingEventPublisher};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

fn req(method: &str, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .expect("request builds")
}

trait RequestExt {
    fn with_admin(self) -> Self;
    fn with_scoped_admin(self) -> Self;
}
impl RequestExt for Request<Body> {
    fn with_admin(mut self) -> Self {
        self.headers_mut()
            .insert("authorization", "Bearer dev-service:admin".parse().unwrap());
        self
    }

    fn with_scoped_admin(mut self) -> Self {
        self.headers_mut().insert(
            "authorization",
            "Bearer dev-service:admin:runtime.stories.read,auth.users.read"
                .parse()
                .unwrap(),
        );
        self
    }
}

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body");
    serde_json::from_slice(&bytes).expect("json body")
}

#[tokio::test]
async fn restart_endpoint_signals_shutdown() {
    let db =
        DbPool::connect_lazy("postgres://localhost/lenso_test").expect("lazy pool should build");
    let ctx = AppContext::new(AppConfig::from_env(), db, Arc::new(LoggingEventPublisher));
    let mut shutdown = ctx.shutdown.subscribe();
    let app = build_router(ctx);

    let response = app
        .oneshot(req("POST", "/admin/system/restart").with_admin())
        .await
        .expect("request completes");
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = json_body(response).await;
    assert_eq!(body["status"], "shutdown_requested");
    assert_eq!(body["service"], "api");
    assert_eq!(body["requires_supervisor"], true);

    tokio::time::timeout(Duration::from_secs(1), shutdown.changed())
        .await
        .expect("shutdown should be signaled")
        .expect("shutdown sender remains alive");
    assert!(*shutdown.borrow());
}

#[tokio::test]
async fn context_endpoint_returns_admin_actor_capabilities() {
    let db =
        DbPool::connect_lazy("postgres://localhost/lenso_test").expect("lazy pool should build");
    let ctx = AppContext::new(AppConfig::from_env(), db, Arc::new(LoggingEventPublisher));
    let app = build_router(ctx);

    let response = app
        .oneshot(req("GET", "/admin/context").with_scoped_admin())
        .await
        .expect("request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["actor"]["kind"], "service");
    assert_eq!(body["actor"]["service_id"], "admin");
    assert_eq!(
        body["scopes"],
        serde_json::json!(["runtime.stories.read", "auth.users.read"])
    );
    assert_eq!(
        body["capabilities"],
        serde_json::json!(["runtime.stories.read", "auth.users.read"])
    );
}
