use anyhow::Context as _;
use kube::CustomResourceExt as _;
use schemars::JsonSchema;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

pub fn generate_contracts() -> anyhow::Result<()> {
    write_yaml(
        "contracts/openapi/app-api.v1.yaml",
        &lenso_api::openapi_document(),
    )?;
    write_yaml(
        "contracts/openapi/autonomous-service-runtime.v1.yaml",
        &generated_autonomous_service_runtime_openapi(),
    )?;
    write_json(
        "contracts/workflows/lenso.workflow-definition.v1.schema.json",
        &generated_workflow_definition_schema(),
    )?;
    write_json(
        "contracts/workflows/lenso.workflow-compatibility.v1.json",
        &generated_workflow_compatibility_artifact(),
    )?;
    write_json(
        "contracts/errors/error-response.v1.schema.json",
        &error_response_schema(),
    )?;
    write_json(
        "contracts/services/lenso-service.v2.schema.json",
        &generated_autonomous_service_schema(),
    )?;
    write_json(
        "contracts/services/support-http.v1.bindings.json",
        &generated_direct_http_bindings(),
    )?;
    write_text(
        "contracts/services/support-grpc.v1.proto",
        lenso_service::DIRECT_GRPC_PROTO_V1_FIXTURE,
    )?;
    write_json(
        "contracts/services/support-grpc.v1.bindings.json",
        &generated_direct_grpc_bindings(),
    )?;
    write_json(
        "contracts/services/lenso-system.v2.schema.json",
        &generated_system_v2_schema(),
    )?;
    write_json(
        "contracts/services/lenso-system.v2.fixture.json",
        &generated_system_v2_fixture(),
    )?;
    write_json(
        "contracts/delivery/lenso.service-release.v1.schema.json",
        &generated_service_release_schema(),
    )?;
    write_json(
        "contracts/delivery/lenso.config-revision.v1.schema.json",
        &generated_delivery_schema::<lenso_service::ConfigRevision>("lenso.config-revision.v1"),
    )?;
    write_json(
        "contracts/delivery/lenso.policy-evidence.v1.schema.json",
        &generated_delivery_schema::<lenso_service::PolicyEvidence>("lenso.policy-evidence.v1"),
    )?;
    write_json(
        "contracts/delivery/lenso.edge-contract.v1.schema.json",
        &generated_delivery_schema::<lenso_service::EdgeContract>("lenso.edge-contract.v1"),
    )?;
    write_json(
        "contracts/delivery/lenso.deployment-plan.v1.schema.json",
        &generated_delivery_schema::<lenso_service::DeploymentPlan>("lenso.deployment-plan.v1"),
    )?;
    write_json(
        "contracts/delivery/lenso.promotion-plan.v1.schema.json",
        &generated_delivery_schema::<lenso_service::PromotionPlan>("lenso.promotion-plan.v1"),
    )?;
    write_json(
        "contracts/delivery/lenso.canary-plan.v1.schema.json",
        &generated_delivery_schema::<lenso_service::CanaryPlan>("lenso.canary-plan.v1"),
    )?;
    write_json(
        "contracts/delivery/lenso.rollback-plan.v1.schema.json",
        &generated_delivery_schema::<lenso_service::RollbackPlan>("lenso.rollback-plan.v1"),
    )?;
    write_json(
        "contracts/delivery/lenso.coordination-outage-proof.v1.schema.json",
        &generated_delivery_schema::<lenso_service::CoordinationOutageEvidence>(
            "lenso.coordination-outage-proof.v1",
        ),
    )?;
    write_json(
        "contracts/delivery/lenso.delivery-console.v1.schema.json",
        &generated_delivery_schema::<lenso_service::DeliveryConsoleProjection>(
            "lenso.delivery-console.v1",
        ),
    )?;
    write_json(
        "contracts/delivery/support.service-release.json",
        &generated_support_service_release(),
    )?;
    write_json(
        "contracts/ga/lenso.ga-support-manifest.v1.schema.json",
        &generated_ga_support_manifest_schema(),
    )?;
    write_json(
        "contracts/ga/lenso.manifest-migration-plan.v1.schema.json",
        &generated_manifest_migration_plan_schema(),
    )?;
    write_json(
        "contracts/ga/lenso.service-upgrade-plan.v1.schema.json",
        &generated_service_upgrade_plan_schema(),
    )?;
    write_json(
        "contracts/ga/lenso.contract-retirement-plan.v1.schema.json",
        &generated_contract_retirement_plan_schema(),
    )?;
    write_json(
        "contracts/ga/lenso.failure-scenario-evidence.v1.schema.json",
        &generated_failure_scenario_evidence_schema(),
    )?;
    write_json(
        "contracts/ga/lenso.delivery-failure-recovery-evidence.v1.schema.json",
        &generated_delivery_failure_recovery_schema(),
    )?;
    write_json(
        "contracts/ga/lenso.performance-profile.v1.schema.json",
        &generated_performance_profile_schema(),
    )?;
    write_json(
        "contracts/ga/lenso.service-restore-evidence.v1.schema.json",
        &generated_service_restore_evidence_schema(),
    )?;
    write_json(
        "contracts/ga/lenso.disaster-recovery-evidence.v1.schema.json",
        &generated_disaster_recovery_evidence_schema(),
    )?;
    write_json(
        "contracts/ga/lenso.support-envelope.v1.schema.json",
        &generated_support_envelope_schema(),
    )?;
    write_json(
        "contracts/ga/lenso.security-review-evidence.v1.schema.json",
        &generated_security_review_evidence_schema(),
    )?;
    write_json(
        "contracts/ga/lenso.ga-support-manifest.v1.json",
        &generated_ga_support_manifest(),
    )?;
    write_text(
        "docs/operations/ga-support.md",
        &generated_ga_support_guidance(),
    )?;
    write_yaml(
        "contracts/operator/lenso-autonomous-service.v1alpha1.crd.yaml",
        &generated_autonomous_service_crd(),
    )?;
    write_yaml(
        "contracts/operator/support.autonomous-service.yaml",
        &generated_support_autonomous_service(),
    )?;
    write_json(
        "contracts/extraction/lenso.extraction-readiness-report.v1.schema.json",
        &generated_extraction_readiness_schema(),
    )?;
    write_json(
        "contracts/extraction/support-ticket.blocked.json",
        &generated_support_ticket_extraction_readiness_blocked(),
    )?;
    write_text(
        "contracts/extraction/support-ticket.blocked.txt",
        &generated_support_ticket_extraction_readiness_blocked_human(),
    )?;
    write_json(
        "contracts/extraction/support-ticket.corrected.json",
        &generated_support_ticket_extraction_readiness_corrected(),
    )?;
    write_text(
        "contracts/extraction/support-ticket.corrected.txt",
        &generated_support_ticket_extraction_readiness_corrected_human(),
    )?;
    write_json(
        "contracts/extraction/lenso.extraction-plan.v1.schema.json",
        &generated_extraction_plan_schema(),
    )?;
    write_json(
        "contracts/extraction/support-ticket.plan.json",
        &generated_support_ticket_extraction_plan(),
    )?;
    write_text(
        "contracts/extraction/support-ticket.plan.txt",
        &generated_support_ticket_extraction_plan_human(),
    )?;
    write_json(
        "contracts/extraction/lenso.extraction-scaffold.v1.schema.json",
        &generated_extraction_scaffold_schema(),
    )?;
    write_json(
        "contracts/extraction/support-ticket.scaffold.json",
        &generated_support_ticket_extraction_scaffold(),
    )?;
    write_text(
        "contracts/extraction/support-ticket.scaffold.patch",
        &generated_support_ticket_extraction_scaffold_patch(),
    )?;
    write_json(
        "contracts/extraction/lenso.extraction-run.v1.schema.json",
        &generated_extraction_run_schema(),
    )?;
    write_json(
        "contracts/extraction/support-ticket.expansion-run.json",
        &generated_support_ticket_extraction_run(),
    )?;
    write_text(
        "contracts/extraction/support-ticket.expansion-run.txt",
        &generated_support_ticket_extraction_run_human(),
    )?;
    write_json(
        "contracts/context/lenso-context.v1.schema.json",
        &generated_common_context_schema(),
    )?;
    write_json(
        "contracts/context/lenso-context.v1.fixture.json",
        &generated_common_context_fixture(),
    )?;
    write_json(
        "contracts/events/lenso/lenso.event-envelope.v1.schema.json",
        &generated_event_envelope_schema(),
    )?;
    write_json(
        "contracts/events/support/support.ticket-opened.v1.schema.json",
        &generated_support_event_schema(),
    )?;
    write_json(
        "contracts/events/support/support.ticket-opened.v1.artifact.json",
        &generated_support_event_contract(),
    )?;
    write_json(
        "contracts/events/support/support.ticket-opened.v1.envelope.json",
        &generated_support_event_envelope(),
    )?;
    write_text(
        "docs/architecture/common-context-contracts.md",
        generated_common_context_glossary(),
    )?;
    write_text(
        "docs/architecture/contract-compatibility.md",
        generated_contract_compatibility(),
    )?;
    write_json(
        "contracts/compatibility/contract-compatibility.v1.json",
        &generated_contract_compatibility_matrix(),
    )?;

    Ok(())
}

