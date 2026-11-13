use std::{fmt, future::Future, pin::Pin};

use crate::ActionError;

type Listener<T> =
    Box<dyn Fn(T) -> Pin<Box<dyn Future<Output = Result<(), ActionError>> + Send>> + Send + Sync>;

/// Process-local notification with awaited listeners in registration order.
pub struct Event<T> {
    id: String,
    next_id: u64,
    listeners: Vec<(u64, Listener<T>)>,
}

impl<T> fmt::Debug for Event<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Event")
            .field("id", &self.id)
            .field("listeners", &self.listeners.len())
            .finish()
    }
}

impl<T> Event<T> {
    /// Creates a local event; delivery is not persisted across process restarts.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            next_id: 0,
            listeners: Vec::new(),
        }
    }

    /// Returns the stable event identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Registers a listener and returns a subscription identifier local to this event.
    pub fn subscribe<F, Fut>(&mut self, listener: F) -> u64
    where
        F: Fn(T) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), ActionError>> + Send + 'static,
    {
        let id = self.next_id;
        self.next_id += 1;
        self.listeners
            .push((id, Box::new(move |value| Box::pin(listener(value)))));
        id
    }

    /// Removes a subscription; future publications no longer invoke it.
    pub fn unsubscribe(&mut self, id: u64) {
        self.listeners
            .retain(|(subscription, _)| *subscription != id);
    }

    /// Publishes a native value without serialization, cloning it for each subscriber.
    ///
    /// # Errors
    /// Stops at the first listener failure; completed effects are not rolled back.
    pub async fn publish(&self, value: T) -> Result<(), ActionError>
    where
        T: Clone + Send,
    {
        for (_, listener) in &self.listeners {
            listener(value.clone()).await?;
        }
        Ok(())
    }
}
