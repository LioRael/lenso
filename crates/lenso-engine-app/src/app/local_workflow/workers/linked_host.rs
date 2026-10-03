use bytes::Bytes;
use http::{HeaderName, HeaderValue, Request};
use lenso_app_plan::ResolvedAppPlan;
use lenso_kernel::{CancellationToken, Kernel, ShutdownOutcome};
use lenso_native_adapter::NativePluginRegistry;
use lenso_web_ingress_plugin::{WebIngressEventBody, WebIngressEventFactory};
use lenso_workers_driver::WorkersDriver;
use serde::Deserialize;
use std::time::Duration;
use wasm_bindgen::prelude::*;

const BODY_LIMIT: usize = 1_048_576;
const HEAD_LIMIT: usize = 16_384;
const SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(250);

mod response_session;
use response_session::ResponseSession;

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
pub async fn open_http(input: String, scope: JsValue) -> Result<ResponseSession, JsValue> {
    // LENSO_LINK_PLUGINS
    let request = request(&input)?;
    let plan: ResolvedAppPlan = serde_json::from_str(include_str!("plan.json")).map_err(error)?;
    let ingress = WebIngressEventFactory::new();
    let driver = WorkersDriver::new();
    let event = EventGuard(driver.clone());
    let cancellation = CancellationToken::new();
    let cancel_guard = CancellationGuard::new(scope.clone(), cancellation.clone());
    // LENSO_WORKERS_FACILITY_PREPARE
    let registry = NativePluginRegistry::new()
        .with_linked_factories()
        .with_factory(ingress.clone());
    // LENSO_WORKERS_FACILITY_BIND
    let app = Kernel::start_native(plan, driver, registry)
        .await
        .map_err(error)?;
    let ready = app.is_ready() && app.is_accepting();
    if !ready {
        let shutdown = app.shutdown(SHUTDOWN_TIMEOUT).await;
        return Err(error(("Workers App is not ready", shutdown)));
    }
    let response = match ingress.handle_response(request, cancellation.clone()).await {
        Ok(response) => response,
        Err(failure) => {
            let shutdown = app.shutdown(SHUTDOWN_TIMEOUT).await;
            return Err(error((failure, shutdown)));
        }
    };
    let (parts, body) = response.into_parts();
    let head = (|| {
        match &body {
            WebIngressEventBody::Buffered(bytes) if bytes.len() > BODY_LIMIT => {
                return Err(error("buffered response body exceeds bound"));
            }
            WebIngressEventBody::WebSocket(_) => {
                return Err(error("linked Workers does not admit WebSocket upgrades"));
            }
            _ => {}
        }
        let headers = parts
            .headers
            .iter()
            .map(|(name, value)| Ok((name.as_str(), value.to_str().map_err(error)?)))
            .collect::<Result<Vec<_>, JsValue>>()?;
        serde_json::to_string(&headers).map_err(error)
    })();
    let headers = match head {
        Ok(headers) => headers,
        Err(failure) => {
            drop(body);
            let shutdown = app.shutdown(SHUTDOWN_TIMEOUT).await;
            return Err(error((failure, shutdown)));
        }
    };
    let (session, finished, failed) =
        ResponseSession::new(parts.status.as_u16(), headers, body, cancellation.clone());
    let lifetime_body = session.body.clone();
    let closed = wasm_bindgen_futures::future_to_promise(async move {
        // This profile creates one independent App per HTTP request. These
        // guards belong to its session, never a shared isolate-wide App.
        let _event = event;
        let _cancel = cancel_guard;
        let cancelled = cancellation.cancelled();
        futures::pin_mut!(cancelled, finished);
        let (terminal, cancelled) = match futures::future::select(cancelled, finished).await {
            futures::future::Either::Left(_) => (true, true),
            futures::future::Either::Right((outcome, _)) => {
                (outcome.unwrap_or(false), cancellation.is_cancelled())
            }
        };
        lifetime_body.cancel();
        let shutdown = app.shutdown(SHUTDOWN_TIMEOUT).await;
        // Stream failure is observed by read(); it does not make a cleanly
        // stopped, independent request App an unsafe Wasm generation. Only
        // unconfirmed cleanup abandons the runner's shared generation.
        if shutdown != ShutdownOutcome::Clean || (!terminal && !failed.get()) {
            return Err(error(("response session cleanup unconfirmed", shutdown)));
        }
        js_sys::JSON::parse(if failed.get() {
            "{\"shutdown\":\"clean\",\"terminal\":\"failed\"}"
        } else if cancelled {
            "{\"shutdown\":\"clean\",\"terminal\":\"cancelled\"}"
        } else {
            "{\"shutdown\":\"clean\",\"terminal\":\"success\"}"
        })
    });
    Ok(session.with_closed(closed))
}
