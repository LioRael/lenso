use app_api::openapi_document;

#[test]
fn openapi_contains_identity_create_user_contract() {
    let document = openapi_document();
    let value = serde_json::to_value(&document).expect("OpenAPI document should serialize");

    let operation = &value["paths"]["/v1/identity/users"]["post"];
    assert_eq!(operation["operationId"], "identity_create_user");
    assert_eq!(
        operation["requestBody"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/CreateUserRequest"
    );
    assert_eq!(
        operation["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/CreateUserResponseEnvelope"
    );

    for status in ["400", "409", "500"] {
        assert_eq!(
            operation["responses"][status]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/ErrorResponse"
        );
    }

    let schemas = &value["components"]["schemas"];
    assert!(schemas["CreateUserRequest"].is_object());
    assert!(schemas["CreateUserResponse"].is_object());
    assert!(schemas["ErrorResponse"].is_object());
    assert!(schemas["ValidationErrorDetail"].is_object());
}

#[test]
fn committed_openapi_artifact_matches_rust_source() {
    let generated =
        serde_json::to_value(openapi_document()).expect("OpenAPI document should serialize");
    let committed: serde_json::Value =
        serde_yaml::from_str(include_str!("../../../contracts/openapi/app-api.v1.yaml"))
            .expect("committed OpenAPI artifact should parse");

    assert_eq!(committed, generated);
}
