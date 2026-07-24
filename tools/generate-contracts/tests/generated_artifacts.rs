#[test]
fn committed_error_schema_matches_generator() {
    let committed: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/errors/error-response.v1.schema.json"
    ))
    .expect("committed error schema should parse");

    assert_eq!(
        committed,
        generate_contracts::generated_error_response_schema()
    );
}

#[test]
fn committed_autonomous_service_runtime_openapi_matches_generator() {
    let committed: serde_yaml::Value = serde_yaml::from_str(include_str!(
        "../../../contracts/openapi/autonomous-service-runtime.v1.yaml"
    ))
    .expect("committed Autonomous Service runtime OpenAPI should parse");
    let generated =
        serde_yaml::to_value(generate_contracts::generated_autonomous_service_runtime_openapi())
            .expect("generated Autonomous Service runtime OpenAPI should serialize");

    assert_eq!(committed, generated);
}

#[test]
fn committed_workflow_definition_schema_matches_generator() {
    let committed: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/workflows/lenso.workflow-definition.v1.schema.json"
    ))
    .expect("committed Workflow Definition schema should parse");

    assert_eq!(
        committed,
        generate_contracts::generated_workflow_definition_schema()
    );
}

#[test]
fn committed_workflow_compatibility_artifact_matches_generator() {
    let committed: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/workflows/lenso.workflow-compatibility.v1.json"
    ))
    .expect("committed Workflow compatibility artifact should parse");

    assert_eq!(
        committed,
        generate_contracts::generated_workflow_compatibility_artifact()
    );
}

#[test]
fn committed_autonomous_service_schema_matches_generator() {
    let committed: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/services/lenso-service.v2.schema.json"
    ))
    .expect("committed Autonomous Service schema should parse");

    assert_eq!(
        committed,
        generate_contracts::generated_autonomous_service_schema()
    );
}

#[test]
fn committed_direct_http_bindings_match_generator() {
    let committed: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/services/support-http.v1.bindings.json"
    ))
    .expect("committed direct HTTP bindings should parse");

    assert_eq!(
        committed,
        generate_contracts::generated_direct_http_bindings()
    );
}

#[test]
fn committed_direct_grpc_artifacts_match_generator() {
    let proto = include_str!("../../../contracts/services/support-grpc.v1.proto");
    let bindings: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/services/support-grpc.v1.bindings.json"
    ))
    .expect("committed direct gRPC bindings should parse");

    assert_eq!(proto, lenso_service::DIRECT_GRPC_PROTO_V1_FIXTURE);
    assert_eq!(
        bindings,
        generate_contracts::generated_direct_grpc_bindings()
    );
}

#[test]
fn committed_system_v2_artifacts_match_generator() {
    let schema: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/services/lenso-system.v2.schema.json"
    ))
    .expect("committed System v2 schema should parse");
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/services/lenso-system.v2.fixture.json"
    ))
    .expect("committed System v2 fixture should parse");

    assert_eq!(schema, generate_contracts::generated_system_v2_schema());
    assert_eq!(fixture, generate_contracts::generated_system_v2_fixture());
}

#[test]
fn committed_production_delivery_artifacts_match_generators() {
    let release_schema: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/delivery/lenso.service-release.v1.schema.json"
    ))
    .expect("committed Service Release schema should parse");
    let release: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/delivery/support.service-release.json"
    ))
    .expect("committed support Service Release should parse");
    let crd: serde_yaml::Value = serde_yaml::from_str(include_str!(
        "../../../contracts/operator/lenso-autonomous-service.v1alpha1.crd.yaml"
    ))
    .expect("committed Autonomous Service CRD should parse");
    let fixture: serde_yaml::Value = serde_yaml::from_str(include_str!(
        "../../../contracts/operator/support.autonomous-service.yaml"
    ))
    .expect("committed Autonomous Service fixture should parse");

    assert_eq!(
        release_schema,
        generate_contracts::generated_service_release_schema()
    );
    assert_eq!(
        release,
        generate_contracts::generated_support_service_release()
    );
    assert_eq!(crd, generate_contracts::generated_autonomous_service_crd());
    assert_eq!(
        fixture,
        generate_contracts::generated_support_autonomous_service()
    );
    let validator =
        jsonschema::validator_for(&release_schema).expect("Service Release schema should compile");
    assert!(validator.is_valid(&release));
    assert_eq!(release["protocol"], "lenso.service-release.v1");
    assert_eq!(fixture["kind"], "LensoAutonomousService");
    assert_eq!(
        fixture["spec"]["releaseDigest"],
        release["releaseDigest"].as_str().expect("release digest")
    );
    assert!(
        fixture["spec"]["workloads"]
            .as_sequence()
            .expect("operator workloads")
            .iter()
            .all(|workload| workload["image"]
                .as_str()
                .is_some_and(|image| image.contains("@sha256:")))
    );
}

