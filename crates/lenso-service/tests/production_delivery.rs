use std::collections::BTreeMap;

use lenso_service::{
    CanaryPlanInput, CanaryState, ConfigField, ConfigFieldActivation, ConfigFieldScope,
    ConfigFieldSensitivity, ConfigOperation, ConfigRevisionActivation, ConfigState,
    ConfigValueType, ContractCompatibilityInput, CoordinationOperationSubject,
    CoordinationOutageClaims, CoordinationOutageInput, CoordinationResumeState, CorsIntent,
    DataPlaneOperation, DeliveryDecision, DeliveryEffects, DeliveryEvidenceReference,
    DeliveryIssueCode, DeliveryPolicyInputs, DeliveryReliabilityContract, DependencyCriticality,
    DependencyReliability, DependencyReliabilityObservation, DeploymentAdapterKind,
    DeploymentEnvironmentBinding, DeploymentState, DeploymentWorkloadSettings,
    DeterministicCoordinationAuthorityProvider, DeterministicGatewayObservationProvider,
    DeterministicOperatorObservationAuthorityProvider, DeterministicReliabilityObservationProvider,
    DeterministicRollbackSafetyProvider, DeterministicSecretProvider, DeterministicTrustProvider,
    EdgeAuthentication, EdgeOperationVisibility, EdgeRoute, EdgeServiceOperation,
    EnvironmentVerificationInput, GatewayEnvironmentBinding, MigrationCompatibilityInput,
    MigrationPhase, OperatorObservationAuthorityProvider, PolicyEvaluationSurface, PolicyEvidence,
    ProductionEligibilityInput, PromotionPlanInput, PromotionState, ProtectedCoordinationOperation,
    RateIntent, ReleaseContractVersion, ReleaseMigration, ReleaseModule, ReleaseProvenance,
    ReleaseRetention, ReleaseRollbackConstraints, ReleaseRolloutGate, ReleaseTrustEvidence,
    ReleaseWorkloadRole, ReliabilityObservation, RollbackCompatibilityInput, RollbackOutcome,
    RollbackSafetyInput, SecretReference, SecretReferenceObservation, SecretReferenceStatus,
    SecurityContinuity, ServiceRelease, ServiceReleaseInput, WorkflowCompatibilityInput,
    WorkloadArtifact, apply_config_activation, apply_deployment, apply_promotion, apply_rollback,
    approve_coordination_resume, approve_promotion, assemble_service_release,
    attach_service_release_signature, attest_coordination_outage, attest_operator_observation,
    attest_production_eligibility_input, build_config_contract, build_config_revision,
    build_edge_contract, coordination_outage_evidence_digest,
    coordination_outage_evidence_integrity_is_valid, diff_service_releases,
    environment_verification_authority_is_valid, environment_verification_digest, evaluate_canary,
    evaluate_delivery_policy, evaluate_production_eligibility, extraction_input_digest,
    observe_deployment, observe_gateway, observe_rollback_convergence, observe_secret_reference,
    plan_canary, plan_config_activation, plan_deployment, plan_gateway_configuration,
    plan_promotion, plan_rollback, production_policy_pack, prove_system_plane_outage,
    reliability_contract_digest, resume_protected_operation, seal_reliability_observation,
    seal_rollback_safety_evidence, verify_service_release_trust, verify_staging_environment,
};

#[test]
fn observation_verifier_cannot_rebind_an_old_signature_to_a_new_challenge() {
    let authority_id = "kubernetes-api:test";
    let signer =
        lenso_service::Ed25519OperatorObservationAuthorityProvider::from_base64_private_keys([(
            authority_id,
            "nWGxne/9WmC6hEr0kuwsxERJxWl7MmkZcDusAxyuf2A=",
        )])
        .expect("RFC 8032 seed is valid");
    let verifier =
        lenso_service::Ed25519OperatorObservationAuthorityProvider::from_base64_public_keys([(
            authority_id,
            "11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo=",
        )])
        .expect("RFC 8032 public key is valid");
    let old_digest = "sha256:verification-time-observation";
    let proof = signer
        .sign(authority_id, old_digest)
        .expect("trusted adapter owns the signing key");
    assert!(verifier.verify(authority_id, old_digest, &proof));
    assert!(!verifier.verify(
        authority_id,
        "sha256:post-approval-challenge-observation",
        &proof,
    ));
    assert!(verifier.sign(authority_id, old_digest).is_none());
}

#[test]
fn service_release_identity_is_environment_independent_and_canonical() {
    let first = assemble_service_release(release_input()).expect("release should assemble");
    let mut reordered_input = release_input();
    reordered_input.modules.reverse();
    reordered_input.workloads.reverse();
    reordered_input.contract_versions.reverse();
    let reordered =
        assemble_service_release(reordered_input).expect("reordered release should assemble");

    assert_eq!(first.protocol, "lenso.service-release.v1");
    assert_eq!(first.release_id, reordered.release_id);
    assert_eq!(first.release_digest, reordered.release_digest);
    assert_eq!(
        first
            .workloads
            .iter()
            .map(|workload| workload.role)
            .collect::<Vec<_>>(),
        vec![
            ReleaseWorkloadRole::Api,
            ReleaseWorkloadRole::Migration,
            ReleaseWorkloadRole::Worker
        ]
    );

    let json = serde_json::to_value(&first).expect("release should serialize");
    let rendered = serde_json::to_string(&json).expect("release JSON should render");
    for forbidden in [
        "environment",
        "namespace",
        "replicas",
        "endpoint",
        "credential",
        "secretValue",
    ] {
        assert!(
            !rendered.contains(forbidden),
            "release must exclude environment field {forbidden}"
        );
    }

    let mut changed_input = release_input();
    changed_input.workloads[0].artifact_digest = digest("changed-api");
    let changed = assemble_service_release(changed_input).expect("changed release should assemble");
    assert_ne!(first.release_id, changed.release_id);
    assert_ne!(first.release_digest, changed.release_digest);
    let diff = diff_service_releases(&first, &changed);
    assert_eq!(diff.protocol, "lenso.service-release-diff.v1");
    assert_eq!(diff.entries.len(), 1);
    assert_eq!(diff.entries[0].subject, "workloads");

    let mut config_changed_input = release_input();
    config_changed_input.config_contract = evidence("config-contract:v2");
    let config_changed = assemble_service_release(config_changed_input)
        .expect("changed Config Contract should assemble");
    let config_diff = diff_service_releases(&first, &config_changed);
    assert_eq!(config_diff.entries[0].subject, "config.contract");
}

#[test]
fn release_trust_rejects_tampering_and_mismatched_provenance_before_mutation() {
    let provider = DeterministicTrustProvider::new([("ci:trusted", "local-test-key")]);
    let mut release = assemble_service_release(release_input()).expect("release should assemble");
    attach_service_release_signature(&mut release, &provider, "ci:trusted")
        .expect("trusted test signer should sign");

    let trusted = verify_service_release_trust(&release, &provider);
    assert_eq!(trusted.decision, DeliveryDecision::Passed);
    assert!(trusted.issues.is_empty());
    assert!(!trusted.effects.mutates_environment);
    assert!(!trusted.effects.appends_ledger);

    let mut tampered = release.clone();
    tampered.service_version = "5.0.1".to_owned();
    let rejected = verify_service_release_trust(&tampered, &provider);
    assert_eq!(rejected.decision, DeliveryDecision::Blocked);
    assert_eq!(rejected.issues[0].code, DeliveryIssueCode::ReleaseTampered);
    assert!(!rejected.effects.mutates_environment);

    let mut mismatched =
        assemble_service_release(release_input()).expect("release should assemble");
    mismatched.workloads[0].provenance.subject_digests = vec![digest("other-artifact")];
    let mismatched = assemble_service_release(ServiceReleaseInput {
        service_id: mismatched.service_id,
        service_version: mismatched.service_version,
        modules: mismatched.modules,
        workloads: mismatched.workloads,
        contract_versions: mismatched.contract_versions,
        config_contract: mismatched.config_contract,
        reliability_contract: mismatched.reliability_contract,
        migrations: mismatched.migrations,
        workflow_compatibility: mismatched.workflow_compatibility,
        verification_evidence: mismatched.verification_evidence,
        rollout_gates: mismatched.rollout_gates,
        rollback: mismatched.rollback,
        retention: mismatched.retention,
    })
    .expect("mismatched provenance remains a structurally valid release");
    let rejected = verify_service_release_trust(&mismatched, &provider);
    assert!(
        rejected
            .issues
            .iter()
            .any(|issue| issue.code == DeliveryIssueCode::ProvenanceSubjectMismatch)
    );
}

#[test]
fn config_revisions_redact_secrets_and_activation_is_stale_safe_and_idempotent() {
    let contract = build_config_contract(
        "config-contract:support:v1",
        vec![
            ConfigField {
                path: "MAX_CONCURRENCY".to_owned(),
                value_type: ConfigValueType::Integer,
                required: true,
                sensitivity: ConfigFieldSensitivity::Public,
                scope: ConfigFieldScope::Service,
                activation: ConfigFieldActivation::Hot,
                mutable: true,
            },
            ConfigField {
                path: "DATABASE_PASSWORD".to_owned(),
                value_type: ConfigValueType::String,
                required: true,
                sensitivity: ConfigFieldSensitivity::Sensitive,
                scope: ConfigFieldScope::Workload,
                activation: ConfigFieldActivation::Restart,
                mutable: true,
            },
        ],
    )
    .expect("config contract should build");
    let secret_provider = DeterministicSecretProvider::new(
        "local-test-provider",
        [(
            "secret-ref:support-db".to_owned(),
            SecretReferenceObservation {
                status: SecretReferenceStatus::Resolved,
                metadata: BTreeMap::from([("rotationStatus".to_owned(), "current".to_owned())]),
            },
        )],
    );
    let revision = build_config_revision(
        "service:support",
        &contract,
        BTreeMap::from([("MAX_CONCURRENCY".to_owned(), serde_json::json!(32))]),
        vec![SecretReference {
            reference_id: "secret-ref:support-db".to_owned(),
            provider: "local-test-provider".to_owned(),
            purpose: "DATABASE_PASSWORD".to_owned(),
            scope: "workload:support-api".to_owned(),
            status: SecretReferenceStatus::Resolved,
            metadata: BTreeMap::from([("rotationStatus".to_owned(), "current".to_owned())]),
        }],
        &secret_provider,
    )
    .expect("non-secret config revision should build");
    let rendered = serde_json::to_string(&revision).expect("revision should serialize");
    assert!(!rendered.contains("database-password-value"));
    assert!(!rendered.contains("secretValue"));
    assert!(rendered.contains("secret-ref:support-db"));

    let plaintext_metadata_provider = DeterministicSecretProvider::new(
        "local-test-provider",
        [(
            "secret-ref:support-db".to_owned(),
            SecretReferenceObservation {
                status: SecretReferenceStatus::Resolved,
                metadata: BTreeMap::from([(
                    "rotationStatus".to_owned(),
                    "database-password-value".to_owned(),
                )]),
            },
        )],
    );
    let plaintext_metadata = build_config_revision(
        "service:support",
        &contract,
        BTreeMap::from([("MAX_CONCURRENCY".to_owned(), serde_json::json!(32))]),
        vec![SecretReference {
            reference_id: "secret-ref:support-db".to_owned(),
            provider: "local-test-provider".to_owned(),
            purpose: "DATABASE_PASSWORD".to_owned(),
            scope: "workload:support-api".to_owned(),
            status: SecretReferenceStatus::Resolved,
            metadata: BTreeMap::from([(
                "rotationStatus".to_owned(),
                "database-password-value".to_owned(),
            )]),
        }],
        &plaintext_metadata_provider,
    )
    .expect_err("plaintext-shaped Secret metadata must fail closed");
    assert!(
        plaintext_metadata
            .iter()
            .any(|issue| issue.code == DeliveryIssueCode::PlaintextSecretDetected)
    );

    let plaintext = build_config_revision(
        "service:support",
        &contract,
        BTreeMap::from([
            ("MAX_CONCURRENCY".to_owned(), serde_json::json!(32)),
            (
                "DATABASE_PASSWORD".to_owned(),
                serde_json::json!("database-password-value"),
            ),
        ]),
        Vec::new(),
        &secret_provider,
    )
    .expect_err("plaintext sensitive config must fail");
    assert!(
        plaintext
            .iter()
            .any(|issue| issue.code == DeliveryIssueCode::PlaintextSecretDetected)
    );

    let mut state = ConfigState::new("production", 7);
    let stage = plan_config_activation(
        &state,
        &contract,
        &revision,
        &secret_provider,
        ConfigOperation::Stage,
    )
    .expect("stage should plan");
    let mut forged_stage = stage.clone();
    forged_stage.effects.mutates_configuration = true;
    assert!(!lenso_service::config_activation_plan_integrity_is_valid(
        &forged_stage
    ));
    assert_eq!(state.environment_revision, 7, "planning must not mutate");
    let staged = apply_config_activation(&mut state, &stage).expect("stage should apply");
    assert_eq!(staged.activation, ConfigRevisionActivation::Staged);

    let activate = plan_config_activation(
        &state,
        &contract,
        &revision,
        &secret_provider,
        ConfigOperation::Activate,
    )
    .expect("activation should plan");
    let activated =
        apply_config_activation(&mut state, &activate).expect("activation should apply");
    assert_eq!(activated.activation, ConfigRevisionActivation::Active);
    assert_eq!(
        state.active_revision_id.as_deref(),
        Some(revision.revision_id.as_str())
    );
    let repeated =
        apply_config_activation(&mut state, &activate).expect("completed activation is idempotent");
    assert_eq!(repeated, activated);
    let mut forged_completed_activation = activate.clone();
    forged_completed_activation.target_revision_id = "config-revision:forged".to_owned();
    assert!(apply_config_activation(&mut state, &forged_completed_activation).is_err());

    let previous_revision = build_config_revision(
        "service:support",
        &contract,
        BTreeMap::from([("MAX_CONCURRENCY".to_owned(), serde_json::json!(16))]),
        vec![SecretReference {
            reference_id: "secret-ref:support-db".to_owned(),
            provider: "local-test-provider".to_owned(),
            purpose: "DATABASE_PASSWORD".to_owned(),
            scope: "workload:support-api".to_owned(),
            status: SecretReferenceStatus::Resolved,
            metadata: BTreeMap::from([("rotationStatus".to_owned(), "current".to_owned())]),
        }],
        &secret_provider,
    )
    .expect("previous Config Revision should build");
    let mut rollback_state = ConfigState::new("production", 11);
    rollback_state.active_revision_id = Some(revision.revision_id.clone());
    rollback_state.previous_revision_id = Some(previous_revision.revision_id.clone());
    assert!(
        plan_config_activation(
            &rollback_state,
            &contract,
            &revision,
            &secret_provider,
            ConfigOperation::Rollback,
        )
        .is_err(),
        "rollback must not target an arbitrary valid revision"
    );
    let rollback = plan_config_activation(
        &rollback_state,
        &contract,
        &previous_revision,
        &secret_provider,
        ConfigOperation::Rollback,
    )
    .expect("explicit previous revision should plan");
    let mut drifted_rollback_state = rollback_state.clone();
    drifted_rollback_state.previous_revision_id = Some("config-revision:other".to_owned());
    assert!(apply_config_activation(&mut drifted_rollback_state, &rollback).is_err());
    let rolled_back = apply_config_activation(&mut rollback_state, &rollback)
        .expect("rollback to explicit previous revision should apply");
    assert_eq!(rolled_back.activation, ConfigRevisionActivation::RolledBack);
    assert_eq!(
        rollback_state.active_revision_id.as_deref(),
        Some(previous_revision.revision_id.as_str())
    );

    let future = plan_config_activation(
        &state,
        &contract,
        &revision,
        &secret_provider,
        ConfigOperation::Activate,
    )
    .expect("a current plan should build");
    state.environment_revision += 1;
    let stale = apply_config_activation(&mut state, &future).expect_err("stale plan must fail");
    assert_eq!(stale.issues[0].code, DeliveryIssueCode::StaleInput);
    assert!(!stale.effects.mutates_configuration);
}

