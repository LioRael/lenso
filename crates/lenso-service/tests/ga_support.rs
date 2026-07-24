use std::collections::BTreeMap;

use lenso_service::{
    ComponentKind, ContractConsumerEvidence, ContractRetirementInput, EvidenceReceiptTrust,
    FailureCondition, FailureObservation, FailureOutcome, FailureScenarioInput, GaComponent,
    GaSupportManifestInput, ManifestFormat, ManifestKind, ManifestMigrationInput,
    RetirementApproval, ServiceUpgradeInput, ServiceUpgradeRuntimeObservation,
    SupportCombinationInput, SupportDecision, SupportStatus, UpgradeEdgeInput, UpgradeWorkload,
    apply_contract_retirement, apply_manifest_migration, assemble_ga_support_manifest,
    assemble_ga_support_manifest_with_trust, contract_retirement_receipt_integrity_is_valid,
    evaluate_failure_scenario, evaluate_ga_support, evaluate_service_upgrade_admission,
    ga_support_manifest_integrity_valid, plan_contract_retirement, plan_manifest_migration,
    plan_service_upgrade,
};
use serde_json::json;

fn digest(value: char) -> String {
    format!("sha256:{}", value.to_string().repeat(64))
}

fn support_manifest() -> lenso_service::GaSupportManifest {
    assemble_ga_support_manifest_with_trust(
        GaSupportManifestInput {
            status: SupportStatus::GeneralAvailability,
            components: vec![
                GaComponent {
                    kind: ComponentKind::Runtime,
                    component_id: "lenso-service".into(),
                    version: "1.0.0".into(),
                    digest: digest('a'),
                },
                GaComponent {
                    kind: ComponentKind::Cli,
                    component_id: "lenso-cli".into(),
                    version: "1.0.0".into(),
                    digest: digest('b'),
                },
            ],
            manifest_formats: vec![
                ManifestFormat {
                    kind: ManifestKind::Provider,
                    version: "lenso.service.v1".into(),
                },
                ManifestFormat {
                    kind: ManifestKind::System,
                    version: "lenso.system.v1".into(),
                },
                ManifestFormat {
                    kind: ManifestKind::System,
                    version: "lenso.system.v2".into(),
                },
                ManifestFormat {
                    kind: ManifestKind::Service,
                    version: "lenso.service.v2".into(),
                },
            ],
            state_versions: vec!["service-store.v1".into(), "service-store.v2".into()],
            adapter_versions: BTreeMap::from([
                ("nats-jetstream".into(), "2.11".into()),
                ("spiffe-spire".into(), "1.12".into()),
            ]),
            documentation: lenso_service::DocumentationIdentity {
                version: "m6-ga".into(),
                digest: digest('d'),
            },
            combinations: vec![SupportCombinationInput {
                combination_id: "ga-1".into(),
                component_references: vec![
                    "cli:lenso-cli@1.0.0".into(),
                    "runtime:lenso-service@1.0.0".into(),
                ],
                state_version: "service-store.v2".into(),
                status: SupportStatus::GeneralAvailability,
            }],
            upgrade_edges: vec![UpgradeEdgeInput {
                edge_id: "state-v1-v2".into(),
                source_format: "service-store.v1".into(),
                target_format: "service-store.v2".into(),
                mixed_version_references: vec![
                    "runtime:lenso-service@1.0.0".into(),
                    "runtime:lenso-service@0.9.0".into(),
                ],
                rollback_safe: false,
            }],
        },
        EvidenceReceiptTrust {
            authorities: BTreeMap::from([
                (
                    "lenso.performance-profile.v1".into(),
                    "test-authority".into(),
                ),
                (
                    "lenso.service-restore-evidence.v1".into(),
                    "test-authority".into(),
                ),
            ]),
            public_keys: BTreeMap::from([(
                "test-authority".into(),
                "-----BEGIN PUBLIC KEY-----\ntest\n-----END PUBLIC KEY-----".into(),
            )]),
        },
    )
    .expect("valid support manifest")
}

#[test]
fn support_manifest_is_canonical_and_unknown_combinations_fail_explicitly() {
    let manifest = support_manifest();
    let mut reordered = manifest.clone();
    reordered.components.reverse();
    let rebuilt = assemble_ga_support_manifest_with_trust(
        reordered.clone().into_input(),
        EvidenceReceiptTrust {
            authorities: reordered.evidence_receipt_authorities,
            public_keys: reordered.receipt_authority_public_keys,
        },
    )
    .unwrap();
    assert_eq!(manifest.manifest_id, rebuilt.manifest_id);
    assert_eq!(manifest.manifest_digest, rebuilt.manifest_digest);

    let supported = evaluate_ga_support(
        &manifest,
        &["runtime:lenso-service@1.0.0", "cli:lenso-cli@1.0.0"],
        "service-store.v2",
    );
    assert_eq!(supported.decision, SupportDecision::Supported);

    let unknown = evaluate_ga_support(
        &manifest,
        &["runtime:lenso-service@1.1.0", "cli:lenso-cli@1.0.0"],
        "service-store.v2",
    );
    assert_eq!(unknown.decision, SupportDecision::Unknown);
    assert_eq!(unknown.issues[0].code.as_str(), "ga_combination_unknown");
    assert!(
        unknown
            .next_actions
            .iter()
            .any(|action| action.contains("manifest"))
    );

    let mut tampered = manifest;
    tampered.components[0].version = "9.9.9".into();
    let rejected = evaluate_ga_support(
        &tampered,
        &["runtime:lenso-service@1.0.0", "cli:lenso-cli@1.0.0"],
        "service-store.v2",
    );
    assert_eq!(rejected.decision, SupportDecision::Blocked);
    assert_eq!(rejected.issues[0].code.as_str(), "ga_manifest_invalid");
}

