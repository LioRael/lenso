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
    let routes = manifest["http_routes"].as_array().expect("http routes");
    assert!(has_route(routes, "GET", "/contacts"));
    assert!(has_route(routes, "POST", "/contacts"));
    assert!(has_route(routes, "GET", "/contacts/{id}"));
    assert!(has_route(routes, "PUT", "/contacts/{id}"));
    assert!(has_route(routes, "PATCH", "/contacts/{id}"));
    assert!(has_route(routes, "DELETE", "/contacts/{id}"));
    assert!(has_route(routes, "DELETE", "/contacts/{id}/purge"));
    assert!(has_route(routes, "GET", "/proxy-fixtures/text"));
    assert!(has_route(routes, "GET", "/proxy-fixtures/oversized"));
    assert!(has_route(routes, "GET", "/proxy-fixtures/slow"));
    let fetch_contact = route(routes, "GET", "/contacts/{id}").expect("fetch contact route");
    assert_eq!(fetch_contact["display_name"], "Fetch Contact");
    assert_eq!(fetch_contact["story_title"], "Fetch Contact");
    let fixture_text = route(routes, "GET", "/proxy-fixtures/text").expect("text fixture route");
    assert_eq!(fixture_text["display_name"], "Fetch Text Fixture");
    assert!(
        routes
            .iter()
            .all(|route| route["capability"] == "remote_crm.contacts.read")
    );
    assert_eq!(
        manifest["capabilities"],
        json!(["remote_crm.contacts.read"])
    );
    let functions = manifest["runtime"]["functions"]
        .as_array()
        .expect("runtime functions");
    assert_eq!(functions.len(), 1);
    assert_eq!(functions[0]["name"], "remote_crm.sync_contact.v1");
    assert_eq!(functions[0]["version"], 1);
    assert_eq!(functions[0]["queue"], "remote-crm");
    assert_eq!(functions[0]["input_schema"], "remote_crm.sync_contact.v1");
    assert_eq!(functions[0]["retry_policy"]["max_attempts"], 3);
    assert_eq!(functions[0]["retry_policy"]["initial_delay_ms"], 1000);
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
    assert!(manifest["runtime"].is_null());
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
    assert!(manifest["runtime"].is_null());
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
async fn runtime_function_invoke_returns_output_envelope() {
    let response = remote_module_example::router()
        .oneshot(
            http::Request::builder()
                .method(http::Method::POST)
                .uri("/lenso/module/v1/runtime/functions/remote_crm.sync_contact.v1/invoke")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(
                    r#"{"request_id":"fnrun_1","function_run_id":"fnrun_1","function_name":"remote_crm.sync_contact.v1","attempt":1,"correlation_id":"corr_1","causation_id":"httpreq_1","actor":{"kind":"service","service_id":"worker","scopes":[]},"trace":{"trace_id":"trace_1","span_id":"span_1","baggage":[]},"input":{"contact_id":"contact_1"}}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let value: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(value["output"]["synced"], true);
    assert_eq!(value["output"]["contact_id"], "contact_1");
    assert_eq!(value["output"]["request_id"], "fnrun_1");
    assert_eq!(value["output"]["function_run_id"], "fnrun_1");
    assert_eq!(value["output"]["correlation_id"], "corr_1");
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
async fn http_contacts_post_route_accepts_json() {
    let response = remote_module_example::router()
        .oneshot(
            http::Request::builder()
                .method(http::Method::POST)
                .uri("/lenso/module/v1/contacts")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(
                    r#"{"id":"contact_new","email":"new@example.com"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let contact: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(contact["id"], "contact_new");
    assert_eq!(contact["email"], "new@example.com");
    assert_eq!(contact["operation"], "created");
    assert_eq!(contact["input"]["email"], "new@example.com");
}

#[tokio::test]
async fn http_contacts_put_and_patch_routes_accept_json() {
    let put = remote_module_example::router()
        .oneshot(
            http::Request::builder()
                .method(http::Method::PUT)
                .uri("/lenso/module/v1/contacts/contact_1")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(r#"{"email":"updated@example.com"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(put.status(), StatusCode::OK);
    let body = axum::body::to_bytes(put.into_body(), usize::MAX)
        .await
        .unwrap();
    let contact: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(contact["id"], "contact_1");
    assert_eq!(contact["email"], "updated@example.com");
    assert_eq!(contact["operation"], "replaced");

    let patch = remote_module_example::router()
        .oneshot(
            http::Request::builder()
                .method(http::Method::PATCH)
                .uri("/lenso/module/v1/contacts/contact_2")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(r#"{"email":"patched@example.com"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(patch.status(), StatusCode::OK);
    let body = axum::body::to_bytes(patch.into_body(), usize::MAX)
        .await
        .unwrap();
    let contact: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(contact["id"], "contact_2");
    assert_eq!(contact["email"], "patched@example.com");
    assert_eq!(contact["operation"], "patched");
}

#[tokio::test]
async fn http_contacts_delete_routes_return_json_or_empty_success() {
    let deleted = remote_module_example::router()
        .oneshot(
            http::Request::builder()
                .method(http::Method::DELETE)
                .uri("/lenso/module/v1/contacts/contact_1")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(deleted.status(), StatusCode::OK);
    let body = axum::body::to_bytes(deleted.into_body(), usize::MAX)
        .await
        .unwrap();
    let contact: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(contact["id"], "contact_1");
    assert_eq!(contact["deleted"], true);

    let purged = remote_module_example::router()
        .oneshot(
            http::Request::builder()
                .method(http::Method::DELETE)
                .uri("/lenso/module/v1/contacts/contact_1/purge")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(purged.status(), StatusCode::NO_CONTENT);
    let body = axum::body::to_bytes(purged.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn http_proxy_fixture_routes_cover_response_policy() {
    let text = remote_module_example::router()
        .oneshot(
            http::Request::builder()
                .uri("/lenso/module/v1/proxy-fixtures/text")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(text.status(), StatusCode::OK);
    assert_eq!(
        text.headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/plain; charset=utf-8")
    );

    let oversized = remote_module_example::router()
        .oneshot(
            http::Request::builder()
                .uri("/lenso/module/v1/proxy-fixtures/oversized")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(oversized.status(), StatusCode::OK);
    assert_eq!(
        oversized
            .headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/json")
    );
    let body = axum::body::to_bytes(oversized.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(body.len() > 4 * 1024 * 1024);
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

fn has_route(routes: &[Value], method: &str, path: &str) -> bool {
    route(routes, method, path).is_some()
}

fn route<'a>(routes: &'a [Value], method: &str, path: &str) -> Option<&'a Value> {
    routes
        .iter()
        .find(|route| route["method"] == method && route["path"] == path)
}