#[test]
fn secret_provider_boundary_returns_only_safe_reference_status() {
    let provider = DeterministicSecretProvider::new(
        "vault",
        [(
            "support-db".to_owned(),
            SecretReferenceObservation {
                status: SecretReferenceStatus::Resolved,
                metadata: BTreeMap::from([
                    ("rotationRevision".to_owned(), "7".to_owned()),
                    ("rotationStatus".to_owned(), "current".to_owned()),
                ]),
            },
        )],
    );
    let reference =
        observe_secret_reference(&provider, "support-db", "DB_PASSWORD", "service:support");
    assert_eq!(reference.provider, "vault");
    assert_eq!(reference.status, SecretReferenceStatus::Resolved);
    assert_eq!(reference.metadata["rotationRevision"], "7");

    let unsafe_provider = DeterministicSecretProvider::new(
        "unsafe",
        [(
            "support-db".to_owned(),
            SecretReferenceObservation {
                status: SecretReferenceStatus::Resolved,
                metadata: BTreeMap::from([("secretValue".to_owned(), "must-not-leak".to_owned())]),
            },
        )],
    );
    let rejected = observe_secret_reference(
        &unsafe_provider,
        "support-db",
        "DB_PASSWORD",
        "service:support",
    );
    assert_eq!(rejected.status, SecretReferenceStatus::Unresolved);
    assert!(rejected.metadata.is_empty());
    assert!(
        !serde_json::to_string(&rejected)
            .unwrap()
            .contains("must-not-leak")
    );
}

#[test]
fn production_policy_is_byte_equivalent_across_surfaces_and_fails_closed() {
    let provider = DeterministicTrustProvider::new([("ci:trusted", "local-test-key")]);
    let mut release = assemble_service_release(release_input()).expect("release should assemble");
    attach_service_release_signature(&mut release, &provider, "ci:trusted")
        .expect("release should sign");
    let trust = verify_service_release_trust(&release, &provider);
    let contract = build_config_contract(
        "config-contract:support:v1",
        vec![ConfigField {
            path: "MAX_CONCURRENCY".to_owned(),
            value_type: ConfigValueType::Integer,
            required: true,
            sensitivity: ConfigFieldSensitivity::Public,
            scope: ConfigFieldScope::Service,
            activation: ConfigFieldActivation::Hot,
            mutable: true,
        }],
    )
    .expect("contract should build");
    let config = build_config_revision(
        "service:support",
        &contract,
        BTreeMap::from([("MAX_CONCURRENCY".to_owned(), serde_json::json!(32))]),
        Vec::new(),
        &test_secret_provider(),
    )
    .expect("config should build");
    let eligibility_input = safe_eligibility(&release);
    let eligibility = evaluate_production_eligibility(&eligibility_input, &release, &provider);
    assert_eq!(eligibility.decision, DeliveryDecision::Passed);
    let inputs = DeliveryPolicyInputs {
        release,
        trust,
        config_contract: contract,
        config,
        eligibility,
        eligibility_input,
    };
    let pack = production_policy_pack();
    let normalized = [
        PolicyEvaluationSurface::Local,
        PolicyEvaluationSurface::Ci,
        PolicyEvaluationSurface::Cli,
        PolicyEvaluationSurface::SystemPlane,
    ]
    .map(|surface| {
        serde_json::to_vec(&evaluate_delivery_policy(
            &pack,
            &inputs,
            &provider,
            &test_secret_provider(),
            surface,
        ))
        .expect("policy evidence should serialize")
    });
    assert!(normalized.windows(2).all(|pair| pair[0] == pair[1]));
    let mut empty_pack = pack.clone();
    empty_pack.rules.clear();
    let bypass = evaluate_delivery_policy(
        &empty_pack,
        &inputs,
        &provider,
        &test_secret_provider(),
        PolicyEvaluationSurface::SystemPlane,
    );
    assert_eq!(bypass.decision, DeliveryDecision::Blocked);
    assert!(!lenso_service::policy_pack_integrity_is_valid(&empty_pack));
    let mut forged_inputs = inputs.clone();
    forged_inputs.trust.release_id = "service-release:forged".to_owned();
    forged_inputs
        .eligibility
        .facts
        .insert("production.eligible".to_owned(), Some(true));
    let forged = evaluate_delivery_policy(
        &pack,
        &forged_inputs,
        &provider,
        &test_secret_provider(),
        PolicyEvaluationSurface::SystemPlane,
    );
    assert_eq!(forged.decision, DeliveryDecision::Blocked);
    let mut forged_signature_inputs = inputs.clone();
    forged_signature_inputs.release.signatures[0].signature = digest("forged-signature");
    let forged_signature = evaluate_delivery_policy(
        &pack,
        &forged_signature_inputs,
        &provider,
        &test_secret_provider(),
        PolicyEvaluationSurface::SystemPlane,
    );
    assert_eq!(forged_signature.decision, DeliveryDecision::Blocked);
    let mut forged_eligibility_inputs = inputs.clone();
    forged_eligibility_inputs
        .eligibility_input
        .workload_identity_production = None;
    let forged_eligibility = evaluate_delivery_policy(
        &pack,
        &forged_eligibility_inputs,
        &provider,
        &test_secret_provider(),
        PolicyEvaluationSurface::SystemPlane,
    );
    assert_eq!(forged_eligibility.decision, DeliveryDecision::Blocked);

    let mut unsafe_input = safe_eligibility(&inputs.release);
    unsafe_input.contracts[0].compatible = Some(false);
    unsafe_input.contracts[0].candidate_major = 2;
    unsafe_input.contracts[0].consumer_migration_evidence = false;
    unsafe_input.migrations.push(MigrationCompatibilityInput {
        migration_id: "support-0002-contract".to_owned(),
        lineage_id: "support-0002".to_owned(),
        sequence: 4,
        phase: MigrationPhase::Irreversible,
        verified: true,
    });
    let blocked_eligibility =
        evaluate_production_eligibility(&unsafe_input, &inputs.release, &provider);
    assert_eq!(blocked_eligibility.decision, DeliveryDecision::Blocked);
    assert!(
        blocked_eligibility
            .issues
            .iter()
            .any(|issue| issue.code == DeliveryIssueCode::ContractIncompatible)
    );
    assert!(
        blocked_eligibility
            .issues
            .iter()
            .any(|issue| issue.code == DeliveryIssueCode::RollbackUnsafe)
    );
    assert!(!blocked_eligibility.effects.mutates_environment);
    let mut cross_lineage = safe_eligibility(&inputs.release);
    cross_lineage.migrations.push(MigrationCompatibilityInput {
        migration_id: "support-0003-contract".to_owned(),
        lineage_id: "support-0003".to_owned(),
        sequence: 4,
        phase: MigrationPhase::Contract,
        verified: true,
    });
    assert_eq!(
        evaluate_production_eligibility(&cross_lineage, &inputs.release, &provider).decision,
        DeliveryDecision::Blocked
    );
}

