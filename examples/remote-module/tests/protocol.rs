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
    assert_eq!(
        manifest["capabilities"],
        json!(["remote_crm.contacts.read"])
    );
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
}
