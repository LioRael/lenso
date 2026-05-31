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
fn committed_identity_user_registered_schema_matches_generator() {
    let committed: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/events/identity/identity.user_registered.v1.schema.json"
    ))
    .expect("committed event schema should parse");

    assert_eq!(
        committed,
        generate_contracts::generated_identity_user_registered_schema()
    );
}