#[test]
fn edge_and_deployment_adapters_are_explicit_portable_and_stale_safe() {
    let release = assemble_service_release(release_input()).expect("release should assemble");
    let provider = test_trust_provider();
    let contract = simple_config_contract();
    let config = simple_config_revision();
    let contract_digest = release
        .contract_versions
        .iter()
        .find(|contract| contract.contract_id == "support-http" && contract.version == "v1")
        .expect("support contract should exist")
        .artifact
        .digest
        .clone();
    let operations = vec![
        EdgeServiceOperation {
            contract_id: "support-http".to_owned(),
            contract_version: "v1".to_owned(),
            contract_digest: contract_digest.clone(),
            operation_id: "getTicket".to_owned(),
            visibility: EdgeOperationVisibility::PublicEligible,
            request_schema_reference: "schema:support-http:getTicket:request".to_owned(),
            response_schema_reference: "schema:support-http:getTicket:response".to_owned(),
        },
        EdgeServiceOperation {
            contract_id: "support-http".to_owned(),
            contract_version: "v1".to_owned(),
            contract_digest,
            operation_id: "adminRuntime".to_owned(),
            visibility: EdgeOperationVisibility::Internal,
            request_schema_reference: "schema:support-http:admin:request".to_owned(),
            response_schema_reference: "schema:support-http:admin:response".to_owned(),
        },
    ];
    let route = EdgeRoute {
        contract_id: "support-http".to_owned(),
        contract_version: "v1".to_owned(),
        operation_id: "getTicket".to_owned(),
        public_path: "/v1/tickets/{ticketId}".to_owned(),
        authentication: EdgeAuthentication::WorkloadOrUser,
        cors: CorsIntent {
            allowed_origins: vec!["https://support.example.test".to_owned()],
            allowed_methods: vec!["GET".to_owned()],
        },
        rate: RateIntent {
            requests: 100,
            window_seconds: 60,
        },
        deprecated: false,
    };
    let edge = build_edge_contract(
        &release,
        &operations,
        "ci:trusted",
        &provider,
        vec![route.clone()],
    )
    .expect("explicit public route should build");
    for malicious_cors in [
        CorsIntent {
            allowed_origins: vec!["https://support.example.test\"; return 200; #".to_owned()],
            allowed_methods: vec!["GET".to_owned()],
        },
        CorsIntent {
            allowed_origins: vec!["https://support.example.test".to_owned()],
            allowed_methods: vec!["GET\nDELETE".to_owned()],
        },
    ] {
        let mut unsafe_cors_route = route.clone();
        unsafe_cors_route.cors = malicious_cors;
        let blocked = build_edge_contract(
            &release,
            &operations,
            "ci:trusted",
            &provider,
            vec![unsafe_cors_route],
        )
        .expect_err("unsafe CORS values must be rejected before Edge authority is signed");
        assert_eq!(blocked[0].code, DeliveryIssueCode::EdgeExposureUnsafe);
    }
    for unsafe_path in [
        "/v1/tickets/{ticket id}",
        "/v1/tickets/{ticketId};return-200",
        "/v1//tickets/{ticketId}",
        "/v1/tickets/{ticketId}/",
        "/v1/tickets/\\escape",
    ] {
        let mut unsafe_path_route = route.clone();
        unsafe_path_route.public_path = unsafe_path.to_owned();
        let blocked = build_edge_contract(
            &release,
            &operations,
            "ci:trusted",
            &provider,
            vec![unsafe_path_route],
        )
        .expect_err("unsafe public path templates must be rejected before signing");
        assert_eq!(blocked[0].code, DeliveryIssueCode::EdgeExposureUnsafe);
    }
    let mut route_substitution = edge.clone();
    route_substitution.routes[0].public_path = "/v1/attacker/{ticketId}".to_owned();
    route_substitution.edge_contract_digest = extraction_input_digest(
        serde_json::to_vec(&(
            route_substitution.protocol.as_str(),
            route_substitution.service_id.as_str(),
            route_substitution.release_id.as_str(),
            route_substitution.release_digest.as_str(),
            route_substitution.operation_catalog_digest.as_str(),
            route_substitution.provider_id.as_str(),
            route_substitution.provider_proof.as_str(),
            route_substitution.routes.as_slice(),
        ))
        .expect("edge contract should serialize"),
    );
    route_substitution.edge_contract_id =
        format!("edge-contract:{}", route_substitution.edge_contract_digest);
    assert!(lenso_service::edge_contract_integrity_is_valid(
        &route_substitution
    ));
    assert!(!lenso_service::edge_contract_authority_is_valid(
        &route_substitution,
        &provider
    ));
    let gateway = plan_gateway_configuration(
        &edge,
        &provider,
        &GatewayEnvironmentBinding {
            environment: "staging".to_owned(),
            gateway_adapter: "local-validation".to_owned(),
            public_origin: "https://staging.support.example.test".to_owned(),
            expected_gateway_revision: 3,
        },
        None,
        &test_gateway_observation_provider(),
    )
    .expect("gateway plan should build");
    assert_eq!(gateway.routes.len(), 1);
    assert_eq!(gateway.routes[0].operation_id, "getTicket");
    assert!(!gateway.effects.mutates_gateway);
    for mutate in ["issues", "next_actions", "effects"] {
        let mut forged = gateway.clone();
        match mutate {
            "issues" => forged.issues.push(lenso_service::DeliveryIssue {
                code: DeliveryIssueCode::EdgeExposureUnsafe,
                message: "forged".to_owned(),
                evidence_references: Vec::new(),
                remediation: "forged".to_owned(),
                next_actions: Vec::new(),
            }),
            "next_actions" => forged.next_actions.push("forged".to_owned()),
            "effects" => forged.effects.mutates_gateway = true,
            _ => unreachable!(),
        }
        assert!(!lenso_service::gateway_plan_integrity_is_valid(&forged));
    }

    let mut unsafe_route = route;
    unsafe_route.operation_id = "adminRuntime".to_owned();
    let blocked = build_edge_contract(
        &release,
        &operations,
        "ci:trusted",
        &provider,
        vec![unsafe_route],
    )
    .expect_err("internal operation must stay private");
    assert_eq!(blocked[0].code, DeliveryIssueCode::EdgeExposureUnsafe);

    let binding = DeploymentEnvironmentBinding {
        environment: "staging".to_owned(),
        expected_environment_revision: 11,
        config_revision_id: config.revision_id.clone(),
        secret_reference_ids: Vec::new(),
        endpoints: BTreeMap::from([(
            "public".to_owned(),
            "https://staging.support.example.test".to_owned(),
        )]),
        placement: BTreeMap::from([("region".to_owned(), "local-1".to_owned())]),
        workloads: vec![
            DeploymentWorkloadSettings {
                workload_id: "support-api".to_owned(),
                replicas: 2,
                port: Some(8080),
                command: Vec::new(),
                health_path: Some("/health/ready".to_owned()),
                disruption_min_available: Some(1),
            },
            DeploymentWorkloadSettings {
                workload_id: "support-worker".to_owned(),
                replicas: 1,
                port: None,
                command: Vec::new(),
                health_path: None,
                disruption_min_available: Some(1),
            },
            DeploymentWorkloadSettings {
                workload_id: "support-migration".to_owned(),
                replicas: 1,
                port: None,
                command: Vec::new(),
                health_path: None,
                disruption_min_available: None,
            },
        ],
        adapter_inputs: BTreeMap::new(),
        gateway_plan_digest: gateway.plan_digest,
        policy_evidence_references: vec!["policy-evidence:test".to_owned()],
    };
    let mut secret_shaped_binding = binding.clone();
    secret_shaped_binding
        .adapter_inputs
        .insert("foo".to_owned(), "hunter2".to_owned());
    assert!(
        plan_deployment(
            &release,
            &contract,
            &config,
            &test_secret_provider(),
            &secret_shaped_binding,
            DeploymentAdapterKind::Kubernetes,
        )
        .expect_err("free-form adapter values must fail closed")
        .iter()
        .any(|issue| issue.code == DeliveryIssueCode::PlaintextSecretDetected)
    );
    let mut credential_endpoint = binding.clone();
    credential_endpoint.endpoints.insert(
        "public".to_owned(),
        "https://user:hunter2@staging.support.example.test".to_owned(),
    );
    assert!(
        plan_deployment(
            &release,
            &contract,
            &config,
            &test_secret_provider(),
            &credential_endpoint,
            DeploymentAdapterKind::Kubernetes,
        )
        .is_err()
    );
    let plans = [
        DeploymentAdapterKind::Local,
        DeploymentAdapterKind::ExternallyManaged,
        DeploymentAdapterKind::Kubernetes,
    ]
    .map(|adapter| {
        plan_deployment(
            &release,
            &contract,
            &config,
            &test_secret_provider(),
            &binding,
            adapter,
        )
        .expect("target should plan")
    });
    assert!(
        plans
            .iter()
            .all(|plan| plan.release_id == release.release_id)
    );
    assert!(plans.iter().all(|plan| !plan.effects.mutates_deployment));

    let mut state = DeploymentState::new("staging", 11);
    let receipt = apply_deployment(&mut state, &plans[2]).expect("deployment should apply");
    let repeated = apply_deployment(&mut state, &plans[2]).expect("apply should be idempotent");
    assert_eq!(receipt, repeated);
    let observation = observe_deployment(&plans[2], &receipt, true);
    assert!(!observation.drifted);
    assert_eq!(observation.observed_release_id, release.release_id);
    let mut changed_service = plans[2].clone();
    changed_service.service_id = "service:other".to_owned();
    assert!(!lenso_service::deployment_plan_integrity_is_valid(
        &changed_service
    ));
    let mut forged_capability = plans[2].clone();
    forged_capability.rollback_capable = !forged_capability.rollback_capable;
    assert!(!lenso_service::deployment_plan_integrity_is_valid(
        &forged_capability
    ));
    assert!(apply_deployment(&mut state, &forged_capability).is_err());
    let mut forged_effects = plans[2].clone();
    forged_effects.effects.mutates_deployment = true;
    assert!(!lenso_service::deployment_plan_integrity_is_valid(
        &forged_effects
    ));

    let stale_plan = plan_deployment(
        &release,
        &contract,
        &config,
        &test_secret_provider(),
        &DeploymentEnvironmentBinding {
            expected_environment_revision: state.environment_revision,
            ..binding
        },
        DeploymentAdapterKind::Kubernetes,
    )
    .expect("current deployment should plan");
    state.environment_revision += 1;
    let stale = apply_deployment(&mut state, &stale_plan).expect_err("stale apply must fail");
    assert_eq!(stale.issues[0].code, DeliveryIssueCode::StaleInput);
    assert!(!stale.effects.mutates_deployment);
}

