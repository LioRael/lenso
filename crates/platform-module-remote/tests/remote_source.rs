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
        "capabilities": ["remote_crm.contacts.read"]
    }))
}

async fn contacts() -> Json<Value> {
    Json(json!({
        "records": [{ "id": "contact_1", "email": "sam@example.com" }],
        "next_cursor": null
    }))
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
    assert!(matches!(module.manifest.admin, Some(AdminSurface::Schema(_))));
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
