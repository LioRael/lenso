use std::{cell::Cell, io::ErrorKind, net::SocketAddr, rc::Rc, time::Duration};

use bytes::Bytes;
use futures::future::LocalBoxFuture;
use http::{HeaderValue, Request, Response};
use lenso_kernel::RuntimeFailure;
use lenso_native_adapter::NativePluginDefinition;
use lenso_web_greetings_plugin_example::GreetingsHttp;
use lenso_web_host::{
    NativeWebHost, TowerMiddlewareOutcome, WebHostError, WebIngressDiagnostics,
    WebIngressEndpointFailure, WebIngressEventBody,
};
use lenso_web_ingress_plugin::{
    WebIngressConfig, WebIngressMiddleware, WebIngressMiddlewareOutcome, WebIngressRequest,
    WebIngressResponse,
};
use lenso_web_query_endpoint_fixture::OrderSearchHttp;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    task::LocalSet,
};

async fn post_greeting(address: SocketAddr) -> String {
    let mut stream = TcpStream::connect(address).await.unwrap();
    stream
        .write_all(
            b"POST /greetings HTTP/1.1\r\n\
Host: localhost\r\n\
content-type: application/json\r\n\
content-length: 16\r\n\
connection: close\r\n\
\r\n\
{\"name\":\"Lenso\"}",
        )
        .await
        .unwrap();
    let mut body = Vec::new();
    match stream.read_to_end(&mut body).await {
        Ok(_) => {}
        Err(error) if error.kind() == ErrorKind::ConnectionReset => {}
        Err(error) => panic!("{error}"),
    }
    String::from_utf8_lossy(&body).into_owned()
}

