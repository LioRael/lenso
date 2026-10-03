//! Faults around a delegated durable operation, without a storage implementation.

use std::future::Future;

use crate::{FaultInjector, FaultPointError, ScenarioBoundary, SimulatorFault};

/// Evidence available at a durable operation's completion boundary.
///
/// A timeout alone supplies no evidence of rollback. `Committed` on an injected
/// lost acknowledgement is test-oracle evidence: a real disconnected caller
/// usually has only `Unknown` and must reconcile through its own Store contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitKnowledge {
    /// The delegate was never invoked, or the backend positively rejected dispatch.
    NotExecuted,
    /// The backend positively confirmed that this operation rolled back.
    RolledBack,
    /// The delegate confirmed commit before a test-owned acknowledgement fault.
    Committed,
    /// The available response cannot establish whether the operation committed.
    Unknown,
}

/// The source of a durable operation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DurableFailureCause<E> {
    /// An error from the real backend or a recorded external completion.
    Backend(E),
    /// An explicit test-owned boundary fault.
    Injected(SimulatorFault),
}

/// A failure with explicit commit evidence, independent of its transport error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DurableFailure<E> {
    /// Evidence, never inferred merely from the error's name.
    pub knowledge: CommitKnowledge,
    /// The backend error or injected boundary condition.
    pub cause: DurableFailureCause<E>,
}

impl<E> DurableFailure<E> {
    /// Records backend evidence. Use `Unknown` unless the backend proves more.
    pub fn backend(knowledge: CommitKnowledge, error: E) -> Self {
        Self {
            knowledge,
            cause: DurableFailureCause::Backend(error),
        }
    }

    fn injected(knowledge: CommitKnowledge, fault: SimulatorFault) -> Self {
        Self {
            knowledge,
            cause: DurableFailureCause::Injected(fault),
        }
    }
}

/// A test-only facade around a finite operation owned by a Plugin's private Store.
///
/// It neither parses SQL nor supplies transactions, retries, deduplication, or
/// results. The delegate executes the real operation. A simulator may instead
/// replay *recorded typed external completions*, which never qualify a database.
/// `AfterDurableCommit` is visited only after a successful delegate completion;
/// `BeforeResponse` deliberately hides its evidence to model an uncertain reply.
#[derive(Clone, Debug)]
pub struct DurableFaultFacade {
    operation: String,
    faults: FaultInjector,
}

impl DurableFaultFacade {
    /// Validates all boundary labels before any delegate can be invoked.
    pub fn new(operation: &str, faults: FaultInjector) -> Result<Self, FaultPointError> {
        for suffix in [
            ".before-operation",
            ".after-durable-commit",
            ".before-response",
        ] {
            crate::faults::validate_point(&format!("{operation}{suffix}"))?;
        }
        // Empty operation names are invalid even though the suffixed label is valid.
        crate::faults::validate_point(operation)?;
        Ok(Self {
            operation: operation.to_owned(),
            faults,
        })
    }

    /// Delegates exactly once, preserving backend evidence unless a fault hides it.
    pub async fn execute<T, E, F, Fut>(&self, delegate: F) -> Result<T, DurableFailure<E>>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, DurableFailure<E>>>,
    {
        if let Err(fault) = self.check(ScenarioBoundary::BeforeOperation) {
            return Err(DurableFailure::injected(
                CommitKnowledge::NotExecuted,
                fault,
            ));
        }
        let result = delegate().await;
        if result.is_ok()
            && let Err(fault) = self.check(ScenarioBoundary::AfterDurableCommit)
        {
            return Err(DurableFailure::injected(CommitKnowledge::Committed, fault));
        }
        if let Err(fault) = self.check(ScenarioBoundary::BeforeResponse) {
            return Err(DurableFailure::injected(CommitKnowledge::Unknown, fault));
        }
        result
    }

    fn check(&self, boundary: ScenarioBoundary) -> Result<(), SimulatorFault> {
        self.faults
            .check_at(&self.operation, boundary)
            .expect("facade constructor validated every boundary")
    }
}