#[test]
fn legacy_v1_support_manifest_without_receipt_trust_keeps_its_digest() {
    let current = support_manifest();
    let legacy = assemble_ga_support_manifest(current.into_input()).unwrap();
    let mut encoded = serde_json::to_value(&legacy).unwrap();
    encoded
        .as_object_mut()
        .unwrap()
        .remove("evidenceReceiptAuthorities");
    encoded
        .as_object_mut()
        .unwrap()
        .remove("receiptAuthorityPublicKeys");
    let decoded: lenso_service::GaSupportManifest = serde_json::from_value(encoded).unwrap();
    assert!(ga_support_manifest_integrity_valid(&decoded));
    assert_eq!(decoded.manifest_digest, legacy.manifest_digest);
}

#[test]
fn manifest_migration_is_deterministic_stale_safe_and_preserves_identity() {
    let source = json!({
        "protocol": "lenso.system.v1",
        "systemId": "support-system",
        "services": [{"serviceId": "support", "moduleId": "support-ticket"}]
    });
    let input = ManifestMigrationInput {
        kind: ManifestKind::System,
        source_format: "lenso.system.v1".into(),
        target_format: "lenso.system.v2".into(),
        source: source.clone(),
        identity_pointers: vec![
            "/systemId".into(),
            "/services/0/serviceId".into(),
            "/services/0/moduleId".into(),
        ],
    };
    let plan = plan_manifest_migration(&input, &support_manifest()).unwrap();
    assert!(!plan.effects.mutates_source);
    let receipt = apply_manifest_migration(&plan, &source, false).unwrap();
    assert_eq!(receipt.migrated["protocol"], "lenso.system.v2");
    assert_eq!(receipt.migrated["systemId"], "support-system");

    let mut changed = source;
    changed["systemId"] = json!("changed");
    let error = apply_manifest_migration(&plan, &changed, false).unwrap_err();
    assert_eq!(error.code.as_str(), "manifest_source_stale");

    let mut tampered = plan;
    tampered.migrated["systemId"] = json!("forged");
    let error = apply_manifest_migration(&tampered, &input.source, false).unwrap_err();
    assert_eq!(error.code.as_str(), "ga_plan_integrity_invalid");
}

#[test]
fn service_upgrade_orders_migration_before_api_and_worker_and_reports_rollback_constraint() {
    let plan = plan_service_upgrade(
        &support_manifest(),
        ServiceUpgradeInput {
            service_id: "support".into(),
            from_release_id: "release-old".into(),
            from_release_digest: digest('1'),
            to_release_id: "release-new".into(),
            to_release_digest: digest('2'),
            config_revision_id: "config-1".into(),
            config_revision_digest: digest('3'),
            source_state_version: "service-store.v1".into(),
            target_state_version: "service-store.v2".into(),
            workflow_artifact_digests: vec![digest('4')],
        },
    )
    .unwrap();

    assert_eq!(
        plan.steps
            .iter()
            .map(|step| step.workload.as_str())
            .collect::<Vec<_>>(),
        ["migration", "api", "worker"]
    );
    assert!(!plan.rollback.automatic_allowed);
    assert!(plan.rollback.approval_boundary.is_some());
    assert!(plan.preserved_identities.contains(&"inbox".into()));
    assert!(plan.preserved_identities.contains(&"story_segment".into()));

    let blocked = evaluate_service_upgrade_admission(
        &plan,
        &ServiceUpgradeRuntimeObservation {
            workload: UpgradeWorkload::Worker,
            current_release_id: "release-old".into(),
            current_state_version: "service-store.v1".into(),
            migration_completed: false,
            workflow_artifact_digests: vec![digest('4')],
        },
    );
    assert_eq!(blocked.decision, SupportDecision::Blocked);
    assert!(!blocked.claims_work);
    assert!(!blocked.mutates_state);

    let admitted = evaluate_service_upgrade_admission(
        &plan,
        &ServiceUpgradeRuntimeObservation {
            workload: UpgradeWorkload::Worker,
            current_release_id: "release-new".into(),
            current_state_version: "service-store.v2".into(),
            migration_completed: true,
            workflow_artifact_digests: vec![digest('4')],
        },
    );
    assert_eq!(admitted.decision, SupportDecision::Supported);
    assert!(admitted.claims_work);

    let mut tampered = plan;
    tampered.input.to_release_id = "forged-release".into();
    let blocked = evaluate_service_upgrade_admission(
        &tampered,
        &ServiceUpgradeRuntimeObservation {
            workload: UpgradeWorkload::Worker,
            current_release_id: "forged-release".into(),
            current_state_version: "service-store.v2".into(),
            migration_completed: true,
            workflow_artifact_digests: vec![digest('4')],
        },
    );
    assert_eq!(blocked.decision, SupportDecision::Blocked);
    assert_eq!(blocked.issues[0].code.as_str(), "ga_plan_integrity_invalid");
}