pub fn generated_autonomous_service_runtime_openapi() -> utoipa::openapi::OpenApi {
    lenso_autonomous_service::openapi_document()
}

pub fn generated_workflow_definition_schema() -> Value {
    lenso_contracts::workflow_definition_schema()
}

pub fn generated_workflow_compatibility_artifact() -> Value {
    lenso_contracts::workflow_compatibility_artifact()
}

pub fn generated_error_response_schema() -> Value {
    error_response_schema()
}

pub fn generated_autonomous_service_schema() -> Value {
    serde_json::from_str(lenso_service::SERVICE_V2_CONTRACT_SCHEMA_JSON)
        .expect("packaged Autonomous Service schema must be valid JSON")
}

pub fn generated_direct_http_bindings() -> Value {
    let openapi: Value = serde_yaml::from_str(lenso_service::DIRECT_HTTP_OPENAPI_V1_FIXTURE_YAML)
        .expect("packaged direct HTTP OpenAPI fixture must be valid YAML");
    serde_json::to_value(
        lenso_service::generate_direct_http_bindings("support-http", "v1", &openapi)
            .expect("packaged direct HTTP OpenAPI fixture must generate bindings"),
    )
    .expect("direct HTTP bindings must serialize")
}

pub fn generated_direct_grpc_bindings() -> Value {
    serde_json::to_value(
        lenso_service::generate_direct_grpc_bindings(
            "support-grpc",
            "v1",
            lenso_service::DIRECT_GRPC_PROTO_V1_FIXTURE,
            lenso_service::DIRECT_GRPC_DESCRIPTOR_V1,
        )
        .expect("packaged direct gRPC Protobuf fixture must generate bindings"),
    )
    .expect("direct gRPC bindings must serialize")
}

pub fn generated_direct_grpc_proto() -> &'static str {
    lenso_service::DIRECT_GRPC_PROTO_V1_FIXTURE
}

pub fn generated_system_v2_schema() -> Value {
    serde_json::from_str(lenso_service::SYSTEM_V2_CONTRACT_SCHEMA_JSON)
        .expect("packaged System v2 schema must be valid JSON")
}

pub fn generated_system_v2_fixture() -> Value {
    serde_json::from_str(lenso_service::MIXED_SYSTEM_V2_FIXTURE_JSON)
        .expect("packaged mixed System v2 fixture must be valid JSON")
}

pub fn generated_service_release_schema() -> Value {
    generated_delivery_schema::<lenso_service::ServiceRelease>("lenso.service-release.v1")
}

pub fn generated_ga_support_manifest_schema() -> Value {
    lenso_service::ga_support_manifest_schema()
}

pub fn generated_manifest_migration_plan_schema() -> Value {
    lenso_service::manifest_migration_plan_schema()
}

pub fn generated_service_upgrade_plan_schema() -> Value {
    lenso_service::service_upgrade_plan_schema()
}

pub fn generated_contract_retirement_plan_schema() -> Value {
    lenso_service::contract_retirement_plan_schema()
}

pub fn generated_failure_scenario_evidence_schema() -> Value {
    lenso_service::failure_scenario_evidence_schema()
}

pub fn generated_delivery_failure_recovery_schema() -> Value {
    lenso_service::delivery_failure_recovery_schema()
}

pub fn generated_performance_profile_schema() -> Value {
    lenso_service::performance_profile_schema()
}

pub fn generated_service_restore_evidence_schema() -> Value {
    lenso_service::service_restore_evidence_schema()
}

pub fn generated_disaster_recovery_evidence_schema() -> Value {
    lenso_service::disaster_recovery_evidence_schema()
}

pub fn generated_support_envelope_schema() -> Value {
    lenso_service::support_envelope_schema()
}

pub fn generated_security_review_evidence_schema() -> Value {
    lenso_service::security_review_evidence_schema()
}

pub fn generated_ga_support_manifest() -> Value {
    use lenso_service::{
        ComponentKind, DocumentationIdentity, EvidenceReceiptTrust, GaComponent,
        GaSupportManifestInput, ManifestFormat, ManifestKind, SupportCombinationInput,
        SupportStatus, UpgradeEdgeInput, assemble_ga_support_manifest_with_trust,
        extraction_input_digest,
    };
    use std::collections::BTreeMap;

    let component = |kind, component_id: &str, version: &str| GaComponent {
        kind,
        component_id: component_id.to_owned(),
        version: version.to_owned(),
        digest: extraction_input_digest(format!("{component_id}@{version}").as_bytes()),
    };
    let components = vec![
        component(ComponentKind::Cli, "@lenso/cli", "0.2.13"),
        component(ComponentKind::Runtime, "lenso-service", "0.1.14"),
        component(ComponentKind::Runtime, "lenso-autonomous-service", "0.1.10"),
        component(ComponentKind::Contracts, "lenso-contracts", "0.3.15"),
        component(ComponentKind::Provider, "lenso-service-provider-v1", "1"),
        component(ComponentKind::Operator, "lenso-operator", "0.1.0"),
        component(
            ComponentKind::RuntimeConsole,
            "@lenso/runtime-console",
            "0.1.2",
        ),
    ];
    let references = components.iter().map(GaComponent::reference).collect();
    let manifest = assemble_ga_support_manifest_with_trust(GaSupportManifestInput {
        status: SupportStatus::GeneralAvailability,
        components,
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
        state_versions: vec!["service-store.v1".into()],
        adapter_versions: BTreeMap::from([
            ("nats-jetstream".into(), "2.11".into()),
            ("spiffe-spire".into(), "1.12".into()),
            ("postgresql".into(), "18".into()),
        ]),
        documentation: DocumentationIdentity {
            version: "m6-ga".into(),
            digest: m6_documentation_digest(),
        },
        combinations: vec![SupportCombinationInput {
            combination_id: "m6-ga-1".into(),
            component_references: references,
            state_version: "service-store.v1".into(),
            status: SupportStatus::GeneralAvailability,
        }],
        upgrade_edges: vec![UpgradeEdgeInput {
            edge_id: "system-v1-v2".into(),
            source_format: "lenso.system.v1".into(),
            target_format: "lenso.system.v2".into(),
            mixed_version_references: vec![],
            rollback_safe: true,
        }],
    }, EvidenceReceiptTrust {
        authorities: [
            "lenso.delivery-failure-recovery-evidence.v1",
            "lenso.performance-profile.v1",
            "lenso.service-restore-evidence.v1",
            "lenso.disaster-recovery-evidence.v1",
            "lenso.support-envelope.v1",
            "lenso.security-review-evidence.v1",
        ]
        .into_iter()
        .map(|protocol| {
            (
                protocol.to_owned(),
                "m6-environment-verifier".to_owned(),
            )
        })
        .collect(),
        public_keys: BTreeMap::from([(
            "m6-environment-verifier".to_owned(),
            "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAgorlaAxUtjd1ZpD49IhDNEFA0aLzJ3ryMawUOd5ZRHE=\n-----END PUBLIC KEY-----"
                .to_owned(),
        )]),
    })
    .expect("the committed GA Support Manifest must be valid");
    serde_json::to_value(manifest).expect("GA Support Manifest must serialize")
}