#[test]
fn staging_verification_and_promotion_preserve_exact_digests_and_human_authority() {
    let (release, trust, config, policy) = trusted_delivery_context();
    let provider = test_trust_provider();
    let policy_inputs = trusted_policy_inputs(&release, &trust, &config);
    let gateway = simple_gateway_plan("staging", 5, "5.0.0");
    let binding = deployment_binding("staging", 17, &config, &gateway.plan_digest, &policy);
    let deployment_plan = plan_deployment(
        &release,
        &policy_inputs.config_contract,
        &config,
        &test_secret_provider(),
        &binding,
        DeploymentAdapterKind::Kubernetes,
    )
    .expect("staging deployment should plan");
    let mut staging_state = DeploymentState::new("staging", 17);
    let deployment =
        apply_deployment(&mut staging_state, &deployment_plan).expect("staging should deploy");
    let (observation, operator_observation) =
        attested_deployment_observation(&deployment_plan, &deployment);
    let verification_input = EnvironmentVerificationInput {
        release: release.clone(),
        trust,
        policy: policy.clone(),
        policy_inputs: policy_inputs.clone(),
        config: config.clone(),
        deployment_plan: deployment_plan.clone(),
        deployment,
        deployment_observation: observation.clone(),
        operator_observation: operator_observation.clone(),
        gateway_plan: gateway.clone(),
        gateway_observation: observe_gateway(
            &gateway,
            gateway.expected_gateway_revision,
            observation.source_observation_id.clone(),
            true,
            &test_gateway_observation_provider(),
        )
        .expect("Gateway authority should attest staging observation"),
        topology_digest: digest("staging-topology:r17"),
        workload_health: BTreeMap::from([
            ("support-api".to_owned(), true),
            ("support-worker".to_owned(), true),
            ("support-migration".to_owned(), true),
        ]),
        evidence_references: operator_evidence_references(&operator_observation),
        freshness_horizon_revision: 20,
    };
    let mut forged_input = verification_input.clone();
    forged_input.operator_observation.authority_proof = digest("forged-operator-proof");
    let forged = verify_staging_environment(
        forged_input,
        &provider,
        &test_secret_provider(),
        &test_operator_observation_provider(),
        &test_gateway_observation_provider(),
    );
    assert_eq!(forged.decision, DeliveryDecision::Blocked);
    let mut blocked_claims = operator_observation.claims.clone();
    blocked_claims.fresh = false;
    blocked_claims.state = "progressing".to_owned();
    blocked_claims.rollout_phase = "observing".to_owned();
    blocked_claims.decision = DeliveryDecision::Blocked;
    let blocked_attestation = attest_operator_observation(
        blocked_claims,
        "kubernetes-api:test-cluster",
        &test_operator_observation_provider(),
    )
    .expect("authority may attest a truthful blocked observation");
    let substituted_observation = lenso_service::observe_deployment_adapter(
        &deployment_plan,
        &verification_input.deployment.receipt_id,
        &blocked_attestation.observation_id,
        &verification_input.deployment.release_id,
        &verification_input.deployment.release_digest,
        &verification_input.deployment.workload_digests,
        &verification_input.deployment.config_revision_id,
        true,
    );
    let mut substituted_input = verification_input.clone();
    substituted_input.operator_observation = blocked_attestation.clone();
    substituted_input.deployment_observation = substituted_observation;
    substituted_input.gateway_observation = observe_gateway(
        &gateway,
        gateway.expected_gateway_revision,
        blocked_attestation.observation_id.clone(),
        true,
        &test_gateway_observation_provider(),
    )
    .expect("Gateway authority should attest the paired source identity");
    substituted_input.evidence_references = operator_evidence_references(&blocked_attestation);
    let substituted = verify_staging_environment(
        substituted_input,
        &provider,
        &test_secret_provider(),
        &test_operator_observation_provider(),
        &test_gateway_observation_provider(),
    );
    assert_eq!(substituted.decision, DeliveryDecision::Blocked);
    assert!(
        substituted
            .issues
            .iter()
            .any(|issue| issue.code == DeliveryIssueCode::ObservationStale)
    );
    let mut forged_gateway_input = verification_input.clone();
    forged_gateway_input.gateway_observation.provider_proof = digest("forged-gateway-proof");
    let forged_gateway = verify_staging_environment(
        forged_gateway_input,
        &provider,
        &test_secret_provider(),
        &test_operator_observation_provider(),
        &test_gateway_observation_provider(),
    );
    assert_eq!(forged_gateway.decision, DeliveryDecision::Blocked);
    let verification = verify_staging_environment(
        verification_input,
        &provider,
        &test_secret_provider(),
        &test_operator_observation_provider(),
        &test_gateway_observation_provider(),
    );
    assert_eq!(verification.decision, DeliveryDecision::Passed);
    assert_eq!(verification.release_digest, release.release_digest);
    let substituted_gateway_observation = observe_gateway(
        &gateway,
        gateway.expected_gateway_revision,
        "operator-observation:sha256:unrelated",
        false,
        &test_gateway_observation_provider(),
    )
    .expect("the Gateway provider may truthfully attest a stale unrelated observation");
    let mut substituted_gateway_verification = verification.clone();
    substituted_gateway_verification.gateway_observation_id =
        substituted_gateway_observation.observation_id;
    substituted_gateway_verification.gateway_configuration_identity =
        substituted_gateway_observation.configuration_identity;
    substituted_gateway_verification.gateway_observation_revision =
        substituted_gateway_observation.revision;
    substituted_gateway_verification.gateway_observation_observed_after =
        substituted_gateway_observation.observed_after;
    substituted_gateway_verification.gateway_observation_fresh =
        substituted_gateway_observation.fresh;
    substituted_gateway_verification.gateway_observation_provider_id =
        substituted_gateway_observation.provider_id;
    substituted_gateway_verification.gateway_observation_provider_proof =
        substituted_gateway_observation.provider_proof;
    substituted_gateway_verification.verification_digest =
        environment_verification_digest(&substituted_gateway_verification);
    substituted_gateway_verification.verification_id = format!(
        "environment-verification:{}",
        substituted_gateway_verification.verification_digest
    );
    assert!(
        !environment_verification_authority_is_valid(
            &substituted_gateway_verification,
            &test_operator_observation_provider(),
            &test_gateway_observation_provider(),
        ),
        "a valid Gateway proof cannot replace the exact fresh source observation"
    );
    let wrong_revision_gateway = observe_gateway(
        &gateway,
        gateway.expected_gateway_revision.saturating_add(1),
        verification.operator_observation_id.clone(),
        true,
        &test_gateway_observation_provider(),
    )
    .expect("the provider may attest the observed wrong revision");
    assert!(
        !wrong_revision_gateway.fresh,
        "the issuer must derive non-fresh from a revision that differs from the exact plan"
    );
    let mut wrong_revision_verification = verification.clone();
    wrong_revision_verification.gateway_observation_id = wrong_revision_gateway.observation_id;
    wrong_revision_verification.gateway_observation_revision = wrong_revision_gateway.revision;
    wrong_revision_verification.gateway_observation_fresh = wrong_revision_gateway.fresh;
    wrong_revision_verification.gateway_observation_provider_proof =
        wrong_revision_gateway.provider_proof;
    wrong_revision_verification.verification_digest =
        environment_verification_digest(&wrong_revision_verification);
    wrong_revision_verification.verification_id = format!(
        "environment-verification:{}",
        wrong_revision_verification.verification_digest
    );
    assert!(
        !environment_verification_authority_is_valid(
            &wrong_revision_verification,
            &test_operator_observation_provider(),
            &test_gateway_observation_provider(),
        ),
        "an exact-plan Gateway proof with the wrong revision must remain blocked"
    );

    let production_gateway = simple_gateway_plan("production", 9, "5.0.0");
    let production_binding = deployment_binding(
        "production",
        31,
        &config,
        &production_gateway.plan_digest,
        &policy,
    );
    let production_deployment = plan_deployment(
        &release,
        &policy_inputs.config_contract,
        &config,
        &test_secret_provider(),
        &production_binding,
        DeploymentAdapterKind::Kubernetes,
    )
    .expect("production deployment should plan");
    let promotion_input = PromotionPlanInput {
        source: verification,
        target_deployment: production_deployment,
        target_gateway: production_gateway,
        policy,
        policy_inputs,
        source_environment_revision: staging_state.environment_revision,
        target_environment_revision: 31,
        target_topology_digest: digest("production-topology:r31"),
        secret_reference_ids: Vec::new(),
        evidence_references: vec!["approval-input:change-42".to_owned()],
    };
    let mut stale_source_revision = promotion_input.clone();
    stale_source_revision.source_environment_revision = stale_source_revision
        .source_environment_revision
        .saturating_sub(1);
    assert!(
        plan_promotion(
            stale_source_revision,
            &provider,
            &test_secret_provider(),
            &test_operator_observation_provider(),
            &test_gateway_observation_provider(),
        )
        .is_err(),
        "Promotion must bind the caller revision to the signed source environment revision"
    );
    let promotion = plan_promotion(
        promotion_input,
        &provider,
        &test_secret_provider(),
        &test_operator_observation_provider(),
        &test_gateway_observation_provider(),
    )
    .expect("exact release should promote");
    assert_eq!(promotion.release_digest, release.release_digest);
    assert!(promotion.workload_digests.values().all(|digest| {
        release
            .workloads
            .iter()
            .any(|item| &item.artifact_digest == digest)
    }));
    assert!(!promotion.effects.mutates_environment);
    let mut forged_promotion = promotion.clone();
    forged_promotion.effects.mutates_environment = true;
    assert!(!lenso_service::promotion_plan_integrity_is_valid(
        &forged_promotion
    ));

    let approval_authority = lenso_service::DeterministicPromotionApprovalAuthority::new(
        "production-release-operator",
        ["user:alice"],
        "test-only-approval-key",
    );
    let protected_evidence = lenso_service::PromotionProtectedEvidence::from_plan(&promotion);
    let approval = approve_promotion(&promotion, "user:alice", &approval_authority)
        .expect("human operator should approve exact plan");
    let mut promotion_state = PromotionState::new("production", 31);
    let mut production_state = DeploymentState::new("production", 31);
    let receipt = apply_promotion(
        &mut promotion_state,
        &mut production_state,
        &promotion,
        &approval,
        &protected_evidence,
        &approval_authority,
    )
    .expect("approved promotion should apply");
    let repeated = apply_promotion(
        &mut promotion_state,
        &mut production_state,
        &promotion,
        &approval,
        &protected_evidence,
        &approval_authority,
    )
    .expect("completed promotion should be idempotent");
    assert_eq!(receipt, repeated);
    assert_eq!(receipt.release_digest, release.release_digest);

    let mut wrong_approval = approval;
    wrong_approval.plan_digest = digest("different-plan");
    assert!(
        apply_promotion(
            &mut promotion_state,
            &mut production_state,
            &promotion,
            &wrong_approval,
            &protected_evidence,
            &approval_authority,
        )
        .is_err()
    );
    let mut rejected_state = PromotionState::new("production", 31);
    let mut rejected_deployment = DeploymentState::new("production", 31);
    let rejected = apply_promotion(
        &mut rejected_state,
        &mut rejected_deployment,
        &promotion,
        &wrong_approval,
        &protected_evidence,
        &approval_authority,
    )
    .expect_err("stale approval must fail before mutation");
    assert_eq!(rejected.issues[0].code, DeliveryIssueCode::ApprovalInvalid);
    assert_eq!(rejected_state.environment_revision, 31);
    assert_eq!(rejected_deployment.environment_revision, 31);
    let mut forged_approval =
        approve_promotion(&promotion, "user:alice", &approval_authority).unwrap();
    forged_approval.authority_proof = digest("forged-authority-proof");
    let rejected = apply_promotion(
        &mut rejected_state,
        &mut rejected_deployment,
        &promotion,
        &forged_approval,
        &protected_evidence,
        &approval_authority,
    )
    .expect_err("forged approval authority must fail before mutation");
    assert_eq!(rejected.issues[0].code, DeliveryIssueCode::ApprovalInvalid);
}

#[test]
fn canary_requires_complete_service_reliability_evidence_before_expansion() {
    let (
        release,
        policy,
        policy_inputs,
        verification,
        production,
        previous,
        previous_receipt,
        previous_observation,
        production_receipt,
        production_observation,
        previous_release,
        previous_policy,
        previous_policy_inputs,
        previous_gateway,
    ) = canary_context();
    let provider = test_trust_provider();
    let contract = reliability_contract();
    let previous_gateway_observation = observe_gateway(
        &previous_gateway,
        previous_gateway.expected_gateway_revision,
        previous_observation.source_observation_id.clone(),
        true,
        &test_gateway_observation_provider(),
    )
    .expect("Gateway authority should attest previous observation");
    let plan = plan_canary(
        CanaryPlanInput {
            release: release.clone(),
            production_deployment: production.clone(),
            production_deployment_receipt: production_receipt.clone(),
            production_deployment_observation: production_observation.clone(),
            reliability_contract: contract.clone(),
            policy: policy.clone(),
            policy_inputs: policy_inputs.clone(),
            environment_verification: verification.clone(),
            previous_known_good_deployment: previous.clone(),
            previous_known_good_receipt: previous_receipt.clone(),
            previous_known_good_observation: previous_observation.clone(),
            previous_known_good_release: previous_release.clone(),
            previous_known_good_policy: previous_policy.clone(),
            previous_known_good_policy_inputs: previous_policy_inputs.clone(),
            previous_known_good_gateway: previous_gateway.clone(),
            previous_known_good_gateway_observation: previous_gateway_observation.clone(),
            initial_percent: 10,
            maximum_percent: 50,
        },
        &provider,
        &test_secret_provider(),
        &test_operator_observation_provider(),
        &test_gateway_observation_provider(),
    )
    .expect("bounded canary should plan from exact evidence");
    let mut relaxed_contract = reliability_contract();
    relaxed_contract.maximum_latency_p99_ms += 1;
    let rejected = plan_canary(
        CanaryPlanInput {
            release: release.clone(),
            production_deployment: production,
            production_deployment_receipt: production_receipt,
            production_deployment_observation: production_observation.clone(),
            reliability_contract: relaxed_contract,
            policy,
            policy_inputs,
            environment_verification: verification,
            previous_known_good_deployment: previous,
            previous_known_good_receipt: previous_receipt,
            previous_known_good_observation: previous_observation,
            previous_known_good_release: previous_release,
            previous_known_good_policy: previous_policy,
            previous_known_good_policy_inputs: previous_policy_inputs,
            previous_known_good_gateway: previous_gateway,
            previous_known_good_gateway_observation: previous_gateway_observation,
            initial_percent: 10,
            maximum_percent: 50,
        },
        &provider,
        &test_secret_provider(),
        &test_operator_observation_provider(),
        &test_gateway_observation_provider(),
    );
    assert_eq!(
        rejected.expect_err("relaxed thresholds must not override the signed release contract")[0]
            .code,
        DeliveryIssueCode::DeploymentInputInvalid
    );
    let mut mismatched_gateway_observation = plan.clone();
    mismatched_gateway_observation.previous_known_good_gateway_configuration_identity =
        digest("different-previous-gateway");
    assert!(!lenso_service::canary_plan_integrity_is_valid(
        &mismatched_gateway_observation
    ));
    let mut forged_canary_plan = plan.clone();
    forged_canary_plan.effects.mutates_deployment = true;
    assert!(!lenso_service::canary_plan_integrity_is_valid(
        &forged_canary_plan
    ));
    let mut state = CanaryState::new(plan.plan_id.clone());

    let mut tampered_plan = plan.clone();
    tampered_plan.maximum_percent = 100;
    let rejected = evaluate_canary(
        &mut state,
        &tampered_plan,
        healthy_reliability_observation(),
        &test_reliability_provider(),
    );
    assert_eq!(rejected.outcome, lenso_service::CanaryOutcome::Pause);
    assert!(state.observations.is_empty());
    assert!(state.decisions.is_empty());

    let mut forged_metrics = healthy_reliability_observation();
    forged_metrics.latency_p99_ms = Some(900);
    let mut forged_observation = seal_reliability_observation(
        &plan,
        &production_observation,
        &test_reliability_provider(),
        forged_metrics,
    )
    .expect("collector should seal observation");
    forged_observation.collector_proof = digest("forged-collector-proof");
    let mut forged_state = CanaryState::new(plan.plan_id.clone());
    let forged = evaluate_canary(
        &mut forged_state,
        &plan,
        forged_observation,
        &test_reliability_provider(),
    );
    assert_eq!(forged.outcome, lenso_service::CanaryOutcome::Pause);
    assert_eq!(forged.next_percent, plan.initial_percent);
    assert!(
        !forged
            .issues
            .iter()
            .any(|issue| issue.code == DeliveryIssueCode::CanaryBreach)
    );

    let mut generic_health_only = healthy_reliability_observation();
    generic_health_only.availability_basis_points = None;
    let generic_health_only = seal_reliability_observation(
        &plan,
        &production_observation,
        &test_reliability_provider(),
        generic_health_only,
    )
    .expect("collector should seal observation");
    let blocked = evaluate_canary(
        &mut state,
        &plan,
        generic_health_only,
        &test_reliability_provider(),
    );
    assert_eq!(blocked.decision, DeliveryDecision::Blocked);
    assert_eq!(blocked.outcome, lenso_service::CanaryOutcome::Pause);
    assert!(
        blocked
            .issues
            .iter()
            .any(|issue| { issue.code == DeliveryIssueCode::ReliabilityEvidenceMissing })
    );
    assert_eq!(blocked.next_percent, 10);

    let healthy_observation = seal_reliability_observation(
        &plan,
        &production_observation,
        &test_reliability_provider(),
        healthy_reliability_observation(),
    )
    .expect("collector should seal observation");
    let healthy = evaluate_canary(
        &mut state,
        &plan,
        healthy_observation.clone(),
        &test_reliability_provider(),
    );
    assert_eq!(healthy.decision, DeliveryDecision::Passed);
    assert_eq!(healthy.outcome, lenso_service::CanaryOutcome::Expand);
    assert_eq!(healthy.next_percent, 20);
    let replayed = evaluate_canary(
        &mut state,
        &plan,
        healthy_observation,
        &test_reliability_provider(),
    );
    assert_eq!(replayed, healthy);
    assert_eq!(state.current_percent, 20);
    assert_eq!(state.observations.len(), 2);
    assert_eq!(state.decisions.len(), 2);
    let mut forged_current_percent = state.clone();
    forged_current_percent.current_percent = 10;
    let mut next_observation = healthy_reliability_observation();
    next_observation.observed_revision = 33;
    let next_observation = seal_reliability_observation(
        &plan,
        &production_observation,
        &test_reliability_provider(),
        next_observation,
    )
    .expect("collector should seal a new observation");
    let rejected = evaluate_canary(
        &mut forged_current_percent,
        &plan,
        next_observation,
        &test_reliability_provider(),
    );
    assert_eq!(rejected.outcome, lenso_service::CanaryOutcome::Pause);
    assert_eq!(forged_current_percent.observations.len(), 2);
    assert_eq!(forged_current_percent.decisions.len(), 2);
    let mut corrupted_history = state.clone();
    corrupted_history.decisions[0].next_percent = 99;
    let replay = corrupted_history.observations[1].clone();
    let rejected = evaluate_canary(
        &mut corrupted_history,
        &plan,
        replay,
        &test_reliability_provider(),
    );
    assert_eq!(rejected.outcome, lenso_service::CanaryOutcome::Pause);

    let mut degraded = healthy_reliability_observation();
    degraded.dependencies[1].available = false;
    degraded.dependencies[1].active_degraded_mode = Some("cached_search".to_owned());
    let degraded = evaluate_canary(
        &mut state,
        &plan,
        seal_reliability_observation(
            &plan,
            &production_observation,
            &test_reliability_provider(),
            degraded,
        )
        .expect("collector should seal observation"),
        &test_reliability_provider(),
    );
    assert_eq!(degraded.decision, DeliveryDecision::Advisory);
    assert_eq!(degraded.outcome, lenso_service::CanaryOutcome::HoldDegraded);

    let mut duplicated_dependency = healthy_reliability_observation();
    duplicated_dependency.dependencies.insert(
        0,
        DependencyReliabilityObservation {
            dependency_id: "payments".to_owned(),
            available: false,
            active_degraded_mode: None,
        },
    );
    let duplicate = evaluate_canary(
        &mut state,
        &plan,
        seal_reliability_observation(
            &plan,
            &production_observation,
            &test_reliability_provider(),
            duplicated_dependency,
        )
        .expect("collector should seal observation"),
        &test_reliability_provider(),
    );
    assert_eq!(duplicate.outcome, lenso_service::CanaryOutcome::Pause);
    assert!(
        duplicate
            .issues
            .iter()
            .any(|issue| { issue.code == DeliveryIssueCode::ReliabilityEvidenceMissing })
    );

    let mut breached = healthy_reliability_observation();
    breached.latency_p99_ms = Some(900);
    let breached = evaluate_canary(
        &mut state,
        &plan,
        seal_reliability_observation(
            &plan,
            &production_observation,
            &test_reliability_provider(),
            breached,
        )
        .expect("collector should seal observation"),
        &test_reliability_provider(),
    );
    assert_eq!(breached.decision, DeliveryDecision::Blocked);
    assert_eq!(breached.outcome, lenso_service::CanaryOutcome::Rollback);
    assert_eq!(breached.next_percent, 0);
    assert_eq!(state.current_percent, 0);
    assert!(
        breached
            .issues
            .iter()
            .any(|issue| { issue.code == DeliveryIssueCode::CanaryBreach })
    );
    assert_eq!(state.observations.len(), 5);
    assert_eq!(state.decisions.len(), 5);
    let mut post_rollback_observation = healthy_reliability_observation();
    post_rollback_observation.observed_revision += 10;
    post_rollback_observation.freshness_horizon_revision += 10;
    let terminal = evaluate_canary(
        &mut state,
        &plan,
        seal_reliability_observation(
            &plan,
            &production_observation,
            &test_reliability_provider(),
            post_rollback_observation,
        )
        .expect("collector should seal post-rollback observation"),
        &test_reliability_provider(),
    );
    assert_eq!(terminal.outcome, lenso_service::CanaryOutcome::Pause);
    assert_eq!(terminal.current_percent, 0);
    assert_eq!(terminal.next_percent, 0);
    assert_eq!(state.current_percent, 0);
    assert_eq!(state.observations.len(), 5);
    assert_eq!(state.decisions.len(), 5);
}