#[tokio::test(flavor = "current_thread")]
async fn serves_a_linked_endpoint_without_a_handwritten_plan() {
    LocalSet::new()
        .run_until(async {
            let running = NativeWebHost::new()
                .with_middleware(TestHeaderMiddleware)
                .plugin::<GreetingsHttp>()
                .bind(SocketAddr::from(([127, 0, 0, 1], 0)))
                .start()
                .await
                .unwrap();
            let response = post_greeting(running.address()).await;
            assert!(
                response.starts_with("HTTP/1.1 201"),
                "unexpected response: {response:?}"
            );
            assert!(response.contains("x-lenso-middleware: active"));
            assert!(
                running
                    .route_manifest()
                    .is_some_and(|manifest| !manifest.routes().is_empty())
            );
            running.shutdown().await.unwrap();
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn replicated_host_serves_a_linked_endpoint_on_one_lane() {
    let running = NativeWebHost::new()
        .plugin::<GreetingsHttp>()
        .bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .start_replicated()
        .await
        .unwrap();
    assert_eq!(running.lane_count(), 1);
    let response = post_greeting(running.address()).await;
    assert!(
        response.starts_with("HTTP/1.1 201"),
        "unexpected response: {response:?}"
    );
    assert!(!running.is_failed());
    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn replicated_host_starts_declared_execution_lanes() {
    let running = NativeWebHost::new()
        .plugin_on_lane::<GreetingsHttp>("web")
        .bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .with_replicated_ready_timeout(Duration::from_secs(3))
        .start_replicated()
        .await
        .unwrap();
    assert_eq!(running.lane_count(), 2);
    let response = tokio::time::timeout(Duration::from_secs(3), post_greeting(running.address()))
        .await
        .expect("cross-lane HTTP request should complete");
    assert!(
        response.starts_with("HTTP/1.1 201"),
        "unexpected response: {response:?}"
    );
    assert!(!running.is_failed());
    running.shutdown().await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn event_host_composes_multiple_native_endpoint_plugins() {
    Box::pin(LocalSet::new().run_until(async {
        let running = NativeWebHost::new()
            .plugin::<GreetingsHttp>()
            .plugin::<OrderSearchHttp>()
            .start_event()
            .await
            .unwrap();
        let response = running
            .handle(
                Request::builder()
                    .method("QUERY")
                    .uri("/orders/search")
                    .header("content-type", "application/json")
                    .body(Bytes::from_static(br#"{"term":"open orders"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.body().as_ref(), br#"{"term":"open orders"}"#);
        running.shutdown().await.unwrap();
    }))
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn ingress_configuration_owns_the_bind_address_without_an_override() {
    LocalSet::new()
        .run_until(async {
            let config = WebIngressConfig::default()
                .with_bind_address(SocketAddr::from(([127, 0, 0, 1], 0)))
                .unwrap();
            let running = NativeWebHost::new()
                .with_ingress_config(config)
                .plugin::<GreetingsHttp>()
                .start()
                .await
                .unwrap();
            assert_ne!(running.address().port(), 8080);
            running.shutdown().await.unwrap();
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn plugin_with_empty_configuration_still_serves() {
    LocalSet::new()
        .run_until(async {
            let running = NativeWebHost::new()
                .plugin_with::<GreetingsHttp>(serde_json::json!({}))
                .unwrap()
                .bind(SocketAddr::from(([127, 0, 0, 1], 0)))
                .start()
                .await
                .unwrap();
            let response = post_greeting(running.address()).await;
            assert!(
                response.starts_with("HTTP/1.1 201"),
                "unexpected response: {response:?}"
            );
            running.shutdown().await.unwrap();
        })
        .await;
}

#[derive(Debug)]
struct TestDiagnostics(Rc<Cell<usize>>);

impl WebIngressDiagnostics for TestDiagnostics {
    fn endpoint_runtime_failure(&self, _event: WebIngressEndpointFailure<'_>) {
        self.0.set(self.0.get() + 1);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn host_shares_diagnostics_and_exposes_event_manifest() {
    LocalSet::new()
        .run_until(async {
            let observed = Rc::new(Cell::new(0));
            let running = NativeWebHost::new()
                .with_diagnostics(TestDiagnostics(observed.clone()))
                .plugin::<GreetingsHttp>()
                .start_event()
                .await
                .unwrap();
            assert!(
                running
                    .route_manifest()
                    .is_some_and(|manifest| !manifest.routes().is_empty())
            );
            assert_eq!(observed.get(), 0);
            running.shutdown().await.unwrap();
        })
        .await;
}

#[derive(Debug)]
struct TestHeaderMiddleware;

impl WebIngressMiddleware for TestHeaderMiddleware {
    fn identity(&self) -> &'static str {
        "test.header"
    }

    fn before_request<'a>(
        &'a self,
        _request: &'a mut WebIngressRequest,
    ) -> LocalBoxFuture<'a, Result<WebIngressMiddlewareOutcome, RuntimeFailure>> {
        Box::pin(std::future::ready(Ok(
            WebIngressMiddlewareOutcome::Continue,
        )))
    }

    fn after_response<'a>(
        &'a self,
        _request: &'a WebIngressRequest,
        response: &'a mut WebIngressResponse,
    ) -> LocalBoxFuture<'a, Result<(), RuntimeFailure>> {
        Box::pin(async move {
            response
                .headers_mut()
                .insert("x-lenso-middleware", HeaderValue::from_static("active"));
            Ok(())
        })
    }
}

#[tokio::test(flavor = "current_thread")]
async fn event_mode_uses_host_ingress_configuration() {
    Box::pin(LocalSet::new().run_until(async {
        let config = WebIngressConfig::default()
            .with_request_limits(1, 1024)
            .unwrap();
        let running = NativeWebHost::new()
            .with_ingress_config(config)
            .plugin::<GreetingsHttp>()
            .start_event()
            .await
            .unwrap();
        let response = running
            .handle(
                Request::post("/greetings")
                    .header("content-type", "application/json")
                    .body(Bytes::from_static(br#"{"name":"Lenso"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 413);
        running.shutdown().await.unwrap();
    }))
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn event_host_accepts_a_tower_layer_policy() {
    let service = tower::service_fn(|request: WebIngressRequest| async move {
        let blocked = request.uri().path() == "/tower-blocked";
        if blocked {
            Ok::<_, RuntimeFailure>(TowerMiddlewareOutcome::Respond(
                Response::builder()
                    .status(403)
                    .body(Bytes::from_static(b"blocked by tower"))
                    .unwrap(),
            ))
        } else {
            Ok(TowerMiddlewareOutcome::Continue)
        }
    });
    let layer = tower::layer::util::Identity::new();

    Box::pin(LocalSet::new().run_until(async {
        let running = NativeWebHost::new()
            .with_tower_layer("test.tower", layer, service)
            .plugin::<GreetingsHttp>()
            .start_event()
            .await
            .unwrap();
        let response = running
            .handle(Request::get("/tower-blocked").body(Bytes::new()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), 403);
        assert_eq!(response.body().as_ref(), b"blocked by tower");
        running.shutdown().await.unwrap();
    }))
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn event_mode_applies_host_middleware_before_shutdown() {
    Box::pin(LocalSet::new().run_until(async {
        let running = NativeWebHost::new()
            .with_middleware(TestHeaderMiddleware)
            .plugin::<GreetingsHttp>()
            .start_event()
            .await
            .unwrap();
        let response = running
            .handle(Request::get("/missing").body(Bytes::new()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.headers()["x-lenso-middleware"], "active");
        running.shutdown().await.unwrap();
    }))
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn event_mode_covers_route_errors_without_a_socket() {
    Box::pin(LocalSet::new().run_until(async {
        let running = NativeWebHost::new()
            .plugin::<GreetingsHttp>()
            .start_event()
            .await
            .unwrap();
        let missing = running
            .handle(Request::get("/missing").body(Bytes::new()).unwrap())
            .await
            .unwrap();
        assert_eq!(missing.status(), 404);
        assert_eq!(
            missing.headers()["content-type"],
            "application/json; charset=utf-8"
        );

        let preserved = running
            .handle_response(Request::get("/missing").body(Bytes::new()).unwrap())
            .await
            .unwrap();
        assert!(matches!(preserved.body(), WebIngressEventBody::Buffered(_)));

        let wrong_method = running
            .handle(
                Request::post("/greetings/search")
                    .body(Bytes::new())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(wrong_method.status(), 405);
        assert_eq!(wrong_method.headers()["allow"], "GET");
        running.shutdown().await.unwrap();
    }))
    .await;
}

#[test]
fn lane_authoring_is_preserved_in_the_resolved_web_plan() {
    let host = NativeWebHost::new().plugin_on_lane::<GreetingsHttp>("web");
    let plan = host.resolve_plan().unwrap();

    assert_eq!(
        plan.plugin_instances()
            .iter()
            .find(|instance| instance.package_id() == GreetingsHttp::PACKAGE_ID)
            .unwrap()
            .execution_lane()
            .as_str(),
        "web"
    );
    assert!(
        plan.execution_lanes()
            .iter()
            .any(|lane| lane.id().as_str() == "web")
    );
}

#[test]
fn inventory_does_not_enable_linked_endpoints() {
    GreetingsHttp::link();
    let error = NativeWebHost::new().resolve_plan().unwrap_err();
    assert!(matches!(error, WebHostError::MissingEndpoint));
}
