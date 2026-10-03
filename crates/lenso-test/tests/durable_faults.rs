use std::{cell::Cell, rc::Rc};

use lenso_test::{
    CommitKnowledge, DurableFailure, DurableFailureCause, DurableFaultFacade, FaultInjector,
    ScenarioBoundary, SimulatorFault, TestSimulator,
};

#[test]
fn pre_dispatch_fault_does_not_even_construct_the_delegate_future() {
    let simulator = TestSimulator::new();
    let faults = simulator.faults();
    faults
        .inject_at(
            "store.write",
            ScenarioBoundary::BeforeOperation,
            SimulatorFault::Timeout,
        )
        .unwrap();
    let facade = DurableFaultFacade::new("store.write", faults).unwrap();
    let invoked = Cell::new(false);
    let result = simulator.run(facade.execute(|| {
        invoked.set(true);
        async { Ok::<_, DurableFailure<()>>(()) }
    }));
    assert!(!invoked.get());
    assert_eq!(result.unwrap_err().knowledge, CommitKnowledge::NotExecuted);
}

#[test]
fn rollback_evidence_is_preserved_and_commit_fault_waits_for_a_real_commit() {
    let simulator = TestSimulator::new();
    let faults = simulator.faults();
    faults
        .inject_at(
            "store.write",
            ScenarioBoundary::AfterDurableCommit,
            SimulatorFault::DroppedConnection,
        )
        .unwrap();
    let facade = DurableFaultFacade::new("store.write", faults.clone()).unwrap();
    let rollback =
        DurableFailure::backend(CommitKnowledge::RolledBack, "confirmed constraint rollback");
    let result = simulator.run(facade.execute(|| async { Err::<(), _>(rollback.clone()) }));
    assert_eq!(result, Err(rollback));
    assert!(
        faults
            .has_pending("store.write.after-durable-commit")
            .unwrap()
    );
    let committed = Rc::new(Cell::new(false));
    let result = simulator.run(facade.execute(|| async {
        committed.set(true);
        Ok::<_, DurableFailure<&str>>(())
    }));
    assert!(committed.get());
    assert_eq!(
        result.unwrap_err(),
        DurableFailure {
            knowledge: CommitKnowledge::Committed,
            cause: DurableFailureCause::Injected(SimulatorFault::DroppedConnection),
        }
    );
    assert!(
        !faults
            .has_pending("store.write.after-durable-commit")
            .unwrap()
    );
}

#[test]
fn unknown_backend_timeout_is_not_reclassified_as_rollback() {
    let simulator = TestSimulator::new();
    let facade = DurableFaultFacade::new("store.write", simulator.faults()).unwrap();
    let timeout = DurableFailure::backend(CommitKnowledge::Unknown, "timeout");
    assert_eq!(
        simulator.run(facade.execute(|| async { Err::<(), _>(timeout.clone()) })),
        Err(timeout)
    );
}

#[test]
fn response_loss_hides_both_positive_commit_and_positive_rollback_evidence() {
    for rolled_back in [false, true] {
        let simulator = TestSimulator::new();
        simulator
            .faults()
            .inject_at(
                "store.write",
                ScenarioBoundary::BeforeResponse,
                SimulatorFault::Timeout,
            )
            .unwrap();
        let facade = DurableFaultFacade::new("store.write", simulator.faults()).unwrap();
        let result = simulator.run(facade.execute(|| async {
            if rolled_back {
                Err(DurableFailure::backend(
                    CommitKnowledge::RolledBack,
                    "constraint",
                ))
            } else {
                Ok(())
            }
        }));
        assert_eq!(result.unwrap_err().knowledge, CommitKnowledge::Unknown);
    }
}

#[test]
fn boundary_validation_rejects_invalid_and_overlong_operation_names() {
    for operation in ["", "with space", &"x".repeat(120)] {
        assert!(DurableFaultFacade::new(operation, FaultInjector::new()).is_err());
    }
}