fn m6_documentation_digest() -> String {
    let documentation = [
        include_str!("../../../docs/operations/m6/upgrade-and-contracts.md"),
        include_str!("../../../docs/operations/m6/failure-backup-and-disaster.md"),
        include_str!("../../../docs/operations/m6/security-and-release.md"),
        include_str!("../../../docs/operations/m6/incident-map.md"),
        include_str!("../../../docs/security/m6-threat-model.md"),
    ]
    .join("\n");
    lenso_service::extraction_input_digest(documentation.as_bytes())
}

pub fn generated_ga_support_guidance() -> String {
    let manifest: lenso_service::GaSupportManifest =
        serde_json::from_value(generated_ga_support_manifest())
            .expect("generated support manifest must deserialize");
    lenso_service::render_ga_support_manifest(&manifest)
}

pub fn generated_delivery_schema<T: JsonSchema>(protocol: &str) -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(T))
        .expect("generated delivery schema must serialize");
    let object = schema
        .as_object_mut()
        .expect("generated delivery schema root must be an object");
    object.insert(
        "$id".to_owned(),
        Value::String(format!(
            "https://contracts.lenso.local/delivery/{protocol}.schema.json"
        )),
    );
    schema
}

pub fn generated_support_service_release() -> Value {
    use lenso_service::{
        DeliveryEvidenceReference, DeterministicTrustProvider, ReleaseContractVersion,
        ReleaseMigration, ReleaseModule, ReleaseProvenance, ReleaseRetention,
        ReleaseRollbackConstraints, ReleaseRolloutGate, ReleaseWorkloadRole, ServiceReleaseInput,
        WorkloadArtifact, assemble_service_release, attach_service_release_signature,
        extraction_input_digest,
    };

    let evidence = |reference: &str| DeliveryEvidenceReference {
        reference: reference.to_owned(),
        digest: extraction_input_digest(reference.as_bytes()),
    };
    let workload = |workload_id: &str, role: ReleaseWorkloadRole| {
        let artifact_digest = extraction_input_digest(workload_id.as_bytes());
        WorkloadArtifact {
            workload_id: workload_id.to_owned(),
            role,
            artifact_reference: "ghcr.io/liorael/support".to_owned(),
            artifact_digest: artifact_digest.clone(),
            media_type: "application/vnd.oci.image.manifest.v1+json".to_owned(),
            display_tag: Some("5.0.0".to_owned()),
            sbom: evidence(&format!("sbom:{workload_id}")),
            provenance: ReleaseProvenance {
                reference: format!("provenance:{workload_id}"),
                digest: extraction_input_digest(format!("provenance:{workload_id}").as_bytes()),
                source: "https://github.com/LioRael/lenso-examples".to_owned(),
                builder: "https://github.com/LioRael/lenso-examples/actions".to_owned(),
                input_digests: vec![extraction_input_digest(b"support-source")],
                subject_digests: vec![artifact_digest],
            },
            signature_subject: format!("workload:{workload_id}"),
        }
    };
    let mut release = assemble_service_release(ServiceReleaseInput {
        service_id: "service:support".to_owned(),
        service_version: "5.0.0".to_owned(),
        modules: vec![
            ReleaseModule {
                module_id: "support-ticket".to_owned(),
                module_version: "4.0.0".to_owned(),
            },
            ReleaseModule {
                module_id: "support-sla".to_owned(),
                module_version: "2.0.0".to_owned(),
            },
        ],
        workloads: vec![
            workload("support-api", ReleaseWorkloadRole::Api),
            workload("support-worker", ReleaseWorkloadRole::Worker),
            workload("support-migration", ReleaseWorkloadRole::Migration),
        ],
        contract_versions: vec![ReleaseContractVersion {
            contract_id: "support-http".to_owned(),
            version: "v1".to_owned(),
            kind: "request_response".to_owned(),
            artifact: evidence("contracts/openapi/support.v1.yaml"),
        }],
        config_contract: evidence("contracts/config/support.v1.schema.json"),
        reliability_contract: evidence("contracts/reliability/support.v1.schema.json"),
        migrations: vec![ReleaseMigration {
            migration_id: "support-0001".to_owned(),
            phase: "expand".to_owned(),
            artifact: evidence("migration:support-0001"),
            reversible: true,
        }],
        workflow_compatibility: vec![evidence("workflow-compatibility:support:v1")],
        verification_evidence: vec![evidence("verification:m4-support")],
        rollout_gates: vec![ReleaseRolloutGate {
            gate_id: "service-reliability".to_owned(),
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
    })
    .expect("support Service Release fixture must assemble");
    let provider = DeterministicTrustProvider::new([("ci:fixture", "fixture-signing-material")]);
    attach_service_release_signature(&mut release, &provider, "ci:fixture")
        .expect("support Service Release fixture must sign");
    serde_json::to_value(release).expect("support Service Release fixture must serialize")
}

pub fn generated_autonomous_service_crd() -> serde_yaml::Value {
    serde_yaml::to_value(lenso_operator::LensoAutonomousService::crd())
        .expect("Autonomous Service CRD must serialize")
}

pub fn generated_support_autonomous_service() -> serde_yaml::Value {
    use lenso_operator::{
        LensoAutonomousService, LensoAutonomousServiceSpec, LensoAutonomousWorkload,
        OperatorPlacement, OperatorScaling, OperatorWorkloadRole,
    };

    let release = generated_support_service_release();
    let workloads = release["workloads"]
        .as_array()
        .expect("support release Workloads")
        .iter()
        .map(|workload| {
            let role = match workload["role"].as_str().expect("Workload role") {
                "api" => OperatorWorkloadRole::Api,
                "worker" => OperatorWorkloadRole::Worker,
                "migration" => OperatorWorkloadRole::Migration,
                _ => OperatorWorkloadRole::Extension,
            };
            LensoAutonomousWorkload {
                workload_id: workload["workloadId"]
                    .as_str()
                    .expect("Workload id")
                    .to_owned(),
                role,
                image: format!(
                    "{}@{}",
                    workload["artifactReference"]
                        .as_str()
                        .expect("artifact reference"),
                    workload["artifactDigest"]
                        .as_str()
                        .expect("artifact digest")
                ),
                replicas: 1,
                port: (role == OperatorWorkloadRole::Api).then_some(8080),
                command: Vec::new(),
                config_map_name: Some("support-config".to_owned()),
                secret_reference_ids: Vec::new(),
                placement: OperatorPlacement::default(),
                scaling: OperatorScaling {
                    min_replicas: 1,
                    max_replicas: 3,
                    target_cpu_utilization: 70,
                },
                disruption_min_available: (role != OperatorWorkloadRole::Migration).then_some(1),
                network_policy_enabled: true,
                readiness_path: (role == OperatorWorkloadRole::Api)
                    .then(|| "/health/ready".to_owned()),
                liveness_path: (role == OperatorWorkloadRole::Api)
                    .then(|| "/health/live".to_owned()),
            }
        })
        .collect();
    serde_yaml::to_value(LensoAutonomousService::new(
        "support-production",
        LensoAutonomousServiceSpec {
            service_id: "service:support".to_owned(),
            environment: "production".to_owned(),
            release_id: release["releaseId"]
                .as_str()
                .expect("release id")
                .to_owned(),
            release_digest: release["releaseDigest"]
                .as_str()
                .expect("release digest")
                .to_owned(),
            config_revision_id: "config:support:production:v5".to_owned(),
            expected_environment_revision: 31,
            secret_references: Vec::new(),
            policy_evidence_references: vec!["policy:production:support:v5".to_owned()],
            evidence_references: vec!["environment-verification:staging:v5".to_owned()],
            workloads,
            rollout_strategy: "bounded_canary".to_owned(),
            rollback_release_id: Some("release:support:4".to_owned()),
        },
    ))
    .expect("support Autonomous Service fixture must serialize")
}

pub fn generated_extraction_readiness_schema() -> Value {
    lenso_service::extraction_readiness_report_schema()
}

pub fn generated_support_ticket_extraction_readiness_blocked() -> Value {
    serde_json::to_value(support_ticket_extraction_readiness_report(true))
        .expect("blocked support-ticket Extraction Readiness Report must serialize")
}

pub fn generated_support_ticket_extraction_readiness_blocked_human() -> String {
    lenso_service::render_extraction_readiness_report(&support_ticket_extraction_readiness_report(
        true,
    ))
}

pub fn generated_support_ticket_extraction_readiness_corrected() -> Value {
    serde_json::to_value(support_ticket_extraction_readiness_report(false))
        .expect("corrected support-ticket Extraction Readiness Report must serialize")
}

pub fn generated_support_ticket_extraction_readiness_corrected_human() -> String {
    lenso_service::render_extraction_readiness_report(&support_ticket_extraction_readiness_report(
        false,
    ))
}

pub fn generated_extraction_plan_schema() -> Value {
    lenso_service::extraction_plan_schema()
}

pub fn generated_support_ticket_extraction_plan() -> Value {
    serde_json::to_value(support_ticket_extraction_plan())
        .expect("support-ticket Extraction Plan must serialize")
}

pub fn generated_support_ticket_extraction_plan_human() -> String {
    lenso_service::render_extraction_plan(&support_ticket_extraction_plan())
}

fn support_sla_updated_schema_source() -> String {
    format!(
        "{}\n",
        serde_json::to_string_pretty(&json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "https://contracts.lenso.local/events/support.sla-updated.v1.schema.json",
            "title": "support.sla-updated.v1",
            "type": "object",
            "required": ["ticketId", "slaHours"],
            "properties": {
                "ticketId": { "type": "string", "minLength": 1 },
                "slaHours": { "type": "integer", "minimum": 1 }
            },
            "additionalProperties": false
        }))
        .expect("support SLA Event schema must serialize")
    )
}

