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
