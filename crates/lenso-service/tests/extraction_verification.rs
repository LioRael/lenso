use lenso_service::{
    ExtractionBehaviorObservation, ExtractionCompatibilityEvidence, ExtractionPolicyEvidence,
    ExtractionReconciliationResult, ExtractionVerificationInputs, ExtractionVerificationIssueCode,
    ExtractionVerificationStatus, verify_extraction_behavior,
};

fn reconciliation() -> ExtractionReconciliationResult {
    let value = serde_json::json!({
        "protocol": "lenso.extraction-reconciliation.v1",
        "reconciliationId": "reconciliation:test",
        "reconciliationDigest": "sha256:test",
        "status": "matched",
        "planId": "plan:support-ticket",
        "planDigest": "sha256:plan",
        "sourceHighWaterMark": "ticket-002",
        "destinationCheckpoint": "checkpoint:002",
        "sourceRecordCount": 2,
        "destinationRecordCount": 2,
        "issues": [], "evidence": [], "normalizedFields": [],
        "linkedAuthorityRemainsAuthoritative": true,
        "candidateWritesAdmitted": false,
        "effects": {"readsSourceSnapshot": true,"readsCandidateSnapshot": true,"mutatesSource": false,"mutatesCandidate": false,"changesAuthority": false}
    });
    serde_json::from_value(value).unwrap()
}

fn observation(implementation: &str) -> ExtractionBehaviorObservation {
    ExtractionBehaviorObservation {
        implementation: implementation.to_owned(),
        module_id: "support-ticket".to_owned(),
        operation_id: "openTicket".to_owned(),
        tenant_id: "tenant-acme".to_owned(),
        actor_id: "user-42".to_owned(),
        response: serde_json::json!({"ticketId":"ticket-003","status":"open"}),
        durable_state: serde_json::json!({"ticket-003":{"status":"open"}}),
        event_effects: vec!["support.ticket-opened.v1:ticket-003".to_owned()],
        workflow_outcomes: vec!["support-triage:started".to_owned()],
        story_evidence: vec!["story:support-ticket:ticket-003:opened".to_owned()],
    }
}

#[test]
fn identical_public_behavior_and_policy_evidence_allows_provisional_cutover() {
    let result = verify_extraction_behavior(ExtractionVerificationInputs {
        reconciliation: reconciliation(),
        linked: observation("linked"),
        candidate: observation("autonomous"),
        compatibility: vec![ExtractionCompatibilityEvidence::compatible(
            "support-web",
            "support-ticket-http.v1",
            "v1",
        )],
        policy: vec![ExtractionPolicyEvidence::passed(
            "single-authoritative-writer",
        )],
        volatile_json_pointers: vec![],
    });
    assert_eq!(result.status, ExtractionVerificationStatus::Verified);
    assert!(result.issues.is_empty());
    assert!(result.provisional_cutover_eligible);
}

#[test]
fn behavior_story_context_and_policy_mismatches_fail_closed() {
    let linked = observation("linked");
    let mut candidate = observation("autonomous");
    candidate.response["status"] = serde_json::json!("queued");
    candidate.tenant_id = "tenant-other".to_owned();
    candidate.story_evidence.clear();
    let result = verify_extraction_behavior(ExtractionVerificationInputs {
        reconciliation: reconciliation(),
        linked,
        candidate,
        compatibility: vec![ExtractionCompatibilityEvidence::incompatible(
            "support-web",
            "support-ticket-http.v1",
            "v1",
            "response field removed",
        )],
        policy: vec![ExtractionPolicyEvidence::failed(
            "single-authoritative-writer",
            "candidate accepted external writes",
        )],
        volatile_json_pointers: vec![],
    });
    let codes = result
        .issues
        .iter()
        .map(|issue| issue.code)
        .collect::<Vec<_>>();
    assert!(codes.contains(&ExtractionVerificationIssueCode::BehaviorMismatch));
    assert!(codes.contains(&ExtractionVerificationIssueCode::ContextMismatch));
    assert!(codes.contains(&ExtractionVerificationIssueCode::StoryMismatch));
    assert!(codes.contains(&ExtractionVerificationIssueCode::ConsumerIncompatible));
    assert!(codes.contains(&ExtractionVerificationIssueCode::PolicyRejected));
    assert_eq!(result.status, ExtractionVerificationStatus::Blocked);
}
