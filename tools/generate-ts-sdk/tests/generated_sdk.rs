#[test]
fn generated_types_include_all_openapi_component_schemas() {
    let document: serde_json::Value =
        serde_yaml::from_str(include_str!("../../../contracts/openapi/app-api.v1.yaml"))
            .expect("OpenAPI contract should parse");
    let schemas = document["components"]["schemas"]
        .as_object()
        .expect("OpenAPI contract should include component schemas");
    let source =
        generate_ts_sdk::generated_types_source().expect("generated types source should render");

    for schema_name in schemas.keys() {
        assert!(
            source.contains(&format!("export type {schema_name}")),
            "generated types should include OpenAPI schema {schema_name}"
        );
    }
}

#[test]
fn committed_generated_files_are_fresh() {
    let committed_types = include_str!("../../../packages/ts-sdk/src/generated/types.ts");
    let committed_client = include_str!("../../../packages/ts-sdk/src/generated/client.ts");

    assert_eq!(
        committed_types,
        generate_ts_sdk::generated_types_source().expect("types should render")
    );
    assert_eq!(
        committed_client,
        generate_ts_sdk::generated_client_source().expect("client should render")
    );
}
