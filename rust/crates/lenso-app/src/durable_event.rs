use std::fmt;

use crate::{ActionError, ErrorCode, Task};

/// A prepared subscriber job; queue drivers own persistence, transactions and delivery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventDelivery {
    /// Stable subscription identifier within the event definition.
    pub subscription: String,
    /// Durable task kind responsible for handling this subscription.
    pub task: String,
    /// Encoded task input derived by the subscriber, not the publisher.
    pub payload: String,
}

impl EventDelivery {
    /// Prepares a low-level delivery from a typed task and native input.
    ///
    /// # Errors
    /// Returns `invalid_input` when the payload cannot be serialized.
    pub fn new<R, A: Task<R>>(
        subscription: impl Into<String>,
        task: &A,
        input: &A::Input,
    ) -> Result<Self, ActionError> {
        Ok(Self {
            subscription: subscription.into(),
            task: task.id().to_owned(),
            payload: serde_json::to_string(input).map_err(|cause| {
                ActionError::new(ErrorCode::InvalidInput, "Invalid event payload")
                    .with_source(cause)
            })?,
        })
    }
}

type Prepare<T> = Box<dyn Fn(&T) -> Result<String, ActionError> + Send + Sync>;
struct Subscription<T> {
    id: String,
    task: String,
    prepare: Prepare<T>,
}

/// Typed durable event whose subscribers map native event values to their own task inputs.
pub struct DurableEvent<T> {
    id: String,
    subscriptions: Vec<Subscription<T>>,
}

impl<T> fmt::Debug for DurableEvent<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DurableEvent")
            .field("id", &self.id)
            .field("subscriptions", &self.subscriptions.len())
            .finish()
    }
}

impl<T> DurableEvent<T> {
    /// Creates an event definition without opening a connection or starting a worker.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            subscriptions: Vec::new(),
        }
    }

    /// Returns the event identifier shared by producers and subscribers.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Connects a named subscription to a typed task without making producers depend on that task.
    ///
    /// # Errors
    /// Returns `conflict` when the subscription identifier is already installed.
    pub fn subscribe<R, A: Task<R>>(
        &mut self,
        id: impl Into<String>,
        task: &A,
        input: impl Fn(&T) -> A::Input + Send + Sync + 'static,
    ) -> Result<(), ActionError> {
        let id = id.into();
        if self
            .subscriptions
            .iter()
            .any(|subscription| subscription.id == id)
        {
            return Err(ActionError::new(
                ErrorCode::Conflict,
                "Duplicate event subscription",
            ));
        }
        self.subscriptions.push(Subscription {
            id,
            task: task.id().to_owned(),
            prepare: Box::new(move |value| {
                serde_json::to_string(&input(value)).map_err(|cause| {
                    ActionError::new(ErrorCode::InvalidInput, "Invalid event payload")
                        .with_source(cause)
                })
            }),
        });
        Ok(())
    }

    /// Removes a subscription from subsequent publications.
    pub fn unsubscribe(&mut self, id: &str) {
        self.subscriptions
            .retain(|subscription| subscription.id != id);
    }

    /// Prepares subscriber inputs before a driver starts its persistence transaction.
    ///
    /// # Errors
    /// Returns serialization or subscriber mapping failures without persisting any jobs.
    pub fn deliveries(&self, value: &T) -> Result<Vec<EventDelivery>, ActionError> {
        self.subscriptions
            .iter()
            .map(|subscription| {
                Ok(EventDelivery {
                    subscription: subscription.id.clone(),
                    task: subscription.task.clone(),
                    payload: (subscription.prepare)(value)?,
                })
            })
            .collect()
    }
}