#[test]
fn committed_ga_support_artifacts_match_the_public_generators() {
    let manifest_schema: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/ga/lenso.ga-support-manifest.v1.schema.json"
    ))
    .expect("committed GA Support Manifest schema should parse");
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/ga/lenso.ga-support-manifest.v1.json"
    ))
    .expect("committed GA Support Manifest should parse");
    let guidance = include_str!("../../../docs/operations/ga-support.md");

    assert_eq!(
        manifest_schema,
        generate_contracts::generated_ga_support_manifest_schema()
    );
    assert_eq!(
        manifest,
        generate_contracts::generated_ga_support_manifest()
    );
    assert_eq!(
        guidance,
        generate_contracts::generated_ga_support_guidance()
    );
    assert!(
        jsonschema::validator_for(&manifest_schema)
            .unwrap()
            .is_valid(&manifest)
    );
    assert_eq!(manifest["protocol"], "lenso.ga-support-manifest.v1");
    assert!(guidance.contains(&manifest["manifestDigest"].as_str().unwrap()));

    let schemas = [
        (
            include_str!("../../../contracts/ga/lenso.manifest-migration-plan.v1.schema.json"),
            generate_contracts::generated_manifest_migration_plan_schema(),
        ),
        (
            include_str!("../../../contracts/ga/lenso.service-upgrade-plan.v1.schema.json"),
            generate_contracts::generated_service_upgrade_plan_schema(),
        ),
        (
            include_str!("../../../contracts/ga/lenso.contract-retirement-plan.v1.schema.json"),
            generate_contracts::generated_contract_retirement_plan_schema(),
        ),
        (
            include_str!("../../../contracts/ga/lenso.failure-scenario-evidence.v1.schema.json"),
            generate_contracts::generated_failure_scenario_evidence_schema(),
        ),
        (
            include_str!(
                "../../../contracts/ga/lenso.delivery-failure-recovery-evidence.v1.schema.json"
            ),
            generate_contracts::generated_delivery_failure_recovery_schema(),
        ),
        (
            include_str!("../../../contracts/ga/lenso.performance-profile.v1.schema.json"),
            generate_contracts::generated_performance_profile_schema(),
        ),
        (
            include_str!("../../../contracts/ga/lenso.service-restore-evidence.v1.schema.json"),
            generate_contracts::generated_service_restore_evidence_schema(),
        ),
        (
            include_str!("../../../contracts/ga/lenso.disaster-recovery-evidence.v1.schema.json"),
            generate_contracts::generated_disaster_recovery_evidence_schema(),
        ),
        (
            include_str!("../../../contracts/ga/lenso.support-envelope.v1.schema.json"),
            generate_contracts::generated_support_envelope_schema(),
        ),
        (
            include_str!("../../../contracts/ga/lenso.security-review-evidence.v1.schema.json"),
            generate_contracts::generated_security_review_evidence_schema(),
        ),
    ];
    for (committed, generated) in schemas {
        let committed: serde_json::Value = serde_json::from_str(committed).unwrap();
        assert_eq!(committed, generated);
        jsonschema::validator_for(&committed).expect("GA contract schema should compile");
    }
}

#[test]
fn production_delivery_openapi_describes_raw_artifact_objects() {
    let openapi: serde_yaml::Value =
        serde_yaml::from_str(include_str!("../../../contracts/openapi/app-api.v1.yaml"))
            .expect("committed application OpenAPI should parse");
    let schema = &openapi["components"]["schemas"]["DeliveryArtifactSchema"];
    let variants = schema["oneOf"]
        .as_sequence()
        .expect("delivery artifacts should be an OpenAPI oneOf");

    assert!(
        variants.iter().all(|variant| variant.get("$ref").is_some()),
        "wire variants must be direct schema references without externally tagged wrappers"
    );
    assert!(variants.iter().any(|variant| {
        variant["$ref"]
            .as_str()
            .is_some_and(|reference| reference.ends_with("/ServiceRelease"))
    }));
    assert_eq!(
        openapi["components"]["schemas"]["DeliveryArtifactRecordRequest"]["properties"]["artifacts"]
            ["items"]["$ref"],
        "#/components/schemas/DeliveryArtifactSchema"
    );
}

