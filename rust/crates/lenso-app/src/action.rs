use std::{
    future::Future,
    time::{Duration, Instant},
};

use crate::{ActionError, CancellationToken, ErrorCode, Principal};

/// Per-call identity and native resources, created by a trusted application entrypoint.
#[derive(Debug, Clone)]
pub struct Context<R> {
    /// Correlation identifier assigned by the entrypoint.
    pub call_id: String,
    /// Authenticated identity, or absence for an anonymous caller.
    pub principal: Option<Principal>,
    /// Application-owned resources; no erased service locator is required.
    pub resources: R,
    /// Cooperative cancellation shared with handlers and resource adapters.
    pub cancellation: CancellationToken,
}

impl<R> Context<R> {
    /// Checks cancellation without implying rollback or interrupting committed effects.
    ///
    /// # Errors
    /// Returns `cancelled` when the caller has requested cancellation.
    pub fn check_cancelled(&self) -> Result<(), ActionError> {
        if self.cancellation.is_cancelled() {
            Err(ActionError::new(ErrorCode::Cancelled, "Call cancelled"))
        } else {
            Ok(())
        }
    }
}

/// A native operation. Serialization is required only by an external adapter.
pub trait Action<R>: Sync {
    /// Native input, including types that have no serialized representation.
    type Input: Send + Sync;
    /// Native result returned directly to same-language callers.
    type Output: Send;

    /// Stable namespaced operation identifier.
    fn id(&self) -> &str;

    /// Validates domain invariants after external decoding or native construction.
    ///
    /// # Errors
    /// Returns `invalid_input` for rejected domain input.
    fn validate_input(&self, _input: &Self::Input) -> Result<(), ActionError> {
        Ok(())
    }

    /// Checks access before executing effects. Public operations explicitly return success.
    fn authorize(
        &self,
        context: &Context<R>,
        input: &Self::Input,
    ) -> impl Future<Output = Result<(), ActionError>> + Send;

    /// Executes business logic after authorization; callers should use [`Executor::call`].
    fn run(
        &self,
        context: &Context<R>,
        input: Self::Input,
    ) -> impl Future<Output = Result<Self::Output, ActionError>> + Send;

    /// Checks implementation invariants before returning output to a caller.
    ///
    /// # Errors
    /// Returns `internal` when the handler produces an invalid result.
    fn validate_output(&self, _output: &Self::Output) -> Result<(), ActionError> {
        Ok(())
    }
}

/// A completed call summary, deliberately excluding business input, output and secrets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallRecord {
    /// Entrypoint-assigned correlation identifier.
    pub call_id: String,
    /// Stable action identifier.
    pub action: String,
    /// Authenticated subject, when present.
    pub subject: Option<String>,
    /// Success or a stable failure classification.
    pub outcome: Result<(), ErrorCode>,
    /// Wall-clock time measured with a monotonic clock.
    pub duration: Duration,
}

/// An infallible synchronous record sink; implementations must not panic and own their latency.
pub trait Observer: Sync {
    /// Receives one summary for every completed invocation.
    fn record(&self, record: CallRecord);
}

impl<F: Fn(CallRecord) + Sync> Observer for F {
    fn record(&self, record: CallRecord) {
        self(record);
    }
}

/// Executes native and external calls through the same validation and access pipeline.
#[derive(Debug, Clone)]
pub struct Executor<O> {
    observer: O,
}

impl<O: Observer> Executor<O> {
    #[cfg(feature = "json")]
    pub(crate) fn observer(&self) -> &dyn Observer {
        &self.observer
    }
    /// Creates an executor with an application-owned record sink.
    pub fn new(observer: O) -> Self {
        Self { observer }
    }

    /// Executes a typed native operation without a JSON round trip.
    ///
    /// # Errors
    /// Returns validation, authorization, cancellation or handler failures.
    pub async fn call<R: Sync, A: Action<R>>(
        &self,
        action: &A,
        context: &Context<R>,
        input: A::Input,
    ) -> Result<A::Output, ActionError> {
        self.invoke(action, context, || Ok(input), Ok).await
    }

    /// Decodes and encodes JSON only at an explicitly selected external interface.
    ///
    /// # Errors
    /// Also returns `invalid_input` for decoding and `internal` for encoding failures.
    #[cfg(feature = "json")]
    pub async fn call_json<R: Sync, A: Action<R>>(
        &self,
        action: &A,
        context: &Context<R>,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ActionError>
    where
        A::Input: serde::de::DeserializeOwned,
        A::Output: serde::Serialize,
    {
        self.invoke(
            action,
            context,
            || {
                serde_json::from_value(input).map_err(|error| {
                    ActionError::new(ErrorCode::InvalidInput, "Invalid input").with_source(error)
                })
            },
            |output| {
                serde_json::to_value(output).map_err(|error| {
                    ActionError::new(ErrorCode::Internal, "Invalid output").with_source(error)
                })
            },
        )
        .await
    }

    async fn invoke<R: Sync, A: Action<R>, T>(
        &self,
        action: &A,
        context: &Context<R>,
        decode: impl FnOnce() -> Result<A::Input, ActionError>,
        encode: impl FnOnce(A::Output) -> Result<T, ActionError>,
    ) -> Result<T, ActionError> {
        let start = Instant::now();
        let result = async {
            context.check_cancelled()?;
            let input = decode()?;
            action.validate_input(&input)?;
            action.authorize(context, &input).await?;
            context.check_cancelled()?;
            let output = action.run(context, input).await?;
            action.validate_output(&output).map_err(|error| {
                if error.code() == ErrorCode::Internal {
                    error
                } else {
                    ActionError::new(ErrorCode::Internal, "Invalid output").with_source(error)
                }
            })?;
            encode(output)
        }
        .await;
        self.observer.record(CallRecord {
            call_id: context.call_id.clone(),
            action: action.id().to_owned(),
            subject: context
                .principal
                .as_ref()
                .map(|principal| principal.subject.clone()),
            outcome: result
                .as_ref()
                .map(|_| ())
                .map_err(|error: &ActionError| error.code()),
            duration: start.elapsed(),
        });
        result
    }
}