fn support_audit_recorded_schema_source() -> String {
    format!(
        "{}\n",
        serde_json::to_string_pretty(&json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "https://contracts.lenso.local/events/support.audit-recorded.v1.schema.json",
            "title": "support.audit-recorded.v1",
            "type": "object",
            "required": ["ticketId", "action"],
            "properties": {
                "ticketId": { "type": "string", "minLength": 1 },
                "action": { "type": "string", "minLength": 1 }
            },
            "additionalProperties": false
        }))
        .expect("support audit Event schema must serialize")
    )
}

pub fn generated_extraction_scaffold_schema() -> Value {
    lenso_service::extraction_scaffold_schema()
}

pub fn generated_support_ticket_extraction_scaffold() -> Value {
    serde_json::to_value(support_ticket_extraction_scaffold())
        .expect("support-ticket Extraction Scaffold must serialize")
}

pub fn generated_support_ticket_extraction_scaffold_patch() -> String {
    support_ticket_extraction_scaffold().patch
}

pub fn generated_extraction_run_schema() -> Value {
    lenso_service::extraction_run_schema()
}

pub fn generated_support_ticket_extraction_run() -> Value {
    serde_json::to_value(support_ticket_extraction_run())
        .expect("support-ticket destination-expansion Run must serialize")
}

pub fn generated_support_ticket_extraction_run_human() -> String {
    lenso_service::render_extraction_run(&support_ticket_extraction_run())
}

fn support_ticket_extraction_run() -> lenso_service::ExtractionRun {
    use lenso_service::{
        ExtractionExpansionOperationKind, ExtractionMigrationArtifact, ExtractionOperationOutcome,
        ExtractionRunEvidence, ExtractionRunEvidenceKind, ExtractionRunInputs,
        ExtractionScaffoldApplyResult, ExtractionScaffoldEffects, ExtractionWorkloadRequest,
    };

    let plan = support_ticket_extraction_plan();
    let scaffold = support_ticket_extraction_scaffold();
    let unchanged_files = scaffold
        .files
        .iter()
        .map(|file| file.path.clone())
        .collect();
    let inputs = ExtractionRunInputs {
        plan: plan.clone(),
        current_plan_inputs: support_ticket_extraction_plan_inputs(),
        scaffold: scaffold.clone(),
        scaffold_apply_result: ExtractionScaffoldApplyResult {
            protocol: "lenso.extraction-scaffold-apply.v1".to_owned(),
            scaffold_id: scaffold.scaffold_id.clone(),
            plan_id: plan.plan_id.clone(),
            created_files: Vec::new(),
            unchanged_files,
            linked_authority_remains_authoritative: true,
            effects: ExtractionScaffoldEffects::default(),
        },
        migrations: vec![ExtractionMigrationArtifact {
            migration_id: "0001_create_support_tickets".to_owned(),
            source_reference: support_ticket_migration_reference().to_owned(),
            source_digest: lenso_service::extraction_input_digest(
                support_ticket_migration_sql().as_bytes(),
            ),
            sql: support_ticket_migration_sql().to_owned(),
        }],
    };
    let mut run = lenso_service::start_destination_expansion(&inputs)
        .expect("support-ticket destination expansion must start");
    for operation in run.ordered_operations.clone() {
        let (outcome, kind, detail) = match operation.kind {
            ExtractionExpansionOperationKind::CreateIsolatedStore => (
                ExtractionOperationOutcome::Created,
                ExtractionRunEvidenceKind::StoreIsolation,
                "candidate Store is isolated and owned only by support-ticket-service",
            ),
            ExtractionExpansionOperationKind::ApplyExpandMigration => (
                ExtractionOperationOutcome::Applied,
                ExtractionRunEvidenceKind::MigrationApplied,
                "expand-first support-ticket schema migration applied idempotently",
            ),
            ExtractionExpansionOperationKind::VerifyMigrationWorkload => (
                ExtractionOperationOutcome::Healthy,
                ExtractionRunEvidenceKind::MigrationWorkloadHealth,
                "public Migration Workload reports the exact plan-owned migration set",
            ),
            ExtractionExpansionOperationKind::VerifyCandidateHealth => (
                ExtractionOperationOutcome::Healthy,
                ExtractionRunEvidenceKind::CandidateHealth,
                "public API Workload health reports ready without candidate authority",
            ),
        };
        let request = ExtractionWorkloadRequest {
            run_id: run.run_id.clone(),
            plan_id: run.plan.plan_id.clone(),
            plan_digest: run.plan.plan_digest.clone(),
            expected_state: run.expected_state.clone(),
            expected_state_digest: run.expected_state_digest.clone(),
            operation: operation.clone(),
        };
        let receipt = lenso_service::build_extraction_operation_receipt(
            &request,
            outcome,
            vec![ExtractionRunEvidence {
                kind,
                subject: operation.operation_id.clone(),
                digest: lenso_service::extraction_input_digest(
                    operation.operation_digest.as_bytes(),
                ),
                detail: detail.to_owned(),
            }],
        )
        .expect("support-ticket operation receipt must build");
        run = lenso_service::record_destination_expansion_receipt(run, receipt)
            .expect("support-ticket operation receipt must record");
    }
    run
}

