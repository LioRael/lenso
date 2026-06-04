use axum::http::StatusCode;
use axum::{Json, Router, routing::get};
use platform_module::{AdminDataSource, AdminListQuery, AdminSurface};
use platform_module_remote::{RemoteAdminDataSource, RemoteModuleConfig, RemoteModuleSource};
use serde_json::{Value, json};
use tokio::net::TcpListener;

async fn spawn_server(router: Router) -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind test server");
    let address = listener.local_addr().expect("test server address");
    tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("test server should run");
    });
    format!("http://{address}")
}

async fn manifest() -> Json<Value> {
    Json(json!({
        "name": "remote-crm",
        "story_display": [],
        "admin": {
            "kind": "schema",
            "entities": [{
                "name": "contacts",
                "label": "Contacts",
                "fields": [],
                "read_capability": "remote_crm.contacts.read"
            }]
        },
        "http_routes": [{
            "method": "GET",
            "path": "/contacts",
            "capability": "remote_crm.contacts.read"
        }, {
            "method": "GET",
            "path": "/contacts/{id}",
            "capability": "remote_crm.contacts.read"
        }],
        "runtime": {
            "functions": [{
                "name": "remote_crm.sync_contact.v1",
                "version": 1,
                "queue": "remote-crm",
                "input_schema": "remote_crm.sync_contact.v1",
                "retry_policy": {
                    "max_attempts": 3,
                    "initial_delay_ms": 1000
                }
            }]
        },
        "capabilities": ["remote_crm.contacts.read"]
    }))
}

async fn manifest_with_invalid_http_route() -> Json<Value> {
    Json(json!({
        "name": "remote-crm",
        "story_display": [],
        "http_routes": [{
            "method": "GET",
            "path": "https://crm.example.test/contacts"
        }],
        "capabilities": []
    }))
}

async fn contacts() -> Json<Value> {
    Json(json!({
        "records": [{ "id": "contact_1", "email": "sam@example.com" }],
        "next_cursor": null
    }))
}

async fn embedded_manifest() -> Json<Value> {
    Json(json!({
        "name": "remote-crm-embedded",
        "story_display": [],
        "admin": {
            "kind": "embedded_custom",
            "runtime": "iframe",
            "entry": {
                "kind": "url",
                "url": "https://remote-crm.example.test/admin",
                "allowed_origins": ["https://remote-crm.example.test"]
            },
            "sandbox": {
                "allow_scripts": true,
                "allow_forms": false,
                "allow_popups": false,
                "allow_same_origin": false
            },
            "permissions": [],
            "fallback_schema": {
                "entities": [{
                    "name": "contacts",
                    "label": "Contacts",
                    "fields": [],
                    "read_capability": "remote_crm.contacts.read"
                }]
            }
        },
        "capabilities": ["remote_crm.contacts.read"]
    }))
}

async fn declarative_manifest() -> Json<Value> {
    Json(json!({
        "name": "remote-crm-declarative",
        "story_display": [],
        "admin": {
            "kind": "declarative_custom",
            "pages": [{
                "name": "overview",
                "label": "Overview",
                "sections": [{
                    "name": "contacts",
                    "label": "Contacts",
                    "component": {
                        "kind": "entity_table",
                        "entity": "contacts"
                    }
                }]
            }],
            "actions": [],
            "fallback_schema": {
                "entities": [{
                    "name": "contacts",
                    "label": "Contacts",
                    "fields": [],
                    "read_capability": "remote_crm.contacts.read"
                }]
            }
        },
        "capabilities": ["remote_crm.contacts.read"]
    }))
}

async fn manifest_error() -> (StatusCode, Json<Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({
            "error": {
                "code": "external_dependency_failure",
                "message": "remote registry database is unavailable",
                "retryable": true,
                "details": [{ "field": "store", "reason": "connection refused" }]
            }
        })),
    )
}

async fn contacts_error() -> (StatusCode, Json<Value>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({
            "error": {
                "code": "external_dependency_failure",
                "message": "crm upstream is unavailable",
                "retryable": true,
                "details": [{ "field": "upstream", "reason": "timeout" }]
            }
        })),
    )
}

async fn contact_missing() -> (StatusCode, Json<Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "error": {
                "code": "not_found",
                "message": "contact contact_404 was not found",
                "retryable": false,
                "details": []
            }
        })),
    )
}

#[tokio::test]
async fn loads_manifest_and_attaches_admin_data_source() {
    let base_url = spawn_server(Router::new().route("/manifest", get(manifest))).await;

    let config = RemoteModuleConfig::new("remote-crm", base_url);
    let module = RemoteModuleSource::new(config)
        .expect("remote source")
        .load()
        .await
        .expect("load remote module");

    assert_eq!(module.manifest.name, "remote-crm");
    assert_eq!(module.manifest.http_routes.len(), 2);
    assert_eq!(module.manifest.http_routes[0].path, "/contacts");
    let runtime = module.manifest.runtime.as_ref().expect("runtime surface");
    assert_eq!(runtime.functions.len(), 1);
    assert_eq!(runtime.functions[0].name, "remote_crm.sync_contact.v1");
    assert_eq!(runtime.functions[0].queue, "remote-crm");
    assert!(matches!(
        module.manifest.admin,
        Some(AdminSurface::Schema(_))
    ));
    assert!(module.admin_data.is_some());
}

