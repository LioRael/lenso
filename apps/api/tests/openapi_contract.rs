use app_api::{build_router, openapi_document};
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use platform_core::{AppConfig, AppContext, LoggingEventPublisher};
use platform_module::{ModuleHttpMethod, ModuleManifest, ModuleSource};
use std::sync::{Arc, Mutex, OnceLock};
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

#[test]
fn openapi_document_does_not_replace_default_admin_catalogs() {
    let _guard = catalog_test_lock()
        .lock()
        .expect("catalog test lock poisoned");
    platform_admin::reset_catalogs_for_test();
    platform_admin::install_default_story_display(vec![story_descriptor(
        "openapi.default.sentinel",
        "OpenAPI Default Sentinel",
    )]);
    platform_admin::install_default_runtime_function_declarations(vec![runtime_declaration(
        "openapi.default.sentinel",
    )]);

    let _ = openapi_document();

    assert!(
        platform_admin::story_display_catalog_snapshot()
            .iter()
            .any(|descriptor| descriptor.display_name == "OpenAPI Default Sentinel")
    );
    assert!(
        platform_admin::runtime_function_declaration_catalog_snapshot()
            .iter()
            .any(|declaration| declaration.name == "openapi.default.sentinel")
    );
}

#[test]
fn openapi_document_does_not_replace_runtime_admin_catalogs() {
    let _guard = catalog_test_lock()
        .lock()
        .expect("catalog test lock poisoned");
    platform_admin::reset_catalogs_for_test();
    platform_admin::install_story_display(vec![story_descriptor(
        "openapi.runtime.sentinel",
        "OpenAPI Runtime Sentinel",
    )]);
    platform_admin::install_runtime_function_declarations(vec![runtime_declaration(
        "openapi.runtime.sentinel",
    )]);

    let _ = openapi_document();

    assert!(
        platform_admin::story_display_catalog_snapshot()
            .iter()
            .any(|descriptor| descriptor.display_name == "OpenAPI Runtime Sentinel")
    );
    assert!(
        platform_admin::runtime_function_declaration_catalog_snapshot()
            .iter()
            .any(|declaration| declaration.name == "openapi.runtime.sentinel")
    );
}

#[test]
fn linked_module_http_routes_are_registered_in_openapi() {
    let document = openapi_document();
    let value = serde_json::to_value(&document).expect("OpenAPI document should serialize");
    let paths = value["paths"].as_object().expect("OpenAPI paths object");

    for manifest in app_bootstrap::module_manifests() {
        for route in manifest.http_routes {
            let path = paths.get(&route.path).unwrap_or_else(|| {
                panic!(
                    "linked module `{}` declares HTTP route `{}` but OpenAPI has no matching path",
                    manifest.name, route.path
                )
            });
            let method = openapi_method(route.method);
            assert!(
                path.get(method).is_some(),
                "linked module `{}` declares HTTP route `{} {}` but OpenAPI has no matching operation",
                manifest.name,
                method.to_uppercase(),
                route.path
            );
        }
    }
}

#[test]
fn linked_module_openapi_routes_are_declared_in_manifest() {
    let document = openapi_document();
    let value = serde_json::to_value(&document).expect("OpenAPI document should serialize");
    let paths = value["paths"].as_object().expect("OpenAPI paths object");
    let manifests = app_bootstrap::module_manifests();

    for owner in app_bootstrap::linked_http_route_owners() {
        let manifest = manifests
            .iter()
            .find(|manifest| manifest.name == owner.module_name)
            .unwrap_or_else(|| {
                panic!(
                    "linked HTTP route owner `{}` has no matching ModuleManifest",
                    owner.module_name
                )
            });
        for (path, operations) in paths {
            if !owner
                .public_prefixes
                .iter()
                .any(|prefix| path.starts_with(prefix))
            {
                continue;
            }
            for method in operations
                .as_object()
                .expect("OpenAPI path item should be an object")
                .keys()
                .filter_map(|method| module_http_method(method))
            {
                assert_manifest_declares_route(manifest, path, method);
            }
        }
    }
}