#[test]
fn contract_retirement_blocks_active_consumers_and_requires_exact_approval() {
    let blocked_input = ContractRetirementInput {
        system_graph_digest: digest('5'),
        environment_evidence_digest: digest('6'),
        evidence_fresh: true,
        contract_id: "support-http".into(),
        retiring_version: "v1".into(),
        replacement_version: "v2".into(),
        deprecation_window_complete: true,
        consumers: vec![ContractConsumerEvidence {
            consumer_id: "console".into(),
            active_version: Some("v1".into()),
            replacement_verified: false,
        }],
    };
    let blocked = plan_contract_retirement(&blocked_input);
    assert_eq!(blocked.decision, SupportDecision::Unsupported);
    assert!(!blocked.effects.retires_contract);

    let mut ready_input = blocked_input;
    ready_input.consumers[0].active_version = Some("v2".into());
    ready_input.consumers[0].replacement_verified = true;
    let ready = plan_contract_retirement(&ready_input);
    let wrong = RetirementApproval {
        plan_digest: digest('7'),
        approver: "release-owner".into(),
        reason: "approved local fixture retirement".into(),
    };
    assert_eq!(
        apply_contract_retirement(&ready, &ready_input, &wrong)
            .unwrap_err()
            .code
            .as_str(),
        "retirement_approval_invalid"
    );
    let approval = RetirementApproval {
        plan_digest: ready.plan_digest.clone(),
        ..wrong
    };
    let receipt = apply_contract_retirement(&ready, &ready_input, &approval).unwrap();
    assert!(receipt.retired);
    assert_eq!(receipt.contract_id, "support-http");
    assert!(contract_retirement_receipt_integrity_is_valid(&receipt));
    let mut tampered_receipt = receipt;
    tampered_receipt.replacement_version = "v3".into();
    assert!(!contract_retirement_receipt_integrity_is_valid(
        &tampered_receipt
    ));

    let mut tampered = ready;
    tampered.replacement_version = "v3".into();
    assert_eq!(
        apply_contract_retirement(&tampered, &ready_input, &approval)
            .unwrap_err()
            .code
            .as_str(),
        "ga_plan_integrity_invalid"
    );
}

#[test]
fn recovery_evidence_rejects_unexpected_continuation_and_requires_cleanup() {
    let evidence = evaluate_failure_scenario(FailureScenarioInput {
        scenario_id: "store-outage".into(),
        condition: FailureCondition::PostgresStoreUnavailable,
        expected: FailureOutcome::RejectWork,
        observations: vec![FailureObservation {
            subject: "support-api".into(),
            expected: None,
            outcome: FailureOutcome::Continue,
            evidence_digest: digest('8'),
        }],
        effects: vec!["request accepted while authority unavailable".into()],
        cleanup_complete: false,
        adapter_version: Some("postgres-18".into()),
        controlled_time_unix_ms: Some(1_721_600_000_000),
    });
    assert_eq!(evidence.decision, SupportDecision::Unsupported);
    assert!(
        evidence
            .issues
            .iter()
            .any(|issue| { issue.code.as_str() == "failure_unexpected_outcome" })
    );
    assert!(
        evidence
            .issues
            .iter()
            .any(|issue| { issue.code.as_str() == "failure_cleanup_incomplete" })
    );
}

#[test]
fn system_plane_outage_allows_established_data_plane_and_pauses_new_mutation() {
    let evidence = evaluate_failure_scenario(FailureScenarioInput {
        scenario_id: "system-plane-outage".into(),
        condition: FailureCondition::SystemPlaneUnavailable,
        expected: FailureOutcome::PauseCoordinatedMutation,
        observations: vec![
            FailureObservation {
                subject: "established-direct-request".into(),
                expected: Some(FailureOutcome::Continue),
                outcome: FailureOutcome::Continue,
                evidence_digest: digest('9'),
            },
            FailureObservation {
                subject: "new-promotion".into(),
                expected: Some(FailureOutcome::PauseCoordinatedMutation),
                outcome: FailureOutcome::PauseCoordinatedMutation,
                evidence_digest: digest('a'),
            },
        ],
        effects: vec!["one established business effect committed once".into()],
        cleanup_complete: true,
        adapter_version: None,
        controlled_time_unix_ms: Some(1_721_600_000_000),
    });
    assert_eq!(evidence.decision, SupportDecision::Supported);
}
