use std::collections::BTreeMap;

use lenso_service::{
    DeliveryConsoleArtifacts, DeliveryFailureCondition, DeliveryFailureRecoveryInput,
    DeliveryFailureStage, DeliveryRecoveryDecision, DeliveryRecoveryIssueCode,
    DeliveryRecoveryScope, DeliveryStateObservation, DisasterRecoveryApproval,
    DisasterRecoveryDecision, DisasterRecoveryObservation, DisasterRecoveryPhase,
    DisasterRecoveryPlanInput, FindingDisposition, KubernetesRecoveryObservation,
    MigrationRecoveryEvidence, PerformanceBudget, PerformanceBudgetDirection, PerformanceDecision,
    PerformanceIssueCode, PerformanceMeasurement, PerformanceMetric, PerformanceProfileInput,
    PerformanceProfileScope, PerformanceRun, PostgresRestoreObservation, ReferenceService,
    ReferenceSystemTopology, RestoreDecision, SecurityFinding, SecurityReleaseSubject,
    SecurityReviewDecision, SecurityReviewInput, SecurityScanEvidence, SecuritySeverity,
    ServiceBackupInput, ServiceRestoreInput, SupportEnvelopeDecision, SupportEnvelopeInput,
    SupportScalePoint, ThreatModelEvidence, ThreatSurface, assemble_service_backup,
    evaluate_delivery_failure_recovery, evaluate_disaster_recovery, evaluate_performance_profile,
    evaluate_security_review, evaluate_service_restore, evaluate_support_envelope,
    extraction_input_digest, plan_disaster_recovery, project_delivery_console,
};

fn digest(subject: &str) -> String {
    extraction_input_digest(subject.as_bytes())
}

fn state(
    observation_id: &str,
    source: &str,
    revision: u64,
    desired: &str,
    observed: &str,
    drifted: bool,
) -> DeliveryStateObservation {
    DeliveryStateObservation {
        observation_id: observation_id.to_owned(),
        source: source.to_owned(),
        revision,
        desired_digest: digest(desired),
        observed_digest: digest(observed),
        fresh: true,
        drifted,
    }
}

#[test]
fn delivery_recovery_preserves_drift_config_and_completed_migration_effects() {
    let evidence = evaluate_delivery_failure_recovery(DeliveryFailureRecoveryInput {
        scenario_id: "migration-partial".to_owned(),
        condition: DeliveryFailureCondition::MigrationFailed,
        stage: DeliveryFailureStage::PartiallyApplied,
        scope: DeliveryRecoveryScope::EnvironmentVerification,
        desired_state: state(
            "desired:19",
            "release-ledger",
            19,
            "desired",
            "desired",
            false,
        ),
        observed_state: state(
            "operator:20",
            "kubernetes-api:test",
            20,
            "desired",
            "partial",
            true,
        ),
        previous_valid_config_revision_id: "config:18".to_owned(),
        attempted_config_revision_id: "config:19".to_owned(),
        active_config_revision_id: "config:18".to_owned(),
        migration: Some(MigrationRecoveryEvidence {
            migration_id: "support-0004".to_owned(),
            completed_effects: vec!["expand-ticket-index".to_owned()],
            remaining_steps: vec!["backfill-ticket-index".to_owned()],
            retry_steps: vec!["backfill-ticket-index".to_owned()],
            state_compatible: true,
            rollback_allowed: true,
            intervention_required: false,
        }),
        infrastructure_mutations: vec!["expand-ticket-index".to_owned()],
        kubernetes: Some(KubernetesRecoveryObservation {
            cluster_identity: "kind:m6-pinned".to_owned(),
            api_server_version: "1.35.0".to_owned(),
            operator_version: "0.2.9".to_owned(),
            gateway_adapter_version: "gateway-api-1.4".to_owned(),
            used_real_api: true,
            observed_resource_version: "1842".to_owned(),
            evidence_digest: digest("kind-observation"),
        }),
        cleanup_complete: true,
    });

    assert_eq!(evidence.decision, DeliveryRecoveryDecision::Passed);
    assert_eq!(evidence.retained_config_revision_id, "config:18");
    assert!(!evidence.effects.mutates_environment);
    assert_eq!(
        evidence.migration.unwrap().retry_steps,
        ["backfill-ticket-index"]
    );
}

