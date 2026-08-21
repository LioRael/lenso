//! Public Capability contract for one owned counter state Module.

use std::{any::Any, fmt, rc::Rc};

use futures::future::LocalBoxFuture;
use lenso_kernel::{InvocationContext, NativeRequestEndpoint, RuntimeFailure};

type ErasedCounterResult = Result<Result<Box<dyn Any>, Box<dyn Any>>, RuntimeFailure>;

#[allow(dead_code)]
mod generated {
    include!("generated.rs");
}

pub use generated::{
    CAPABILITY_ID, CounterClient, CounterEndpoint, CounterIncrement,
    CounterIncrementInvocationError, CounterProvider, CounterRead, CounterReadInvocationError,
    DESCRIPTOR_VERSION, INCREMENT_OPERATION, IncrementError, IncrementRequest, IncrementResponse,
    READ_OPERATION, ReadError, ReadRequest, ReadResponse, UnknownDomainError,
    decode_increment_error, decode_increment_request, decode_increment_response, decode_read_error,
    decode_read_request, decode_read_response, encode_increment_error, encode_increment_request,
    encode_increment_response, encode_read_error, encode_read_request, encode_read_response,
};

/// A native runtime provider may report a typed Domain Error or an execution failure.
///
/// The generated [`CounterProvider`] remains the canonical Domain-only binding. This
/// adapter is for native implementations whose host work can also return a
/// [`RuntimeFailure`].
pub trait CounterRuntimeProvider: fmt::Debug + 'static {
    /// Reads one durable counter value.
    fn read(
        &self,
        context: InvocationContext,
        request: ReadRequest,
    ) -> LocalBoxFuture<'static, Result<ReadResponse, CounterReadInvocationError>>;

    /// Atomically increments one durable counter value.
    fn increment(
        &self,
        context: InvocationContext,
        request: IncrementRequest,
    ) -> LocalBoxFuture<'static, Result<IncrementResponse, CounterIncrementInvocationError>>;
}

/// Native endpoint adapter that keeps storage failures distinct from Domain Errors.
#[derive(Debug)]
pub struct CounterRuntimeEndpoint<P> {
    provider: Rc<P>,
}

impl<P: CounterRuntimeProvider> CounterRuntimeEndpoint<P> {
    /// Creates a counter endpoint around one Module-owned provider.
    pub fn new(provider: P) -> Self {
        Self {
            provider: Rc::new(provider),
        }
    }
}

impl<P: CounterRuntimeProvider> NativeRequestEndpoint for CounterRuntimeEndpoint<P> {
    fn capability_id(&self) -> &'static str {
        CAPABILITY_ID
    }

    fn descriptor_version(&self) -> &'static str {
        DESCRIPTOR_VERSION
    }

    fn operations(&self) -> &'static [&'static str] {
        &["read", "increment"]
    }

    fn invoke(
        &self,
        operation: &str,
        request: Box<dyn Any>,
        context: InvocationContext,
    ) -> LocalBoxFuture<'static, ErasedCounterResult> {
        match operation {
            "read" => {
                let Ok(request) = request.downcast::<ReadRequest>() else {
                    return Box::pin(futures::future::ready(Err(
                        RuntimeFailure::ProtocolViolation {
                            capability: CAPABILITY_ID,
                        },
                    )));
                };
                let provider = Rc::clone(&self.provider);
                Box::pin(async move { map_provider_result(provider.read(context, *request).await) })
            }
            "increment" => {
                let Ok(request) = request.downcast::<IncrementRequest>() else {
                    return Box::pin(futures::future::ready(Err(
                        RuntimeFailure::ProtocolViolation {
                            capability: CAPABILITY_ID,
                        },
                    )));
                };
                let provider = Rc::clone(&self.provider);
                Box::pin(async move {
                    map_increment_result(provider.increment(context, *request).await)
                })
            }
            _ => Box::pin(futures::future::ready(Err(
                RuntimeFailure::UnknownOperation {
                    capability: CAPABILITY_ID,
                    operation: operation.to_owned(),
                },
            ))),
        }
    }
}

fn map_provider_result<T: 'static>(
    result: Result<T, CounterReadInvocationError>,
) -> ErasedCounterResult {
    match result {
        Ok(value) => Ok(Ok(Box::new(value))),
        Err(CounterReadInvocationError::Domain(error)) => Ok(Err(Box::new(error))),
        Err(CounterReadInvocationError::Runtime(error)) => Err(error),
    }
}

fn map_increment_result(
    result: Result<IncrementResponse, CounterIncrementInvocationError>,
) -> ErasedCounterResult {
    match result {
        Ok(value) => Ok(Ok(Box::new(value))),
        Err(CounterIncrementInvocationError::Domain(error)) => Ok(Err(Box::new(error))),
        Err(CounterIncrementInvocationError::Runtime(error)) => Err(error),
    }
}
