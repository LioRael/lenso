use platform_http::{ErrorResponse, ProblemErrorDetail};
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(components(schemas(ErrorResponse, ProblemErrorDetail)))]
struct ErrorSchemaApi;

#[test]
fn error_response_schema_is_exportable() {
    let value =
        serde_json::to_value(ErrorSchemaApi::openapi()).expect("OpenAPI document should serialize");
    let schemas = &value["components"]["schemas"];

    assert!(schemas["ErrorResponse"].is_object());
    assert!(schemas["ProblemErrorDetail"].is_object());
    assert_eq!(
        schemas["ErrorResponse"]["properties"]["type"]["type"],
        "string"
    );
    assert_eq!(
        schemas["ProblemErrorDetail"]["properties"]["reason"]["type"],
        "string"
    );
}
