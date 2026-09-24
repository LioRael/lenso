//! A bounded Workers HTTP Host probe for the shared Web Ingress parity corpus.
#![cfg(all(target_arch = "wasm32", target_os = "unknown"))]

use bytes::Bytes;
use http::{HeaderName, HeaderValue, Request};
use js_sys::{Function, Reflect};
use lenso_app_plan::{
    AppComposition, CapabilityBinding, CapabilityEndpointPlan, CapabilityRequirementPlan,
    PluginInstancePlan, ResolvedAppPlan,
};
use lenso_capability_http_client::{
    self as http_client, Client, SEND_OPERATION, SendError, SendRequest, SendResponse,
};
use lenso_capability_http_endpoint as http_endpoint;
use lenso_http_egress_plugin::{HttpEgressConfig, HttpEgressEventFactory};
use lenso_kernel::{
    CancellationToken, InvocationContext, Kernel, RuntimeDriver, RuntimeFailure, ShutdownOutcome,
};
use lenso_native_adapter::{
    NativePluginFactory, NativePluginFactoryContext, NativePluginInstance, NativePluginRegistry,
};
use lenso_portable_http_endpoint_fixture::linked as shared_endpoint;
use lenso_web_http_parity_fixture::{HttpParityEndpointFactory, plan};
use lenso_web_ingress_plugin::{SessionCookieConfig, WebIngressConfig, WebIngressEventFactory};
use lenso_workers_driver::WorkersDriver;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use wasm_bindgen::{JsCast as _, prelude::*};

const BODY_LIMIT: usize = 65_536;
const HEAD_LIMIT: usize = 16_384;
const REQUEST_TIMEOUT: Duration = Duration::from_millis(500);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(200);

#[wasm_bindgen(raw_module = "@lenso/workers-runtime/http")]
extern "C" {
    #[wasm_bindgen(js_name = cancellation)]
    fn set_cancellation(scope: &JsValue, callback: &JsValue);
}

struct CancellationGuard {
    scope: JsValue,
    _callback: Closure<dyn FnMut()>,
}

impl CancellationGuard {
    fn new(scope: JsValue, token: CancellationToken) -> Self {
        let callback = Closure::new(move || token.cancel());
        set_cancellation(&scope, callback.as_ref());
        Self {
            scope,
            _callback: callback,
        }
    }
}

impl Drop for CancellationGuard {
    fn drop(&mut self) {
        set_cancellation(&self.scope, &JsValue::NULL);
    }
}

struct EventGuard(WorkersDriver);

