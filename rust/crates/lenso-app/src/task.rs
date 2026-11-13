use std::future::Future;

use serde::{Serialize, de::DeserializeOwned};

use crate::{ActionError, CancellationToken};

/// Native resources and execution identity for one durable task attempt.
#[derive(Debug)]
pub struct TaskContext<'a, R> {
    /// Stable identifier used by handlers to deduplicate effects across retries.
    pub job_id: &'a str,
    /// One-based attempt number, including recovered expired leases.
    pub attempt: u32,
    /// Application-owned native dependencies.
    pub resources: &'a R,
    /// Cooperative cancellation, renewed by the queue driver while awaiting the handler.
    pub cancellation: CancellationToken,
}

/// A durable operation whose payload and result explicitly support serialization.
pub trait Task<R>: Sync {
    /// Persisted input, decoded anew on each attempt.
    type Input: Serialize + DeserializeOwned + Send;
    /// Persisted result returned to status readers.
    type Output: Serialize + DeserializeOwned + Send;

    /// Stable task kind shared by producers and workers.
    fn id(&self) -> &str;

    /// Executes one attempt; externally visible effects must tolerate at-least-once delivery.
    fn run(
        &self,
        context: &TaskContext<'_, R>,
        input: Self::Input,
    ) -> impl Future<Output = Result<Self::Output, ActionError>> + Send;
}
