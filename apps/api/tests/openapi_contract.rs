use app_api::{build_router, openapi_document};
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use platform_core::{AppConfig, AppContext, LoggingEventPublisher};
use std::sync::Arc;
use tower::ServiceExt;

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

#[tokio::test]
async fn scalar_docs_route_serves_openapi_reference() {
    let ctx = AppContext::new(
        AppConfig::from_env(),
        platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build"),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/docs")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);

    let content_type = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .expect("docs response should include content type")
        .to_str()
        .expect("content type should be valid");
    assert!(content_type.starts_with("text/html"));

    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body should read");
    let body = String::from_utf8(bytes.to_vec()).expect("body should be utf-8");

    assert!(body.contains("@scalar/api-reference"));
    assert!(body.contains("url: \"/openapi.json\""));
}
