use http::StatusCode;
use serde_json::{Value, json};
use tower::ServiceExt;

#[tokio::test]
async fn manifest_matches_remote_module_protocol() {
    let response = remote_module_example::router()
        .oneshot(
            http::Request::builder()
                .uri("/lenso/module/v1/manifest")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let manifest: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(manifest["name"], "remote-crm");
    assert_eq!(manifest["admin"]["kind"], "schema");
    assert_eq!(manifest["admin"]["entities"][0]["name"], "contacts");
    assert_eq!(manifest["http_routes"][0]["method"], "GET");
    assert_eq!(manifest["http_routes"][0]["path"], "/contacts");
    assert_eq!(
        manifest["http_routes"][0]["capability"],
        "remote_crm.contacts.read"
    );
    assert_eq!(
        manifest["capabilities"],
        json!(["remote_crm.contacts.read"])
    );
}

#[tokio::test]
async fn embedded_manifest_matches_remote_module_protocol() {
    let response = remote_module_example::router()
        .oneshot(
            http::Request::builder()
                .uri("/lenso/module/v1/embedded/manifest")
                .header("host", "127.0.0.1:4100")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let manifest: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(manifest["name"], "remote-crm-embedded");
    assert_eq!(manifest["admin"]["kind"], "embedded_custom");
    assert_eq!(manifest["http_routes"][0]["path"], "/contacts");
    assert_eq!(manifest["admin"]["runtime"], "iframe");
    assert_eq!(manifest["admin"]["entry"]["kind"], "url");
    assert_eq!(
        manifest["admin"]["entry"]["url"],
        "http://127.0.0.1:4100/lenso/module/v1/embedded/admin"
    );
    assert_eq!(
        manifest["admin"]["entry"]["allowed_origins"],
        json!(["http://127.0.0.1:4100"])
    );
    assert_eq!(
        manifest["admin"]["fallback_schema"]["entities"][0]["name"],
        "contacts"
    );
}

#[tokio::test]
async fn declarative_manifest_matches_remote_module_protocol() {
    let response = remote_module_example::router()
        .oneshot(
            http::Request::builder()
                .uri("/lenso/module/v1/declarative/manifest")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let manifest: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(manifest["name"], "remote-crm-declarative");
    assert_eq!(manifest["admin"]["kind"], "declarative_custom");
    assert_eq!(manifest["http_routes"][0]["path"], "/contacts");
    assert_eq!(manifest["admin"]["pages"][0]["name"], "overview");
    assert_eq!(
        manifest["admin"]["pages"][0]["sections"][0]["component"]["kind"],
        "metric_strip"
    );
    assert_eq!(
        manifest["admin"]["pages"][0]["sections"][1]["component"]["kind"],
        "entity_table"
    );
    assert_eq!(
        manifest["admin"]["pages"][0]["sections"][2]["component"]["kind"],
        "entity_detail"
    );
    assert_eq!(
        manifest["admin"]["pages"][0]["sections"][2]["component"]["entity"],
        "contacts"
    );
    assert_eq!(
        manifest["admin"]["fallback_schema"]["entities"][0]["name"],
        "contacts"
    );
}

#[tokio::test]
async fn embedded_admin_page_is_served_for_iframe_rendering() {
    let response = remote_module_example::router()
        .oneshot(
            http::Request::builder()
                .uri("/lenso/module/v1/embedded/admin")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("Remote CRM Embedded Admin"));
    assert!(html.contains("iframe / no bridge"));
}

#[tokio::test]
async fn contacts_list_returns_records_and_cursor_shape() {
    let response = remote_module_example::router()
        .oneshot(
            http::Request::builder()
                .uri("/lenso/module/v1/admin/contacts?limit=2")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let page: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(page["records"].as_array().unwrap().len(), 2);
    assert_eq!(page["records"][0]["email"], "ada@example.com");
    assert_eq!(page["next_cursor"], "contact_2");
}

#[tokio::test]
async fn http_contacts_route_returns_resource_json() {
    let response = remote_module_example::router()
        .oneshot(
            http::Request::builder()
                .uri("/lenso/module/v1/contacts/contact_1")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let contact: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(contact["id"], "contact_1");
    assert_eq!(contact["email"], "ada@example.com");
}

#[tokio::test]
async fn contact_detail_returns_one_record_or_404() {
    let found = remote_module_example::router()
        .oneshot(
            http::Request::builder()
                .uri("/lenso/module/v1/admin/contacts/contact_1")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(found.status(), StatusCode::OK);

    let body = axum::body::to_bytes(found.into_body(), usize::MAX)
        .await
        .unwrap();
    let detail: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(detail["record"]["email"], "ada@example.com");

    let missing = remote_module_example::router()
        .oneshot(
            http::Request::builder()
                .uri("/lenso/module/v1/admin/contacts/nope")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    let body = axum::body::to_bytes(missing.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(error["error"]["code"], "not_found");
    assert_eq!(error["error"]["retryable"], false);
    assert_eq!(error["error"]["message"], "contact nope was not found");
}
