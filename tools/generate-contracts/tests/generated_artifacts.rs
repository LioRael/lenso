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
