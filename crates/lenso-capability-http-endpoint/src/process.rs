//! Request-only Process v1 serving for an authored HTTP Endpoint.
//!
//! Use `#[endpoint(standalone)]` to generate the same routes and typed dispatch
//! without linked Host registration, then call [`serve`] with the owned endpoint.
//! The v1 SDK executes requests serially; endpoints may share state with
//! `Rc<RefCell<_>>` and do not need `Send` or `Sync`.
//!
//! Process v1 carries no Kernel invocation context. Each typed call receives a
//! fresh, invocation-local context with a checked monotonic ID, no caller,
//! deadline, budget, ordinary extensions, or sealed authority. Its cancellation
//! token is local: Host cancellation is not propagated by v1. This context is
//! not Host authority, and HTTP credentials are never converted into grants.
//! Futures run with `futures::executor::block_on`, not a Tokio runtime.
//! Host Capability dependencies and instance configuration are not injected.

use std::{cell::Cell, io};

use futures::executor::block_on;
use lenso_kernel::{CancellationToken, InvocationContext, RuntimeFailure};
use lenso_process_sdk::{ProcessOutcome, ProcessPlugin};
use lenso_runtime_codec::{JSON_REQUEST_ABI_V1, JsonCapabilityDescriptor, JsonPluginDescriptor};
use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    CAPABILITY_ID, DESCRIBE_OPERATION, DESCRIPTOR_VERSION, DescribeRequest, EndpointProvider,
    HANDLE_OPERATION, HandleRequest, HttpEndpoint,
};

/// Serves an owned Endpoint on the reserved Process v1 stdin/stdout transport.
///
/// Do not write application output to stdout; use stderr for diagnostics.
pub fn serve<P: HttpEndpoint>(endpoint: P) -> io::Result<()> {
    lenso_process_sdk::serve(&ProcessEndpoint::new(endpoint))
}

/// Adapts typed Endpoint operations to the synchronous, request-only v1 SDK.
///
/// One wrapper retains one provider and one local invocation-ID sequence.
/// It deliberately does not implement the concurrent Authoring V2 interface.
#[derive(Debug)]
pub struct ProcessEndpoint<P> {
    endpoint: P,
    last_invocation_id: Cell<u64>,
}

impl<P: HttpEndpoint> ProcessEndpoint<P> {
    /// Retains an Endpoint for serial invocations.
    ///
    /// Generated dispatch clones the provider for each call. Persistent mutable
    /// state must be shared across those clones, as described by [`HttpEndpoint`].
    #[must_use]
    pub const fn new(endpoint: P) -> Self {
        Self {
            endpoint,
            last_invocation_id: Cell::new(0),
        }
    }

    fn local_context(&self) -> Result<InvocationContext, String> {
        let id = self
            .last_invocation_id
            .get()
            .checked_add(1)
            .ok_or_else(|| "Process Endpoint invocation IDs exhausted".to_owned())?;
        self.last_invocation_id.set(id);
        Ok(InvocationContext::new(id, None, CancellationToken::new()))
    }
}

impl<P: HttpEndpoint> ProcessPlugin for ProcessEndpoint<P> {
    fn descriptor(&self) -> Value {
        json!(JsonPluginDescriptor {
            abi: JSON_REQUEST_ABI_V1.to_owned(),
            capabilities: vec![JsonCapabilityDescriptor {
                capability_id: CAPABILITY_ID.to_owned(),
                descriptor_version: DESCRIPTOR_VERSION.to_owned(),
                request_operations: vec![
                    DESCRIBE_OPERATION.to_owned(),
                    HANDLE_OPERATION.to_owned()
                ],
                stream_operations: Vec::new(),
            }],
            required_capabilities: Vec::new(),
        })
    }

    fn invoke(&self, capability: &str, operation: &str, request: Value) -> ProcessOutcome {
        if capability != CAPABILITY_ID {
            return ProcessOutcome::Failure(format!("unknown Endpoint capability `{capability}`"));
        }
        // Decode before allocating a context or entering any provider code.
        match operation {
            DESCRIBE_OPERATION => {
                let request = match serde_json::from_value::<DescribeRequest>(request) {
                    Ok(request) => request,
                    Err(error) => return ProcessOutcome::Failure(error.to_string()),
                };
                let context = match self.local_context() {
                    Ok(context) => context,
                    Err(error) => return ProcessOutcome::Failure(error),
                };
                encode_outcome(block_on(self.endpoint.describe(context, request)))
            }
            HANDLE_OPERATION => {
                let request = match serde_json::from_value::<HandleRequest>(request) {
                    Ok(request) => request,
                    Err(error) => return ProcessOutcome::Failure(error.to_string()),
                };
                let context = match self.local_context() {
                    Ok(context) => context,
                    Err(error) => return ProcessOutcome::Failure(error),
                };
                encode_outcome(block_on(self.endpoint.handle(context, request)))
            }
            _ => ProcessOutcome::Failure(format!("unknown Endpoint operation `{operation}`")),
        }
    }
}

fn encode_outcome<T: Serialize, E: Serialize>(
    outcome: Result<Result<T, E>, RuntimeFailure>,
) -> ProcessOutcome {
    let (value, domain_error) = match outcome {
        Ok(Ok(response)) => (serde_json::to_value(response), false),
        Ok(Err(error)) => (serde_json::to_value(error), true),
        Err(error) => return ProcessOutcome::Failure(format!("{error:?}")),
    };
    match value {
        Ok(value) if domain_error => ProcessOutcome::DomainError(value),
        Ok(value) => ProcessOutcome::Success(value),
        Err(error) => ProcessOutcome::Failure(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EndpointFuture, EndpointRoute};

    #[derive(Clone, Debug)]
    struct NeverCalled;

    impl HttpEndpoint for NeverCalled {
        const ROUTES: &'static [EndpointRoute] = &[EndpointRoute::new("test", "GET", "/")];

        fn dispatch(&self, _: InvocationContext, _: HandleRequest) -> EndpointFuture {
            panic!("exhausted IDs must never dispatch");
        }
    }

    #[test]
    fn exhausted_local_ids_fail_closed_without_reuse() {
        let process = ProcessEndpoint::new(NeverCalled);
        process.last_invocation_id.set(u64::MAX);
        for _ in 0..2 {
            assert!(matches!(
                process.invoke(CAPABILITY_ID, DESCRIBE_OPERATION, json!({})),
                ProcessOutcome::Failure(_)
            ));
        }
        assert_eq!(process.last_invocation_id.get(), u64::MAX);
    }
}