#[tokio::test]
async fn rejects_remote_manifest_with_non_local_http_routes() {
    let base_url =
        spawn_server(Router::new().route("/manifest", get(manifest_with_invalid_http_route))).await;

    let config = RemoteModuleConfig::new("remote-crm", base_url);
    let error = RemoteModuleSource::new(config)
        .expect("remote source")
        .load()
        .await
        .expect_err("invalid remote route should fail manifest load");

    assert_eq!(error.code, platform_core::ErrorCode::Validation);
    assert_eq!(
        error.public_message,
        "remote module manifest contains invalid HTTP route declarations"
    );
    assert_eq!(
        error.details[0].field.as_deref(),
        Some("http_routes.0.path")
    );
}

#[tokio::test]
async fn loads_embedded_custom_manifest_without_admin_data_source() {
    let base_url = spawn_server(Router::new().route("/manifest", get(embedded_manifest))).await;

    let config = RemoteModuleConfig::new("remote-crm-embedded", base_url);
    let module = RemoteModuleSource::new(config)
        .expect("remote source")
        .load()
        .await
        .expect("load remote module");

    assert_eq!(module.manifest.name, "remote-crm-embedded");
    assert!(matches!(
        module.manifest.admin,
        Some(AdminSurface::EmbeddedCustom(_))
    ));
    assert!(module.admin_data.is_none());
}

#[tokio::test]
async fn loads_declarative_custom_manifest_with_admin_data_source() {
    let base_url = spawn_server(Router::new().route("/manifest", get(declarative_manifest))).await;

    let config = RemoteModuleConfig::new("remote-crm-declarative", base_url);
    let module = RemoteModuleSource::new(config)
        .expect("remote source")
        .load()
        .await
        .expect("load remote module");

    assert_eq!(module.manifest.name, "remote-crm-declarative");
    assert!(matches!(
        module.manifest.admin,
        Some(AdminSurface::DeclarativeCustom(_))
    ));
    assert!(module.admin_data.is_some());
}

#[tokio::test]
async fn remote_admin_data_source_lists_records() {
    let base_url = spawn_server(Router::new().route("/admin/contacts", get(contacts))).await;

    let source =
        RemoteAdminDataSource::new(RemoteModuleConfig::new("remote-crm", base_url)).unwrap();
    let page = source
        .list("contacts", &AdminListQuery::new(50, None))
        .await
        .expect("list remote records");

    assert_eq!(page.records.len(), 1);
    assert_eq!(page.records[0]["email"], "sam@example.com");
    assert!(page.next_cursor.is_none());
}

#[tokio::test]
async fn manifest_error_envelope_preserves_remote_message_and_retryability() {
    let base_url = spawn_server(Router::new().route("/manifest", get(manifest_error))).await;

    let error = RemoteModuleSource::new(RemoteModuleConfig::new("remote-crm", base_url))
        .expect("remote source")
        .load()
        .await
        .expect_err("manifest load should fail");

    assert_eq!(error.code, platform_core::ErrorCode::ExternalDependency);
    assert_eq!(
        error.public_message,
        "remote registry database is unavailable"
    );
    assert!(error.retryable);
    assert!(
        error.details.iter().any(
            |detail| detail.field.as_deref() == Some("remote_status") && detail.reason == "500"
        )
    );
}

#[tokio::test]
async fn admin_list_error_envelope_preserves_remote_message() {
    let base_url = spawn_server(Router::new().route("/admin/contacts", get(contacts_error))).await;

    let source =
        RemoteAdminDataSource::new(RemoteModuleConfig::new("remote-crm", base_url)).unwrap();
    let error = source
        .list("contacts", &AdminListQuery::new(50, None))
        .await
        .expect_err("list should fail");

    assert_eq!(error.code, platform_core::ErrorCode::ExternalDependency);
    assert_eq!(error.public_message, "crm upstream is unavailable");
    assert!(error.retryable);
}

#[tokio::test]
async fn admin_detail_not_found_envelope_preserves_remote_message() {
    let base_url =
        spawn_server(Router::new().route("/admin/contacts/contact_404", get(contact_missing)))
            .await;

    let source =
        RemoteAdminDataSource::new(RemoteModuleConfig::new("remote-crm", base_url)).unwrap();
    let error = source
        .get("contacts", "contact_404")
        .await
        .expect_err("remote envelope should be preserved");

    assert_eq!(error.code, platform_core::ErrorCode::NotFound);
    assert_eq!(error.public_message, "contact contact_404 was not found");
    assert!(!error.retryable);
}