fn support_ticket_extraction_scaffold() -> lenso_service::ExtractionScaffold {
    use lenso_service::{ExtractionScaffoldArtifact, ExtractionScaffoldInputs};

    let plan = support_ticket_extraction_plan();
    let inputs = ExtractionScaffoldInputs {
        plan,
        module: support_ticket_extraction_module(),
        artifacts: vec![
            ExtractionScaffoldArtifact {
                contract_id: "support-ticket-http.v1".to_owned(),
                version: "v1".to_owned(),
                contents: lenso_service::DIRECT_HTTP_OPENAPI_V1_FIXTURE_YAML.to_owned(),
                protobuf_descriptor: None,
            },
            ExtractionScaffoldArtifact {
                contract_id: "support-grpc.v1".to_owned(),
                version: "v1".to_owned(),
                contents: lenso_service::DIRECT_GRPC_PROTO_V1_FIXTURE.to_owned(),
                protobuf_descriptor: Some(lenso_service::DIRECT_GRPC_DESCRIPTOR_V1.to_vec()),
            },
            ExtractionScaffoldArtifact {
                contract_id: "support.sla-updated.v1".to_owned(),
                version: "v1".to_owned(),
                contents: support_sla_updated_schema_source(),
                protobuf_descriptor: None,
            },
            ExtractionScaffoldArtifact {
                contract_id: "support.audit-recorded.v1".to_owned(),
                version: "v1".to_owned(),
                contents: support_audit_recorded_schema_source(),
                protobuf_descriptor: None,
            },
        ],
    };
    lenso_service::generate_extraction_scaffold(&inputs)
        .expect("support-ticket Extraction Scaffold must generate")
}

fn support_ticket_extraction_plan() -> lenso_service::ExtractionPlan {
    lenso_service::generate_extraction_plan(&support_ticket_extraction_plan_inputs())
        .expect("corrected support-ticket readiness must generate an Extraction Plan")
}

fn support_ticket_extraction_plan_inputs() -> lenso_service::ExtractionPlanInputs {
    use lenso_service::{
        CommonContextRequirement, ExtractionAuthorityKind, ExtractionContractArtifactFormat,
        ExtractionContractDirection, ExtractionContractKind, ExtractionEvidenceDigest,
        ExtractionExpectedAuthority, ExtractionPlanContractVersion, ExtractionPlanInputs,
        ServiceTenancyMode, extraction_input_digest,
    };

    ExtractionPlanInputs {
        readiness_report: support_ticket_extraction_readiness_report(false),
        module: support_ticket_extraction_module(),
        system: support_ticket_extraction_system(),
        contract_versions: vec![
            ExtractionPlanContractVersion {
                contract_id: "support-ticket-http.v1".to_owned(),
                version: "v1".to_owned(),
                kind: ExtractionContractKind::Service,
                direction: ExtractionContractDirection::Provides,
                artifact_reference: "contracts/openapi/support.v1.yaml".to_owned(),
                artifact_digest: extraction_input_digest(
                    lenso_service::DIRECT_HTTP_OPENAPI_V1_FIXTURE_YAML.as_bytes(),
                ),
                artifact_format: ExtractionContractArtifactFormat::Openapi,
                tenancy_mode: ServiceTenancyMode::Required,
                required_context: vec![
                    CommonContextRequirement::Story,
                    CommonContextRequirement::Trace,
                    CommonContextRequirement::ServicePrincipal,
                    CommonContextRequirement::Tenant,
                    CommonContextRequirement::Deadline,
                    CommonContextRequirement::IdempotencyKey,
                ],
                producer_id: None,
                consumer_ids: Vec::new(),
            },
            ExtractionPlanContractVersion {
                contract_id: "support-grpc.v1".to_owned(),
                version: "v1".to_owned(),
                kind: ExtractionContractKind::Service,
                direction: ExtractionContractDirection::Consumes,
                artifact_reference: "contracts/services/support-grpc.v1.proto".to_owned(),
                artifact_digest: extraction_input_digest(
                    lenso_service::DIRECT_GRPC_PROTO_V1_FIXTURE.as_bytes(),
                ),
                artifact_format: ExtractionContractArtifactFormat::Protobuf,
                tenancy_mode: ServiceTenancyMode::Required,
                required_context: vec![
                    CommonContextRequirement::Trace,
                    CommonContextRequirement::ServicePrincipal,
                    CommonContextRequirement::Tenant,
                    CommonContextRequirement::Deadline,
                    CommonContextRequirement::IdempotencyKey,
                ],
                producer_id: Some("support-sla-service".to_owned()),
                consumer_ids: Vec::new(),
            },
            ExtractionPlanContractVersion {
                contract_id: "support.sla-updated.v1".to_owned(),
                version: "v1".to_owned(),
                kind: ExtractionContractKind::Event,
                direction: ExtractionContractDirection::Consumes,
                artifact_reference: "contracts/events/support.sla-updated.v1.schema.json"
                    .to_owned(),
                artifact_digest: extraction_input_digest(
                    support_sla_updated_schema_source().as_bytes(),
                ),
                artifact_format: ExtractionContractArtifactFormat::JsonSchema,
                tenancy_mode: ServiceTenancyMode::Required,
                required_context: vec![
                    CommonContextRequirement::Story,
                    CommonContextRequirement::Trace,
                    CommonContextRequirement::ServicePrincipal,
                    CommonContextRequirement::Tenant,
                    CommonContextRequirement::Causation,
                    CommonContextRequirement::Region,
                ],
                producer_id: Some("support-sla-service".to_owned()),
                consumer_ids: Vec::new(),
            },
            ExtractionPlanContractVersion {
                contract_id: "support.audit-recorded.v1".to_owned(),
                version: "v1".to_owned(),
                kind: ExtractionContractKind::Event,
                direction: ExtractionContractDirection::Consumes,
                artifact_reference: "contracts/events/support.audit-recorded.v1.schema.json"
                    .to_owned(),
                artifact_digest: extraction_input_digest(
                    support_audit_recorded_schema_source().as_bytes(),
                ),
                artifact_format: ExtractionContractArtifactFormat::JsonSchema,
                tenancy_mode: ServiceTenancyMode::Required,
                required_context: vec![
                    CommonContextRequirement::Story,
                    CommonContextRequirement::Trace,
                    CommonContextRequirement::ServicePrincipal,
                    CommonContextRequirement::Tenant,
                    CommonContextRequirement::Causation,
                    CommonContextRequirement::Region,
                ],
                producer_id: Some("support-audit-service".to_owned()),
                consumer_ids: Vec::new(),
            },
        ],
        expected_authority: ExtractionExpectedAuthority {
            kind: ExtractionAuthorityKind::LinkedHost,
            owner_id: "support-host".to_owned(),
            revision: "support-authority-r7".to_owned(),
        },
        evidence_digests: vec![
            ExtractionEvidenceDigest {
                reference: "readiness-evidence:boundary-and-contracts".to_owned(),
                digest: extraction_input_digest(b"support-ticket-boundary-contract-evidence-v1"),
            },
            ExtractionEvidenceDigest {
                reference: "readiness-evidence:postgres-observation".to_owned(),
                digest: extraction_input_digest(b"support-store-2026-07-19:25000000:17179869184"),
            },
            ExtractionEvidenceDigest {
                reference: support_ticket_migration_reference().to_owned(),
                digest: extraction_input_digest(support_ticket_migration_sql().as_bytes()),
            },
        ],
    }
}

