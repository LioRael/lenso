use bytes::Bytes;
use http::{HeaderName, HeaderValue, Request};
use lenso_app_plan::ResolvedAppPlan;
use lenso_kernel::{CancellationToken, Kernel, RuntimeDriver, ShutdownOutcome};
use lenso_native_adapter::NativePluginRegistry;
use lenso_web_ingress_plugin::WebIngressEventFactory;
use lenso_workers_driver::WorkersDriver;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use wasm_bindgen::prelude::*;

const BODY_LIMIT: usize = 1_048_576;
const HEAD_LIMIT: usize = 16_384;
const SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(250);

#[wasm_bindgen(raw_module = "@lenso/workers-runtime/http")]
extern "C" {
    #[wasm_bindgen(js_name = cancellation)]
    fn set_cancellation(scope: &JsValue, callback: &JsValue);
}

// LENSO_WORKERS_FACILITY_IMPORT

struct CancellationGuard {
    scope: JsValue,
    _callback: Closure<dyn FnMut()>,
}

impl CancellationGuard {
    fn new(scope: JsValue, token: CancellationToken) -> Self {
        let callback = Closure::new(move || token.cancel());
        set_cancellation(&scope, callback.as_ref());
        Self { scope, _callback: callback }
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
    let mut head_bytes = 0;
    for (name, value) in input.headers {
        head_bytes += name.len() + value.len();
        if head_bytes > HEAD_LIMIT {
            return Err(error("request head exceeds bound"));
        }
        request.headers_mut().append(
            HeaderName::from_bytes(name.as_bytes()).map_err(error)?,
            HeaderValue::from_str(&value).map_err(error)?,
        );
    }
    Ok(request)
}

#[wasm_bindgen]
pub async fn handle_http(input: String, scope: JsValue) -> Result<String, JsValue> {
    // LENSO_LINK_PLUGINS
    let request = request(&input)?;
    let plan: ResolvedAppPlan = serde_json::from_str(include_str!("plan.json")).map_err(error)?;
    let ingress = WebIngressEventFactory::new();
    let driver = WorkersDriver::new();
    let _event = EventGuard(driver.clone());
    let cancellation = CancellationToken::new();
    let _cancellation = CancellationGuard::new(scope.clone(), cancellation.clone());
    // LENSO_WORKERS_FACILITY_PREPARE
    let registry = NativePluginRegistry::new()
        .with_linked_factories()
        .with_factory(ingress.clone());
    // LENSO_WORKERS_FACILITY_BIND
    let app = Kernel::start_native(plan, driver, registry).await.map_err(error)?;
    let ready = app.is_ready() && app.is_accepting();
    if !ready {
        let shutdown = app.shutdown(SHUTDOWN_TIMEOUT).await;
        return Err(error(("Workers App is not ready", shutdown)));
    }
    let response = ingress.handle(request, cancellation.clone()).await;
    let shutdown = app.shutdown(SHUTDOWN_TIMEOUT).await;
    if shutdown != ShutdownOutcome::Clean {
        return Err(error(shutdown));
    }
    let (parts, body) = response.map_err(error)?.into_parts();
    if body.len() > BODY_LIMIT {
        return Err(error("response body exceeds bound"));
    }
    let headers = parts.headers.iter().map(|(name, value)| {
        Ok((name.as_str().to_owned(), value.to_str().map_err(error)?.to_owned()))
    }).collect::<Result<Vec<_>, JsValue>>()?;
    serde_json::to_string(&HttpReceipt {
        status: parts.status.as_u16(), headers, body: body.to_vec(), ready,
        shutdown: "clean", cancelled: cancellation.is_cancelled(),
    }).map_err(error)
}
