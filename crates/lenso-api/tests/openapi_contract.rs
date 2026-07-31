use lenso_api::openapi_document;

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