fn support_ticket_migration_reference() -> &'static str {
    "modules/support-ticket/migrations/0001_tickets.sql"
}

fn support_ticket_migration_sql() -> &'static str {
    "create schema if not exists support;\n\ncreate table if not exists support.tickets (\n    id text primary key,\n    title text not null,\n    status text not null,\n    created_at timestamptz not null\n);\n"
}

fn support_ticket_extraction_module() -> lenso_contracts::ModuleManifest {
    use lenso_contracts::{
        AdminSchema, ConsoleArea, ConsolePackage, ConsoleSurface, EntitySchema,
        EventHandlerDeclaration, EventSurface, FieldSchema, FieldType, ModuleHttpMethod,
        ModuleHttpRoute, ModuleManifest, RuntimeFunctionDeclaration, RuntimeSurface,
        ScheduledFunctionDeclaration, ServiceOperationMetadata, StoryDisplayDescriptor,
        StoryDisplaySource, WorkflowDataContract, WorkflowDefinition, WorkflowStepDeclaration,
    };

    ModuleManifest::builder("support-ticket")
        .capabilities(vec!["support.tickets.read".to_owned()])
        .http_routes(vec![ModuleHttpRoute {
            method: ModuleHttpMethod::Get,
            path: "/v1/tickets/{ticket_id}".to_owned(),
            capability: Some("support.tickets.read".to_owned()),
            display_name: Some("Get ticket".to_owned()),
            story_title: Some("Support ticket opened".to_owned()),
            operation: Some(ServiceOperationMetadata {
                operation_id: Some("getTicket".to_owned()),
                summary: Some("Get ticket".to_owned()),
                ..ServiceOperationMetadata::default()
            }),
        }])
        .events(EventSurface {
            handlers: vec![
                EventHandlerDeclaration {
                    name: "apply_sla_update".to_owned(),
                    event_name: "support.sla-updated.v1".to_owned(),
                    operation: None,
                },
                EventHandlerDeclaration {
                    name: "record_audit".to_owned(),
                    event_name: "support.audit-recorded.v1".to_owned(),
                    operation: None,
                },
            ],
        })
        .runtime(RuntimeSurface {
            functions: vec![RuntimeFunctionDeclaration {
                name: "support-ticket.reindex.v1".to_owned(),
                version: 1,
                queue: "support-ticket".to_owned(),
                input_schema: Some("support-ticket.reindex.v1".to_owned()),
                retry_policy: None,
                operation: None,
            }],
            schedules: vec![ScheduledFunctionDeclaration {
                name: "support-ticket-reindex".to_owned(),
                function_name: "support-ticket.reindex.v1".to_owned(),
                cron: "0 * * * *".to_owned(),
                input: json!({}),
            }],
            workflows: vec![WorkflowDefinition::new(
                "support-ticket",
                "ticket_triage",
                "v1",
                WorkflowDataContract::new("support.ticket-triage-input", "v1"),
                WorkflowDataContract::new("support.ticket-triage-result", "v1"),
                vec![WorkflowStepDeclaration::new("classify")],
            )],
        })
        .admin(AdminSchema {
            entities: vec![EntitySchema {
                name: "tickets".to_owned(),
                label: "Tickets".to_owned(),
                fields: vec![FieldSchema {
                    name: "id".to_owned(),
                    label: "ID".to_owned(),
                    field_type: FieldType::String,
                    nullable: false,
                }],
                read_capability: "support.tickets.read".to_owned(),
            }],
        })
        .console(vec![ConsoleSurface {
            name: "support-tickets".to_owned(),
            label: "Support tickets".to_owned(),
            area: ConsoleArea::Data,
            route: "/support/tickets".to_owned(),
            package: ConsolePackage {
                name: "@lenso/support-ticket-console".to_owned(),
                export: "supportTicketConsoleModule".to_owned(),
            },
            icon: None,
            required_capabilities: vec!["support.tickets.read".to_owned()],
            navigation: None,
        }])
        .story_display(vec![StoryDisplayDescriptor {
            source: StoryDisplaySource::ExecutionName {
                name: "support-ticket.reindex.v1".to_owned(),
            },
            display_name: "Reindex support tickets".to_owned(),
            story_title: Some("Support ticket maintenance".to_owned()),
        }])
        .build()
}

fn support_ticket_extraction_system() -> Value {
    json!({
        "protocol": "lenso.system.v2",
        "systemId": "support-system",
        "host": { "hostId": "support-host", "modules": ["support-ticket"] },
        "providers": [{
            "providerId": "notification-provider",
            "modules": ["notification-gateway"]
        }],
        "autonomousServices": [{
            "serviceId": "support-sla-service",
            "modules": ["support-sla"],
            "workloads": [{ "workloadId": "support-sla-api", "role": "api" }]
        }],
        "contracts": [{
            "contractId": "support.sla-updated.v1",
            "version": "v1",
            "producerKind": "autonomous_service",
            "producerId": "support-sla-service",
            "artifact": {
                "format": "json_schema",
                "path": "contracts/events/support.sla-updated.v1.schema.json"
            },
            "tenancyMode": "required"
        }],
        "consumers": [{
            "consumerId": "support-ticket-sla-updates",
            "ownerKind": "host",
            "ownerId": "support-host",
            "contractId": "support.sla-updated.v1",
            "tenancyMode": "required"
        }]
    })
}

