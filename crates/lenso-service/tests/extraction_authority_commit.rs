use lenso_service::{
    ExtractionApproval, ExtractionAuthorityCommitInputs, ExtractionAuthorityCommitRevalidation,
    ExtractionAuthorityCommitStatus, ExtractionCandidateHealthEvidence,
    ExtractionFastRollbackIssueCode, ExtractionProvisionalCutoverRun, ExtractionQuiescenceRun,
    ExtractionReconciliationResult, ExtractionReconciliationStatus, ExtractionVerificationResult,
    commit_extraction_authority, extraction_input_digest, record_autonomous_mutation,
    request_fast_extraction_rollback,
};

fn verified_cutover() -> ExtractionProvisionalCutoverRun {
    let (_, verification, quiescence) = final_artifacts();
    let mut cutover: ExtractionProvisionalCutoverRun = serde_json::from_value(serde_json::json!({
        "protocol":"lenso.extraction-provisional-cutover.v1","cutoverId":"cutover:support",
        "cutoverDigest":"sha256:cutover","revision":2,"status":"verified","planId":"plan:support",
        "planDigest":"sha256:plan","authorityRevision":"authority-r7","routingRevisionBefore":"routing-r9",
        "routingRevisionCurrent":"provisional-r10","candidateServiceId":"support-ticket-service",
        "verificationDigest":verification.verification_digest,
        "quiescenceDigest":quiescence.quiescence_digest,
        "sourceHighWaterMark":"ticket-42","destinationCheckpoint":"checkpoint:42",
        "route":"candidate_verification_only","externalMutationsPaused":true,"linkedMutationsOpen":false,
        "linkedAuthoritative":true,"candidateAuthoritative":false,"candidateHealthy":true,
        "declaredVerificationTrafficOnly":true,"verificationEffectsIsolated":true,
        "linkedBusinessProbePassed":false,"applyReceipts":[],"rollbackReceipts":[],"evidence":[]
    })).unwrap();
    cutover.cutover_digest.clear();
    cutover.cutover_digest = extraction_input_digest(&serde_json::to_vec(&cutover).unwrap());
    cutover
}

fn approval(cutover: &ExtractionProvisionalCutoverRun) -> ExtractionApproval {
    ExtractionApproval::bind(
        cutover,
        &candidate_health(),
        "approval-001",
        "operator:alice",
        true,
    )
}

fn candidate_health() -> ExtractionCandidateHealthEvidence {
    ExtractionCandidateHealthEvidence::bind(
        "plan:support",
        "support-ticket-service",
        "http://127.0.0.1:4212",
        true,
        true,
    )
}

fn revalidation(
    _cutover: &ExtractionProvisionalCutoverRun,
) -> ExtractionAuthorityCommitRevalidation {
    let (reconciliation, verification, quiescence) = final_artifacts();
    ExtractionAuthorityCommitRevalidation {
        reconciliation,
        verification,
        quiescence,
        candidate_health: candidate_health(),
    }
}

