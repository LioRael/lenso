use lenso_service::{
    ExtractionApproval, ExtractionAuthorityCommitInputs, ExtractionAuthorityCommitStatus,
    ExtractionFastRollbackIssueCode, ExtractionProvisionalCutoverRun, commit_extraction_authority,
    record_autonomous_mutation, request_fast_extraction_rollback,
};

fn verified_cutover() -> ExtractionProvisionalCutoverRun {
    serde_json::from_value(serde_json::json!({
        "protocol":"lenso.extraction-provisional-cutover.v1","cutoverId":"cutover:support",
        "cutoverDigest":"sha256:cutover","revision":2,"status":"verified","planId":"plan:support",
        "planDigest":"sha256:plan","authorityRevision":"authority-r7","routingRevisionBefore":"routing-r9",
        "routingRevisionCurrent":"provisional-r10","candidateServiceId":"support-ticket-service",
        "verificationDigest":"sha256:verification","quiescenceDigest":"sha256:quiescence",
        "sourceHighWaterMark":"ticket-42","destinationCheckpoint":"checkpoint:42",
        "route":"candidate_verification_only","externalMutationsPaused":true,"linkedMutationsOpen":false,
        "linkedAuthoritative":true,"candidateAuthoritative":false,"candidateHealthy":true,
        "declaredVerificationTrafficOnly":true,"verificationEffectsIsolated":true,
        "linkedBusinessProbePassed":false,"applyReceipts":[],"rollbackReceipts":[],"evidence":[]
    })).unwrap()
}

fn approval(cutover: &ExtractionProvisionalCutoverRun) -> ExtractionApproval {
    ExtractionApproval::bind(cutover, "approval-001", "operator:alice", true)
}

#[test]
fn exact_authorized_approval_commits_one_autonomous_authority_then_blocks_fast_rollback() {
    let cutover = verified_cutover();
    let committed = commit_extraction_authority(ExtractionAuthorityCommitInputs {
        cutover: cutover.clone(),
        approval: approval(&cutover),
        current_authority_revision: "authority-r7".to_owned(),
        current_routing_revision: "provisional-r10".to_owned(),
        current_system_graph_revision: "system-r12".to_owned(),
        candidate_healthy: true,
    })
    .expect("exact approval commits");
    assert_eq!(committed.status, ExtractionAuthorityCommitStatus::Committed);
    assert!(committed.candidate_authoritative);
    assert!(!committed.linked_authoritative);
    assert!(committed.candidate_mutations_open);
    assert!(committed.linked_recovery_read_only);
    assert_eq!(
        committed.commit_receipts.len(),
        1,
        "authority, routing and graph share one CAS receipt"
    );

    let mutated = record_autonomous_mutation(committed, "mutation:ticket-43");
    let blocked = request_fast_extraction_rollback(&mutated, false)
        .expect_err("reverse movement needs separate review");
    assert_eq!(
        blocked.code,
        ExtractionFastRollbackIssueCode::ReverseMigrationEvidenceRequired
    );
}

#[test]
fn stale_or_unauthorized_approval_fails_before_mutation() {
    let cutover = verified_cutover();
    let mut stale = approval(&cutover);
    stale.plan_digest = "sha256:stale".to_owned();
    let error = commit_extraction_authority(ExtractionAuthorityCommitInputs {
        cutover,
        approval: stale,
        current_authority_revision: "authority-r7".to_owned(),
        current_routing_revision: "provisional-r10".to_owned(),
        current_system_graph_revision: "system-r12".to_owned(),
        candidate_healthy: true,
    })
    .expect_err("stale approval");
    assert!(!error.mutation_started);
}
