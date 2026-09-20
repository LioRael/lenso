//! Explicit, one-shot fault injection at test-owned boundaries.

use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    rc::Rc,
};

/// A small, target-neutral vocabulary for normal scenario fault boundaries.
///
/// Callers can still use a project-specific bounded label through
/// [`FaultInjector::inject`], but this type covers the lifecycle boundaries
/// that every deterministic scenario should name consistently.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScenarioBoundary {
    /// Immediately before the business operation starts.
    BeforeOperation,
    /// Immediately after a durable mutation commits.
    AfterDurableCommit,
    /// Immediately before the response is projected to the caller.
    BeforeResponse,
    /// While a streaming response is in progress.
    DuringResponse,
    /// At the scenario deadline boundary.
    Timeout,
    /// At cooperative cancellation.
    Cancellation,
    /// When a client connection drops.
    DroppedConnection,
    /// When generation-owned cleanup runs.
    Cleanup,
    /// When a required resource is acquired.
    ResourceAcquire,
}

impl ScenarioBoundary {
    const fn label(self) -> &'static str {
        match self {
            Self::BeforeOperation => "before-operation",
            Self::AfterDurableCommit => "after-durable-commit",
            Self::BeforeResponse => "before-response",
            Self::DuringResponse => "during-response",
            Self::Timeout => "timeout",
            Self::Cancellation => "cancellation",
            Self::DroppedConnection => "dropped-connection",
            Self::Cleanup => "cleanup",
            Self::ResourceAcquire => "resource-acquire",
        }
    }
}

/// A bounded failure condition that a test may inject at an explicit boundary.
///
/// The receiving Adapter or Plugin test support decides how to project this
/// condition into its domain. The simulator does not invent transport, storage,
/// or cleanup behavior on its behalf.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SimulatorFault {
    /// The operation's configured deadline elapsed.
    Timeout,
    /// The caller or generation requested cooperative cancellation.
    Cancellation,
    /// The peer disconnected before it observed a response.
    DroppedConnection,
    /// Generation-owned cleanup reported a failure.
    CleanupFailure,
    /// A required private resource could not be acquired.
    ResourceUnavailable,
}

/// Error returned when a caller names an unsafe or unstable scenario boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FaultPointError;

impl std::fmt::Display for FaultPointError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("fault point must be a bounded ASCII scenario label")
    }
}

impl std::error::Error for FaultPointError {}

/// Test-owned, FIFO fault queues indexed by a stable boundary label.
///
/// A queued fault is consumed only when [`Self::check`] reaches the matching
/// boundary. No fault is implicit, and an empty queue always permits work.
#[derive(Clone, Debug, Default)]
pub struct FaultInjector {
    queues: Rc<RefCell<BTreeMap<String, VecDeque<SimulatorFault>>>>,
}

impl FaultInjector {
    /// Creates an empty injector that permits every boundary.
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues one fault for the next visit to `point`.
    pub fn inject(&self, point: &str, fault: SimulatorFault) -> Result<(), FaultPointError> {
        validate_point(point)?;
        self.queues
            .borrow_mut()
            .entry(point.to_owned())
            .or_default()
            .push_back(fault);
        Ok(())
    }

    /// Queues one fault at a canonical boundary belonging to `operation`.
    pub fn inject_at(
        &self,
        operation: &str,
        boundary: ScenarioBoundary,
        fault: SimulatorFault,
    ) -> Result<(), FaultPointError> {
        self.inject(&format!("{operation}.{}", boundary.label()), fault)
    }

    /// Consumes and returns the next configured fault at `point`, if any.
    pub fn check(&self, point: &str) -> Result<Result<(), SimulatorFault>, FaultPointError> {
        validate_point(point)?;
        let mut queues = self.queues.borrow_mut();
        let fault = queues.get_mut(point).and_then(VecDeque::pop_front);
        if queues.get(point).is_some_and(VecDeque::is_empty) {
            queues.remove(point);
        }
        Ok(fault.map_or(Ok(()), Err))
    }

    /// Checks the next fault at a canonical boundary belonging to `operation`.
    pub fn check_at(
        &self,
        operation: &str,
        boundary: ScenarioBoundary,
    ) -> Result<Result<(), SimulatorFault>, FaultPointError> {
        self.check(&format!("{operation}.{}", boundary.label()))
    }

    /// Returns whether a boundary still has queued faults.
    pub fn has_pending(&self, point: &str) -> Result<bool, FaultPointError> {
        validate_point(point)?;
        Ok(self
            .queues
            .borrow()
            .get(point)
            .is_some_and(|queue| !queue.is_empty()))
    }
}

pub(crate) fn validate_point(point: &str) -> Result<(), FaultPointError> {
    if point.is_empty()
        || point.len() > 128
        || !point
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'))
    {
        return Err(FaultPointError);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faults_are_consumed_once_in_test_selected_order() {
        let faults = FaultInjector::new();
        faults
            .inject(
                "oauth.after-durable-commit",
                SimulatorFault::DroppedConnection,
            )
            .unwrap();
        faults
            .inject("oauth.after-durable-commit", SimulatorFault::Timeout)
            .unwrap();

        assert!(faults.has_pending("oauth.after-durable-commit").unwrap());
        assert_eq!(
            faults.check("oauth.after-durable-commit").unwrap(),
            Err(SimulatorFault::DroppedConnection)
        );
        assert_eq!(
            faults.check("oauth.after-durable-commit").unwrap(),
            Err(SimulatorFault::Timeout)
        );
        assert_eq!(faults.check("oauth.after-durable-commit").unwrap(), Ok(()));
        assert!(!faults.has_pending("oauth.after-durable-commit").unwrap());
    }

    #[test]
    fn canonical_boundaries_do_not_hide_the_operation_identity() {
        let faults = FaultInjector::new();
        faults
            .inject_at(
                "oauth.consume",
                ScenarioBoundary::AfterDurableCommit,
                SimulatorFault::DroppedConnection,
            )
            .unwrap();

        assert_eq!(
            faults
                .check_at("oauth.consume", ScenarioBoundary::AfterDurableCommit)
                .unwrap(),
            Err(SimulatorFault::DroppedConnection)
        );
        assert_eq!(
            faults
                .check_at("oauth.create", ScenarioBoundary::AfterDurableCommit)
                .unwrap(),
            Ok(())
        );
    }

    #[test]
    fn fault_points_reject_unbounded_or_sensitive_shape() {
        let faults = FaultInjector::new();

        assert_eq!(
            faults.inject("before response", SimulatorFault::Timeout),
            Err(FaultPointError)
        );
        assert_eq!(faults.check(""), Err(FaultPointError));
    }
}