#[test]
fn delivery_recovery_blocks_preapply_mutation_and_repeated_effects() {
    let evidence = evaluate_delivery_failure_recovery(DeliveryFailureRecoveryInput {
        scenario_id: "migration-invalid".to_owned(),
        condition: DeliveryFailureCondition::MigrationFailed,
        stage: DeliveryFailureStage::BeforeApply,
        scope: DeliveryRecoveryScope::EnvironmentVerification,
        desired_state: state(
            "desired:19",
            "release-ledger",
            19,
            "desired",
            "desired",
            false,
        ),
        observed_state: state("mock:19", "fixture", 19, "desired", "desired", false),
        previous_valid_config_revision_id: "config:18".to_owned(),
        attempted_config_revision_id: "config:19".to_owned(),
        active_config_revision_id: "config:19".to_owned(),
        migration: Some(MigrationRecoveryEvidence {
            migration_id: "support-0004".to_owned(),
            completed_effects: vec!["expand-ticket-index".to_owned()],
            remaining_steps: vec!["backfill-ticket-index".to_owned()],
            retry_steps: vec!["expand-ticket-index".to_owned()],
            state_compatible: true,
            rollback_allowed: false,
            intervention_required: true,
        }),
        infrastructure_mutations: vec!["expand-ticket-index".to_owned()],
        kubernetes: None,
        cleanup_complete: false,
    });

    assert_eq!(evidence.decision, DeliveryRecoveryDecision::Blocked);
    for code in [
        DeliveryRecoveryIssueCode::PreApplyMutation,
        DeliveryRecoveryIssueCode::LastValidConfigurationLost,
        DeliveryRecoveryIssueCode::MigrationEffectWouldRepeat,
        DeliveryRecoveryIssueCode::EnvironmentEvidenceInvalid,
        DeliveryRecoveryIssueCode::CleanupIncomplete,
    ] {
        assert!(evidence.issues.iter().any(|issue| issue.code == code));
    }
}

fn required_metrics(run_offset: u64) -> Vec<PerformanceMeasurement> {
    [
        (PerformanceMetric::DirectCallLatency, "us", 800),
        (
            PerformanceMetric::DirectCallThroughput,
            "requests_per_second",
            2_000,
        ),
        (PerformanceMetric::ResolverClientOverhead, "us", 100),
        (PerformanceMetric::PublishToConsumeLatency, "us", 2_500),
        (PerformanceMetric::InboxOutboxLag, "us", 1_500),
        (PerformanceMetric::WorkflowTransitionLatency, "us", 3_000),
        (PerformanceMetric::WorkflowTimerDelay, "us", 5_000),
        (PerformanceMetric::StoryFreshness, "us", 8_000),
        (PerformanceMetric::ConsoleQueryLatency, "us", 10_000),
        (PerformanceMetric::ConvergenceLatency, "us", 20_000),
        (PerformanceMetric::CpuUtilization, "basis_points", 3_500),
        (PerformanceMetric::MemoryBytes, "bytes", 256_000_000),
        (PerformanceMetric::DatabaseConnections, "connections", 24),
        (PerformanceMetric::BrokerBytes, "bytes", 4_000_000),
    ]
    .into_iter()
    .map(|(metric, unit, value)| PerformanceMeasurement {
        metric,
        unit: unit.to_owned(),
        value: value + run_offset,
    })
    .collect()
}

fn budgets() -> Vec<PerformanceBudget> {
    required_metrics(0)
        .into_iter()
        .map(|measurement| PerformanceBudget {
            metric: measurement.metric,
            unit: measurement.unit,
            direction: if measurement.metric == PerformanceMetric::DirectCallThroughput {
                PerformanceBudgetDirection::AtLeast
            } else {
                PerformanceBudgetDirection::AtMost
            },
            threshold: if measurement.metric == PerformanceMetric::DirectCallThroughput {
                1_500
            } else {
                measurement.value * 2
            },
        })
        .collect()
}