fn support_ticket_extraction_readiness_report(
    blocked: bool,
) -> lenso_service::ExtractionReadinessReport {
    use lenso_service::{
        CompatibilityCategory, ExtractionBoundaryEvidence, ExtractionBoundaryReference,
        ExtractionBoundaryReferenceKind, ExtractionConsumerCompatibilityEvidence,
        ExtractionContractDirection, ExtractionContractEvidence, ExtractionContractKind,
        ExtractionCursorEvidence, ExtractionDataAccessEvidence, ExtractionDataAccessKind,
        ExtractionDataEvidenceSource, ExtractionDataTableEvidence, ExtractionDataVolumeEvidence,
        ExtractionEvidenceStatus, ExtractionMigrationEvidence, ExtractionReadinessEvidence,
        ExtractionServiceDataEvidence, ExtractionTransactionEvidence,
    };

    let module = support_ticket_extraction_module();
    let system = support_ticket_extraction_system();
    let mut evidence = ExtractionReadinessEvidence {
        boundary: Some(ExtractionBoundaryEvidence {
            complete: true,
            evidence_references: vec!["analyzer:rust/support-ticket".to_owned()],
            references: Vec::new(),
        }),
        contracts: Some(vec![
            ExtractionContractEvidence {
                subject: "http:GET /v1/tickets/{ticket_id}".to_owned(),
                kind: ExtractionContractKind::Service,
                direction: ExtractionContractDirection::Provides,
                status: ExtractionEvidenceStatus::Present,
                contract_id: Some("support-ticket-http.v1".to_owned()),
                evidence_references: vec!["contracts/openapi/support-ticket.v1.yaml".to_owned()],
            },
            ExtractionContractEvidence {
                subject: "event-handler:apply_sla_update".to_owned(),
                kind: ExtractionContractKind::Event,
                direction: ExtractionContractDirection::Consumes,
                status: ExtractionEvidenceStatus::Present,
                contract_id: Some("support.sla-updated.v1".to_owned()),
                evidence_references: vec![
                    "contracts/events/support.sla-updated.v1.schema.json".to_owned(),
                ],
            },
            ExtractionContractEvidence {
                subject: "event-handler:record_audit".to_owned(),
                kind: ExtractionContractKind::Event,
                direction: ExtractionContractDirection::Consumes,
                status: ExtractionEvidenceStatus::Present,
                contract_id: Some("support.audit-recorded.v1".to_owned()),
                evidence_references: vec![
                    "contracts/events/support.audit-recorded.v1.schema.json".to_owned(),
                ],
            },
        ]),
        active_consumers: Some(vec![ExtractionConsumerCompatibilityEvidence {
            consumer_id: "support-ticket-sla-updates".to_owned(),
            contract_id: "support.sla-updated.v1".to_owned(),
            classification: CompatibilityCategory::Safe,
            evidence_references: vec!["system:consumer/support-ticket-sla-updates".to_owned()],
            next_action: "No action needed.".to_owned(),
        }]),
        service_data: Some(ExtractionServiceDataEvidence {
            complete: true,
            evidence_references: vec!["analyzer:postgres/support-ticket".to_owned()],
            tables: vec![
                ExtractionDataTableEvidence {
                    table: "support.tickets".to_owned(),
                    owner_module: Some("support-ticket".to_owned()),
                    source: ExtractionDataEvidenceSource::StaticDeclaration,
                    volume: None,
                    cursor: None,
                    evidence_references: vec![
                        "modules/support-ticket/migrations/0001_tickets.sql".to_owned(),
                    ],
                },
                ExtractionDataTableEvidence {
                    table: "support.tickets".to_owned(),
                    owner_module: Some("support-ticket".to_owned()),
                    source: ExtractionDataEvidenceSource::LiveStoreObservation {
                        observation_id: "support-store-2026-07-19".to_owned(),
                        store: "host-postgres".to_owned(),
                        read_only: true,
                    },
                    volume: Some(ExtractionDataVolumeEvidence {
                        approximate_rows: Some(25_000_000),
                        approximate_bytes: Some(17_179_869_184),
                        evidence_references: vec!["postgres:pg_class/support.tickets".to_owned()],
                    }),
                    cursor: Some(ExtractionCursorEvidence {
                        column: "id".to_owned(),
                        high_water_mark: "25000000".to_owned(),
                        trustworthy: true,
                        evidence_references: vec!["postgres:max(support.tickets.id)".to_owned()],
                    }),
                    evidence_references: vec![
                        "postgres:observation/support-store-2026-07-19".to_owned(),
                    ],
                },
            ],
            migrations: vec![ExtractionMigrationEvidence {
                migration: "0001_create_support_tickets".to_owned(),
                owner_module: Some("support-ticket".to_owned()),
                source: ExtractionDataEvidenceSource::StaticDeclaration,
                evidence_references: vec![
                    "modules/support-ticket/migrations/0001_tickets.sql".to_owned(),
                ],
            }],
            access_paths: vec![ExtractionDataAccessEvidence {
                accessor_module: "support-ticket".to_owned(),
                table: "support.tickets".to_owned(),
                access: ExtractionDataAccessKind::ReadWrite,
                source: ExtractionDataEvidenceSource::StaticDeclaration,
                evidence_references: vec!["modules/support-ticket/src/store.rs:14".to_owned()],
            }],
            transactions: vec![ExtractionTransactionEvidence {
                transaction: "support-ticket-update".to_owned(),
                participating_modules: vec!["support-ticket".to_owned()],
                source: ExtractionDataEvidenceSource::StaticDeclaration,
                evidence_references: vec!["modules/support-ticket/src/store.rs:41".to_owned()],
            }],
        }),
    };
    if blocked {
        evidence
            .boundary
            .as_mut()
            .expect("boundary fixture")
            .references = vec![
            ExtractionBoundaryReference {
                kind: ExtractionBoundaryReferenceKind::CrossModuleImport,
                from_module: "support-ticket".to_owned(),
                to_module: "support-sla".to_owned(),
                symbol: "support_sla::internal::SlaPolicy".to_owned(),
                evidence_reference: "modules/support-ticket/src/lib.rs:12".to_owned(),
            },
            ExtractionBoundaryReference {
                kind: ExtractionBoundaryReferenceKind::InProcessBoundaryCall,
                from_module: "support-ticket".to_owned(),
                to_module: "support-sla".to_owned(),
                symbol: "support_sla::public::evaluate".to_owned(),
                evidence_reference: "modules/support-ticket/src/service.rs:41".to_owned(),
            },
        ];
        let contracts = evidence.contracts.as_mut().expect("contract fixture");
        contracts[0].status = ExtractionEvidenceStatus::Missing;
        contracts[0].contract_id = None;
        contracts[2].status = ExtractionEvidenceStatus::Missing;
        contracts[2].contract_id = None;
        let consumer = &mut evidence
            .active_consumers
            .as_mut()
            .expect("consumer fixture")[0];
        consumer.classification = CompatibilityCategory::Breaking;
        consumer.next_action = "Migrate the Consumer to support.sla-updated.v1.".to_owned();
        let service_data = evidence
            .service_data
            .as_mut()
            .expect("service data fixture");
        service_data.tables.extend([
            ExtractionDataTableEvidence {
                table: "support.sla_policies".to_owned(),
                owner_module: Some("support-sla".to_owned()),
                source: ExtractionDataEvidenceSource::StaticDeclaration,
                volume: None,
                cursor: None,
                evidence_references: vec!["modules/support-sla/migrations/0001.sql".to_owned()],
            },
            ExtractionDataTableEvidence {
                table: "support.audit_events".to_owned(),
                owner_module: None,
                source: ExtractionDataEvidenceSource::StaticDeclaration,
                volume: None,
                cursor: None,
                evidence_references: vec!["migrations/0009_support_audit.sql".to_owned()],
            },
        ]);
        service_data.migrations.push(ExtractionMigrationEvidence {
            migration: "0009_support_audit".to_owned(),
            owner_module: None,
            source: ExtractionDataEvidenceSource::StaticDeclaration,
            evidence_references: vec!["migrations/0009_support_audit.sql".to_owned()],
        });
        service_data
            .access_paths
            .push(ExtractionDataAccessEvidence {
                accessor_module: "support-ticket".to_owned(),
                table: "support.sla_policies".to_owned(),
                access: ExtractionDataAccessKind::Read,
                source: ExtractionDataEvidenceSource::StaticDeclaration,
                evidence_references: vec!["modules/support-ticket/src/sla.rs:28".to_owned()],
            });
        service_data
            .transactions
            .push(ExtractionTransactionEvidence {
                transaction: "ticket-and-sla-update".to_owned(),
                participating_modules: vec!["support-sla".to_owned(), "support-ticket".to_owned()],
                source: ExtractionDataEvidenceSource::StaticDeclaration,
                evidence_references: vec!["modules/support-ticket/src/sla.rs:52".to_owned()],
            });
    }
    lenso_service::evaluate_extraction_readiness(&module, &system, &evidence)
}

