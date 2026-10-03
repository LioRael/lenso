//! The same source Plugin is compiled into both generated Native and Workers Apps.
use futures::future::LocalBoxFuture;
use lenso_capability_http_stream_endpoint as endpoint;
use lenso_kernel::{InvocationContext, NativeStreamItem, NativeStreamSession, RuntimeFailure};
use std::{any::Any, cell::Cell, rc::Rc};

#[lenso::plugin]
#[derive(Clone, Debug, Default)]
pub struct Probe {}

#[lenso::provides(endpoint::StreamEndpoint)]
impl Probe {
    async fn describe_stream(
        &self,
        _: InvocationContext,
        _: endpoint::DescribeRequest,
    ) -> Result<endpoint::DescribeResponse, endpoint::DescribeError> {
        Ok(endpoint::DescribeResponse {
            routes: vec![endpoint::DescribeResponseRoutesItem {
                method: "GET".into(),
                path: "/events".into(),
                route_id: "events".into(),
            }],
        })
    }

    async fn handle_stream(
        &self,
        context: InvocationContext,
        request: endpoint::HandleRequest,
    ) -> Result<ProbeSession, endpoint::HandleError> {
        Ok(ProbeSession {
            step: Rc::new(Cell::new(0)),
            context,
            mode: request.query.unwrap_or_default(),
        })
    }
}

#[derive(Debug)]
struct ProbeSession {
    step: Rc<Cell<usize>>,
    context: InvocationContext,
    mode: String,
}

fn chunk(bytes: Vec<u8>) -> NativeStreamItem {
    NativeStreamItem::Message(Box::new(endpoint::HandleResponse {
        kind: endpoint::HandleResponseKind::Chunk,
        status: None,
        headers: None,
        body: Some(bytes.into()),
    }))
}

impl NativeStreamSession for ProbeSession {
    fn send(&self, _: Box<dyn Any>) -> LocalBoxFuture<'static, Result<(), RuntimeFailure>> {
        Box::pin(async { Ok(()) })
    }
    fn close_send(&self) -> LocalBoxFuture<'static, Result<(), RuntimeFailure>> {
        Box::pin(async { Ok(()) })
    }
    fn cancel(&self) {
        self.context.cancellation().cancel();
    }
    fn receive(&self) -> LocalBoxFuture<'static, Result<NativeStreamItem, RuntimeFailure>> {
        let step = self.step.get();
        self.step.set(step + 1);
        let mode = self.mode.clone();
        let context = self.context.clone();
        Box::pin(async move {
            match step {
                0 => Ok(NativeStreamItem::Message(Box::new(
                    endpoint::HandleResponse {
                        kind: endpoint::HandleResponseKind::Head,
                        status: Some(200),
                        body: None,
                        headers: Some(vec![endpoint::HandleResponseHeadersItem {
                            name: "content-type".into(),
                            value: "application/octet-stream".into(),
                        }]),
                    },
                ))),
                1 => Ok(chunk(b"first\0\xff".to_vec())),
                2 if mode == "hold" => {
                    context.cancellation().cancelled().await;
                    Err(RuntimeFailure::Cancelled {
                        request_id: context.request_id(),
                    })
                }
                2 if mode == "fail" => Err(RuntimeFailure::PluginFailure {
                    detail: "fixture terminal failure".into(),
                }),
                2 if mode == "domain" => Ok(NativeStreamItem::Terminal(Err(Box::new(
                    endpoint::HandleError::Rejected,
                )))),
                2 if mode == "missing-terminal" => Ok(NativeStreamItem::PeerHalfClosed),
                2 if mode == "oversize" => Ok(chunk(vec![b'x'; 65_537])),
                2 if mode == "half-close" => Ok(NativeStreamItem::PeerHalfClosed),
                2..=8 if mode == "many" => Ok(chunk(b"second".to_vec())),
                2 => Ok(chunk(b"second".to_vec())),
                _ if mode == "missing-terminal" => {
                    context.cancellation().cancelled().await;
                    Err(RuntimeFailure::Cancelled {
                        request_id: context.request_id(),
                    })
                }
                _ => Ok(NativeStreamItem::Terminal(Ok(()))),
            }
        })
    }
}