fn run(index: u64) -> PerformanceRun {
    PerformanceRun {
        run_id: format!("m6-three-service:{index}"),
        release_set_digest: digest("release-set"),
        dataset_digest: digest("dataset"),
        concurrency: 32,
        duration_ms: 60_000,
        warmup_ms: 5_000,
        machine: BTreeMap::from([
            ("cpu".to_owned(), "apple-m4".to_owned()),
            ("memory".to_owned(), "32-gib".to_owned()),
        ]),
        infrastructure: BTreeMap::from([
            ("postgres".to_owned(), "18".to_owned()),
            ("nats".to_owned(), "2.12".to_owned()),
        ]),
        measurements: required_metrics(index),
        system_plane_data_plane_requests: 0,
        runtime_console_data_plane_requests: 0,
        telemetry_data_plane_requests: 0,
        policy_data_plane_requests: 0,
        registry_data_plane_requests: 0,
    }
}

fn topology() -> ReferenceSystemTopology {
    ReferenceSystemTopology {
        topology_digest: digest("three-service-topology"),
        services: ["catalog", "orders", "support"]
            .into_iter()
            .map(|name| ReferenceService {
                service_id: format!("service:{name}"),
                contract_id: format!("{name}.http.v1"),
                store_id: format!("postgres:{name}"),
                release_digest: digest(name),
            })
            .collect(),
        transport_adapter_version: "nats-2.12".to_owned(),
        identity_adapter_version: "spire-1.14".to_owned(),
        deployment_adapter_version: "operator-0.2.9".to_owned(),
    }
}

#[test]
fn three_service_profile_records_repeated_budgeted_environment_evidence() {
    let profile = evaluate_performance_profile(PerformanceProfileInput {
        scope: PerformanceProfileScope::EnvironmentVerification,
        support_manifest_digest: digest("ga-support-manifest"),
        topology: topology(),
        budgets: budgets(),
        runs: vec![run(0), run(1), run(2)],
        variance_tolerance_basis_points: 1_000,
    });

    assert_eq!(profile.decision, PerformanceDecision::Passed);
    assert_eq!(profile.topology.services.len(), 3);
    assert_eq!(profile.runs.len(), 3);
    assert!(
        profile
            .variance_basis_points
            .values()
            .all(|variance| *variance <= 1_000)
    );
}

#[test]
fn performance_profile_blocks_replicas_hidden_dependencies_and_missing_runs() {
    let mut invalid_topology = topology();
    invalid_topology.services[1].contract_id = invalid_topology.services[0].contract_id.clone();
    let mut dependent_run = run(0);
    dependent_run.system_plane_data_plane_requests = 1;
    let profile = evaluate_performance_profile(PerformanceProfileInput {
        scope: PerformanceProfileScope::EnvironmentVerification,
        support_manifest_digest: digest("ga-support-manifest"),
        topology: invalid_topology,
        budgets: budgets(),
        runs: vec![dependent_run],
        variance_tolerance_basis_points: 1_000,
    });

    assert_eq!(profile.decision, PerformanceDecision::Blocked);
    for code in [
        PerformanceIssueCode::TopologyInvalid,
        PerformanceIssueCode::HiddenDataPlaneDependency,
        PerformanceIssueCode::EnvironmentEvidenceInsufficient,
    ] {
        assert!(profile.issues.iter().any(|issue| issue.code == code));
    }
}