#[tokio::test]
async fn core_profile_router_does_not_mount_identity_routes() {
    let _guard = catalog_test_lock()
        .lock()
        .expect("catalog test lock poisoned");
    let _ = openapi_document();

    let mut config = AppConfig::from_env();
    config.module_sources.linked_profile = "core".to_owned();
    let ctx = AppContext::new(
        config,
        platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build"),
        Arc::new(LoggingEventPublisher),
    );
    let app = app_api::try_build_router(ctx).expect("core profile router should build");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/identity/users")
                .method("POST")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn served_core_profile_openapi_omits_demo_identity_paths_after_demo_document_assembly() {
    let _guard = catalog_test_lock()
        .lock()
        .expect("catalog test lock poisoned");
    let _ = openapi_document();

    let mut config = AppConfig::from_env();
    config.module_sources.linked_profile = "core".to_owned();
    let ctx = AppContext::new(
        config,
        platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build"),
        Arc::new(LoggingEventPublisher),
    );
    let app = app_api::try_build_router(ctx).expect("core profile router should build");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/openapi.json")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);

    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body should read");
    let document: serde_json::Value =
        serde_json::from_slice(&bytes).expect("served OpenAPI should be JSON");
    let paths = document["paths"]
        .as_object()
        .expect("OpenAPI paths should be an object");
    let tags = document["tags"]
        .as_array()
        .expect("OpenAPI tags should be an array");

    assert!(!paths.contains_key("/v1/identity/users"));
    assert!(!paths.contains_key("/v1/identity/me"));
    assert!(!tags.iter().any(|tag| tag["name"] == "identity"));
}

fn assert_manifest_declares_route(manifest: &ModuleManifest, path: &str, method: ModuleHttpMethod) {
    assert!(
        manifest
            .http_routes
            .iter()
            .any(|route| route.path == path && route.method == method),
        "OpenAPI route `{} {}` belongs to linked module `{}` but is missing from ModuleManifest::http_routes",
        openapi_method(method).to_uppercase(),
        path,
        manifest.name
    );
}

fn openapi_method(method: ModuleHttpMethod) -> &'static str {
    match method {
        ModuleHttpMethod::Get => "get",
        ModuleHttpMethod::Post => "post",
        ModuleHttpMethod::Put => "put",
        ModuleHttpMethod::Patch => "patch",
        ModuleHttpMethod::Delete => "delete",
        _ => panic!("unsupported module HTTP method in OpenAPI guard"),
    }
}

fn module_http_method(method: &str) -> Option<ModuleHttpMethod> {
    match method {
        "get" => Some(ModuleHttpMethod::Get),
        "post" => Some(ModuleHttpMethod::Post),
        "put" => Some(ModuleHttpMethod::Put),
        "patch" => Some(ModuleHttpMethod::Patch),
        "delete" => Some(ModuleHttpMethod::Delete),
        _ => None,
    }
}

fn story_descriptor(name: &str, display_name: &str) -> platform_core::StoryDisplayDescriptor {
    platform_core::StoryDisplayDescriptor {
        source: platform_core::StoryDisplaySource::ExecutionName {
            name: name.to_owned(),
        },
        display_name: display_name.to_owned(),
        story_title: None,
    }
}

fn runtime_declaration(name: &str) -> platform_admin::AdminRuntimeFunctionDeclarationMetadata {
    platform_admin::AdminRuntimeFunctionDeclarationMetadata {
        module_name: "openapi-contract-test".to_owned(),
        module_source: ModuleSource::Linked,
        name: name.to_owned(),
        version: 1,
        queue: "openapi-contract-test".to_owned(),
        input_schema: None,
        retry_policy: None,
    }
}

fn catalog_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[tokio::test]
async fn scalar_docs_route_serves_openapi_reference() {
    let _guard = catalog_test_lock()
        .lock()
        .expect("catalog test lock poisoned");
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
