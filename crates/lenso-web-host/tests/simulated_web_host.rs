use std::{cell::Cell, rc::Rc, time::Duration};

use bytes::Bytes;
use futures::future::LocalBoxFuture;
use http::{HeaderValue, Request};
use lenso_app_plan::{CapabilityEndpointPlan, authoring::PluginDescriptor};
use lenso_capability_http_endpoint::{
    CAPABILITY_ID as HTTP_CAPABILITY_ID, DESCRIBE_OPERATION as HTTP_DESCRIBE_OPERATION,
    DESCRIPTOR_VERSION as HTTP_DESCRIPTOR_VERSION, HANDLE_OPERATION as HTTP_HANDLE_OPERATION,
};
use lenso_capability_http_stream_endpoint::{
    CAPABILITY_ID as STREAM_CAPABILITY_ID, DESCRIBE_OPERATION as STREAM_DESCRIBE_OPERATION,
    DESCRIPTOR_VERSION as STREAM_DESCRIPTOR_VERSION, HANDLE_OPERATION as STREAM_HANDLE_OPERATION,
};
use lenso_capability_websocket_endpoint::{
    CAPABILITY_ID as WEBSOCKET_CAPABILITY_ID,
    CONNECT_WEBSOCKET_OPERATION as WEBSOCKET_CONNECT_OPERATION, ConnectWebsocketResponse,
    ConnectWebsocketResponseKind, DESCRIBE_WEBSOCKET_OPERATION as WEBSOCKET_DESCRIBE_OPERATION,
    DESCRIPTOR_VERSION as WEBSOCKET_DESCRIPTOR_VERSION,
};
use lenso_kernel::{
    DeterministicDriver, Kernel, NativeApp, RuntimeDriver, RuntimeFailure, ShutdownOutcome,
};
use lenso_web_duplex_fixture::{DuplexFactory, PACKAGE_ID as DUPLEX_PACKAGE_ID};
use lenso_web_greetings_plugin_example::GreetingsHttp;
use lenso_web_host::{NativeWebHost, SimulatedWebHost};
use lenso_web_http_parity_fixture::{
    HttpParityEndpointFactory, PACKAGE_ID as HTTP_PARITY_PACKAGE_ID,
};
use lenso_web_ingress_plugin::{
    SessionCookieConfig, WebIngressConfig, WebIngressMiddleware, WebIngressMiddlewareOutcome,
    WebIngressRequest, WebIngressResponse, WebSocketConfig,
};

fn start_simulated(host: NativeWebHost) -> (DeterministicDriver, NativeApp, SimulatedWebHost) {
    let prepared = host.prepare_simulated().unwrap();
    let (plan, registry, simulated) = prepared.into_parts();
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start_native(plan, driver.clone(), registry))
        .unwrap();
    (driver, app, simulated)
}

fn shutdown(driver: &DeterministicDriver, app: &NativeApp) {
    assert_eq!(
        driver.run(app.shutdown(Duration::from_secs(1))),
        ShutdownOutcome::Clean
    );
}

#[derive(Debug)]
struct ResponseMarker(Rc<Cell<usize>>);

impl WebIngressMiddleware for ResponseMarker {
    fn identity(&self) -> &'static str {
        "test.simulated-response-marker"
    }

    fn before_request<'a>(
        &'a self,
        _request: &'a mut WebIngressRequest,
    ) -> LocalBoxFuture<'a, Result<WebIngressMiddlewareOutcome, RuntimeFailure>> {
        Box::pin(async { Ok(WebIngressMiddlewareOutcome::Continue) })
    }

    fn after_response<'a>(
        &'a self,
        _request: &'a WebIngressRequest,
        response: &'a mut WebIngressResponse,
    ) -> LocalBoxFuture<'a, Result<(), RuntimeFailure>> {
        let observed = self.0.clone();
        Box::pin(async move {
            observed.set(observed.get() + 1);
            response.headers_mut().insert(
                "x-lenso-simulated-host",
                HeaderValue::from_static("real-ingress"),
            );
            Ok(())
        })
    }
}

