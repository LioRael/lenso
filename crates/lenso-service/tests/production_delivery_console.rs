use lenso_service::{DeliveryConsoleArtifacts, DeliveryConsoleState, project_delivery_console};

#[test]
fn delivery_console_projection_is_explainable_read_only_and_secret_free() {
    let projection = project_delivery_console(DeliveryConsoleArtifacts {
        artifacts: vec![
            serde_json::json!({
                "protocol": "lenso.service-release.v1",
                "releaseId": "release:support:5",
                "releaseDigest": "sha256:release5",
                "serviceId": "service:support",
                "workloads": [{
                    "workloadId": "support-api",
                    "artifactDigest": "sha256:api5",
                    "sbom": {"reference": "sbom:api5", "digest": "sha256:sbom5"},
                    "provenance": {"reference": "provenance:api5", "digest": "sha256:prov5"}
                }],
                "signatures": [{"signer": "ci:trusted", "signature": "must-not-leak"}]
            }),
            serde_json::json!({
                "protocol": "lenso.release-trust-evidence.v1",
                "releaseId": "release:support:5",
                "decision": "passed",
                "signatures": [{"signer": "ci:trusted", "status": "trusted"}],
                "workloads": [{"workloadId": "support-api", "provenanceSubjectMatches": true}]
            }),
            serde_json::json!({
                "protocol": "lenso.policy-evidence.v1",
                "evidenceId": "policy:prod:5",
                "decision": "passed",
                "packId": "production-default"
            }),
            serde_json::json!({
                "protocol": "lenso.config-revision.v1",
                "revisionId": "config:5",
                "values": {"PUBLIC_URL": "https://safe.example", "SECRET": "must-not-leak"},
                "secretReferences": [{
                    "referenceId": "secret:db:5",
                    "provider": "vault",
                    "purpose": "database",
                    "scope": "service",
                    "status": "resolved",
                    "metadata": {"rotationRevision": "7"}
                }]
            }),
            serde_json::json!({
                "protocol": "lenso.config-activation-receipt.v1",
                "targetRevisionId": "config:5",
                "previousRevisionId": "config:4",
                "activation": "active"
            }),
            serde_json::json!({
                "protocol": "lenso.deployment-observation.v1",
                "environment": "production",
                "desiredReleaseId": "release:support:5",
                "observedReleaseId": "release:support:5",
                "configRevisionId": "config:5",
                "drifted": false,
                "fresh": true
            }),
            serde_json::json!({
                "protocol": "lenso.edge-contract.v1",
                "edgeContractId": "edge:support:5",
                "routes": [{"publicPath": "/v1/tickets", "operationId": "listTickets"}]
            }),
            serde_json::json!({
                "protocol": "lenso.reliability-observation.v1",
                "observationId": "reliability-observation:canary-5",
                "observedRevision": 42,
                "fresh": true,
                "observationWindowSeconds": 600,
                "sampleCount": 1000,
                "genericProcessHealthy": true,
                "workloadReadiness": {"support-api": true},
                "workloadLiveness": {"support-api": true},
                "availabilityBasisPoints": 9990,
                "latencyP99Ms": 120,
                "errorBudgetUsedBasisPoints": 40,
                "queueBacklog": 4,
                "workflowBacklog": 2,
                "timerLagMs": 100,
                "retryExhaustion": 0,
                "compensationPressure": 0,
                "dependencies": [{"dependencyId": "database", "available": true}],
                "failureDomains": {"zone-a": true, "zone-b": true},
                "scalingCheckPassed": true,
                "disruptionCheckPassed": true,
                "availabilityCheckPassed": true,
                "evidenceReferences": ["runtime-story:canary-5"]
            }),
            serde_json::json!({
                "protocol": "lenso.canary-decision.v1",
                "decisionId": "canary:5:breach",
                "decision": "blocked",
                "outcome": "rollback",
                "currentPercent": 10,
                "nextPercent": 10,
                "issues": [{
                    "code": "canary_breach",
                    "message": "latency objective breached",
                    "evidenceReferences": ["runtime-story:canary-5"],
                    "remediation": "restore previous release",
                    "nextActions": ["start bounded rollback"]
                }]
            }),
            serde_json::json!({
                "protocol": "lenso.rollback-receipt.v1",
                "receiptId": "rollback:5",
                "outcome": "intervention_required",
                "approvalBoundaryRequired": true,
                "remainingRisks": [{
                    "code": "rollback_incomplete",
                    "message": "migration is irreversible",
                    "evidenceReferences": ["migration:5"],
                    "remediation": "limit exposure",
                    "nextActions": ["request intervention approval"]
                }],
                "evidenceReferences": ["runtime-story:rollback-5"]
            }),
        ],
    });

    assert_eq!(projection.state, DeliveryConsoleState::InterventionRequired);
    assert!(projection.read_only);
    assert!(projection.apply_actions.is_empty());
    assert_eq!(
        projection.release.as_ref().unwrap().release_id,
        "release:support:5"
    );
    assert_eq!(projection.supply_chain.len(), 1);
    assert_eq!(projection.supply_chain[0].signature_status, "trusted");
    assert_eq!(
        projection.configuration.active_revision_id.as_deref(),
        Some("config:5")
    );
    assert_eq!(
        projection.configuration.previous_revision_id.as_deref(),
        Some("config:4")
    );
    assert_eq!(projection.configuration.secret_references.len(), 1);
    assert_eq!(
        projection.edge.as_ref().unwrap().contract_id,
        "edge:support:5"
    );
    assert!(!projection.issues.is_empty());
    assert_eq!(projection.canary_observations.len(), 1);
    assert_eq!(projection.canary_observations[0].latency_p99_ms, Some(120));
    assert!(
        projection
            .next_actions
            .contains(&"request intervention approval".to_owned())
    );
    assert!(
        projection
            .runtime_story_references
            .contains(&"runtime-story:canary-5".to_owned())
    );

    let rendered = serde_json::to_string(&projection).unwrap();
    for forbidden in ["must-not-leak", "private-key", "secretValue", "credentials"] {
        assert!(!rendered.contains(forbidden));
    }
}

