use app_api::{build_router, openapi_document};
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use platform_core::{
    AppConfig, AppContext, LoggingEventPublisher, ModuleConfig, ModuleSourcesConfig,
};
use platform_module::{ModuleHttpMethod, ModuleManifest, ModuleSource};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};
use tower::ServiceExt;

fn app_config_with_default_modules() -> AppConfig {
    let mut config = AppConfig::from_env();
    // ponytail: route/profile tests assert built-in module state, not local .env toggles.
    config.module_sources = ModuleSourcesConfig::default();
    config.modules.clear();
    config
}

#[test]
fn openapi_contains_auth_dev_session_contract() {
    let document = openapi_document();
    let value = serde_json::to_value(&document).expect("OpenAPI document should serialize");

    let operation = &value["paths"]["/v1/auth/dev/sessions"]["post"];
    assert_eq!(operation["operationId"], "auth_create_dev_session");
    assert_eq!(
        operation["requestBody"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/CreateDevSessionRequest"
    );
    assert_eq!(
        operation["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/CreateDevSessionResponse"
    );

    for status in ["400", "403", "500"] {
        assert_eq!(
            operation["responses"][status]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/ErrorResponse"
        );
    }

    let revoke = &value["paths"]["/v1/auth/sessions/revoke"]["post"];
    assert_eq!(revoke["operationId"], "auth_revoke_session");
    assert_eq!(
        revoke["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/RevokeSessionResponse"
    );

    for status in ["401", "500"] {
        assert_eq!(
            revoke["responses"][status]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/ErrorResponse"
        );
    }
}

#[test]
fn openapi_contains_auth_password_contract() {
    let document = openapi_document();
    let value = serde_json::to_value(&document).expect("OpenAPI document should serialize");

    let register = &value["paths"]["/v1/auth/password/register"]["post"];
    assert_eq!(register["operationId"], "auth_password_register");
    assert_eq!(
        register["requestBody"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/PasswordRegisterRequest"
    );
    assert_eq!(
        register["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/PasswordSessionResponse"
    );

    for status in ["400", "409", "500"] {
        assert_eq!(
            register["responses"][status]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/ErrorResponse"
        );
    }

    let login = &value["paths"]["/v1/auth/password/login"]["post"];
    assert_eq!(login["operationId"], "auth_password_login");
    assert_eq!(
        login["requestBody"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/PasswordLoginRequest"
    );
    assert_eq!(
        login["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/PasswordSessionResponse"
    );

    for status in ["400", "401", "500"] {
        assert_eq!(
            login["responses"][status]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/ErrorResponse"
        );
    }
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
    platform_admin::install_default_runtime_function_declarations(vec![runtime_declaration(
        "openapi.default.sentinel",
    )]);

    let _ = openapi_document();

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
    platform_admin::install_runtime_function_declarations(vec![runtime_declaration(
        "openapi.runtime.sentinel",
    )]);

    let _ = openapi_document();

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
async fn disabled_story_module_router_does_not_mount_story_routes() {
    let _guard = catalog_test_lock()
        .lock()
        .expect("catalog test lock poisoned");
    let _ = openapi_document();

    let mut config = app_config_with_default_modules();
    config.modules.insert(
        "platform-story".to_owned(),
        ModuleConfig {
            enabled: Some(false),
            values: BTreeMap::new(),
        },
    );
    let ctx = AppContext::new(
        config,
        platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build"),
        Arc::new(LoggingEventPublisher),
    );
    let app = app_api::try_build_router(ctx).expect("demo profile router should build");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/runtime/stories")
                .method("GET")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn served_openapi_omits_disabled_story_module_routes() {
    let _guard = catalog_test_lock()
        .lock()
        .expect("catalog test lock poisoned");
    let _ = openapi_document();

    let mut config = app_config_with_default_modules();
    config.modules.insert(
        "platform-story".to_owned(),
        ModuleConfig {
            enabled: Some(false),
            values: BTreeMap::new(),
        },
    );
    let ctx = AppContext::new(
        config,
        platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build"),
        Arc::new(LoggingEventPublisher),
    );
    let app = app_api::try_build_router(ctx).expect("demo profile router should build");

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

    assert!(!paths.contains_key("/admin/runtime/stories"));
    assert!(!paths.contains_key("/admin/runtime/stories/{correlation_id}"));
    assert!(!paths.contains_key("/admin/runtime/stories/{correlation_id}/heatmap"));
    assert!(!paths.contains_key("/admin/runtime/stories/{correlation_id}/technical-operations"));
}

#[tokio::test]
async fn served_core_profile_openapi_omits_demo_auth_paths_after_demo_document_assembly() {
    let _guard = catalog_test_lock()
        .lock()
        .expect("catalog test lock poisoned");
    let _ = openapi_document();

    let mut config = app_config_with_default_modules();
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

    assert!(!paths.contains_key("/v1/auth/dev/sessions"));
    assert!(!paths.contains_key("/v1/auth/password/register"));
    assert!(!tags.iter().any(|tag| tag["name"] == "auth"));
}

#[tokio::test]
async fn served_core_profile_openapi_keeps_composed_auth_routes() {
    let _guard = catalog_test_lock()
        .lock()
        .expect("catalog test lock poisoned");
    let _ = openapi_document();

    let mut config = app_config_with_default_modules();
    config.module_sources.linked_profile = "core".to_owned();
    let ctx = AppContext::new(
        config,
        platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build"),
        Arc::new(LoggingEventPublisher),
    );
    let composition = app_bootstrap::HostComposition::new()
        .with_linked_module(app_bootstrap::auth_linked_module())
        .with_linked_module(app_bootstrap::auth_password_linked_module());
    let app = app_api::try_build_router_with_composition(ctx, &composition)
        .expect("core profile auth composition router should build");

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

    assert!(paths.contains_key("/v1/auth/dev/sessions"));
    assert!(paths.contains_key("/v1/auth/password/register"));
    assert!(tags.iter().any(|tag| tag["name"] == "auth"));
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