#[test]
fn rollback_is_automatic_only_for_a_verified_non_destructive_path() {
    let (
        release,
        policy,
        policy_inputs,
        verification,
        production,
        previous,
        previous_receipt,
        previous_observation,
        production_receipt,
        production_observation,
        previous_release,
        previous_policy,
        previous_policy_inputs,
        previous_gateway,
    ) = canary_context();
    let provider = test_trust_provider();
    let previous_gateway_observation = observe_gateway(
        &previous_gateway,
        previous_gateway.expected_gateway_revision,
        previous_observation.source_observation_id.clone(),
        true,
        &test_gateway_observation_provider(),
    )
    .expect("Gateway authority should attest previous observation");
    let plan = plan_canary(
        CanaryPlanInput {
            release,
            production_deployment: production.clone(),
            production_deployment_receipt: production_receipt,
            production_deployment_observation: production_observation.clone(),
            reliability_contract: reliability_contract(),
            policy,
            policy_inputs,
            environment_verification: verification,
            previous_known_good_deployment: previous.clone(),
            previous_known_good_receipt: previous_receipt.clone(),
            previous_known_good_observation: previous_observation.clone(),
            previous_known_good_release: previous_release,
            previous_known_good_policy: previous_policy,
            previous_known_good_policy_inputs: previous_policy_inputs,
            previous_known_good_gateway: previous_gateway.clone(),
            previous_known_good_gateway_observation: previous_gateway_observation.clone(),
            initial_percent: 10,
            maximum_percent: 50,
        },
        &provider,
        &test_secret_provider(),
        &test_operator_observation_provider(),
        &test_gateway_observation_provider(),
    )
    .expect("canary should plan");
    let mut canary_state = CanaryState::new(plan.plan_id.clone());
    let mut observation = healthy_reliability_observation();
    observation.error_budget_used_basis_points = Some(700);
    let breach_observation = seal_reliability_observation(
        &plan,
        &production_observation,
        &test_reliability_provider(),
        observation,
    )
    .expect("collector should seal observation");
    let breach = evaluate_canary(
        &mut canary_state,
        &plan,
        breach_observation.clone(),
        &test_reliability_provider(),
    );
    assert_eq!(breach.outcome, lenso_service::CanaryOutcome::Rollback);

    let safety_provider = test_rollback_safety_provider();
    let safe = seal_rollback_safety_evidence(
        &plan,
        &production,
        &previous,
        52,
        &safety_provider,
        RollbackSafetyInput {
            migrations_reversible: true,
            destructive_changes_absent: true,
            workflows_downgrade_safe: true,
            config_revision_compatible: true,
            secret_references_resolvable: true,
            edge_configuration_compatible: true,
            adapter_recovery_complete: true,
            policy_approved: true,
            evidence_references: vec!["rollback-proof:42".to_owned()],
        },
    )
    .expect("safety provider should attest recovery inputs");
    let gateway = simple_gateway_plan("production", 9, "5.0.0");
    let previous_gateway = simple_gateway_plan("production", 8, "4.9.0");
    let rollback = plan_rollback(
        &plan,
        &breach,
        &breach_observation,
        &test_reliability_provider(),
        &production,
        &gateway,
        &previous,
        &previous_gateway,
        52,
        safe.clone(),
        &safety_provider,
        &provider,
    )
    .expect("safe breach should produce a rollback plan");
    let mut forged_safety = safe;
    forged_safety.migrations_reversible = false;
    assert!(
        plan_rollback(
            &plan,
            &breach,
            &breach_observation,
            &test_reliability_provider(),
            &production,
            &simple_gateway_plan("production", 9, "5.0.0"),
            &previous,
            &simple_gateway_plan("production", 8, "4.9.0"),
            52,
            forged_safety,
            &safety_provider,
            &provider,
        )
        .is_err()
    );
    let mut state = lenso_service::RollbackState::new(
        production.environment.clone(),
        production.release_id.clone(),
        production.config_revision_id.clone(),
        52,
        10,
    );
    let gateway_observation = observe_gateway(
        &previous_gateway,
        previous_gateway.expected_gateway_revision,
        previous_observation.source_observation_id.clone(),
        true,
        &test_gateway_observation_provider(),
    )
    .expect("Gateway authority should attest rollback observation");
    let convergence = observe_rollback_convergence(
        &rollback,
        &previous,
        &previous_receipt,
        &previous_observation,
        &previous_gateway,
        &gateway_observation,
        &provider,
        &test_gateway_observation_provider(),
        &safety_provider,
        vec!["operator-observation:rollback".to_owned()],
    )
    .expect("trusted adapter receipts should seal convergence");
    let mut forged_observation = previous_observation.clone();
    forged_observation.receipt_id = "deployment-receipt:forged".to_owned();
    assert!(
        observe_rollback_convergence(
            &rollback,
            &previous,
            &previous_receipt,
            &forged_observation,
            &previous_gateway,
            &gateway_observation,
            &provider,
            &test_gateway_observation_provider(),
            &safety_provider,
            Vec::new(),
        )
        .is_err()
    );
    assert!(
        apply_rollback(
            &mut state,
            &rollback,
            None,
            &safety_provider,
            "automation:canary-controller"
        )
        .is_err()
    );
    let receipt = apply_rollback(
        &mut state,
        &rollback,
        Some(&convergence),
        &safety_provider,
        "automation:canary-controller",
    )
    .expect("verified safe rollback should apply");
    let repeated = apply_rollback(
        &mut state,
        &rollback,
        Some(&convergence),
        &safety_provider,
        "automation:canary-controller",
    )
    .expect("rollback should be idempotent");
    assert_eq!(receipt, repeated);
    assert!(
        apply_rollback(
            &mut state,
            &rollback,
            None,
            &safety_provider,
            "automation:canary-controller"
        )
        .is_err()
    );
    assert!(
        apply_rollback(
            &mut state,
            &rollback,
            Some(&convergence),
            &safety_provider,
            ""
        )
        .is_err()
    );
    assert_eq!(receipt.outcome, RollbackOutcome::RolledBack);
    assert_eq!(state.active_release_id, previous.release_id);
    assert_eq!(state.active_config_revision_id, previous.config_revision_id);
    assert!(receipt.effects.mutates_configuration);
    assert!(
        rollback
            .prohibited_actions
            .contains(&"delete_service_data".to_owned())
    );
    let mut tampered_rollback = rollback.clone();
    tampered_rollback.automatic_allowed = false;
    let mut untouched = lenso_service::RollbackState::new(
        production.environment.clone(),
        production.release_id.clone(),
        production.config_revision_id.clone(),
        52,
        10,
    );
    assert!(
        apply_rollback(
            &mut untouched,
            &tampered_rollback,
            Some(&convergence),
            &safety_provider,
            "automation:canary-controller"
        )
        .is_err()
    );
    untouched.environment_revision = 53;
    assert!(
        apply_rollback(
            &mut untouched,
            &rollback,
            Some(&convergence),
            &safety_provider,
            "automation:canary-controller"
        )
        .is_err()
    );

    let unsafe_evidence = seal_rollback_safety_evidence(
        &plan,
        &production,
        &previous,
        52,
        &safety_provider,
        RollbackSafetyInput {
            migrations_reversible: false,
            destructive_changes_absent: true,
            workflows_downgrade_safe: true,
            config_revision_compatible: true,
            secret_references_resolvable: true,
            edge_configuration_compatible: true,
            adapter_recovery_complete: true,
            policy_approved: true,
            evidence_references: vec!["rollback-proof:unsafe".to_owned()],
        },
    )
    .expect("safety provider should attest unsafe recovery inputs");
    let paused_plan = plan_rollback(
        &plan,
        &breach,
        &breach_observation,
        &test_reliability_provider(),
        &production,
        &gateway,
        &previous,
        &previous_gateway,
        52,
        unsafe_evidence,
        &safety_provider,
        &provider,
    )
    .expect("unsafe recovery should still produce an explainable pause plan");
    let mut paused_state = lenso_service::RollbackState::new(
        production.environment,
        production.release_id,
        production.config_revision_id,
        52,
        10,
    );
    let paused = apply_rollback(
        &mut paused_state,
        &paused_plan,
        None,
        &safety_provider,
        "automation:canary-controller",
    )
    .expect("unsafe recovery should pause without destructive effects");
    assert_eq!(paused.outcome, RollbackOutcome::InterventionRequired);
    assert_eq!(paused_state.exposure_percent, 10);
    assert_eq!(
        paused_state.active_release_id,
        paused_plan.failed_release_id
    );
    assert!(!paused.effects.mutates_deployment);
    assert!(paused.approval_boundary_required);
}