#[test]
fn delivery_console_projects_only_latest_environment_observation_and_lifecycle() {
    let projection = project_delivery_console(DeliveryConsoleArtifacts {
        artifacts: vec![
            serde_json::json!({
                "protocol": "lenso.config-revision.v1",
                "revisionId": "config:6",
                "secretReferences": []
            }),
            serde_json::json!({
                "protocol": "lenso.config-activation-receipt.v1",
                "targetRevisionId": "config:6",
                "previousRevisionId": "config:5",
                "activation": "rolled_back"
            }),
            serde_json::json!({
                "protocol": "lenso.deployment-observation.v1",
                "environment": "production",
                "desiredReleaseId": "release:old",
                "observedReleaseId": "release:old",
                "configRevisionId": "config:5",
                "drifted": true,
                "fresh": false
            }),
            serde_json::json!({
                "protocol": "lenso.rollback-receipt.v1",
                "receiptId": "rollback:old",
                "outcome": "rolled_back"
            }),
            serde_json::json!({
                "protocol": "lenso.promotion-receipt.v1",
                "receiptId": "promotion:new"
            }),
            serde_json::json!({
                "protocol": "lenso.deployment-observation.v1",
                "environment": "production",
                "desiredReleaseId": "release:new",
                "observedReleaseId": "release:new",
                "configRevisionId": "config:6",
                "drifted": false,
                "fresh": true
            }),
        ],
    });

    assert_eq!(projection.deployments.len(), 1);
    assert_eq!(projection.deployments[0].observed_release_id, "release:new");
    assert_eq!(projection.state, DeliveryConsoleState::Ready);
    assert_eq!(
        projection.configuration.active_revision_id.as_deref(),
        Some("config:6")
    );
    assert!(!projection.configuration.drifted);
}

#[test]
fn delivery_console_uses_append_order_for_the_current_state() {
    let projection = project_delivery_console(DeliveryConsoleArtifacts {
        artifacts: vec![
            serde_json::json!({
                "protocol": "lenso.delivery-state.v1",
                "artifactId": "state:z-hash",
                "state": "paused"
            }),
            serde_json::json!({
                "protocol": "lenso.delivery-state.v1",
                "artifactId": "state:a-hash",
                "state": "ready"
            }),
        ],
    });

    assert_eq!(projection.state, DeliveryConsoleState::Ready);
}
