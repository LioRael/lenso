//! Request-owned pull bridge; the JavaScript runner leases the Wasm generation.
use super::error;
use bytes::Bytes;
use futures::channel::oneshot;
use js_sys::Promise;
use lenso_kernel::{CancellationToken, RuntimeFailure};
use lenso_web_ingress_plugin::{WebIngressEventBody, WebIngressResponseStream};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::future_to_promise;

const CHUNK_LIMIT: usize = 65_536;

pub(super) enum SessionBody {
    Streaming(WebIngressResponseStream),
    Buffered(RefCell<Bytes>),
}

impl SessionBody {
    async fn receive(&self, limit: usize) -> Result<Option<Bytes>, RuntimeFailure> {
        match self {
            Self::Streaming(body) => body.receive().await,
            Self::Buffered(body) => {
                let mut bytes = body.borrow_mut();
                let length = bytes.len().min(limit);
                Ok((length > 0).then(|| bytes.split_to(length)))
            }
        }
    }

    pub(super) fn cancel(&self) {
        match self {
            Self::Streaming(body) => body.cancel(),
            Self::Buffered(body) => *body.borrow_mut() = Bytes::new(),
        }
    }
}

#[wasm_bindgen]
pub struct ResponseSession {
    status: u16,
    headers: String,
    pub(super) body: Rc<SessionBody>,
    closed: Promise,
    finish: Rc<RefCell<Option<oneshot::Sender<bool>>>>,
    failed: Rc<Cell<bool>>,
    receiving: Rc<Cell<bool>>,
    cancellation: CancellationToken,
}

impl ResponseSession {
    pub(super) fn new(
        status: u16,
        headers: String,
        body: WebIngressEventBody,
        cancellation: CancellationToken,
    ) -> (Self, oneshot::Receiver<bool>, Rc<Cell<bool>>) {
        let body = match body {
            WebIngressEventBody::Streaming(body) => SessionBody::Streaming(body),
            WebIngressEventBody::Buffered(body) => SessionBody::Buffered(RefCell::new(body)),
            WebIngressEventBody::WebSocket(_) => unreachable!("admitted HTTP body"),
        };
        let (finish, finished) = oneshot::channel();
        let failed = Rc::new(Cell::new(false));
        (
            Self {
                status,
                headers,
                body: Rc::new(body),
                closed: Promise::resolve(&JsValue::NULL),
                finish: Rc::new(RefCell::new(Some(finish))),
                failed: failed.clone(),
                receiving: Rc::new(Cell::new(false)),
                cancellation,
            },
            finished,
            failed,
        )
    }

    pub(super) fn with_closed(mut self, closed: Promise) -> Self {
        self.closed = closed;
        self
    }
}

struct Reading(Rc<Cell<bool>>);
impl Drop for Reading {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

#[wasm_bindgen]
impl ResponseSession {
    #[wasm_bindgen(getter)]
    pub fn status(&self) -> u16 {
        self.status
    }
    #[wasm_bindgen(getter)]
    pub fn headers(&self) -> String {
        self.headers.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn closed(&self) -> Promise {
        self.closed.clone()
    }

    /// Exactly one provider receive per transport pull. Buffered routes are
    /// sliced to the same chunk budget without changing their business code.
    pub fn read(&self, max_bytes: u32) -> Promise {
        let limit = usize::try_from(max_bytes).unwrap_or(usize::MAX);
        if limit == 0 || limit > CHUNK_LIMIT || self.receiving.replace(true) {
            self.failed.set(true);
            self.cancellation.cancel();
            return Promise::reject(&error("invalid or overlapping response read"));
        }
        let reading = Reading(self.receiving.clone());
        let body = self.body.clone();
        let finish = self.finish.clone();
        let failed = self.failed.clone();
        let cancellation = self.cancellation.clone();
        future_to_promise(async move {
            let _reading = reading;
            if cancellation.is_cancelled() {
                return Err(error("response session cancelled"));
            }
            let received = body.receive(limit).await;
            // Only the owning request's typed cancellation is expected here.
            // A provider/protocol failure remains sticky even if cancel races it.
            let expected_cancel = cancellation.is_cancelled()
                && matches!(&received, Err(RuntimeFailure::Cancelled { .. }));
            let value = received.map_err(error).and_then(|bytes| match bytes {
                Some(bytes) if bytes.len() > limit => Err(error("response chunk exceeds bound")),
                Some(bytes) => Ok(js_sys::Uint8Array::from(bytes.as_ref()).into()),
                None => Ok(JsValue::NULL),
            });
            if value.is_err() {
                if !expected_cancel {
                    failed.set(true);
                }
                body.cancel();
            }
            if match &value {
                Err(_) => true,
                Ok(value) => value.is_null(),
            } {
                if let Some(finish) = finish.borrow_mut().take() {
                    let _ = finish.send(value.is_ok() || expected_cancel);
                }
            }
            value
        })
    }
}

impl Drop for ResponseSession {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}
