use lenso_service::{
    ExtractionDrainSnapshot, ExtractionPlan, ExtractionQuiescenceIssueCode,
    ExtractionQuiescenceStatus, cancel_extraction_quiescence, record_extraction_drain,
    start_extraction_quiescence,
};

fn plan() -> ExtractionPlan {
    serde_json::from_str(include_str!(
        "../../../contracts/extraction/support-ticket.plan.json"
    ))
    .unwrap()
}

#[test]
fn unresolved_work_blocks_without_abandoning_it_and_cancel_reopens_linked_mutations() {
    let run = start_extraction_quiescence(&plan(), "support-authority-r7").unwrap();
    assert!(run.linked_mutations_paused);
    let blocked = record_extraction_drain(
        run,
        ExtractionDrainSnapshot {
            in_flight_requests: 0,
            outbox_messages: 1,
            inbox_messages: 0,
            scheduled_functions: 0,
            timers: 0,
            durable_workflows: 0,
            unresolved: vec!["outbox:support.ticket-opened:42".to_owned()],
            timed_out: false,
        },
    );
    assert_eq!(blocked.status, ExtractionQuiescenceStatus::Blocked);
    assert_eq!(
        blocked.issues[0].code,
        ExtractionQuiescenceIssueCode::DrainIncomplete
    );
    let cancelled = cancel_extraction_quiescence(blocked, "operator cancelled after drain failure");
    assert_eq!(cancelled.status, ExtractionQuiescenceStatus::Cancelled);
    assert!(!cancelled.linked_mutations_paused);
    assert!(cancelled.linked_authority_remains_authoritative);
}

#[test]
fn empty_drain_records_stable_evidence_without_transferring_authority() {
    let run = start_extraction_quiescence(&plan(), "support-authority-r7").unwrap();
    let drained = record_extraction_drain(run, ExtractionDrainSnapshot::empty());
    assert_eq!(drained.status, ExtractionQuiescenceStatus::Drained);
    assert!(drained.linked_mutations_paused);
    assert!(drained.linked_authority_remains_authoritative);
    assert!(!drained.candidate_authoritative);
    assert!(drained.evidence.iter().any(|e| e.kind == "drain_complete"));
}