#[test]
fn signed_release_can_prohibit_automatic_rollback() {
    let (
        release,
        policy,
        policy_inputs,
        verification,
        production,
        previous,
        previous_receipt,
        previous_observation,
        production_receipt,
        production_observation,
        previous_release,
        previous_policy,
        previous_policy_inputs,
        previous_gateway,
    ) = canary_context_with_automatic_rollback(false);
    let provider = test_trust_provider();
    let previous_gateway_observation = observe_gateway(
        &previous_gateway,
        previous_gateway.expected_gateway_revision,
        previous_observation.source_observation_id.clone(),
        true,
        &test_gateway_observation_provider(),
    )
    .expect("Gateway authority should attest previous observation");
    let canary = plan_canary(
        CanaryPlanInput {
            release: release.clone(),
            production_deployment: production.clone(),
            production_deployment_receipt: production_receipt,
            production_deployment_observation: production_observation.clone(),
            reliability_contract: reliability_contract(),
            policy,
            policy_inputs,
            environment_verification: verification,
            previous_known_good_deployment: previous.clone(),
            previous_known_good_receipt: previous_receipt,
            previous_known_good_observation: previous_observation,
            previous_known_good_release: previous_release,
            previous_known_good_policy: previous_policy,
            previous_known_good_policy_inputs: previous_policy_inputs,
            previous_known_good_gateway: previous_gateway.clone(),
            previous_known_good_gateway_observation: previous_gateway_observation,
            initial_percent: 10,
            maximum_percent: 50,
        },
        &provider,
        &test_secret_provider(),
        &test_operator_observation_provider(),
        &test_gateway_observation_provider(),
    )
    .expect("release should still permit a bounded canary");
    assert!(!canary.release_rollback_constraints.automatic_allowed);
    let mut raw = healthy_reliability_observation();
    raw.error_budget_used_basis_points = Some(700);
    let breach_observation = seal_reliability_observation(
        &canary,
        &production_observation,
        &test_reliability_provider(),
        raw,
    )
    .expect("collector should seal the breach");
    let mut state = CanaryState::new(canary.plan_id.clone());
    let breach = evaluate_canary(
        &mut state,
        &canary,
        breach_observation.clone(),
        &test_reliability_provider(),
    );
    let safety_provider = test_rollback_safety_provider();
    let safety = seal_rollback_safety_evidence(
        &canary,
        &production,
        &previous,
        52,
        &safety_provider,
        RollbackSafetyInput {
            migrations_reversible: true,
            destructive_changes_absent: true,
            workflows_downgrade_safe: true,
            config_revision_compatible: true,
            secret_references_resolvable: true,
            edge_configuration_compatible: true,
            adapter_recovery_complete: true,
            policy_approved: true,
            evidence_references: vec!["rollback-proof:release-policy".to_owned()],
        },
    )
    .expect("safety evidence should be valid but cannot override release policy");
    let rollback = plan_rollback(
        &canary,
        &breach,
        &breach_observation,
        &test_reliability_provider(),
        &production,
        &simple_gateway_plan_for_release(&release, "production", 9),
        &previous,
        &previous_gateway,
        52,
        safety,
        &safety_provider,
        &provider,
    )
    .expect("release policy should produce an explainable intervention plan");
    assert!(!rollback.automatic_allowed);
    assert!(!rollback.release_rollback_constraints.automatic_allowed);
    assert_eq!(
        rollback.issues[0].code,
        DeliveryIssueCode::RollbackIncomplete
    );
}

#[test]
fn converged_data_plane_survives_coordination_loss_and_resumes_without_duplicates() {
    let (_, _, _, _, production, ..) = canary_context();
    let mut deployment_state = DeploymentState::new(
        production.environment.clone(),
        production.expected_environment_revision,
    );
    let receipt = apply_deployment(&mut deployment_state, &production)
        .expect("production deployment should converge");
    let (observation, operator_observation) =
        attested_deployment_observation(&production, &receipt);
    let operations = [
        DataPlaneOperation::DirectRequest,
        DataPlaneOperation::Event,
        DataPlaneOperation::DurableWorkflow,
        DataPlaneOperation::Inbox,
        DataPlaneOperation::Outbox,
        DataPlaneOperation::Timer,
        DataPlaneOperation::Retry,
        DataPlaneOperation::Compensation,
        DataPlaneOperation::RuntimeStory,
    ]
    .into_iter()
    .map(|operation| (operation, true))
    .collect();
    let outage_provider = test_coordination_outage_provider();
    let approval_provider = test_coordination_approval_provider();
    let outage_observation = attest_coordination_outage(
        CoordinationOutageClaims {
            protocol: lenso_service::COORDINATION_OUTAGE_OBSERVATION_PROTOCOL.to_owned(),
            deployment_plan_id: production.plan_id.clone(),
            deployment_plan_digest: production.plan_digest.clone(),
            deployment_receipt_id: receipt.receipt_id.clone(),
            deployment_observation_id: observation.observation_id.clone(),
            operator_observation_id: operator_observation.observation_id.clone(),
            operator_observation_digest: operator_observation.observation_digest.clone(),
            environment_revision_after: receipt.environment_revision_after,
            release_id: receipt.release_id.clone(),
            release_digest: receipt.release_digest.clone(),
            config_revision_id: receipt.config_revision_id.clone(),
            system_plane_available: false,
            runtime_console_available: false,
            autonomous_service_running: true,
            selected_gateway_running: true,
            selected_transport_running: true,
            gateway_is_data_plane: true,
            gateway_requires_live_policy: false,
            gateway_requires_live_release_metadata: false,
            last_valid_config_revision_available: true,
            secret_provider_lease_valid: true,
            secret_rotation_policy_preserved: true,
            operation_results: operations,
            security: SecurityContinuity {
                workload_identity_enforced: true,
                tenant_context_enforced: true,
                call_policy_enforced: true,
                service_authorization_enforced: true,
            },
            durable_checkpoint_id: "outage-checkpoint:production:53".to_owned(),
            evidence_references: vec!["runtime-story:outage-window-53".to_owned()],
        },
        "data-plane-probe:test",
        &outage_provider,
    )
    .expect("trusted Data Plane probe should attest the outage");
    let input = CoordinationOutageInput {
        deployment_plan: production.clone(),
        deployment: receipt,
        deployment_observation: observation,
        operator_observation,
        outage_observation,
    };
    let proof = prove_system_plane_outage(
        input.clone(),
        &outage_provider,
        &test_operator_observation_provider(),
    );
    assert_eq!(proof.decision, DeliveryDecision::Passed);
    assert_eq!(proof.continued_operations.len(), 9);
    assert_eq!(proof.blocked_operations.len(), 4);
    assert_eq!(proof.config_revision_id, production.config_revision_id);
    assert!(proof.blocked_operations.iter().any(|item| {
        item.operation == ProtectedCoordinationOperation::Promotion && !item.next_actions.is_empty()
    }));
    assert!(!proof.effects.mutates_environment);
    assert!(coordination_outage_evidence_integrity_is_valid(
        &proof,
        &outage_provider,
        &test_operator_observation_provider(),
    ));

    let mut blocked_input = input.clone();
    let mut failing_claims = blocked_input.outage_observation.claims.clone();
    failing_claims.system_plane_available = true;
    blocked_input.outage_observation =
        attest_coordination_outage(failing_claims, "data-plane-probe:test", &outage_provider)
            .expect("the provider may truthfully attest a failing outage observation");
    let blocked_proof = prove_system_plane_outage(
        blocked_input,
        &outage_provider,
        &test_operator_observation_provider(),
    );
    assert_eq!(blocked_proof.decision, DeliveryDecision::Blocked);
    let mut laundered_proof = blocked_proof;
    laundered_proof.decision = DeliveryDecision::Passed;
    laundered_proof.issues.clear();
    laundered_proof.proof_digest = coordination_outage_evidence_digest(&laundered_proof);
    laundered_proof.proof_id =
        format!("coordination-outage-proof:{}", laundered_proof.proof_digest);
    assert!(
        !coordination_outage_evidence_integrity_is_valid(
            &laundered_proof,
            &outage_provider,
            &test_operator_observation_provider(),
        ),
        "signed failing claims cannot be laundered by recomputing unsigned derived fields"
    );

    let operation_subject = CoordinationOperationSubject::DeploymentMutation(production.clone());
    let approval = approve_coordination_resume(
        &proof,
        "protected-operation:deployment:54",
        &operation_subject,
        54,
        "coordination-authority:test",
        &outage_provider,
        &test_operator_observation_provider(),
        &approval_provider,
    )
    .expect("restored authority should sign the exact resume operation");
    let mut resume_state = CoordinationResumeState::default();
    let first = resume_protected_operation(
        &mut resume_state,
        &proof,
        &approval,
        &operation_subject,
        54,
        &outage_provider,
        &test_operator_observation_provider(),
        &approval_provider,
    )
    .expect("restored coordination should resume from durable evidence");
    let repeated = resume_protected_operation(
        &mut resume_state,
        &proof,
        &approval,
        &operation_subject,
        54,
        &outage_provider,
        &test_operator_observation_provider(),
        &approval_provider,
    )
    .expect("resume should be idempotent");
    assert_eq!(first, repeated);
    assert_eq!(resume_state.receipts.len(), 1);
    assert_eq!(first.effects, DeliveryEffects::default());
    let recovered = resume_protected_operation(
        &mut CoordinationResumeState::default(),
        &proof,
        &approval,
        &operation_subject,
        54,
        &outage_provider,
        &test_operator_observation_provider(),
        &approval_provider,
    )
    .expect("a process restart should recover the same deterministic authorization");
    assert_eq!(first, recovered);
    assert!(
        resume_protected_operation(
            &mut resume_state,
            &proof,
            &approval,
            &operation_subject,
            53,
            &outage_provider,
            &test_operator_observation_provider(),
            &approval_provider,
        )
        .is_err(),
        "idempotent replay must still require the signed coordination revision"
    );
    let mut forged_approval = approval.clone();
    forged_approval.authority_proof = digest("forged-resume-approval");
    assert!(
        resume_protected_operation(
            &mut resume_state,
            &proof,
            &forged_approval,
            &operation_subject,
            54,
            &outage_provider,
            &test_operator_observation_provider(),
            &approval_provider,
        )
        .is_err(),
        "an arbitrary approval string or forged authority proof must not resume mutation"
    );
    let mut substituted_plan = production.clone();
    substituted_plan.config_revision_id = "config-revision:substituted".to_owned();
    let substituted_subject = CoordinationOperationSubject::DeploymentMutation(substituted_plan);
    assert!(
        resume_protected_operation(
            &mut resume_state,
            &proof,
            &approval,
            &substituted_subject,
            54,
            &outage_provider,
            &test_operator_observation_provider(),
            &approval_provider,
        )
        .is_err(),
        "the same approval must not authorize a different actual mutation payload"
    );
    let mut forged_input = input;
    forged_input.outage_observation.authority_proof = digest("forged-outage-authority");
    let forged_proof = prove_system_plane_outage(
        forged_input,
        &outage_provider,
        &test_operator_observation_provider(),
    );
    assert_eq!(forged_proof.decision, DeliveryDecision::Blocked);
    assert!(
        resume_protected_operation(
            &mut resume_state,
            &forged_proof,
            &approval,
            &operation_subject,
            54,
            &outage_provider,
            &test_operator_observation_provider(),
            &approval_provider,
        )
        .is_err(),
        "forged outage evidence must never authorize a resume"
    );
    assert_eq!(resume_state.receipts.len(), 1);
}

type CanaryContext = (
    ServiceRelease,
    PolicyEvidence,
    DeliveryPolicyInputs,
    lenso_service::EnvironmentVerification,
    lenso_service::DeploymentPlan,
    lenso_service::DeploymentPlan,
    lenso_service::DeploymentReceipt,
    lenso_service::DeploymentObservation,
    lenso_service::DeploymentReceipt,
    lenso_service::DeploymentObservation,
    ServiceRelease,
    PolicyEvidence,
    DeliveryPolicyInputs,
    lenso_service::GatewayConfigurationPlan,
);