fn final_artifacts() -> (
    ExtractionReconciliationResult,
    ExtractionVerificationResult,
    ExtractionQuiescenceRun,
) {
    let mut reconciliation: ExtractionReconciliationResult =
        serde_json::from_value(serde_json::json!({
            "protocol":"lenso.extraction-reconciliation.v1",
            "reconciliationId":"reconciliation:support",
            "reconciliationDigest":"",
            "status":"matched",
            "planId":"plan:support",
            "planDigest":"sha256:plan",
            "sourceHighWaterMark":"ticket-42",
            "destinationCheckpoint":"checkpoint:42",
            "sourceRecordCount":1,
            "destinationRecordCount":1,
            "issues":[],"evidence":[],"normalizedFields":[],
            "linkedAuthorityRemainsAuthoritative":true,
            "candidateWritesAdmitted":false,
            "effects":{"readsSourceSnapshot":true,"readsCandidateSnapshot":true,
                "mutatesSource":false,"mutatesCandidate":false,"changesAuthority":false}
        }))
        .unwrap();
    reconciliation.reconciliation_digest =
        extraction_input_digest(&serde_json::to_vec(&reconciliation).unwrap());

    let mut verification: ExtractionVerificationResult =
        serde_json::from_value(serde_json::json!({
            "protocol":"lenso.extraction-verification.v1",
            "verificationId":"verification:support",
            "verificationDigest":"",
            "status":"verified","planId":"plan:support",
            "reconciliationId":"reconciliation:support",
            "reconciliationDigest":reconciliation.reconciliation_digest,
            "issues":[],"evidence":[],
            "compatibility":[],"policy":[],"volatileJsonPointers":[],
            "provisionalCutoverEligible":true,"linkedAuthorityRemainsAuthoritative":true,
            "effects":{"invokesLinkedPublicContract":true,"invokesCandidatePublicContract":true,
                "routesExternalMutations":false,"changesAuthority":false,
                "requiresRuntimeConsole":false,"requiresSystemPlaneForBusinessExecution":false}
        }))
        .unwrap();
    verification.verification_digest =
        extraction_input_digest(&serde_json::to_vec(&verification).unwrap());

    let mut quiescence: ExtractionQuiescenceRun = serde_json::from_value(serde_json::json!({
        "protocol":"lenso.extraction-quiescence.v1","quiescenceId":"quiescence:support",
        "quiescenceDigest":"","revision":3,"status":"quiesced",
        "planId":"plan:support","planDigest":"sha256:plan",
        "expectedAuthorityRevision":"authority-r7","linkedMutationsPaused":true,
        "linkedReadInspectionAvailable":true,"linkedAuthorityRemainsAuthoritative":true,
        "candidateAuthoritative":false,
        "drain":{"inFlightRequests":0,"outboxMessages":0,"inboxMessages":0,
            "scheduledFunctions":0,"timers":0,"durableWorkflows":0,"unresolved":[],"timedOut":false},
        "stableSourceHighWaterMark":"ticket-42","destinationCheckpoint":"checkpoint:42",
        "issues":[],"evidence":[],"nextActions":[],
        "effects":{"pausesLinkedMutations":true,"drainsInFlightWork":true,
            "copiesFinalDelta":true,"routesCandidateTraffic":false,"changesAuthority":false,
            "requiresRuntimeConsole":false,"requiresSystemPlaneForBusinessExecution":false}
    }))
    .unwrap();
    quiescence.quiescence_digest =
        extraction_input_digest(&serde_json::to_vec(&quiescence).unwrap());
    (reconciliation, verification, quiescence)
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
        revalidation: revalidation(&cutover),
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
    let blocked = request_fast_extraction_rollback(&mutated, None)
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
        cutover: cutover.clone(),
        approval: stale,
        current_authority_revision: "authority-r7".to_owned(),
        current_routing_revision: "provisional-r10".to_owned(),
        current_system_graph_revision: "system-r12".to_owned(),
        revalidation: revalidation(&cutover),
    })
    .expect_err("stale approval");
    assert!(!error.mutation_started);
}

#[test]
fn failed_final_reconciliation_revalidation_fails_before_commit() {
    let cutover = verified_cutover();
    let mut final_state = revalidation(&cutover);
    final_state.reconciliation.status = ExtractionReconciliationStatus::Blocked;
    let error = commit_extraction_authority(ExtractionAuthorityCommitInputs {
        cutover: cutover.clone(),
        approval: approval(&cutover),
        current_authority_revision: cutover.authority_revision.clone(),
        current_routing_revision: cutover.routing_revision_current.clone(),
        current_system_graph_revision: "system-r12".to_owned(),
        revalidation: final_state,
    })
    .expect_err("final reconciliation must still match");
    assert!(!error.mutation_started);
}

#[test]
fn mutated_cutover_payload_is_rejected_even_when_approval_is_rebound() {
    let mut cutover = verified_cutover();
    cutover.candidate_service_id = "attacker-service".to_owned();
    let error = commit_extraction_authority(ExtractionAuthorityCommitInputs {
        cutover: cutover.clone(),
        approval: approval(&cutover),
        current_authority_revision: cutover.authority_revision.clone(),
        current_routing_revision: cutover.routing_revision_current.clone(),
        current_system_graph_revision: "system-r12".to_owned(),
        revalidation: revalidation(&cutover),
    })
    .expect_err("cutover content must remain bound to its digest");
    assert!(!error.mutation_started);
}

#[test]
fn changed_health_or_reconciliation_digest_is_rejected_before_commit() {
    let cutover = verified_cutover();
    let mut health_changed = revalidation(&cutover);
    health_changed.candidate_health.store_ready = false;
    let health_error = commit_extraction_authority(ExtractionAuthorityCommitInputs {
        cutover: cutover.clone(),
        approval: approval(&cutover),
        current_authority_revision: cutover.authority_revision.clone(),
        current_routing_revision: cutover.routing_revision_current.clone(),
        current_system_graph_revision: "system-r12".to_owned(),
        revalidation: health_changed,
    })
    .expect_err("changed candidate health must fail integrity validation");
    assert!(!health_error.mutation_started);

    let mut reconciliation_changed = revalidation(&cutover);
    reconciliation_changed.verification.reconciliation_digest = "sha256:older".to_owned();
    let reconciliation_error = commit_extraction_authority(ExtractionAuthorityCommitInputs {
        cutover: cutover.clone(),
        approval: approval(&cutover),
        current_authority_revision: cutover.authority_revision.clone(),
        current_routing_revision: cutover.routing_revision_current.clone(),
        current_system_graph_revision: "system-r12".to_owned(),
        revalidation: reconciliation_changed,
    })
    .expect_err("verification must bind the exact reconciliation digest");
    assert!(!reconciliation_error.mutation_started);
}
