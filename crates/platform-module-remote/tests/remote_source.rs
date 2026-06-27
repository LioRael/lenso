use axum::http::StatusCode;
use axum::{Json, Router, routing::get, routing::post};
use platform_module::{
    AdminActionDangerLevel, AdminActionSource, AdminDataSource, AdminListQuery, AdminQuerySource,
    AdminSurface, FieldType, LifecycleActivationRunPolicy, LifecycleStartupCheckKind,
};
use platform_module_remote::{
    RemoteAdminActionSource, RemoteAdminDataSource, RemoteModuleConfig, RemoteModuleSource,
};
#[allow(dead_code)]
#[path = "../src/protocol.rs"]
mod protocol;
use protocol::RemoteManifestEnvelope;
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
        "events": {
            "handlers": [{
                "name": "sync_contact_on_user_registered",
                "event_name": "identity.user_registered.v1"
            }]
        },
        "lifecycle": {
            "startup_checks": [{
                "name": "sync contact function is registered",
                "required": true,
                "kind": "function_registered",
                "function_name": "remote_crm.sync_contact.v1"
            }],
            "activation_jobs": [{
                "name": "sync contacts on startup",
                "function_name": "remote_crm.sync_contact.v1",
                "run_policy": "every_startup",
                "input": { "reason": "worker_startup" },
                "required": true
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

async fn service_manifest() -> Json<Value> {
    Json(json!({
        "name": "support-service",
        "protocol": "lenso.service.v1",
        "modules": [{
            "name": "support-ticket",
            "story_display": [],
            "admin": {
                "kind": "schema",
                "entities": [{
                    "name": "tickets",
                    "label": "Tickets",
                    "fields": [],
                    "read_capability": "support_ticket.tickets.read"
                }]
            },
            "http_routes": [{
                "method": "GET",
                "path": "/tickets/{id}",
                "capability": "support_ticket.tickets.read"
            }],
            "capabilities": ["support_ticket.tickets.read"]
        }]
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
                "sections": [
                    {
                        "name": "health",
                        "label": "Health",
                        "component": {
                            "kind": "query_value",
                            "query": "health",
                            "capability": "remote_crm.health.read",
                            "value_path": "contacts"
                        }
                    },
                    {
                        "name": "contacts",
                        "label": "Contacts",
                        "component": {
                            "kind": "entity_table",
                            "entity": "contacts"
                        }
                    }
                ]
            }],
            "actions": [{
                "name": "sync_contacts",
                "label": "Sync contacts",
                "capability": "remote_crm.contacts.sync",
                "input_schema": {
                    "fields": [{
                        "name": "dry_run",
                        "label": "Dry run",
                        "field_type": { "kind": "boolean" },
                        "required": false,
                        "description": "Preview the sync without writing remote data"
                    }]
                },
                "confirmation": {
                    "message": "Sync remote contacts now?",
                    "required_phrase": "SYNC"
                },
                "danger_level": "medium"
            }],
            "fallback_schema": {
                "entities": [{
                    "name": "contacts",
                    "label": "Contacts",
                    "fields": [],
                    "read_capability": "remote_crm.contacts.read"
                }]
            }
        },
        "capabilities": ["remote_crm.contacts.read", "remote_crm.contacts.sync", "remote_crm.health.read"]
    }))
}

async fn health() -> Json<Value> {
    Json(json!({
        "data": {
            "contacts": 1,
            "healthy": true
        }
    }))
}