#[test]
fn committed_extraction_readiness_artifacts_match_generator() {
    let schema: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/extraction/lenso.extraction-readiness-report.v1.schema.json"
    ))
    .expect("committed Extraction Readiness Report schema should parse");
    let blocked: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/extraction/support-ticket.blocked.json"
    ))
    .expect("committed blocked support-ticket report should parse");
    let corrected: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/extraction/support-ticket.corrected.json"
    ))
    .expect("committed corrected support-ticket report should parse");
    let blocked_human = include_str!("../../../contracts/extraction/support-ticket.blocked.txt");
    let corrected_human =
        include_str!("../../../contracts/extraction/support-ticket.corrected.txt");

    assert_eq!(
        schema,
        generate_contracts::generated_extraction_readiness_schema()
    );
    assert_eq!(
        blocked,
        generate_contracts::generated_support_ticket_extraction_readiness_blocked()
    );
    assert_eq!(
        corrected,
        generate_contracts::generated_support_ticket_extraction_readiness_corrected()
    );
    assert_eq!(
        blocked_human,
        generate_contracts::generated_support_ticket_extraction_readiness_blocked_human()
    );
    assert_eq!(
        corrected_human,
        generate_contracts::generated_support_ticket_extraction_readiness_corrected_human()
    );
    let validator = jsonschema::validator_for(&schema)
        .expect("Extraction Readiness Report schema should compile");
    assert!(validator.is_valid(&blocked));
    assert!(validator.is_valid(&corrected));
}

#[test]
fn committed_extraction_plan_artifacts_match_generator() {
    let schema = serde_json::from_str::<serde_json::Value>(include_str!(
        "../../../contracts/extraction/lenso.extraction-plan.v1.schema.json"
    ))
    .expect("committed Extraction Plan schema must parse");
    let plan = serde_json::from_str::<serde_json::Value>(include_str!(
        "../../../contracts/extraction/support-ticket.plan.json"
    ))
    .expect("committed support-ticket Extraction Plan must parse");
    let human = include_str!("../../../contracts/extraction/support-ticket.plan.txt");

    assert_eq!(
        schema,
        generate_contracts::generated_extraction_plan_schema()
    );
    assert_eq!(
        plan,
        generate_contracts::generated_support_ticket_extraction_plan()
    );
    assert_eq!(
        human,
        generate_contracts::generated_support_ticket_extraction_plan_human()
    );
    assert_eq!(
        plan["protocol"],
        serde_json::json!("lenso.extraction-plan.v1")
    );
    assert_eq!(
        plan["proposedService"]["workloads"]
            .as_array()
            .expect("workloads")
            .iter()
            .map(|workload| workload["role"].as_str().expect("role"))
            .collect::<Vec<_>>(),
        vec!["api", "worker", "migration"]
    );
    assert_eq!(plan["proposedService"]["store"]["isolated"], true);
    assert_eq!(plan["effects"]["writesRepositoryFiles"], false);
    assert_eq!(plan["effects"]["startsWorkloads"], false);
    assert_eq!(plan["effects"]["copiesData"], false);
    assert_eq!(plan["effects"]["changesAuthority"], false);
}

#[test]
fn committed_extraction_scaffold_artifacts_match_generator() {
    let schema = serde_json::from_str::<serde_json::Value>(include_str!(
        "../../../contracts/extraction/lenso.extraction-scaffold.v1.schema.json"
    ))
    .expect("committed Extraction Scaffold schema must parse");
    let scaffold = serde_json::from_str::<serde_json::Value>(include_str!(
        "../../../contracts/extraction/support-ticket.scaffold.json"
    ))
    .expect("committed support-ticket Extraction Scaffold must parse");
    let patch = include_str!("../../../contracts/extraction/support-ticket.scaffold.patch");

    assert_eq!(
        schema,
        generate_contracts::generated_extraction_scaffold_schema()
    );
    assert_eq!(
        scaffold,
        generate_contracts::generated_support_ticket_extraction_scaffold()
    );
    assert_eq!(
        patch,
        generate_contracts::generated_support_ticket_extraction_scaffold_patch()
    );
    let validator =
        jsonschema::validator_for(&schema).expect("Extraction Scaffold schema should compile");
    assert!(validator.is_valid(&scaffold));
    assert_eq!(
        scaffold["protocol"],
        serde_json::json!("lenso.extraction-scaffold.v1")
    );
    assert_eq!(
        scaffold["preservedIdentity"]["moduleName"],
        serde_json::json!("support-ticket")
    );
    assert_eq!(
        scaffold["preservedIdentity"]["operationIds"],
        serde_json::json!(["getTicket"])
    );
    assert_eq!(scaffold["linkedAuthorityRemainsAuthoritative"], true);
    assert_eq!(scaffold["providerCompatibilityPreserved"], true);
    assert_eq!(scaffold["effects"]["writesRepositoryFiles"], false);
    assert!(patch.contains("src/bin/api.rs"));
    assert!(patch.contains("src/bin/worker.rs"));
    assert!(patch.contains("src/bin/migration.rs"));
}

