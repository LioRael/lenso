//! A bounded Workers HTTP Host probe for the shared Web Ingress parity corpus.
#![cfg(all(target_arch = "wasm32", target_os = "unknown"))]

use bytes::Bytes;
use http::{HeaderName, HeaderValue, Request};
use lenso_kernel::{CancellationToken, Kernel, ShutdownOutcome};
use lenso_native_adapter::NativePluginRegistry;
use lenso_web_http_parity_fixture::{HttpParityEndpointFactory, plan};
use lenso_web_ingress_plugin::{SessionCookieConfig, WebIngressConfig, WebIngressEventFactory};
use lenso_workers_driver::WorkersDriver;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use wasm_bindgen::prelude::*;

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

/// Starts one event App, dispatches through the Plan-bound Web Ingress Plugin,
/// then confirms shutdown before returning bytes to the JS HTTP transport.
#[wasm_bindgen]
pub async fn handle_http(input: String, scope: JsValue) -> Result<String, JsValue> {
    let request = request(&input)?;
    let configuration = configuration()?;
    let ingress = WebIngressEventFactory::new();
    let driver = WorkersDriver::new();
    let _event = EventGuard(driver.clone());
    let cancellation = CancellationToken::new();
    let _cancellation = CancellationGuard::new(scope, cancellation.clone());
    let app = Kernel::start_native(
        plan(configuration),
        driver,
        NativePluginRegistry::new()
            .with_factory(HttpParityEndpointFactory)
            .with_factory(ingress.clone()),
    )
    .await
    .map_err(error)?;
    let ready = app.is_ready() && app.is_accepting();
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
