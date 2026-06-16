use platform_http::{ErrorResponse, ValidationErrorDetail};
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(components(schemas(ErrorResponse, ValidationErrorDetail)))]
struct ErrorSchemaApi;

#[test]
fn error_response_schema_is_exportable() {
    let value =
        serde_json::to_value(ErrorSchemaApi::openapi()).expect("OpenAPI document should serialize");
    let schemas = &value["components"]["schemas"];

    assert!(schemas["ErrorResponse"].is_object());
    assert!(schemas["ValidationErrorDetail"].is_object());
    assert_eq!(
        schemas["ErrorResponse"]["properties"]["error"]["$ref"],
        "#/components/schemas/ErrorBody"
    );
    assert_eq!(
        schemas["ValidationErrorDetail"]["properties"]["reason"]["type"],
        "string"
    );
}