fn canary_context() -> CanaryContext {
    canary_context_with_automatic_rollback(true)
}

fn canary_context_with_automatic_rollback(automatic_allowed: bool) -> CanaryContext {
    let (release, trust, config, policy) =
        trusted_delivery_context_with_automatic_rollback(automatic_allowed);
    let provider = test_trust_provider();
    let policy_inputs = trusted_policy_inputs(&release, &trust, &config);
    let staging_gateway = simple_gateway_plan_for_release(&release, "staging", 5);
    let staging_plan = plan_deployment(
        &release,
        &policy_inputs.config_contract,
        &config,
        &test_secret_provider(),
        &deployment_binding(
            "staging",
            17,
            &config,
            &staging_gateway.plan_digest,
            &policy,
        ),
        DeploymentAdapterKind::Kubernetes,
    )
    .expect("staging should plan");
    let mut staging_state = DeploymentState::new("staging", 17);
    let staging_receipt =
        apply_deployment(&mut staging_state, &staging_plan).expect("staging should deploy");
    let (staging_observation, operator_observation) =
        attested_deployment_observation(&staging_plan, &staging_receipt);
    let verification = verify_staging_environment(
        EnvironmentVerificationInput {
            release: release.clone(),
            trust,
            policy: policy.clone(),
            policy_inputs: policy_inputs.clone(),
            config: config.clone(),
            deployment_plan: staging_plan.clone(),
            deployment: staging_receipt,
            deployment_observation: staging_observation.clone(),
            operator_observation: operator_observation.clone(),
            gateway_plan: staging_gateway.clone(),
            gateway_observation: observe_gateway(
                &staging_gateway,
                staging_gateway.expected_gateway_revision,
                staging_observation.source_observation_id,
                true,
                &test_gateway_observation_provider(),
            )
            .expect("Gateway authority should attest staging observation"),
            topology_digest: digest("staging-topology:r17"),
            workload_health: BTreeMap::from([
                ("support-api".to_owned(), true),
                ("support-worker".to_owned(), true),
                ("support-migration".to_owned(), true),
            ]),
            evidence_references: operator_evidence_references(&operator_observation),
            freshness_horizon_revision: 20,
        },
        &provider,
        &test_secret_provider(),
        &test_operator_observation_provider(),
        &test_gateway_observation_provider(),
    );
    let production_gateway = simple_gateway_plan_for_release(&release, "production", 9);
    let production_binding = deployment_binding(
        "production",
        31,
        &config,
        &production_gateway.plan_digest,
        &policy,
    );
    let production = plan_deployment(
        &release,
        &policy_inputs.config_contract,
        &config,
        &test_secret_provider(),
        &production_binding,
        DeploymentAdapterKind::Kubernetes,
    )
    .expect("production should plan");
    let mut previous_input = release_input();
    previous_input.service_version = "4.9.0".to_owned();
    for workload in &mut previous_input.workloads {
        workload.artifact_digest = digest(&format!(
            "previous:{}:{}",
            workload.workload_id, previous_input.service_version
        ));
        workload.provenance.subject_digests = vec![workload.artifact_digest.clone()];
    }
    let mut previous_release =
        assemble_service_release(previous_input).expect("previous release should assemble");
    attach_service_release_signature(&mut previous_release, &provider, "ci:trusted")
        .expect("previous release should sign");
    let previous_trust = verify_service_release_trust(&previous_release, &provider);
    let previous_config = build_config_revision(
        "service:support",
        &simple_config_contract(),
        BTreeMap::from([("MAX_CONCURRENCY".to_owned(), serde_json::json!(16))]),
        Vec::new(),
        &test_secret_provider(),
    )
    .expect("previous config should build");
    let previous_eligibility_input = safe_eligibility(&previous_release);
    let previous_policy_inputs = DeliveryPolicyInputs {
        release: previous_release.clone(),
        trust: previous_trust,
        config_contract: simple_config_contract(),
        config: previous_config.clone(),
        eligibility: evaluate_production_eligibility(
            &previous_eligibility_input,
            &previous_release,
            &provider,
        ),
        eligibility_input: previous_eligibility_input,
    };
    let previous_policy = evaluate_delivery_policy(
        &production_policy_pack(),
        &previous_policy_inputs,
        &provider,
        &test_secret_provider(),
        PolicyEvaluationSurface::Local,
    );
    let previous_gateway = simple_gateway_plan_for_release(&previous_release, "production", 8);
    let previous_binding = deployment_binding(
        "production",
        31,
        &previous_config,
        &previous_gateway.plan_digest,
        &previous_policy,
    );
    let previous = plan_deployment(
        &previous_release,
        &policy_inputs.config_contract,
        &previous_config,
        &test_secret_provider(),
        &previous_binding,
        DeploymentAdapterKind::Kubernetes,
    )
    .expect("previous deployment should plan");
    let mut previous_state = DeploymentState::new("production", 31);
    let previous_receipt = apply_deployment(&mut previous_state, &previous)
        .expect("previous deployment should have an observed baseline");
    let previous_observation = observe_deployment(&previous, &previous_receipt, true);
    let mut production_state = DeploymentState::new("production", 31);
    let production_receipt = apply_deployment(&mut production_state, &production)
        .expect("production candidate should have a rollout observation");
    let production_observation = observe_deployment(&production, &production_receipt, true);
    (
        release,
        policy,
        policy_inputs,
        verification,
        production,
        previous,
        previous_receipt,
        previous_observation,
        production_receipt,
        production_observation,
        previous_release,
        previous_policy,
        previous_policy_inputs,
        previous_gateway,
    )
}

fn reliability_contract() -> DeliveryReliabilityContract {
    DeliveryReliabilityContract {
        protocol: "lenso.reliability-contract.v1".to_owned(),
        contract_id: "reliability:support:v1".to_owned(),
        minimum_observation_seconds: 300,
        minimum_sample_count: 100,
        minimum_availability_basis_points: 9_950,
        maximum_latency_p99_ms: 500,
        maximum_error_budget_used_basis_points: 500,
        maximum_queue_backlog: 100,
        maximum_workflow_backlog: 50,
        maximum_timer_lag_ms: 2_000,
        maximum_retry_exhaustion: 2,
        maximum_compensation_pressure: 2,
        minimum_healthy_failure_domains: 2,
        dependencies: vec![
            DependencyReliability {
                dependency_id: "payments".to_owned(),
                criticality: DependencyCriticality::Critical,
                allowed_degraded_modes: Vec::new(),
            },
            DependencyReliability {
                dependency_id: "search".to_owned(),
                criticality: DependencyCriticality::Degradable,
                allowed_degraded_modes: vec!["cached_search".to_owned()],
            },
            DependencyReliability {
                dependency_id: "analytics".to_owned(),
                criticality: DependencyCriticality::Optional,
                allowed_degraded_modes: Vec::new(),
            },
        ],
    }
}

fn healthy_reliability_observation() -> ReliabilityObservation {
    ReliabilityObservation {
        protocol: String::new(),
        observation_id: String::new(),
        canary_plan_id: String::new(),
        canary_plan_digest: String::new(),
        release_id: String::new(),
        release_digest: String::new(),
        environment: String::new(),
        deployment_plan_id: String::new(),
        deployment_plan_digest: String::new(),
        deployment_observation_id: String::new(),
        collector_id: "test-reliability-adapter".to_owned(),
        collector_proof: String::new(),
        observed_revision: 32,
        freshness_horizon_revision: 40,
        fresh: true,
        observation_window_seconds: 600,
        sample_count: 1_000,
        generic_process_healthy: true,
        workload_readiness: BTreeMap::from([
            ("support-api".to_owned(), true),
            ("support-worker".to_owned(), true),
            ("support-migration".to_owned(), true),
        ]),
        workload_liveness: BTreeMap::from([
            ("support-api".to_owned(), true),
            ("support-worker".to_owned(), true),
            ("support-migration".to_owned(), true),
        ]),
        availability_basis_points: Some(9_999),
        latency_p99_ms: Some(120),
        error_budget_used_basis_points: Some(40),
        queue_backlog: Some(4),
        workflow_backlog: Some(2),
        timer_lag_ms: Some(100),
        retry_exhaustion: Some(0),
        compensation_pressure: Some(0),
        dependencies: vec![
            DependencyReliabilityObservation {
                dependency_id: "payments".to_owned(),
                available: true,
                active_degraded_mode: None,
            },
            DependencyReliabilityObservation {
                dependency_id: "search".to_owned(),
                available: true,
                active_degraded_mode: None,
            },
            DependencyReliabilityObservation {
                dependency_id: "analytics".to_owned(),
                available: true,
                active_degraded_mode: None,
            },
        ],
        failure_domains: BTreeMap::from([("zone-a".to_owned(), true), ("zone-b".to_owned(), true)]),
        scaling_check_passed: Some(true),
        disruption_check_passed: Some(true),
        availability_check_passed: Some(true),
        evidence_references: vec!["runtime-story:canary-window-60".to_owned()],
    }
}

fn trusted_delivery_context() -> (
    ServiceRelease,
    ReleaseTrustEvidence,
    lenso_service::ConfigRevision,
    PolicyEvidence,
) {
    trusted_delivery_context_with_automatic_rollback(true)
}

fn trusted_delivery_context_with_automatic_rollback(
    automatic_allowed: bool,
) -> (
    ServiceRelease,
    ReleaseTrustEvidence,
    lenso_service::ConfigRevision,
    PolicyEvidence,
) {
    let provider = test_trust_provider();
    let mut input = release_input();
    input.rollback.automatic_allowed = automatic_allowed;
    let mut release = assemble_service_release(input).expect("release should assemble");
    attach_service_release_signature(&mut release, &provider, "ci:trusted")
        .expect("release should sign");
    let trust = verify_service_release_trust(&release, &provider);
    let config = simple_config_revision();
    let policy_inputs = trusted_policy_inputs(&release, &trust, &config);
    let policy = evaluate_delivery_policy(
        &production_policy_pack(),
        &policy_inputs,
        &provider,
        &test_secret_provider(),
        PolicyEvaluationSurface::Local,
    );
    (release, trust, config, policy)
}

fn test_trust_provider() -> DeterministicTrustProvider {
    DeterministicTrustProvider::new([("ci:trusted", "local-test-key")])
}

fn test_secret_provider() -> DeterministicSecretProvider {
    DeterministicSecretProvider::new("test-secret-provider", std::iter::empty())
}

fn test_operator_observation_provider() -> DeterministicOperatorObservationAuthorityProvider {
    DeterministicOperatorObservationAuthorityProvider::new([(
        "kubernetes-api:test-cluster",
        "test-operator-observation-key",
    )])
}

fn test_gateway_observation_provider() -> DeterministicGatewayObservationProvider {
    DeterministicGatewayObservationProvider::new(
        "gateway-api:test-cluster",
        "test-gateway-observation-key",
    )
}

fn test_coordination_outage_provider() -> DeterministicCoordinationAuthorityProvider {
    DeterministicCoordinationAuthorityProvider::new([(
        "data-plane-probe:test",
        "test-outage-observation-key",
    )])
}

fn test_coordination_approval_provider() -> DeterministicCoordinationAuthorityProvider {
    DeterministicCoordinationAuthorityProvider::new([(
        "coordination-authority:test",
        "test-coordination-approval-key",
    )])
}

fn attested_deployment_observation(
    plan: &lenso_service::DeploymentPlan,
    receipt: &lenso_service::DeploymentReceipt,
) -> (
    lenso_service::DeploymentObservation,
    lenso_service::OperatorObservationAttestation,
) {
    let workload_health = plan
        .workloads
        .iter()
        .map(|workload| (workload.workload_id.clone(), true))
        .collect();
    let provisional = lenso_service::observe_deployment_adapter(
        plan,
        &receipt.receipt_id,
        "operator-observation:pending",
        &receipt.release_id,
        &receipt.release_digest,
        &receipt.workload_digests,
        &receipt.config_revision_id,
        true,
    );
    let attestation = attest_operator_observation(
        lenso_service::operator_observation_claims_from_deployment(
            plan,
            &provisional,
            workload_health,
        ),
        "kubernetes-api:test-cluster",
        &test_operator_observation_provider(),
    )
    .expect("test Operator authority should attest the observation");
    let observation = lenso_service::observe_deployment_adapter(
        plan,
        &receipt.receipt_id,
        &attestation.observation_id,
        &receipt.release_id,
        &receipt.release_digest,
        &receipt.workload_digests,
        &receipt.config_revision_id,
        true,
    );
    (observation, attestation)
}