fn duplex_descriptor() -> PluginDescriptor {
    PluginDescriptor::new(DUPLEX_PACKAGE_ID, "0.0.0", "test.duplex")
        .with_capability(
            CapabilityEndpointPlan::new(
                STREAM_CAPABILITY_ID,
                STREAM_DESCRIPTOR_VERSION,
                [STREAM_DESCRIBE_OPERATION, STREAM_HANDLE_OPERATION],
            )
            .with_stream_operation(STREAM_HANDLE_OPERATION),
        )
        .with_capability(
            CapabilityEndpointPlan::new(
                WEBSOCKET_CAPABILITY_ID,
                WEBSOCKET_DESCRIPTOR_VERSION,
                [WEBSOCKET_DESCRIBE_OPERATION, WEBSOCKET_CONNECT_OPERATION],
            )
            .with_stream_operation(WEBSOCKET_CONNECT_OPERATION),
        )
}

fn http_parity_descriptor() -> PluginDescriptor {
    PluginDescriptor::new(HTTP_PARITY_PACKAGE_ID, "0.0.0", "web").with_capability(
        CapabilityEndpointPlan::new(
            HTTP_CAPABILITY_ID,
            HTTP_DESCRIPTOR_VERSION,
            [HTTP_DESCRIBE_OPERATION, HTTP_HANDLE_OPERATION],
        ),
    )
}

fn websocket_request() -> Request<Bytes> {
    Request::builder()
        .uri("/socket/test")
        .header("authorization", "Bearer proof")
        .header("origin", "https://client.invalid")
        .header("upgrade", "websocket")
        .header("connection", "keep-alive, Upgrade")
        .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
        .header("sec-websocket-version", "13")
        .header("sec-websocket-protocol", "lenso.echo")
        .body(Bytes::new())
        .unwrap()
}

