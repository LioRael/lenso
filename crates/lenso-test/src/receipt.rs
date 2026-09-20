//! Stable, bounded scenario receipts for deterministic test evidence.

use std::{cell::RefCell, rc::Rc, time::Duration};

use crate::{FaultPointError, SimulatorFault, TestSimulator, faults::validate_point};

/// A non-sensitive lifecycle transition that a scenario may record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScenarioTransition {
    /// The operation began.
    Started,
    /// The operation reached a controlled pause boundary.
    Paused,
    /// The test released the controlled pause boundary.
    Resumed,
    /// The operation completed its durable mutation.
    DurableCommit,
    /// The operation began projecting its response.
    ResponseStarted,
    /// Generation-owned cleanup began.
    CleanupStarted,
}

/// A finite terminal classification for one scenario operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScenarioTerminal {
    /// The operation returned normally.
    Succeeded,
    /// Business or admission rules rejected the operation.
    Rejected,
    /// Cooperative cancellation ended the operation.
    Cancelled,
    /// A deadline ended the operation.
    TimedOut,
    /// A non-domain failure ended the operation.
    Failed,
    /// A durable effect may have occurred but the caller lacks a final result.
    Uncertain,
}

/// One sanitized, deterministic scenario receipt entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScenarioReceiptEvent {
    /// Virtual monotonic timestamp at which the event was recorded.
    pub virtual_time: Duration,
    /// Stable generation identifier, never user input or credential material.
    pub generation_id: String,
    /// Stable operation identifier, never request payload or credential material.
    pub operation_id: String,
    /// The lifecycle transition observed at this boundary.
    pub transition: ScenarioTransition,
    /// Explicit injected fault observed at this boundary, when applicable.
    pub fault: Option<SimulatorFault>,
    /// Finite outcome when this entry terminates the operation.
    pub terminal: Option<ScenarioTerminal>,
}

/// A test-owned receipt recorder attached to one [`TestSimulator`].
///
/// It accepts only bounded ASCII identifiers and finite enums so test evidence
/// cannot accidentally include request payloads, secrets, or arbitrary errors.
#[derive(Clone, Debug)]
pub struct ScenarioReceipt {
    simulator: TestSimulator,
    events: Rc<RefCell<Vec<ScenarioReceiptEvent>>>,
}

impl ScenarioReceipt {
    pub(crate) fn new(simulator: TestSimulator) -> Self {
        Self {
            simulator,
            events: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// Records one non-terminal transition.
    pub fn transition(
        &self,
        generation_id: &str,
        operation_id: &str,
        transition: ScenarioTransition,
    ) -> Result<(), FaultPointError> {
        self.record(generation_id, operation_id, transition, None, None)
    }

    /// Records one explicitly injected fault at the current boundary.
    pub fn fault(
        &self,
        generation_id: &str,
        operation_id: &str,
        transition: ScenarioTransition,
        fault: SimulatorFault,
    ) -> Result<(), FaultPointError> {
        self.record(generation_id, operation_id, transition, Some(fault), None)
    }

    /// Records one terminal result.
    pub fn terminal(
        &self,
        generation_id: &str,
        operation_id: &str,
        terminal: ScenarioTerminal,
    ) -> Result<(), FaultPointError> {
        self.record(
            generation_id,
            operation_id,
            ScenarioTransition::ResponseStarted,
            None,
            Some(terminal),
        )
    }

    /// Returns a stable snapshot in record order.
    pub fn events(&self) -> Vec<ScenarioReceiptEvent> {
        self.events.borrow().clone()
    }

    fn record(
        &self,
        generation_id: &str,
        operation_id: &str,
        transition: ScenarioTransition,
        fault: Option<SimulatorFault>,
        terminal: Option<ScenarioTerminal>,
    ) -> Result<(), FaultPointError> {
        validate_point(generation_id)?;
        validate_point(operation_id)?;
        self.events.borrow_mut().push(ScenarioReceiptEvent {
            virtual_time: self.simulator.now(),
            generation_id: generation_id.to_owned(),
            operation_id: operation_id.to_owned(),
            transition,
            fault,
            terminal,
        });
        Ok(())
    }
}

impl TestSimulator {
    /// Creates an empty, stable receipt for one test scenario.
    pub fn receipt(&self) -> ScenarioReceipt {
        ScenarioReceipt::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipt_records_virtual_time_faults_and_uncertain_terminal_state() {
        let simulator = TestSimulator::new();
        let receipt = simulator.receipt();

        receipt
            .transition(
                "generation-1",
                "oauth.consume-1",
                ScenarioTransition::Started,
            )
            .unwrap();
        simulator.advance(Duration::from_millis(5));
        receipt
            .transition(
                "generation-1",
                "oauth.consume-1",
                ScenarioTransition::DurableCommit,
            )
            .unwrap();
        receipt
            .fault(
                "generation-1",
                "oauth.consume-1",
                ScenarioTransition::ResponseStarted,
                SimulatorFault::DroppedConnection,
            )
            .unwrap();
        receipt
            .terminal(
                "generation-1",
                "oauth.consume-1",
                ScenarioTerminal::Uncertain,
            )
            .unwrap();

        assert_eq!(
            receipt.events(),
            vec![
                ScenarioReceiptEvent {
                    virtual_time: Duration::ZERO,
                    generation_id: "generation-1".to_owned(),
                    operation_id: "oauth.consume-1".to_owned(),
                    transition: ScenarioTransition::Started,
                    fault: None,
                    terminal: None,
                },
                ScenarioReceiptEvent {
                    virtual_time: Duration::from_millis(5),
                    generation_id: "generation-1".to_owned(),
                    operation_id: "oauth.consume-1".to_owned(),
                    transition: ScenarioTransition::DurableCommit,
                    fault: None,
                    terminal: None,
                },
                ScenarioReceiptEvent {
                    virtual_time: Duration::from_millis(5),
                    generation_id: "generation-1".to_owned(),
                    operation_id: "oauth.consume-1".to_owned(),
                    transition: ScenarioTransition::ResponseStarted,
                    fault: Some(SimulatorFault::DroppedConnection),
                    terminal: None,
                },
                ScenarioReceiptEvent {
                    virtual_time: Duration::from_millis(5),
                    generation_id: "generation-1".to_owned(),
                    operation_id: "oauth.consume-1".to_owned(),
                    transition: ScenarioTransition::ResponseStarted,
                    fault: None,
                    terminal: Some(ScenarioTerminal::Uncertain),
                },
            ]
        );
    }
}