fn operator_evidence_references(
    attestation: &lenso_service::OperatorObservationAttestation,
) -> Vec<String> {
    vec![
        attestation.observation_id.clone(),
        attestation.observation_digest.clone(),
        format!(
            "operator-observation-authority:{}",
            attestation.authority_id
        ),
        format!("operator-observation-proof:{}", attestation.authority_proof),
    ]
}

fn test_reliability_provider() -> DeterministicReliabilityObservationProvider {
    DeterministicReliabilityObservationProvider::new([(
        "test-reliability-adapter",
        "test-reliability-key",
    )])
}

fn test_rollback_safety_provider() -> DeterministicRollbackSafetyProvider {
    DeterministicRollbackSafetyProvider::new("test-rollback-safety", "test-rollback-safety-key")
}

fn trusted_policy_inputs(
    release: &ServiceRelease,
    trust: &ReleaseTrustEvidence,
    config: &lenso_service::ConfigRevision,
) -> DeliveryPolicyInputs {
    let eligibility_input = safe_eligibility(release);
    DeliveryPolicyInputs {
        release: release.clone(),
        trust: trust.clone(),
        config_contract: simple_config_contract(),
        config: config.clone(),
        eligibility: evaluate_production_eligibility(
            &eligibility_input,
            release,
            &test_trust_provider(),
        ),
        eligibility_input,
    }
}

fn simple_gateway_plan(
    environment: &str,
    revision: u64,
    service_version: &str,
) -> lenso_service::GatewayConfigurationPlan {
    let mut release_input = release_input();
    release_input.service_version = service_version.to_owned();
    if service_version != "5.0.0" {
        for workload in &mut release_input.workloads {
            workload.artifact_digest = digest(&format!(
                "previous:{}:{service_version}",
                workload.workload_id
            ));
            workload.provenance.subject_digests = vec![workload.artifact_digest.clone()];
        }
    }
    let release = assemble_service_release(release_input).expect("release should assemble");
    simple_gateway_plan_for_release(&release, environment, revision)
}

fn simple_gateway_plan_for_release(
    release: &ServiceRelease,
    environment: &str,
    revision: u64,
) -> lenso_service::GatewayConfigurationPlan {
    let provider = test_trust_provider();
    let contract_digest = release
        .contract_versions
        .iter()
        .find(|contract| contract.contract_id == "support-http" && contract.version == "v1")
        .expect("support contract should exist")
        .artifact
        .digest
        .clone();
    let operations = [EdgeServiceOperation {
        contract_id: "support-http".to_owned(),
        contract_version: "v1".to_owned(),
        contract_digest,
        operation_id: "getTicket".to_owned(),
        visibility: EdgeOperationVisibility::PublicEligible,
        request_schema_reference: "schema:support-http:getTicket:request".to_owned(),
        response_schema_reference: "schema:support-http:getTicket:response".to_owned(),
    }];
    let edge = build_edge_contract(
        &release,
        &operations,
        "ci:trusted",
        &provider,
        vec![EdgeRoute {
            contract_id: "support-http".to_owned(),
            contract_version: "v1".to_owned(),
            operation_id: "getTicket".to_owned(),
            public_path: "/v1/tickets/{ticketId}".to_owned(),
            authentication: EdgeAuthentication::WorkloadOrUser,
            cors: CorsIntent {
                allowed_origins: vec![format!("https://{environment}.support.example.test")],
                allowed_methods: vec!["GET".to_owned()],
            },
            rate: RateIntent {
                requests: 100,
                window_seconds: 60,
            },
            deprecated: false,
        }],
    )
    .expect("edge should build");
    plan_gateway_configuration(
        &edge,
        &provider,
        &GatewayEnvironmentBinding {
            environment: environment.to_owned(),
            gateway_adapter: "local-validation".to_owned(),
            public_origin: format!("https://{environment}.support.example.test"),
            expected_gateway_revision: revision,
        },
        None,
        &test_gateway_observation_provider(),
    )
    .expect("gateway should plan")
}

fn deployment_binding(
    environment: &str,
    revision: u64,
    config: &lenso_service::ConfigRevision,
    gateway_plan_digest: &str,
    policy: &PolicyEvidence,
) -> DeploymentEnvironmentBinding {
    DeploymentEnvironmentBinding {
        environment: environment.to_owned(),
        expected_environment_revision: revision,
        config_revision_id: config.revision_id.clone(),
        secret_reference_ids: Vec::new(),
        endpoints: BTreeMap::from([(
            "public".to_owned(),
            format!("https://{environment}.support.example.test"),
        )]),
        placement: BTreeMap::from([("region".to_owned(), "local-1".to_owned())]),
        workloads: vec![
            DeploymentWorkloadSettings {
                workload_id: "support-api".to_owned(),
                replicas: 2,
                port: Some(8080),
                command: Vec::new(),
                health_path: Some("/health/ready".to_owned()),
                disruption_min_available: Some(1),
            },
            DeploymentWorkloadSettings {
                workload_id: "support-worker".to_owned(),
                replicas: 1,
                port: None,
                command: Vec::new(),
                health_path: None,
                disruption_min_available: Some(1),
            },
            DeploymentWorkloadSettings {
                workload_id: "support-migration".to_owned(),
                replicas: 1,
                port: None,
                command: Vec::new(),
                health_path: None,
                disruption_min_available: None,
            },
        ],
        adapter_inputs: BTreeMap::new(),
        gateway_plan_digest: gateway_plan_digest.to_owned(),
        policy_evidence_references: vec![
            policy.evidence_id.clone(),
            policy.evidence_digest.clone(),
        ],
    }
}

fn simple_config_revision() -> lenso_service::ConfigRevision {
    let contract = simple_config_contract();
    build_config_revision(
        "service:support",
        &contract,
        BTreeMap::from([("MAX_CONCURRENCY".to_owned(), serde_json::json!(32))]),
        Vec::new(),
        &test_secret_provider(),
    )
    .expect("config revision should build")
}

fn simple_config_contract() -> lenso_service::ConfigContractDefinition {
    build_config_contract(
        "config-contract:support:v1",
        vec![ConfigField {
            path: "MAX_CONCURRENCY".to_owned(),
            value_type: ConfigValueType::Integer,
            required: true,
            sensitivity: ConfigFieldSensitivity::Public,
            scope: ConfigFieldScope::Service,
            activation: ConfigFieldActivation::Hot,
            mutable: true,
        }],
    )
    .expect("config contract should build")
}

fn safe_eligibility(release: &ServiceRelease) -> ProductionEligibilityInput {
    let input = ProductionEligibilityInput {
        release_id: String::new(),
        release_digest: String::new(),
        provider_id: String::new(),
        provider_proof: String::new(),
        system_graph_digest: digest("system-graph:r7"),
        contracts: release
            .contract_versions
            .iter()
            .map(|contract| ContractCompatibilityInput {
                contract_id: contract.contract_id.clone(),
                current_major: 1,
                candidate_major: contract
                    .version
                    .trim_start_matches('v')
                    .split('.')
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or_default(),
                compatible: Some(true),
                active_consumers: vec!["service:portal".to_owned()],
                consumer_migration_evidence: true,
                retiring: false,
                deprecation_window_complete: false,
            })
            .collect(),
        migrations: release
            .migrations
            .iter()
            .enumerate()
            .map(|(index, migration)| MigrationCompatibilityInput {
                migration_id: migration.migration_id.clone(),
                lineage_id: migration.migration_id.clone(),
                sequence: u32::try_from(index + 1).expect("test migration sequence fits u32"),
                phase: match migration.phase.as_str() {
                    "expand" => MigrationPhase::Expand,
                    "backfill" => MigrationPhase::Backfill,
                    "verify" => MigrationPhase::Verify,
                    "contract" => MigrationPhase::Contract,
                    _ => MigrationPhase::Irreversible,
                },
                verified: true,
            })
            .collect(),
        workflows: WorkflowCompatibilityInput {
            new_starts_compatible: Some(true),
            in_flight_compatible: Some(true),
            downgrade_safe: Some(true),
        },
        rollback: RollbackCompatibilityInput {
            prior_release_compatible: Some(true),
            schema_compatible: Some(true),
            workflow_compatible: Some(true),
            config_compatible: Some(true),
            secret_references_compatible: Some(true),
            edge_compatible: Some(true),
            adapter_capable: Some(true),
            previous_release_id: "service-release:previous".to_owned(),
            previous_release_digest: digest("previous-release"),
            previous_deployment_plan_id: "deployment-plan:previous".to_owned(),
            previous_deployment_plan_digest: digest("previous-deployment"),
            previous_config_revision_id: "config-revision:previous".to_owned(),
            previous_config_revision_digest: digest("previous-config"),
            previous_secret_reference_ids: vec!["secret:support:database:v4".to_owned()],
            previous_gateway_plan_id: "gateway-plan:previous".to_owned(),
            previous_gateway_plan_digest: digest("previous-gateway"),
            previous_gateway_configuration_identity: digest("previous-gateway-configuration"),
            previous_adapter: "kubernetes".to_owned(),
        },
        provider_compatibility_verified: Some(true),
        workload_identity_production: Some(true),
        tenancy_mode_production: Some(true),
        tenant_context_enforced: Some(true),
        call_policies_declared: Some(true),
        dependencies_ready: Some(true),
        resilience_declared: Some(true),
        reliability_contract_complete: Some(true),
        edge_contract_valid: Some(true),
        environment_verification_fresh: Some(true),
    };
    attest_production_eligibility_input(release, &test_trust_provider(), "ci:trusted", input)
        .expect("test eligibility should attest")
}

fn release_input() -> ServiceReleaseInput {
    let config_contract = simple_config_contract();
    ServiceReleaseInput {
        service_id: "service:support".to_owned(),
        service_version: "5.0.0".to_owned(),
        modules: vec![
            ReleaseModule {
                module_id: "support-sla".to_owned(),
                module_version: "2.0.0".to_owned(),
            },
            ReleaseModule {
                module_id: "support-ticket".to_owned(),
                module_version: "4.0.0".to_owned(),
            },
        ],
        workloads: vec![
            workload("support-worker", ReleaseWorkloadRole::Worker),
            workload("support-api", ReleaseWorkloadRole::Api),
            workload("support-migration", ReleaseWorkloadRole::Migration),
        ],
        contract_versions: vec![
            ReleaseContractVersion {
                contract_id: "support.ticket-opened".to_owned(),
                version: "v1".to_owned(),
                kind: "event".to_owned(),
                artifact: evidence("contract:event:v1"),
            },
            ReleaseContractVersion {
                contract_id: "support-http".to_owned(),
                version: "v1".to_owned(),
                kind: "request_response".to_owned(),
                artifact: evidence("contract:http:v1"),
            },
        ],
        config_contract: DeliveryEvidenceReference {
            reference: config_contract.reference,
            digest: config_contract.digest,
        },
        reliability_contract: DeliveryEvidenceReference {
            reference: "reliability-contract:v1".to_owned(),
            digest: reliability_contract_digest(&reliability_contract()),
        },
        migrations: vec![ReleaseMigration {
            migration_id: "support-0001".to_owned(),
            phase: "expand".to_owned(),
            artifact: evidence("migration:0001"),
            reversible: true,
        }],
        workflow_compatibility: vec![evidence("workflow-compatibility:v1")],
        verification_evidence: vec![evidence("verification:m4")],
        rollout_gates: vec![ReleaseRolloutGate {
            gate_id: "reliability".to_owned(),
            evidence_kind: "service_reliability".to_owned(),
            required: true,
        }],
        rollback: ReleaseRollbackConstraints {
            previous_release_required: true,
            automatic_allowed: true,
            blocked_by_irreversible_migration: true,
        },
        retention: ReleaseRetention {
            evidence_days: 90,
            artifact_days: 365,
        },
    }
}

fn workload(workload_id: &str, role: ReleaseWorkloadRole) -> WorkloadArtifact {
    WorkloadArtifact {
        workload_id: workload_id.to_owned(),
        role,
        artifact_reference: "ghcr.io/liorael/support".to_owned(),
        artifact_digest: digest(workload_id),
        media_type: "application/vnd.oci.image.manifest.v1+json".to_owned(),
        display_tag: Some("5.0.0".to_owned()),
        sbom: evidence(&format!("sbom:{workload_id}")),
        provenance: ReleaseProvenance {
            reference: format!("provenance:{workload_id}"),
            digest: digest(&format!("provenance:{workload_id}")),
            source: "https://github.com/LioRael/lenso-examples".to_owned(),
            builder: "https://github.com/LioRael/lenso-examples/actions".to_owned(),
            input_digests: vec![digest("source-tree")],
            subject_digests: vec![digest(workload_id)],
        },
        signature_subject: format!("workload:{workload_id}"),
    }
}

fn evidence(reference: &str) -> DeliveryEvidenceReference {
    DeliveryEvidenceReference {
        reference: reference.to_owned(),
        digest: digest(reference),
    }
}

fn digest(value: &str) -> String {
    extraction_input_digest(value.as_bytes())
}
