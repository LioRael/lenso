#![allow(
    dead_code,
    clippy::single_match_else,
    clippy::unused_async,
    clippy::unused_async_trait_impl
)]

include!("../../generated/notifications.rs");

#[doc(hidden)]
mod __native_support {
    pub use futures::future::LocalBoxFuture;
    pub use lenso_kernel::{InvocationContext, RuntimeFailure};
}

#[derive(Clone, Debug)]
struct AsyncNotifications;

impl AsyncNotifications {
    async fn notify(
        &self,
        _context: InvocationContext,
        event: NotifyRequest,
    ) -> Result<(), RuntimeFailure> {
        if event.message == "fail" {
            Err(RuntimeFailure::PluginFailure {
                detail: "notification handler failed".to_owned(),
            })
        } else {
            Ok(())
        }
    }
}

__lenso_native_lower_notifications!(AsyncNotifications, __native_support);

#[cfg(test)]
mod tests {
    use std::any::Any;

    use futures::executor::block_on;
    use lenso_kernel::{CancellationToken, NativeEventEndpoint};

    use super::*;

    fn context() -> InvocationContext {
        InvocationContext::new(1, None, CancellationToken::new())
    }

    #[test]
    fn generated_event_lowering_awaits_typed_handlers_and_preserves_runtime_failures() {
        let endpoint = NotificationsEndpoint::new(AsyncNotifications);
        block_on(endpoint.publish(
            NOTIFY_OPERATION,
            Box::new(NotifyRequest {
                message: "accepted".to_owned(),
                sequence: 1,
            }),
            context(),
        ))
        .expect("typed Event should reach the async provider");

        let error = block_on(endpoint.publish(
            NOTIFY_OPERATION,
            Box::new(NotifyRequest {
                message: "fail".to_owned(),
                sequence: 2,
            }),
            context(),
        ))
        .expect_err("provider Runtime Failure should remain observable to the Adapter");
        assert!(matches!(error, RuntimeFailure::PluginFailure { .. }));
    }

    #[test]
    fn generated_event_endpoint_rejects_the_wrong_erased_type() {
        let endpoint = NotificationsEndpoint::new(AsyncNotifications);
        let event: Box<dyn Any> = Box::new("not a notification".to_owned());
        let error = block_on(endpoint.publish(NOTIFY_OPERATION, event, context()))
            .expect_err("wrong Event types must fail closed");
        assert_eq!(
            error,
            RuntimeFailure::ProtocolViolation {
                capability: CAPABILITY_ID,
            }
        );
    }
}