#[test]
fn simulated_host_reuses_real_event_ingress_for_buffered_routing_middleware_and_manifest() {
    let observed = Rc::new(Cell::new(0));
    let (driver, app, simulated) = start_simulated(
        NativeWebHost::new()
            .with_middleware(ResponseMarker(observed.clone()))
            .plugin::<GreetingsHttp>(),
    );

    let manifest = simulated.route_manifest().expect("event Ingress is active");
    assert!(
        manifest
            .routes()
            .iter()
            .any(|route| route.method == "POST" && route.path == "/greetings")
    );

    let response = driver
        .run(
            simulated.request(
                Request::builder()
                    .method("POST")
                    .uri("/greetings")
                    .header("content-type", "application/json")
                    .body(Bytes::from_static(br#"{"name":"Lenso"}"#))
                    .unwrap(),
            ),
        )
        .unwrap();
    assert_eq!(response.status(), 201);
    assert_eq!(response.headers()["x-lenso-simulated-host"], "real-ingress");
    assert!(response.body().starts_with(br#"{"id":"greeting-1"#));
    assert_eq!(observed.get(), 1);

    shutdown(&driver, &app);
}

#[test]
fn simulated_host_preserves_real_stream_websocket_and_disconnect_cancellation() {
    let config = WebIngressConfig::default()
        .with_websocket(WebSocketConfig::new(vec!["https://client.invalid".to_owned()]).unwrap())
        .unwrap();
    let (driver, app, simulated) = start_simulated(
        NativeWebHost::new()
            .with_ingress_config(config)
            .plugin::<GreetingsHttp>()
            .factory(DuplexFactory, duplex_descriptor()),
    );

    let stream = driver
        .run(simulated.open_stream(Request::get("/stream").body(Bytes::new()).unwrap()))
        .unwrap()
        .into_body();
    assert_eq!(
        driver.run(stream.receive()).unwrap().unwrap(),
        Bytes::from_static(&[0, 1, 255])
    );
    assert_eq!(
        driver.run(stream.receive()).unwrap().unwrap(),
        Bytes::from_static(&[2, 3, 254])
    );
    assert!(driver.run(stream.receive()).unwrap().is_none());
    assert!(stream.is_closed());
    drop(stream);

    let websocket = driver
        .run(simulated.open_websocket(websocket_request()))
        .unwrap()
        .into_body();
    assert_eq!(websocket.protocol(), Some("lenso.echo"));
    let close = ConnectWebsocketResponse {
        kind: ConnectWebsocketResponseKind::Close,
        protocol: None,
        text: None,
        body: None,
        code: Some(1000),
        reason: Some("done".to_owned()),
    };
    driver.run(websocket.send(close)).unwrap();
    assert_eq!(
        driver.run(websocket.receive()).unwrap().unwrap().kind,
        ConnectWebsocketResponseKind::Close
    );
    assert!(driver.run(websocket.receive()).unwrap().is_none());
    drop(websocket);

    let disconnected =
        simulated.begin_request(Request::get("/stream?hold").body(Bytes::new()).unwrap());
    disconnected.disconnect();
    assert!(disconnected.is_disconnected());
    let response = driver.run(disconnected.send_buffered()).unwrap();
    assert_eq!(response.status(), 503);

    shutdown(&driver, &app);
}

#[test]
fn simulated_host_projects_timeout_malformed_response_and_credential_evidence_through_ingress() {
    let config = WebIngressConfig::default()
        .with_request_timeout(Duration::from_millis(25))
        .unwrap()
        .with_session_cookie(
            SessionCookieConfig::new("__Host-session", "__Host-csrf", "x-csrf-token").unwrap(),
        )
        .unwrap();
    let (driver, app, simulated) = start_simulated(
        NativeWebHost::new()
            .with_ingress_config(config)
            .plugin::<GreetingsHttp>()
            .factory(HttpParityEndpointFactory, http_parity_descriptor()),
    );

    // The parity fixture does not cooperate with cancellation on `/blocked`.
    // Advancing only the deterministic driver proves the event ingress retains
    // the real endpoint deadline and projects its safe HTTP failure.
    let timer_driver = driver.clone();
    let blocked = simulated.request(Request::get("/blocked").body(Bytes::new()).unwrap());
    let advance_timeout = async move {
        timer_driver.yield_now().await;
        timer_driver.advance(Duration::from_millis(25));
    };
    let (blocked, ()) = driver.run(futures::future::join(blocked, advance_timeout));
    let blocked = blocked.unwrap();
    assert_eq!(blocked.status(), 504);
    assert_eq!(blocked.body().as_ref(), br#"{"error":"endpoint_timeout"}"#);

    // This invalid status is emitted by the actual Endpoint Capability fixture,
    // then rejected by real ingress response normalization.
    let malformed = driver
        .run(simulated.request(Request::get("/invalid").body(Bytes::new()).unwrap()))
        .unwrap();
    assert_eq!(malformed.status(), 502);
    assert_eq!(
        malformed.body().as_ref(),
        br#"{"error":"invalid_endpoint_response"}"#
    );

    // Ingress extracts one credential as evidence for the Capability request,
    // but strips the original sensitive transport headers before dispatch.
    let bearer = driver
        .run(
            simulated.request(
                Request::get("/echo/bearer")
                    .header("authorization", "Bearer proof-token")
                    .body(Bytes::new())
                    .unwrap(),
            ),
        )
        .unwrap();
    let bearer: serde_json::Value = serde_json::from_slice(bearer.body()).unwrap();
    assert_eq!(bearer["credential"]["scheme"], "bearer");
    assert_eq!(bearer["credential"]["value"], "proof-token");
    assert!(
        bearer["headers"]
            .as_array()
            .unwrap()
            .iter()
            .all(|header| header["name"] != "authorization" && header["name"] != "cookie")
    );

    let session = driver
        .run(
            simulated.request(
                Request::get("/echo/session")
                    .header("cookie", "__Host-session=session-token")
                    .body(Bytes::new())
                    .unwrap(),
            ),
        )
        .unwrap();
    let session: serde_json::Value = serde_json::from_slice(session.body()).unwrap();
    assert_eq!(session["credential"]["scheme"], "session");
    assert_eq!(session["credential"]["value"], "session-token");

    // Multiple Authorization fields are rejected before any credential becomes
    // auth evidence, using the same ingress path as a socket-backed Host.
    let ambiguous = driver
        .run(
            simulated.request(
                Request::get("/echo/rejected")
                    .header("authorization", "Bearer first")
                    .header("authorization", "Bearer second")
                    .body(Bytes::new())
                    .unwrap(),
            ),
        )
        .unwrap();
    assert_eq!(ambiguous.status(), 400);
    assert_eq!(ambiguous.body().as_ref(), br#"{"error":"bad_request"}"#);

    shutdown(&driver, &app);
}