fn verified_restore_input() -> ServiceRestoreInput {
    let state_digests = BTreeMap::from([
        ("business".to_owned(), digest("business")),
        ("inbox".to_owned(), digest("inbox")),
        ("outbox".to_owned(), digest("outbox")),
        ("stories".to_owned(), digest("stories")),
        ("workflows".to_owned(), digest("workflows")),
        ("workflow_timers".to_owned(), digest("workflow-timers")),
        ("compensation".to_owned(), digest("compensation")),
        (
            "federation_cursors".to_owned(),
            digest("federation-cursors"),
        ),
    ]);
    let backup = assemble_service_backup(ServiceBackupInput {
        service_id: "service:support".to_owned(),
        store_id: "postgres:support-primary".to_owned(),
        format_version: "lenso.service-backup-format.v1".to_owned(),
        schema_version: "service-store.v2".to_owned(),
        release_digest: digest("release"),
        config_revision_digest: digest("config"),
        contract_version_digests: BTreeMap::from([
            (
                "support.commands.v1".to_owned(),
                digest("commands-contract"),
            ),
            ("support.events.v1".to_owned(), digest("events-contract")),
        ]),
        store_checkpoint_digest: digest("checkpoint"),
        broker_position: Some(101),
        restore_preconditions: vec![
            "isolated passive Store".to_owned(),
            "exact release available".to_owned(),
        ],
        point_in_time_unix_ms: 1_721_600_000_000,
        captured_at_unix_ms: 1_721_600_000_000,
        freshness_horizon_unix_ms: 1_721_686_400_000,
        snapshot_digest: digest("snapshot"),
        post_checkpoint_work_digest: digest("post-checkpoint-work"),
        encryption_key_reference: "kms://backup/support".to_owned(),
        encryption_algorithm: "aes-256-gcm".to_owned(),
        state_digests: state_digests.clone(),
        outbox_sequence: 41,
        inbox_sequence: 53,
        workflow_timer_sequence: 67,
        story_sequence: 79,
        completed: true,
    })
    .unwrap();
    ServiceRestoreInput {
        restored_snapshot_digest: backup.input.snapshot_digest.clone(),
        restored_release_digest: backup.input.release_digest.clone(),
        restored_config_revision_digest: backup.input.config_revision_digest.clone(),
        restored_workflow_timer_sequence: backup.input.workflow_timer_sequence,
        restored_story_sequence: backup.input.story_sequence,
        backup: backup.clone(),
        target_store_id: "postgres:support-passive".to_owned(),
        target_was_clean: true,
        expected_service_id: backup.input.service_id.clone(),
        expected_format_version: backup.input.format_version.clone(),
        expected_schema_version: backup.input.schema_version.clone(),
        expected_release_digest: backup.input.release_digest.clone(),
        expected_config_revision_digest: backup.input.config_revision_digest.clone(),
        expected_contract_version_digests: backup.input.contract_version_digests.clone(),
        key_reference_available: true,
        observed_at_unix_ms: 1_721_600_010_000,
        restored_state_digests: state_digests,
        restored_contract_version_digests: backup.input.contract_version_digests.clone(),
        replay_outbox_from_sequence: 42,
        replay_inbox_from_sequence: 54,
        authoritative_workload_count: 0,
        business_invariants_verified: true,
        post_checkpoint_work_reconciled: true,
        recovery_time_ms: 45_000,
        intentional_loss_bound_ms: 0,
        replay_bound_count: 3,
        remaining_story_gaps: Vec::new(),
        cleanup_complete: true,
        postgres: PostgresRestoreObservation {
            provider: "postgresql".to_owned(),
            version: "18".to_owned(),
            instance_identity: "postgres:m6-restore-1".to_owned(),
            used_real_instance: true,
            observation_digest: digest("postgres-observation"),
        },
    }
}

fn verified_restore() -> lenso_service::ServiceRestoreEvidence {
    evaluate_service_restore(verified_restore_input())
}

#[test]
fn backup_restore_preserves_authoritative_state_and_replay_boundaries() {
    let restore = verified_restore();
    assert_eq!(restore.decision, RestoreDecision::Passed);
    assert!(!restore.production_mutated);
    assert_eq!(restore.restored_state_digests.len(), 8);
}

#[test]
fn backup_restore_fails_closed_for_corrupt_stale_wrong_or_unavailable_recovery_sets() {
    let mutations: Vec<Box<dyn Fn(&mut ServiceRestoreInput)>> = vec![
        Box::new(|input| input.backup.backup_digest = digest("corrupt")),
        Box::new(|input| input.backup.input.completed = false),
        Box::new(|input| input.observed_at_unix_ms = u64::MAX),
        Box::new(|input| input.expected_service_id = "service:wrong".to_owned()),
        Box::new(|input| input.expected_release_digest = digest("wrong-release")),
        Box::new(|input| input.expected_format_version = "unsupported.v9".to_owned()),
        Box::new(|input| input.key_reference_available = false),
    ];
    for mutate in mutations {
        let mut input = verified_restore_input();
        mutate(&mut input);
        assert_eq!(
            evaluate_service_restore(input).decision,
            RestoreDecision::Blocked
        );
    }
}