async fn sync_contacts(Json(input): Json<Value>) -> Json<Value> {
    Json(json!({
        "result": {
            "synced": true,
            "dry_run": input.get("dry_run").and_then(Value::as_bool).unwrap_or(false)
        }
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
    let lifecycle = module
        .manifest
        .lifecycle
        .as_ref()
        .expect("lifecycle surface");
    assert_eq!(lifecycle.startup_checks.len(), 1);
    let startup_check = &lifecycle.startup_checks[0];
    assert_eq!(startup_check.name, "sync contact function is registered");
    assert!(startup_check.required);
    assert!(matches!(
        &startup_check.check,
        LifecycleStartupCheckKind::FunctionRegistered { function_name }
            if function_name == "remote_crm.sync_contact.v1"
    ));
    assert_eq!(lifecycle.activation_jobs.len(), 1);
    let activation_job = &lifecycle.activation_jobs[0];
    assert_eq!(activation_job.name, "sync contacts on startup");
    assert_eq!(activation_job.function_name, "remote_crm.sync_contact.v1");
    assert_eq!(
        activation_job.run_policy,
        LifecycleActivationRunPolicy::EveryStartup
    );
    assert_eq!(activation_job.input, json!({ "reason": "worker_startup" }));
    assert!(activation_job.required);
    let mut registry = platform_runtime::FunctionRegistry::default();
    module.binding.register_functions(&mut registry);
    assert!(registry.get("remote_crm.sync_contact.v1").is_some());
    let mut event_registry = platform_core::EventHandlerRegistry::default();
    module.binding.register_event_handlers(
        &mut event_registry,
        &platform_module::EventHandlerRegistrationContext::empty(),
    );
    assert_eq!(
        event_registry.handler_count("identity.user_registered.v1"),
        1
    );
    assert!(matches!(
        module.manifest.admin,
        Some(AdminSurface::Schema(_))
    ));
    assert!(module.admin_data.is_some());
}

#[tokio::test]
async fn loads_service_manifest_as_provider_with_modules() {
    let base_url = spawn_server(Router::new().route("/manifest", get(service_manifest))).await;

    let config = RemoteModuleConfig::new("support-service", base_url.clone());
    let loaded = RemoteModuleSource::new(config)
        .expect("remote source")
        .load_all()
        .await
        .expect("service manifest should load");

    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].module.manifest.name, "support-ticket");
    assert_eq!(
        loaded[0].config.base_url,
        format!("{base_url}/modules/support-ticket")
    );
    assert!(loaded[0].module.admin_data.is_some());
}

#[test]
fn service_manifest_accepts_v6_provider_fields() {
    let value = serde_json::json!({
        "name": "support-suite-provider",
        "version": "0.2.0",
        "provider": {
            "name": "support-suite-provider",
            "vendor": "Lenso",
            "summary": "Support workflow provider"
        },
        "compatibility": {
            "remoteProtocolVersion": "1",
            "requiredHostFeatures": ["service.status"]
        },
        "health": {
            "readyUrl": "http://127.0.0.1:4110/lenso/service/v1/ready",
            "statusUrl": "http://127.0.0.1:4110/lenso/service/v1/status"
        },
        "localProcess": {
            "command": "pnpm --dir examples/support-ticket start",
            "autoStart": true,
            "readyTimeoutMs": 30000
        },
        "modules": [
            {
                "name": "support-ticket",
                "version": "0.1.0",
                "capabilities": ["support_ticket.tickets.read"]
            }
        ]
    });

    let envelope: RemoteManifestEnvelope = serde_json::from_value(value).unwrap();
    let RemoteManifestEnvelope::Service(service) = envelope else {
        panic!("expected service envelope");
    };

    assert_eq!(service.name, "support-suite-provider");
    assert_eq!(service.version.as_deref(), Some("0.2.0"));
    assert_eq!(
        service.provider.as_ref().unwrap().vendor.as_deref(),
        Some("Lenso")
    );
    assert_eq!(
        service.health.as_ref().unwrap().ready_url.as_deref(),
        Some("http://127.0.0.1:4110/lenso/service/v1/ready")
    );
    assert_eq!(service.modules[0].name, "support-ticket");
}

#[test]
fn service_manifest_accepts_v5_shape() {
    let value = serde_json::json!({
        "name": "support-service",
        "modules": [{ "name": "support-ticket", "version": "0.1.0" }]
    });

    let envelope: RemoteManifestEnvelope = serde_json::from_value(value).unwrap();
    let RemoteManifestEnvelope::Service(service) = envelope else {
        panic!("expected service envelope");
    };

    assert_eq!(service.name, "support-service");
    assert!(service.provider.is_none());
    assert_eq!(service.modules.len(), 1);
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
    let Some(AdminSurface::DeclarativeCustom(surface)) = &module.manifest.admin else {
        panic!("expected declarative custom admin surface");
    };
    let action = surface.actions.first().expect("action declared");
    assert_eq!(action.name, "sync_contacts");
    assert_eq!(action.danger_level, AdminActionDangerLevel::Medium);
    let input_field = action
        .input_schema
        .as_ref()
        .and_then(|schema| schema.fields.first())
        .expect("action input field declared");
    assert_eq!(input_field.name, "dry_run");
    assert_eq!(input_field.field_type, FieldType::Boolean);
    assert_eq!(
        action
            .confirmation
            .as_ref()
            .and_then(|confirmation| confirmation.required_phrase.as_deref()),
        Some("SYNC")
    );
    assert!(module.admin_data.is_some());
    assert!(module.admin_actions.is_some());
    assert!(module.admin_queries.is_some());
}

#[tokio::test]
async fn remote_admin_action_source_invokes_declared_action() {
    let base_url =
        spawn_server(Router::new().route("/admin/actions/sync_contacts", post(sync_contacts)))
            .await;

    let source =
        RemoteAdminActionSource::new(RemoteModuleConfig::new("remote-crm", base_url)).unwrap();
    let output = source
        .invoke("sync_contacts", json!({ "dry_run": true }))
        .await
        .expect("invoke remote action");

    assert_eq!(output["synced"], true);
    assert_eq!(output["dry_run"], true);
}

#[tokio::test]
async fn remote_admin_query_source_reads_declared_query() {
    let base_url = spawn_server(Router::new().route("/admin/queries/health", get(health))).await;

    let source =
        RemoteAdminDataSource::new(RemoteModuleConfig::new("remote-crm", base_url)).unwrap();
    let output = source.query("health").await.expect("query remote value");

    assert_eq!(output["contacts"], 1);
    assert_eq!(output["healthy"], true);
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
