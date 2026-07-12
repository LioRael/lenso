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