#[test]
fn disaster_recovery_requires_fencing_exact_approval_and_observed_budgets() {
    let restore = verified_restore();
    let plan = plan_disaster_recovery(DisasterRecoveryPlanInput {
        phase: DisasterRecoveryPhase::Cutover,
        service_id: "service:support".to_owned(),
        primary_region: "cn-east-1".to_owned(),
        passive_region: "cn-east-2".to_owned(),
        restore_evidence: restore,
        expected_release_digest: digest("release"),
        expected_config_revision_digest: digest("config"),
        expected_contract_set_digest: digest("contracts"),
        expected_active_state_digest: digest("active-state-cutover"),
        authoritative_environment_count_before: 1,
        planned_at_unix_ms: 1_721_600_005_000,
        freshness_horizon_unix_ms: 1_721_600_020_000,
        rpo_budget_ms: 30_000,
        rto_budget_ms: 120_000,
        primary_fenced: true,
        passive_fenced: false,
        passive_health_verified: true,
        passive_identity_verified: true,
        passive_contracts_verified: true,
        failback_steps: vec![
            "re-seed prior primary".to_owned(),
            "verify and fence".to_owned(),
            "request failback approval".to_owned(),
        ],
    });
    assert_eq!(plan.decision, DisasterRecoveryDecision::Ready);
    let evidence = evaluate_disaster_recovery(
        &plan,
        &DisasterRecoveryApproval {
            plan_digest: plan.plan_digest.clone(),
            phase: DisasterRecoveryPhase::Cutover,
            approver: "incident-commander".to_owned(),
            reason: "pinned environment drill".to_owned(),
            approved_at_unix_ms: 1_721_600_010_000,
        },
        DisasterRecoveryObservation {
            plan_digest: plan.plan_digest.clone(),
            phase: DisasterRecoveryPhase::Cutover,
            observed_at_unix_ms: 1_721_600_011_000,
            active_state_digest: digest("active-state-cutover"),
            authoritative_environment_count: 1,
            primary_fenced: true,
            passive_fenced: false,
            passive_became_authoritative: true,
            primary_became_authoritative: false,
            traffic_switched: true,
            observed_rpo_ms: 10_000,
            observed_rto_ms: 60_000,
            release_digest: digest("release"),
            config_revision_digest: digest("config"),
            contract_set_digest: digest("contracts"),
            workload_identity_preserved: true,
            duplicate_business_effects: 0,
            lost_committed_effects: 0,
            requests_events_workflows_stories_verified: true,
            cleanup_complete: true,
            evidence_digest: digest("dr-observation"),
        },
    );
    assert_eq!(evidence.decision, DisasterRecoveryDecision::Passed);

    let failback = plan_disaster_recovery(DisasterRecoveryPlanInput {
        phase: DisasterRecoveryPhase::Failback,
        service_id: plan.input.service_id.clone(),
        primary_region: plan.input.primary_region.clone(),
        passive_region: plan.input.passive_region.clone(),
        restore_evidence: plan.input.restore_evidence.clone(),
        expected_release_digest: plan.input.expected_release_digest.clone(),
        expected_config_revision_digest: plan.input.expected_config_revision_digest.clone(),
        expected_contract_set_digest: plan.input.expected_contract_set_digest.clone(),
        expected_active_state_digest: digest("active-state-failback"),
        authoritative_environment_count_before: 1,
        planned_at_unix_ms: 1_721_600_012_000,
        freshness_horizon_unix_ms: 1_721_600_030_000,
        rpo_budget_ms: 30_000,
        rto_budget_ms: 120_000,
        primary_fenced: false,
        passive_fenced: true,
        passive_health_verified: true,
        passive_identity_verified: true,
        passive_contracts_verified: true,
        failback_steps: vec![
            "fence passive".to_owned(),
            "grant primary authority".to_owned(),
            "switch traffic".to_owned(),
        ],
    });
    assert_eq!(failback.decision, DisasterRecoveryDecision::Ready);
    let failback_evidence = evaluate_disaster_recovery(
        &failback,
        &DisasterRecoveryApproval {
            plan_digest: failback.plan_digest.clone(),
            phase: DisasterRecoveryPhase::Failback,
            approver: "incident-commander".to_owned(),
            reason: "separately reviewed failback".to_owned(),
            approved_at_unix_ms: 1_721_600_013_000,
        },
        DisasterRecoveryObservation {
            plan_digest: failback.plan_digest.clone(),
            phase: DisasterRecoveryPhase::Failback,
            observed_at_unix_ms: 1_721_600_014_000,
            active_state_digest: digest("active-state-failback"),
            authoritative_environment_count: 1,
            primary_fenced: false,
            passive_fenced: true,
            passive_became_authoritative: false,
            primary_became_authoritative: true,
            traffic_switched: true,
            observed_rpo_ms: 5_000,
            observed_rto_ms: 30_000,
            release_digest: digest("release"),
            config_revision_digest: digest("config"),
            contract_set_digest: digest("contracts"),
            workload_identity_preserved: true,
            duplicate_business_effects: 0,
            lost_committed_effects: 0,
            requests_events_workflows_stories_verified: true,
            cleanup_complete: true,
            evidence_digest: digest("failback-observation"),
        },
    );
    assert_eq!(failback_evidence.decision, DisasterRecoveryDecision::Passed);
    assert_ne!(evidence.plan_digest, failback_evidence.plan_digest);
}