impl Drop for EventGuard {
    fn drop(&mut self) {
        self.0.request_shutdown();
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HttpInput {
    method: String,
    uri: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

#[derive(Serialize)]
struct HttpReceipt {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    ready: bool,
    shutdown: &'static str,
    cancelled: bool,
}

fn error(value: impl std::fmt::Debug) -> JsValue {
    JsValue::from_str(&format!("{value:?}"))
}

fn request(input: &str) -> Result<Request<Bytes>, JsValue> {
    // The JS Host bounds the body before JSON encoding. Retain a bound here
    // for direct Wasm callers before serde allocates request fields.
    if input.len() > BODY_LIMIT * 4 + HEAD_LIMIT * 6 {
        return Err(error("serialized request exceeds bound"));
    }
    let input: HttpInput = serde_json::from_str(input).map_err(error)?;
    if input.body.len() > BODY_LIMIT {
        return Err(error("request body exceeds bound"));
    }
    let mut request = Request::builder()
        .method(input.method.as_str())
        .uri(input.uri.as_str())
        .body(Bytes::from(input.body))
        .map_err(error)?;
    for (name, value) in input.headers {
        request.headers_mut().append(
            HeaderName::from_bytes(name.as_bytes()).map_err(error)?,
            HeaderValue::from_str(&value).map_err(error)?,
        );
    }
    Ok(request)
}

fn configuration() -> Result<String, JsValue> {
    let config = WebIngressConfig::default()
        .with_session_cookie(
            SessionCookieConfig::new("__Host-session", "__Host-csrf", "x-csrf-token")
                .map_err(error)?,
        )
        .map_err(error)?
        .with_request_limits(BODY_LIMIT, HEAD_LIMIT)
        .map_err(error)?
        .with_request_timeout(REQUEST_TIMEOUT)
        .map_err(error)?;
    serde_json::to_string(&config).map_err(error)
}

fn response_limit_receipt(ready: bool, cancelled: bool) -> HttpReceipt {
    HttpReceipt {
        status: 502,
        headers: vec![
            ("content-type".into(), "application/json".into()),
            ("x-content-type-options".into(), "nosniff".into()),
        ],
        body: br#"{"error":"response_body_too_large"}"#.to_vec(),
        ready,
        shutdown: "clean",
        cancelled,
    }
}

#[derive(Debug)]
struct EgressCallerFactory;

impl NativePluginFactory for EgressCallerFactory {
    fn package_id(&self) -> &'static str {
        "fixture.egress-caller"
    }

    fn instantiate(
        &self,
        _: NativePluginFactoryContext<'_>,
    ) -> Result<NativePluginInstance, RuntimeFailure> {
        Ok(NativePluginInstance::default())
    }
}

fn scope_field(scope: &JsValue, name: &str) -> Result<JsValue, JsValue> {
    Reflect::get(scope, &JsValue::from_str(name)).map_err(error)
}

fn egress_plan(config: &HttpEgressConfig) -> Result<ResolvedAppPlan, JsValue> {
    AppComposition::new(
        vec![
            PluginInstancePlan::new("egress", "lenso.http-egress")
                .with_configuration(serde_json::to_string(config).map_err(error)?)
                .with_capability(CapabilityEndpointPlan::new(
                    http_client::CAPABILITY_ID,
                    http_client::DESCRIPTOR_VERSION,
                    [SEND_OPERATION],
                )),
            PluginInstancePlan::new("caller", "fixture.egress-caller").with_requirement(
                CapabilityRequirementPlan::one(
                    http_client::CAPABILITY_ID,
                    http_client::DESCRIPTOR_VERSION,
                ),
            ),
        ],
        vec![CapabilityBinding::new(
            "caller",
            http_client::CAPABILITY_ID,
            http_client::DESCRIPTOR_VERSION,
            "egress",
        )],
    )
    .resolve()
    .map_err(error)
}

fn shared_endpoint_plan(configuration: String) -> Result<ResolvedAppPlan, JsValue> {
    AppComposition::new(
        vec![
            PluginInstancePlan::new("endpoint", shared_endpoint::PACKAGE_ID).with_capability(
                CapabilityEndpointPlan::new(
                    http_endpoint::CAPABILITY_ID,
                    http_endpoint::DESCRIPTOR_VERSION,
                    [
                        http_endpoint::DESCRIBE_OPERATION,
                        http_endpoint::HANDLE_OPERATION,
                    ],
                ),
            ),
            PluginInstancePlan::new("ingress", lenso_web_ingress_plugin::PACKAGE_ID)
                .with_configuration(configuration)
                .with_requirement(CapabilityRequirementPlan::many(
                    http_endpoint::CAPABILITY_ID,
                    http_endpoint::DESCRIPTOR_VERSION,
                )),
        ],
        vec![CapabilityBinding::new(
            "ingress",
            http_endpoint::CAPABILITY_ID,
            http_endpoint::DESCRIPTOR_VERSION,
            "endpoint",
        )],
    )
    .resolve()
    .map_err(error)
}

fn strip_shared_fixture_prefix(mut request: Request<Bytes>) -> Result<Request<Bytes>, JsValue> {
    let original = request.uri().to_string();
    let path = original
        .strip_prefix("/_shared")
        .filter(|path| path.starts_with('/'))
        .ok_or_else(|| error("invalid shared fixture path"))?;
    *request.uri_mut() = path.parse().map_err(error)?;
    Ok(request)
}

fn egress_outcome(
    result: Result<Result<SendResponse, SendError>, RuntimeFailure>,
) -> serde_json::Value {
    match result {
        Ok(Ok(response)) => serde_json::json!({
            "kind": "response",
            "status": response.status,
            "body": response.body.as_slice(),
        }),
        Ok(Err(SendError::DestinationNotAllowed)) => {
            serde_json::json!({"kind": "destination_not_allowed"})
        }
        Ok(Err(other)) => serde_json::json!({
            "kind": "domain_error",
            "detail": format!("{other:?}"),
        }),
        Err(RuntimeFailure::Cancelled { .. }) => serde_json::json!({"kind": "cancelled"}),
        Err(other) => serde_json::json!({
            "kind": "runtime_failure",
            "detail": format!("{other:?}"),
        }),
    }
}

async fn egress_probe(path: &str, scope: JsValue) -> Result<String, JsValue> {
    let origin = scope_field(&scope, "upstreamOrigin")?
        .as_string()
        .ok_or_else(|| error("missing upstream origin"))?;
    let transport: Function = scope_field(&scope, "httpFetch")?
        .dyn_into()
        .map_err(error)?;
    let url = match path {
        "/egress/get" => format!("{origin}/get"),
        "/egress/cancel" => format!("{origin}/slow"),
        "/egress/denied" => "http://127.0.0.1:1/denied".to_owned(),
        _ => return Err(error("unknown egress probe")),
    };
    let config = HttpEgressConfig::new([origin.as_str()])
        .map_err(error)?
        .with_timeouts(Duration::from_millis(1_000), Duration::from_millis(1_000))
        .map_err(error)?;
    let driver = WorkersDriver::new();
    let _event = EventGuard(driver.clone());
    let cancellation = CancellationToken::new();
    let _cancellation = CancellationGuard::new(scope.clone(), cancellation.clone());
    let app = Kernel::start_native(
        egress_plan(&config)?,
        driver.clone(),
        NativePluginRegistry::new()
            .with_factory(EgressCallerFactory)
            .with_factory(HttpEgressEventFactory::from_js(transport)),
    )
    .await
    .map_err(error)?;
    let ready = app.is_ready() && app.is_accepting();
    if !ready {
        let shutdown = app.shutdown(Duration::from_millis(500)).await;
        return Err(if shutdown == ShutdownOutcome::Clean {
            error("HTTP Egress App is not ready")
        } else {
            error(shutdown)
        });
    }
    let request = SendRequest {
        method: "GET".into(),
        url,
        headers: vec![],
        body: Vec::new().into(),
    };
    let result = if path == "/egress/cancel" {
        let context = InvocationContext::new(7, None, cancellation.clone());
        let call = app.invoke_with_context::<Client>("caller", SEND_OPERATION, context, request);
        let cancel = async {
            driver
                .sleep_until(driver.now() + Duration::from_millis(50))
                .await;
            cancellation.cancel();
        };
        let (result, ()) = futures::join!(call, cancel);
        result
    } else {
        app.invoke::<Client>("caller", SEND_OPERATION, request)
            .await
    };
    let shutdown = app.shutdown(Duration::from_millis(500)).await;
    if shutdown != ShutdownOutcome::Clean {
        return Err(error(shutdown));
    }
    let proof = scope_field(&scope, "egressProof")?;
    let started = scope_field(&proof, "started")?
        .as_f64()
        .ok_or_else(|| error("invalid started count"))?;
    let aborted = scope_field(&proof, "aborted")?
        .as_f64()
        .ok_or_else(|| error("invalid aborted count"))?;
    let receipt = HttpReceipt {
        status: 200,
        headers: vec![
            ("content-type".into(), "application/json".into()),
            ("x-egress-started".into(), started.to_string()),
            ("x-egress-aborted".into(), aborted.to_string()),
        ],
        body: serde_json::to_vec(&egress_outcome(result)).map_err(error)?,
        ready,
        shutdown: "clean",
        cancelled: cancellation.is_cancelled(),
    };
    serde_json::to_string(&receipt).map_err(error)
}

/// Starts one event App, dispatches through the Plan-bound Web Ingress Plugin,
/// then confirms shutdown before returning bytes to the JS HTTP transport.
#[wasm_bindgen]
pub async fn handle_http(input: String, scope: JsValue) -> Result<String, JsValue> {
    let request = request(&input)?;
    if request.uri().path().starts_with("/egress/") {
        return egress_probe(request.uri().path(), scope).await;
    }
    let shared = request.uri().path().starts_with("/_shared/");
    let request = if shared {
        strip_shared_fixture_prefix(request)?
    } else {
        request
    };
    let configuration = configuration()?;
    let ingress = WebIngressEventFactory::new();
    let driver = WorkersDriver::new();
    let _event = EventGuard(driver.clone());
    let cancellation = CancellationToken::new();
    let _cancellation = CancellationGuard::new(scope, cancellation.clone());
    let (plan, registry) = if shared {
        (
            shared_endpoint_plan(configuration)?,
            NativePluginRegistry::new()
                .with_factory(shared_endpoint::NativeEndpointFactory)
                .with_factory(ingress.clone()),
        )
    } else {
        (
            plan(configuration),
            NativePluginRegistry::new()
                .with_factory(HttpParityEndpointFactory)
                .with_factory(ingress.clone()),
        )
    };
    let app = Kernel::start_native(plan, driver, registry)
        .await
        .map_err(error)?;
    let ready = app.is_ready() && app.is_accepting();
    if !ready {
        let shutdown = app.shutdown(SHUTDOWN_TIMEOUT).await;
        return Err(if shutdown == ShutdownOutcome::Clean {
            error("HTTP App is not ready or accepting")
        } else {
            error(shutdown)
        });
    }
    let response = ingress.handle(request, cancellation.clone()).await;
    let shutdown = app.shutdown(SHUTDOWN_TIMEOUT).await;
    if shutdown != ShutdownOutcome::Clean {
        return Err(error(shutdown));
    }
    let response = response.map_err(error)?;
    let (parts, body) = response.into_parts();
    let cancelled = cancellation.is_cancelled();
    let receipt = if body.len() > BODY_LIMIT {
        response_limit_receipt(ready, cancelled)
    } else {
        let headers = parts
            .headers
            .iter()
            .map(|(name, value)| {
                Ok((
                    name.as_str().to_owned(),
                    value.to_str().map_err(error)?.to_owned(),
                ))
            })
            .collect::<Result<Vec<_>, JsValue>>()?;
        HttpReceipt {
            status: parts.status.as_u16(),
            headers,
            body: body.to_vec(),
            ready,
            shutdown: "clean",
            cancelled,
        }
    };
    serde_json::to_string(&receipt).map_err(error)
}

/// Exposes the exact source corpus used by native/event regression tests.
#[wasm_bindgen]
pub fn parity_corpus() -> String {
    lenso_web_http_parity_fixture::CORPUS.to_owned()
}