#[test]
fn committed_extraction_run_artifacts_match_generator() {
    let schema = serde_json::from_str::<serde_json::Value>(include_str!(
        "../../../contracts/extraction/lenso.extraction-run.v1.schema.json"
    ))
    .expect("committed Extraction Run schema must parse");
    let run = serde_json::from_str::<serde_json::Value>(include_str!(
        "../../../contracts/extraction/support-ticket.expansion-run.json"
    ))
    .expect("committed support-ticket Extraction Run must parse");
    let human = include_str!("../../../contracts/extraction/support-ticket.expansion-run.txt");

    assert_eq!(
        schema,
        generate_contracts::generated_extraction_run_schema()
    );
    assert_eq!(
        run,
        generate_contracts::generated_support_ticket_extraction_run()
    );
    assert_eq!(
        human,
        generate_contracts::generated_support_ticket_extraction_run_human()
    );
    let validator =
        jsonschema::validator_for(&schema).expect("Extraction Run schema should compile");
    assert!(validator.is_valid(&run));
    assert_eq!(run["protocol"], "lenso.extraction-run.v1");
    assert_eq!(run["currentPhase"]["status"], "succeeded");
    assert_eq!(run["receipts"].as_array().expect("receipts").len(), 4);
    assert_eq!(run["effects"]["copiesServiceData"], false);
    assert_eq!(run["effects"]["mutatesSourceStore"], false);
    assert_eq!(run["effects"]["changesAuthority"], false);
    assert_eq!(run["effects"]["performsDestructiveCleanup"], false);
}

#[test]
fn generated_support_ticket_candidate_compiles_through_public_entrypoints() {
    let scaffold = serde_json::from_value::<lenso_service::ExtractionScaffold>(
        generate_contracts::generated_support_ticket_extraction_scaffold(),
    )
    .expect("generated Extraction Scaffold must decode");
    let root = std::env::temp_dir().join(format!(
        "lenso-support-ticket-candidate-{}",
        std::process::id()
    ));
    if root.exists() {
        std::fs::remove_dir_all(&root).expect("stale candidate test root should be removable");
    }
    std::fs::create_dir_all(&root).expect("candidate test root should be creatable");
    for file in &scaffold.files {
        let path = root.join(&file.path);
        std::fs::create_dir_all(path.parent().expect("generated file parent"))
            .expect("generated parent should be creatable");
        std::fs::write(path, &file.contents).expect("generated file should be writable");
    }
    let manifest = root.join(&scaffold.destination_root).join("Cargo.toml");
    let target = root.join("target");
    let output = std::process::Command::new(env!("CARGO"))
        .args(["check", "--manifest-path"])
        .arg(&manifest)
        .env("CARGO_TARGET_DIR", &target)
        .output()
        .expect("generated candidate cargo check should run");
    assert!(
        output.status.success(),
        "generated candidate failed to compile:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::remove_dir_all(&root).expect("candidate test root should be removable");
}

#[test]
fn committed_common_context_schema_matches_generator() {
    let committed: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/context/lenso-context.v1.schema.json"
    ))
    .expect("committed common context schema should parse");

    assert_eq!(
        committed,
        generate_contracts::generated_common_context_schema()
    );
}

#[test]
fn committed_common_context_fixture_matches_generator() {
    let committed: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/context/lenso-context.v1.fixture.json"
    ))
    .expect("committed common context fixture should parse");

    assert_eq!(
        committed,
        generate_contracts::generated_common_context_fixture()
    );
}

#[test]
fn committed_event_envelope_schema_matches_generator() {
    let committed: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/events/lenso/lenso.event-envelope.v1.schema.json"
    ))
    .expect("committed Event Envelope schema should parse");

    assert_eq!(
        committed,
        generate_contracts::generated_event_envelope_schema()
    );
}

#[test]
fn committed_support_event_artifacts_match_generator() {
    let schema: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/events/support/support.ticket-opened.v1.schema.json"
    ))
    .expect("committed support Event schema should parse");
    let contract: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/events/support/support.ticket-opened.v1.artifact.json"
    ))
    .expect("committed support Event Contract artifact should parse");
    let envelope: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/events/support/support.ticket-opened.v1.envelope.json"
    ))
    .expect("committed support Event Envelope fixture should parse");

    assert_eq!(schema, generate_contracts::generated_support_event_schema());
    assert_eq!(
        contract,
        generate_contracts::generated_support_event_contract()
    );
    assert_eq!(
        envelope,
        generate_contracts::generated_support_event_envelope()
    );
}
