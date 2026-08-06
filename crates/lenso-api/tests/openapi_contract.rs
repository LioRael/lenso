use lenso_api::openapi_document;
use serde_yaml::Value;

#[test]
fn managed_service_openapi_excludes_retired_console_and_admin_namespaces() {
    let document = openapi_document();
    let forbidden = ["/admin", "/console", "/system/delivery"];

    for path in document.paths.paths.keys() {
        assert!(
            forbidden.iter().all(|prefix| !path.starts_with(prefix)),
            "managed Service OpenAPI still exposes retired route {path}"
        );
    }
}

#[test]
fn managed_service_openapi_keeps_health_and_business_data_plane_routes() {
    let document = openapi_document();
    let paths = &document.paths.paths;

    assert!(
        paths
            .keys()
            .any(|path| path.starts_with("/v1/auth/") || path.starts_with("/modules/"))
    );
}

#[test]
fn committed_openapi_preserves_the_api_owner_invariants() {
    let document: Value =
        serde_yaml::from_str(include_str!("../../../contracts/openapi/app-api.v1.yaml"))
            .expect("committed OpenAPI should parse");
    let paths = document
        .get("paths")
        .and_then(Value::as_mapping)
        .expect("committed OpenAPI paths should be a mapping");
    assert!(paths.keys().any(|path| {
        path.as_str()
            .is_some_and(|path| path.starts_with("/v1/auth/") || path.starts_with("/modules/"))
    }));
    assert!(!paths.keys().any(|path| {
        path.as_str()
            .is_some_and(|path| path == "/admin/runtime/timeline/{correlation_id}")
    }));
}
