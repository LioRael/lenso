use lenso_service::{
    ExtractionProvisionalCutoverInputs, ExtractionProvisionalCutoverStatus,
    ExtractionQuiescenceRun, ExtractionTrafficRoute, ExtractionVerificationResult,
    fail_provisional_cutover, start_provisional_cutover,
};

fn verification() -> ExtractionVerificationResult {
    serde_json::from_value(serde_json::json!({
        "protocol":"lenso.extraction-verification.v1","verificationId":"verification:support",
        "verificationDigest":"sha256:verification","status":"verified","planId":"plan:support",
        "reconciliationId":"reconciliation:support","issues":[],"evidence":[],"compatibility":[],
        "policy":[],"volatileJsonPointers":[],"provisionalCutoverEligible":true,
        "linkedAuthorityRemainsAuthoritative":true,
        "effects":{"invokesLinkedPublicContract":true,"invokesCandidatePublicContract":true,
        "routesExternalMutations":false,"changesAuthority":false,"requiresRuntimeConsole":false,
        "requiresSystemPlaneForBusinessExecution":false}
    }))
    .unwrap()
}

fn quiescence() -> ExtractionQuiescenceRun {
    serde_json::from_value(serde_json::json!({
        "protocol":"lenso.extraction-quiescence.v1","quiescenceId":"quiescence:support",
        "quiescenceDigest":"sha256:quiescence","revision":3,"status":"quiesced",
        "planId":"plan:support","planDigest":"sha256:plan","expectedAuthorityRevision":"authority-r7",
        "linkedMutationsPaused":true,"linkedReadInspectionAvailable":true,
        "linkedAuthorityRemainsAuthoritative":true,"candidateAuthoritative":false,
        "stableSourceHighWaterMark":"ticket-42","destinationCheckpoint":"checkpoint:42",
        "issues":[],"evidence":[],"nextActions":[],
        "effects":{"pausesLinkedMutations":true,"drainsInFlightWork":true,"copiesFinalDelta":true,
        "routesCandidateTraffic":false,"changesAuthority":false,"requiresRuntimeConsole":false,
        "requiresSystemPlaneForBusinessExecution":false}
    })).unwrap()
}

#[test]
fn injected_candidate_failure_rolls_back_routing_and_reopens_linked_writes_once() {
    let run = start_provisional_cutover(ExtractionProvisionalCutoverInputs {
        plan_id: "plan:support".to_owned(),
        plan_digest: "sha256:plan".to_owned(),
        authority_revision: "authority-r7".to_owned(),
        routing_revision: "routing-r9".to_owned(),
        candidate_service_id: "support-ticket-service".to_owned(),
        candidate_healthy: true,
        verification: verification(),
        quiescence: quiescence(),
    })
    .unwrap();
    assert_eq!(run.route, ExtractionTrafficRoute::CandidateVerificationOnly);
    assert!(run.external_mutations_paused);

    let rolled_back = fail_provisional_cutover(
        run,
        "candidate verification returned 503",
        "operator:local-test",
        true,
    );
    assert_eq!(
        rolled_back.status,
        ExtractionProvisionalCutoverStatus::RolledBack
    );
    assert_eq!(rolled_back.route, ExtractionTrafficRoute::Linked);
    assert!(rolled_back.linked_mutations_open);
    assert!(rolled_back.linked_authoritative);
    assert!(!rolled_back.candidate_authoritative);
    assert!(rolled_back.linked_business_probe_passed);
    assert_eq!(rolled_back.rollback_receipts.len(), 2);

    let repeated = fail_provisional_cutover(
        rolled_back.clone(),
        "candidate verification returned 503",
        "operator:local-test",
        true,
    );
    assert_eq!(repeated, rolled_back, "rollback receipts are idempotent");
}