pub fn generated_common_context_schema() -> Value {
    serde_json::from_str(lenso_service::COMMON_CONTEXT_V1_SCHEMA_JSON)
        .expect("packaged common context schema must be valid JSON")
}

pub fn generated_common_context_fixture() -> Value {
    serde_json::from_str(lenso_service::COMMON_CONTEXT_V1_FIXTURE_JSON)
        .expect("packaged common context fixture must be valid JSON")
}

pub fn generated_event_envelope_schema() -> Value {
    serde_json::from_str(lenso_service::EVENT_ENVELOPE_V1_SCHEMA_JSON)
        .expect("packaged Event Envelope schema must be valid JSON")
}

pub fn generated_support_event_contract() -> Value {
    let service: lenso_service::AutonomousServiceContract =
        serde_json::from_str(lenso_service::AUTONOMOUS_SERVICE_V2_FIXTURE_JSON)
            .expect("packaged Autonomous Service fixture must be valid");
    let payload_schema = generated_support_event_schema();
    serde_json::to_value(
        lenso_service::generate_event_contract(
            &service,
            &service.event_contracts[0],
            &payload_schema,
        )
        .expect("support Event Contract must generate"),
    )
    .expect("generated support Event Contract must serialize")
}

pub fn generated_support_event_schema() -> Value {
    serde_json::from_str(lenso_service::SUPPORT_EVENT_SCHEMA_JSON)
        .expect("packaged support Event schema must be valid JSON")
}

pub fn generated_support_event_envelope() -> Value {
    let contract: lenso_service::GeneratedEventContract =
        serde_json::from_value(generated_support_event_contract())
            .expect("generated support Event Contract must deserialize");
    let context: lenso_service::CommonContextContract =
        serde_json::from_str(lenso_service::COMMON_CONTEXT_V1_FIXTURE_JSON)
            .expect("packaged common context fixture must be valid");
    let envelope = lenso_service::EventEnvelope::new(
        &contract,
        "event_support_ticket_01",
        "2026-07-14T10:15:30Z",
        context,
        json!({
            "ticketId": "ticket_01",
            "openedAt": "2026-07-14T10:15:00Z"
        }),
    );
    assert!(lenso_service::validate_event_envelope(&contract, &envelope).is_empty());
    serde_json::to_value(envelope).expect("generated support Event Envelope must serialize")
}

pub fn generated_common_context_glossary() -> &'static str {
    lenso_service::COMMON_CONTEXT_GLOSSARY_MARKDOWN
}

pub fn generated_contract_compatibility() -> &'static str {
    lenso_service::CONTRACT_COMPATIBILITY_MARKDOWN
}

pub fn generated_contract_compatibility_matrix() -> Value {
    let mut rows = Vec::new();
    for (kind, fixtures, evaluate) in [
        (
            "event_contract",
            lenso_service::EVENT_COMPATIBILITY_FIXTURES,
            lenso_service::evaluate_event_compatibility
                as fn(&Value) -> lenso_service::ContractCompatibilityResult,
        ),
        (
            "config_contract",
            lenso_service::CONFIG_COMPATIBILITY_FIXTURES,
            lenso_service::evaluate_config_compatibility
                as fn(&Value) -> lenso_service::ContractCompatibilityResult,
        ),
        (
            "reliability_contract",
            lenso_service::RELIABILITY_COMPATIBILITY_FIXTURES,
            lenso_service::evaluate_reliability_compatibility
                as fn(&Value) -> lenso_service::ContractCompatibilityResult,
        ),
    ] {
        for fixture in fixtures {
            let input: Value = serde_json::from_str(fixture.json)
                .expect("compatibility fixture must be valid JSON");
            rows.push(
                json!({"contractKind": kind, "name": fixture.name, "result": evaluate(&input)}),
            );
        }
    }
    let before: lenso_service::GeneratedEventContract =
        serde_json::from_value(generated_support_event_contract())
            .expect("generated support Event Contract must deserialize");
    for (name, candidate) in generated_support_event_compatibility_candidates(&before) {
        rows.push(json!({
            "contractKind": "generated_event_contract",
            "name": name,
            "result": lenso_service::evaluate_generated_event_contract_compatibility(
                &before,
                &candidate,
            )
        }));
    }
    Value::Array(rows)
}

fn generated_support_event_compatibility_candidates(
    before: &lenso_service::GeneratedEventContract,
) -> Vec<(&'static str, lenso_service::GeneratedEventContract)> {
    let mut safe = before.clone();
    safe.contract_version = "v2".to_owned();
    safe.event_type = "support.ticket-opened.v2".to_owned();
    safe.artifact.path = "contracts/events/support/support.ticket-opened.v2.schema.json".to_owned();
    safe.payload_schema["title"] = json!("support.ticket-opened.v2");
    safe.payload_schema["properties"]["priority"] = json!({"type": "string"});
    let mut needs_attention = safe.clone();
    needs_attention
        .operating_regions
        .push("eu-west-1".to_owned());
    let mut breaking = safe.clone();
    breaking
        .context
        .required
        .push(lenso_service::CommonContextRequirement::Deadline);
    let mut blocked = safe.clone();
    blocked.protocol = "lenso.event-contract.v2".to_owned();
    vec![
        ("safe", safe),
        ("needs_attention", needs_attention),
        ("breaking", breaking),
        ("blocked", blocked),
    ]
}

fn write_yaml(path: impl AsRef<Path>, value: &impl serde::Serialize) -> anyhow::Result<()> {
    let path = path.as_ref();
    ensure_parent(path)?;
    let rendered = serde_yaml::to_string(value).context("contract should serialize as yaml")?;
    fs::write(path, rendered).with_context(|| format!("failed to write {}", path.display()))
}

fn write_json(path: impl AsRef<Path>, value: &Value) -> anyhow::Result<()> {
    let path = path.as_ref();
    ensure_parent(path)?;
    let rendered = format!(
        "{}\n",
        serde_json::to_string_pretty(value).context("contract should serialize as json")?
    );
    fs::write(path, rendered).with_context(|| format!("failed to write {}", path.display()))
}

fn write_text(path: impl AsRef<Path>, value: &str) -> anyhow::Result<()> {
    let path = path.as_ref();
    ensure_parent(path)?;
    fs::write(path, value).with_context(|| format!("failed to write {}", path.display()))
}

fn ensure_parent(path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    Ok(())
}

fn error_response_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://contracts.lenso.local/errors/error-response.v1.schema.json",
        "title": "ErrorResponse",
        "type": "object",
        "required": [
            "type",
            "title",
            "status",
            "detail",
            "code",
            "request_id",
            "correlation_id",
            "errors"
        ],
        "properties": {
            "type": {
                "type": "string",
                "format": "uri-reference"
            },
            "title": { "type": "string" },
            "status": {
                "type": "integer",
                "minimum": 100,
                "maximum": 599
            },
            "detail": { "type": "string" },
            "code": { "type": "string" },
            "request_id": { "type": ["string", "null"] },
            "correlation_id": { "type": ["string", "null"] },
            "errors": {
                "type": "array",
                "items": { "$ref": "#/$defs/ProblemErrorDetail" }
            },
            "next_actions": {
                "type": ["array", "null"],
                "items": { "type": "string" }
            }
        },
        "$defs": {
            "ProblemErrorDetail": {
                "type": "object",
                "required": ["field", "reason"],
                "properties": {
                    "field": { "type": ["string", "null"] },
                    "reason": { "type": "string" }
                },
                "additionalProperties": false
            }
        },
        "additionalProperties": false
    })
}
