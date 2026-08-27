use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::{
    DEFAULT_EVENT_QUEUE_CAPACITY, DEFAULT_REQUEST_MAX_CONCURRENCY, DEFAULT_REQUEST_QUEUE_CAPACITY,
    PlanResolutionError,
};

/// The cardinality of one Plugin's Capability requirement.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityCardinality {
    /// Exactly one provider must be bound.
    One,
    /// Zero or one provider may be bound.
    Optional,
    /// Zero or more providers may be bound in deterministic order.
    Many,
}

/// The transport-independent interaction semantics of one Capability Operation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityOperationKind {
    /// One request produces one response or Domain Error.
    Request,
    /// One open establishes an ordered, bidirectional stream session.
    Stream,
    /// One publication is delivered to zero or more subscribers.
    Event,
}

/// The bounded admission policy materialized for one request Operation.
///
/// `queue_capacity` counts requests waiting for one of the
/// `max_concurrency` execution slots. A zero queue capacity is valid and
/// makes admission fail immediately while all execution slots are occupied.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RequestAdmissionPlan {
    queue_capacity: usize,
    max_concurrency: usize,
}

impl RequestAdmissionPlan {
    /// Creates a bounded request admission policy.
    pub const fn new(queue_capacity: usize, max_concurrency: usize) -> Self {
        Self {
            queue_capacity,
            max_concurrency,
        }
    }

    /// Returns the maximum number of requests waiting for an execution slot.
    pub const fn queue_capacity(self) -> usize {
        self.queue_capacity
    }

    /// Returns the maximum number of requests executing concurrently.
    pub const fn max_concurrency(self) -> usize {
        self.max_concurrency
    }

    pub(super) fn validate(
        self,
        capability_id: &str,
        operation: &str,
    ) -> Result<(), PlanResolutionError> {
        if self.max_concurrency == 0 {
            return Err(PlanResolutionError::InvalidRequestAdmission {
                capability_id: capability_id.to_owned(),
                operation: operation.to_owned(),
                queue_capacity: self.queue_capacity,
                max_concurrency: self.max_concurrency,
            });
        }
        Ok(())
    }
}

impl Default for RequestAdmissionPlan {
    fn default() -> Self {
        Self::new(
            DEFAULT_REQUEST_QUEUE_CAPACITY,
            DEFAULT_REQUEST_MAX_CONCURRENCY,
        )
    }
}

/// The bounded volatile mailbox policy materialized for one Event binding.
///
/// Capacity counts all accepted Events that have not completed handling. Zero
/// is valid and makes every publication to the binding report exhausted.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EventAdmissionPlan {
    capacity: usize,
}

impl EventAdmissionPlan {
    /// Creates one Event mailbox policy.
    pub const fn new(capacity: usize) -> Self {
        Self { capacity }
    }

    /// Returns the maximum number of accepted Events retained by the binding.
    pub const fn capacity(self) -> usize {
        self.capacity
    }
}

impl Default for EventAdmissionPlan {
    fn default() -> Self {
        Self::new(DEFAULT_EVENT_QUEUE_CAPACITY)
    }
}

/// The finite restart mode selected for one Plugin Instance.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RestartMode {
    /// Do not recreate a failed generation.
    Never,
    /// Recreate failed generations within a bounded attempt window.
    OnFailure,
}

/// Bounded supervision settings materialized in the Resolved App Plan.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RestartPolicy {
    mode: RestartMode,
    max_attempts: usize,
    window: Duration,
    backoff: Duration,
    jitter: Duration,
    stability: Duration,
}

impl RestartPolicy {
    /// Creates a policy that never recreates a failed generation.
    pub const fn never() -> Self {
        Self {
            mode: RestartMode::Never,
            max_attempts: 0,
            window: Duration::ZERO,
            backoff: Duration::ZERO,
            jitter: Duration::ZERO,
            stability: Duration::ZERO,
        }
    }

    /// Creates a finite on-failure policy.
    pub const fn on_failure(
        max_attempts: usize,
        window: Duration,
        backoff: Duration,
        jitter: Duration,
        stability: Duration,
    ) -> Self {
        Self {
            mode: RestartMode::OnFailure,
            max_attempts,
            window,
            backoff,
            jitter,
            stability,
        }
    }

    /// Returns the selected restart mode.
    pub const fn mode(self) -> RestartMode {
        self.mode
    }

    /// Returns the maximum number of recreation attempts in one window.
    pub const fn max_attempts(self) -> usize {
        self.max_attempts
    }

    /// Returns the rolling attempt window.
    pub const fn window(self) -> Duration {
        self.window
    }

    /// Returns the initial exponential backoff duration.
    pub const fn backoff(self) -> Duration {
        self.backoff
    }

    /// Returns the maximum jitter requested from the Runtime Driver.
    pub const fn jitter(self) -> Duration {
        self.jitter
    }

    /// Returns the ready period after which the attempt budget becomes stable again.
    pub const fn stability(self) -> Duration {
        self.stability
    }

    pub(super) fn validate(&self, instance_key: &str) -> Result<(), PlanResolutionError> {
        if self.mode == RestartMode::OnFailure && (self.max_attempts == 0 || self.window.is_zero())
        {
            return Err(PlanResolutionError::InvalidRestartPolicy {
                instance_key: instance_key.to_owned(),
                max_attempts: self.max_attempts,
                window: self.window,
            });
        }
        Ok(())
    }
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self::never()
    }
}

/// Whether a failed Plugin Instance is allowed to remain unavailable.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginCriticality {
    /// Exhaustion leaves this Plugin unavailable when it is not required by a `one` binding.
    #[default]
    NonCritical,
    /// Exhaustion fails the App even when no `one` binding reaches this Plugin.
    Critical,
}

impl PluginCriticality {
    /// Returns whether this criticality requires a terminal App outcome on exhaustion.
    pub const fn is_critical(self) -> bool {
        matches!(self, Self::Critical)
    }
}