fn scale_point(service_count: u32) -> SupportScalePoint {
    SupportScalePoint {
        service_count,
        workload_count: service_count * 2,
        store_count: service_count,
        contract_count: service_count * 2,
        workflow_count: service_count,
        tenant_count: 100,
        topology_digest: digest(&format!("topology-{service_count}")),
        environment_digest: digest("environment"),
        compatible_baseline_digest: digest("compatible-baseline"),
        environment_verification: true,
        environment_drift_detected: false,
        measurement_digests: [
            "startup",
            "rollout",
            "direct_calls",
            "events",
            "inbox_outbox",
            "workflows",
            "timers",
            "compensation",
            "story_federation",
            "policy",
            "console",
            "failure_recovery",
            "connections",
            "backlog",
            "resources",
            "evidence_freshness",
        ]
        .into_iter()
        .map(|name| (name.to_owned(), digest(name)))
        .collect(),
        budgets_passed: true,
        repeated_run_count: 3,
        variance_basis_points: 400,
        system_plane_data_plane_requests: 0,
        runtime_console_data_plane_requests: 0,
        telemetry_data_plane_requests: 0,
        policy_data_plane_requests: 0,
        registry_data_plane_requests: 0,
        saturation_signal: "postgres_connection_budget".to_owned(),
        bottlenecks: vec!["database connections".to_owned()],
        cleanup_complete: true,
    }
}

#[test]
fn support_envelope_is_bounded_to_three_ten_and_twenty_services() {
    let envelope = evaluate_support_envelope(SupportEnvelopeInput {
        support_manifest_digest: digest("manifest"),
        adapter_versions: BTreeMap::from([
            ("deployment".to_owned(), "operator-0.2.9".to_owned()),
            ("transport".to_owned(), "nats-2.12".to_owned()),
        ]),
        points: vec![scale_point(20), scale_point(3), scale_point(10)],
        recommended_service_limit: 20,
        variance_tolerance_basis_points: 1_000,
    });
    assert_eq!(envelope.decision, SupportEnvelopeDecision::Passed);
    assert_eq!(
        envelope
            .points
            .iter()
            .map(|point| point.service_count)
            .collect::<Vec<_>>(),
        [3, 10, 20]
    );
}

#[test]
fn security_review_binds_all_threats_findings_and_scans_to_release_subjects() {
    let artifact_digest = digest("artifact");
    let surfaces = [
        ThreatSurface::WorkloadIdentity,
        ThreatSurface::TransportBinding,
        ThreatSurface::Delegation,
        ThreatSurface::Tenancy,
        ThreatSurface::EventReplayAndPoisoning,
        ThreatSurface::ExtractionAndCutover,
        ThreatSurface::WorkflowControls,
        ThreatSurface::ReleaseSigning,
        ThreatSurface::Secrets,
        ThreatSurface::BackupAndRestore,
        ThreatSurface::AdminActions,
        ThreatSurface::EmbeddedConsole,
        ThreatSurface::PolicyBypass,
        ThreatSurface::StaleEvidence,
        ThreatSurface::AgentBoundaries,
    ];
    let evidence = evaluate_security_review(
        SecurityReviewInput {
            support_manifest_digest: digest("manifest"),
            release_subjects: vec![SecurityReleaseSubject {
                component_id: "lenso-service".to_owned(),
                version: "0.1.15".to_owned(),
                source_commit: "a".repeat(40),
                artifact_digest: artifact_digest.clone(),
                provenance_digest: digest("provenance"),
                sbom_digest: digest("sbom"),
            }],
            threat_models: surfaces
                .into_iter()
                .map(|surface| ThreatModelEvidence {
                    surface,
                    model_version: "m6.v1".to_owned(),
                    model_digest: digest(&format!("{surface:?}")),
                    reviewed: true,
                })
                .collect(),
            findings: vec![SecurityFinding {
                finding_id: "M6-SEC-001".to_owned(),
                finding_digest: digest("finding"),
                severity: SecuritySeverity::Medium,
                surface: ThreatSurface::StaleEvidence,
                affected_subject_digests: vec![artifact_digest],
                owner: "runtime-security".to_owned(),
                disposition: FindingDisposition::Remediated,
                remediation_reference: Some("commit:aaaaaaaa".to_owned()),
                risk_acceptance: None,
                contains_sensitive_material: false,
            }],
            scans: [
                "dependency-audit",
                "provenance-verification",
                "secret-scan",
                "static-analysis",
            ]
            .into_iter()
            .map(|scanner_id| SecurityScanEvidence {
                scanner_id: scanner_id.to_owned(),
                scanner_version: "1.0".to_owned(),
                subject_digest: digest("artifact"),
                result_digest: digest(scanner_id),
                completed: true,
            })
            .collect(),
            reviewer: "security-reviewer".to_owned(),
            reviewed_at_unix_ms: 1_721_600_000_000,
            freshness_horizon_unix_ms: 1_721_686_400_000,
        },
        1_721_600_001_000,
    );
    assert_eq!(evidence.decision, SecurityReviewDecision::Passed);
    assert_eq!(evidence.threat_models.len(), 15);
}

#[test]
fn runtime_console_projects_ga_evidence_without_mutation_or_sensitive_values() {
    let projection = project_delivery_console(DeliveryConsoleArtifacts {
        artifacts: vec![
            serde_json::json!({
                "protocol": "lenso.ga-support-manifest.v1",
                "manifestId": "ga-support:m6",
                "manifestDigest": digest("manifest"),
                "status": "candidate"
            }),
            serde_json::json!({
                "protocol": "lenso.security-review-evidence.v1",
                "reviewId": "security-review:m6",
                "reviewDigest": digest("review"),
                "supportManifestDigest": digest("manifest"),
                "decision": "blocked",
                "issues": [{"code": "security_review_stale", "message": "stale"}],
                "nextActions": ["refresh review"],
                "secretValue": "must-not-project"
            }),
        ],
    });
    assert_eq!(
        projection
            .ga_operations
            .support_manifest
            .as_ref()
            .unwrap()
            .status,
        "candidate"
    );
    let security = projection.ga_operations.security_review.unwrap();
    assert_eq!(security.status, "blocked");
    assert!(security.stale);
    assert_eq!(security.next_actions, ["refresh review"]);
    assert!(
        !serde_json::to_string(&security)
            .unwrap()
            .contains("must-not-project")
    );
    assert!(projection.read_only);
    assert!(projection.apply_actions.is_empty());
}
