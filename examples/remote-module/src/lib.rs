use axum::extract::{Path, Query};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::Html;
use axum::response::{IntoResponse, Response};
use axum::{
    Json, Router,
    routing::{delete, get, patch, post, put},
};
use platform_module::{
    AdminDeclarativeComponent, AdminDeclarativePage, AdminDeclarativeSection,
    AdminDeclarativeSurface, AdminEmbeddedEntry, AdminEmbeddedRuntime, AdminEmbeddedSurface,
    AdminMetricBinding, AdminSandboxPolicy, AdminSchema, EntitySchema, FieldSchema, FieldType,
    LifecycleActivationJobDeclaration, LifecycleActivationRunPolicy,
    LifecycleStartupCheckDeclaration, LifecycleStartupCheckKind, LifecycleSurface,
    ModuleHttpMethod, ModuleHttpRoute, ModuleManifest, RuntimeFunctionDeclaration,
    RuntimeRetryPolicyDeclaration, RuntimeSurface,
};

use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Duration;

#[derive(Debug, Clone)]
struct Contact {
    id: &'static str,
    email: &'static str,
    name: &'static str,
    company: &'static str,
    active: bool,
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    #[serde(default = "default_limit")]
    limit: usize,
    cursor: Option<String>,
}

#[derive(Debug, serde::Serialize)]
struct ListResponse {
    records: Vec<Value>,
    next_cursor: Option<String>,
}

#[derive(Debug, serde::Serialize)]
struct DetailResponse {
    record: Value,
}

#[derive(Debug, Deserialize)]
struct RuntimeFunctionInvokeRequest {
    request_id: String,
    function_run_id: String,
    function_name: String,
    attempt: u32,
    correlation_id: String,
    causation_id: Option<String>,
    actor: Value,
    trace: Value,
    input: Value,
}

#[derive(Debug, serde::Serialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Debug, serde::Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
    retryable: bool,
    details: Vec<ErrorDetail>,
}

#[derive(Debug, serde::Serialize)]
struct ErrorDetail {
    field: Option<&'static str>,
    reason: String,
}

const CONTACTS: &[Contact] = &[
    Contact {
        id: "contact_1",
        email: "ada@example.com",
        name: "Ada Lovelace",
        company: "Analytical Engines Ltd",
        active: true,
    },
    Contact {
        id: "contact_2",
        email: "grace@example.com",
        name: "Grace Hopper",
        company: "Compiler Systems",
        active: true,
    },
    Contact {
        id: "contact_3",
        email: "katherine@example.com",
        name: "Katherine Johnson",
        company: "Orbital Mechanics Co",
        active: false,
    },
];

const OVERSIZED_PROXY_RESPONSE_BYTES: usize = (4 * 1024 * 1024) + 1;

#[must_use]
pub fn router() -> Router {
    Router::new()
        .route("/123", get(manifest))
        .route("/lenso/module/v1/manifest", get(manifest))
        .route(
            "/lenso/module/v1/declarative/manifest",
            get(declarative_manifest),
        )
        .route(
            "/lenso/module/v1/declarative/admin/contacts",
            get(list_contacts),
        )
        .route(
            "/lenso/module/v1/declarative/admin/contacts/{id}",
            get(get_contact),
        )
        .route("/lenso/module/v1/embedded/manifest", get(embedded_manifest))
        .route("/lenso/module/v1/embedded/admin", get(embedded_admin))
        .route("/lenso/module/v1/contacts", get(get_http_contacts))
        .route("/lenso/module/v1/contacts", post(post_http_contact))
        .route("/lenso/module/v1/contacts/{id}", get(get_http_contact))
        .route("/lenso/module/v1/contacts/{id}", put(put_http_contact))
        .route("/lenso/module/v1/contacts/{id}", patch(patch_http_contact))
        .route(
            "/lenso/module/v1/contacts/{id}",
            delete(delete_http_contact),
        )
        .route(
            "/lenso/module/v1/contacts/{id}/purge",
            delete(delete_http_contact_empty),
        )
        .route(
            "/lenso/module/v1/proxy-fixtures/text",
            get(get_proxy_fixture_text),
        )
        .route(
            "/lenso/module/v1/proxy-fixtures/oversized",
            get(get_proxy_fixture_oversized),
        )
        .route(
            "/lenso/module/v1/proxy-fixtures/slow",
            get(get_proxy_fixture_slow),
        )
        .route(
            "/lenso/module/v1/runtime/functions/{function_name}/invoke",
            post(invoke_runtime_function),
        )
        .route("/lenso/module/v1/admin/contacts", get(list_contacts))
        .route("/lenso/module/v1/admin/contacts/{id}", get(get_contact))
}

async fn manifest() -> Json<ModuleManifest> {
    Json(
        ModuleManifest::builder("remote-crm")
            .admin(contacts_schema())
            .http_routes(contact_http_routes())
            .runtime(runtime_surface())
            .lifecycle(lifecycle_surface())
            .capabilities(vec!["remote_crm.contacts.read".to_owned()])
            .build(),
    )
}

async fn declarative_manifest() -> Json<ModuleManifest> {
    Json(
        ModuleManifest::builder("remote-crm-declarative")
            .declarative_admin(AdminDeclarativeSurface {
                pages: vec![AdminDeclarativePage {
                    name: "overview".to_owned(),
                    label: "Overview".to_owned(),
                    sections: vec![
                        AdminDeclarativeSection {
                            name: "contact_health".to_owned(),
                            label: "Contact Health".to_owned(),
                            component: AdminDeclarativeComponent::MetricStrip {
                                metrics: vec![
                                    AdminMetricBinding {
                                        label: "Fields".to_owned(),
                                        value_path:
                                            "fallback_schema.entities.contacts.fields.count"
                                                .to_owned(),
                                    },
                                    AdminMetricBinding {
                                        label: "Capability".to_owned(),
                                        value_path:
                                            "fallback_schema.entities.contacts.read_capability"
                                                .to_owned(),
                                    },
                                ],
                            },
                        },
                        AdminDeclarativeSection {
                            name: "contact_fields".to_owned(),
                            label: "Contact Fields".to_owned(),
                            component: AdminDeclarativeComponent::EntityTable {
                                entity: "contacts".to_owned(),
                            },
                        },
                        AdminDeclarativeSection {
                            name: "contact_detail".to_owned(),
                            label: "Contact Detail".to_owned(),
                            component: AdminDeclarativeComponent::EntityDetail {
                                entity: "contacts".to_owned(),
                            },
                        },
                    ],
                }],
                actions: vec![],
                fallback_schema: Some(contacts_schema()),
            })
            .http_routes(contact_http_routes())
            .capabilities(vec!["remote_crm.contacts.read".to_owned()])
            .build(),
    )
}

async fn embedded_manifest(headers: HeaderMap) -> Json<ModuleManifest> {
    let origin = request_origin(&headers);
    Json(
        ModuleManifest::builder("remote-crm-embedded")
            .embedded_admin(AdminEmbeddedSurface {
                runtime: AdminEmbeddedRuntime::Iframe,
                entry: AdminEmbeddedEntry::Url {
                    url: format!("{origin}/lenso/module/v1/embedded/admin"),
                    allowed_origins: vec![origin],
                },
                sandbox: AdminSandboxPolicy {
                    allow_scripts: true,
                    allow_forms: false,
                    allow_popups: false,
                    allow_same_origin: false,
                },
                permissions: vec![],
                fallback_schema: Some(contacts_schema()),
            })
            .http_routes(contact_http_routes())
            .capabilities(vec!["remote_crm.contacts.read".to_owned()])
            .build(),
    )
}

async fn embedded_admin() -> Html<&'static str> {
    Html(
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Remote CRM Admin</title>
    <style>
      :root {
        color-scheme: dark;
        font-family: ui-monospace, "SFMono-Regular", Menlo, Monaco, Consolas, monospace;
        background: #090b0a;
        color: #f2f4ef;
      }
      body {
        margin: 0;
        min-height: 100vh;
        background: linear-gradient(180deg, #111511 0%, #090b0a 100%);
      }
      main {
        display: grid;
        gap: 14px;
        padding: 18px;
      }
      h1 {
        margin: 0;
        font-size: 14px;
        letter-spacing: 0;
      }
      p {
        margin: 0;
        color: #a3ada6;
        font-size: 12px;
        line-height: 1.5;
      }
      table {
        width: 100%;
        border-collapse: collapse;
        font-size: 12px;
      }
      th,
      td {
        border-bottom: 1px solid #232a25;
        padding: 8px;
        text-align: left;
      }
      th {
        color: #7f8a82;
        font-weight: 600;
      }
      .status {
        display: inline-flex;
        border: 1px solid #2f3a33;
        padding: 3px 6px;
        color: #d7ff42;
        font-size: 11px;
      }
    </style>
  </head>
  <body>
    <main>
      <div>
        <h1>Remote CRM Embedded Admin</h1>
        <p>This page is served by the remote module and rendered in a sandboxed iframe with no host bridge.</p>
      </div>
      <span class="status">iframe / no bridge</span>
      <table aria-label="Remote contacts summary">
        <thead>
          <tr><th>Contact</th><th>Company</th><th>Status</th></tr>
        </thead>
        <tbody>
          <tr><td>Ada Lovelace</td><td>Analytical Engines Ltd</td><td>active</td></tr>
          <tr><td>Grace Hopper</td><td>Compiler Systems</td><td>active</td></tr>
          <tr><td>Katherine Johnson</td><td>Orbital Mechanics Co</td><td>paused</td></tr>
        </tbody>
      </table>
    </main>
  </body>
</html>"#,
    )
}

async fn get_http_contacts(Query(query): Query<ListQuery>) -> Json<ListResponse> {
    list_contacts(Query(query)).await
}

async fn get_http_contact(headers: HeaderMap, Path(id): Path<String>) -> Response {
    if headers.contains_key("authorization") {
        return remote_error(
            StatusCode::BAD_REQUEST,
            "validation_failed",
            "caller authorization must not be forwarded".to_owned(),
            false,
        );
    }
    match CONTACTS.iter().find(|contact| contact.id == id) {
        Some(contact) => Json(contact_to_value(contact)).into_response(),
        None => remote_error(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("contact {id} was not found"),
            false,
        ),
    }
}

async fn post_http_contact(headers: HeaderMap, Json(input): Json<Value>) -> Response {
    write_http_contact(headers, "created", None, input).await
}

async fn put_http_contact(
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<Value>,
) -> Response {
    write_http_contact(headers, "replaced", Some(id), input).await
}

async fn patch_http_contact(
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<Value>,
) -> Response {
    write_http_contact(headers, "patched", Some(id), input).await
}

async fn delete_http_contact(headers: HeaderMap, Path(id): Path<String>) -> Response {
    if headers.contains_key("authorization") {
        return remote_error(
            StatusCode::BAD_REQUEST,
            "validation_failed",
            "caller authorization must not be forwarded".to_owned(),
            false,
        );
    }
    Json(json!({
        "id": id,
        "deleted": true,
    }))
    .into_response()
}

async fn delete_http_contact_empty(headers: HeaderMap, Path(_id): Path<String>) -> Response {
    if headers.contains_key("authorization") {
        return remote_error(
            StatusCode::BAD_REQUEST,
            "validation_failed",
            "caller authorization must not be forwarded".to_owned(),
            false,
        );
    }
    StatusCode::NO_CONTENT.into_response()
}

async fn write_http_contact(
    headers: HeaderMap,
    operation: &'static str,
    id: Option<String>,
    input: Value,
) -> Response {
    if headers.contains_key("authorization") {
        return remote_error(
            StatusCode::BAD_REQUEST,
            "validation_failed",
            "caller authorization must not be forwarded".to_owned(),
            false,
        );
    }
    Json(json!({
        "id": id.unwrap_or_else(|| input
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("contact_created")
            .to_owned()),
        "email": input.get("email").and_then(Value::as_str).unwrap_or(""),
        "operation": operation,
        "input": input,
    }))
    .into_response()
}

async fn get_proxy_fixture_text() -> &'static str {
    "not json"
}

async fn get_proxy_fixture_oversized() -> Json<Value> {
    Json(json!({
        "payload": "x".repeat(OVERSIZED_PROXY_RESPONSE_BYTES),
    }))
}

async fn get_proxy_fixture_slow() -> Json<Value> {
    tokio::time::sleep(Duration::from_millis(200)).await;
    Json(json!({ "status": "eventually_ready" }))
}

async fn invoke_runtime_function(
    Path(function_name): Path<String>,
    Json(request): Json<RuntimeFunctionInvokeRequest>,
) -> Response {
    if function_name != "remote_crm.sync_contact.v1"
        || request.function_name != "remote_crm.sync_contact.v1"
    {
        return remote_error(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("runtime function {function_name} was not found"),
            false,
        );
    }

    Json(json!({
        "output": {
            "synced": true,
            "contact_id": request
                .input
                .get("contact_id")
                .and_then(Value::as_str)
                .unwrap_or(""),
            "request_id": request.request_id,
            "function_run_id": request.function_run_id,
            "attempt": request.attempt,
            "correlation_id": request.correlation_id,
            "causation_id": request.causation_id,
            "actor_kind": request
                .actor
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or(""),
            "trace_id": request
                .trace
                .get("trace_id")
                .and_then(Value::as_str)
                .unwrap_or(""),
        }
    }))
    .into_response()
}

async fn list_contacts(Query(query): Query<ListQuery>) -> Json<ListResponse> {
    let start = query
        .cursor
        .as_deref()
        .and_then(|cursor| CONTACTS.iter().position(|contact| contact.id == cursor))
        .map_or(0, |index| index + 1);
    let limit = query.limit.clamp(1, 100);
    let records = CONTACTS
        .iter()
        .skip(start)
        .take(limit)
        .map(contact_to_value)
        .collect::<Vec<_>>();
    let next_cursor = (start + records.len() < CONTACTS.len())
        .then(|| records.last())
        .flatten()
        .and_then(|record| record.get("id"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);

    Json(ListResponse {
        records,
        next_cursor,
    })
}

async fn get_contact(Path(id): Path<String>) -> Result<Json<DetailResponse>, Response> {
    CONTACTS
        .iter()
        .find(|contact| contact.id == id)
        .map(|contact| {
            Json(DetailResponse {
                record: contact_to_value(contact),
            })
        })
        .ok_or_else(|| {
            remote_error(
                StatusCode::NOT_FOUND,
                "not_found",
                format!("contact {id} was not found"),
                false,
            )
        })
}

fn contact_to_value(contact: &Contact) -> Value {
    json!({
        "id": contact.id,
        "email": contact.email,
        "name": contact.name,
        "company": contact.company,
        "active": contact.active,
    })
}

fn default_limit() -> usize {
    50
}

fn request_origin(headers: &HeaderMap) -> String {
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .filter(|value| *value == "http" || *value == "https")
        .unwrap_or("http");
    let host = headers
        .get("host")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .unwrap_or("127.0.0.1:4100");
    format!("{scheme}://{host}")
}

fn contacts_schema() -> AdminSchema {
    AdminSchema {
        entities: vec![EntitySchema {
            name: "contacts".to_owned(),
            label: "Contacts".to_owned(),
            read_capability: "remote_crm.contacts.read".to_owned(),
            fields: vec![
                FieldSchema {
                    name: "id".to_owned(),
                    label: "ID".to_owned(),
                    field_type: FieldType::String,
                    nullable: false,
                },
                FieldSchema {
                    name: "email".to_owned(),
                    label: "Email".to_owned(),
                    field_type: FieldType::String,
                    nullable: false,
                },
                FieldSchema {
                    name: "name".to_owned(),
                    label: "Name".to_owned(),
                    field_type: FieldType::String,
                    nullable: false,
                },
                FieldSchema {
                    name: "company".to_owned(),
                    label: "Company".to_owned(),
                    field_type: FieldType::String,
                    nullable: false,
                },
                FieldSchema {
                    name: "active".to_owned(),
                    label: "Active".to_owned(),
                    field_type: FieldType::Boolean,
                    nullable: false,
                },
            ],
        }],
    }
}

fn contact_http_routes() -> Vec<ModuleHttpRoute> {
    vec![
        ModuleHttpRoute {
            method: ModuleHttpMethod::Get,
            path: "/contacts".to_owned(),
            capability: Some("remote_crm.contacts.read".to_owned()),
            display_name: Some("List Contacts".to_owned()),
            story_title: Some("List Contacts".to_owned()),
        },
        ModuleHttpRoute {
            method: ModuleHttpMethod::Post,
            path: "/contacts".to_owned(),
            capability: Some("remote_crm.contacts.read".to_owned()),
            display_name: Some("Create Contact".to_owned()),
            story_title: Some("Create Contact".to_owned()),
        },
        ModuleHttpRoute {
            method: ModuleHttpMethod::Get,
            path: "/contacts/{id}".to_owned(),
            capability: Some("remote_crm.contacts.read".to_owned()),
            display_name: Some("Fetch Contact".to_owned()),
            story_title: Some("Fetch Contact".to_owned()),
        },
        ModuleHttpRoute {
            method: ModuleHttpMethod::Put,
            path: "/contacts/{id}".to_owned(),
            capability: Some("remote_crm.contacts.read".to_owned()),
            display_name: Some("Replace Contact".to_owned()),
            story_title: Some("Replace Contact".to_owned()),
        },
        ModuleHttpRoute {
            method: ModuleHttpMethod::Patch,
            path: "/contacts/{id}".to_owned(),
            capability: Some("remote_crm.contacts.read".to_owned()),
            display_name: Some("Update Contact".to_owned()),
            story_title: Some("Update Contact".to_owned()),
        },
        ModuleHttpRoute {
            method: ModuleHttpMethod::Delete,
            path: "/contacts/{id}".to_owned(),
            capability: Some("remote_crm.contacts.read".to_owned()),
            display_name: Some("Delete Contact".to_owned()),
            story_title: Some("Delete Contact".to_owned()),
        },
        ModuleHttpRoute {
            method: ModuleHttpMethod::Delete,
            path: "/contacts/{id}/purge".to_owned(),
            capability: Some("remote_crm.contacts.read".to_owned()),
            display_name: Some("Purge Contact".to_owned()),
            story_title: Some("Purge Contact".to_owned()),
        },
        ModuleHttpRoute {
            method: ModuleHttpMethod::Get,
            path: "/proxy-fixtures/text".to_owned(),
            capability: Some("remote_crm.contacts.read".to_owned()),
            display_name: Some("Fetch Text Fixture".to_owned()),
            story_title: Some("Fetch Text Fixture".to_owned()),
        },
        ModuleHttpRoute {
            method: ModuleHttpMethod::Get,
            path: "/proxy-fixtures/oversized".to_owned(),
            capability: Some("remote_crm.contacts.read".to_owned()),
            display_name: Some("Fetch Oversized Fixture".to_owned()),
            story_title: Some("Fetch Oversized Fixture".to_owned()),
        },
        ModuleHttpRoute {
            method: ModuleHttpMethod::Get,
            path: "/proxy-fixtures/slow".to_owned(),
            capability: Some("remote_crm.contacts.read".to_owned()),
            display_name: Some("Fetch Slow Fixture".to_owned()),
            story_title: Some("Fetch Slow Fixture".to_owned()),
        },
    ]
}

fn runtime_surface() -> RuntimeSurface {
    RuntimeSurface {
        functions: vec![RuntimeFunctionDeclaration {
            name: "remote_crm.sync_contact.v1".to_owned(),
            version: 1,
            queue: "remote-crm".to_owned(),
            input_schema: Some("remote_crm.sync_contact.v1".to_owned()),
            retry_policy: Some(RuntimeRetryPolicyDeclaration {
                max_attempts: 3,
                initial_delay_ms: 1000,
            }),
        }],
    }
}

fn lifecycle_surface() -> LifecycleSurface {
    LifecycleSurface {
        startup_checks: vec![LifecycleStartupCheckDeclaration {
            name: "sync contact function is registered".to_owned(),
            required: true,
            check: LifecycleStartupCheckKind::FunctionRegistered {
                function_name: "remote_crm.sync_contact.v1".to_owned(),
            },
        }],
        activation_jobs: vec![LifecycleActivationJobDeclaration {
            name: "sync contacts on startup".to_owned(),
            function_name: "remote_crm.sync_contact.v1".to_owned(),
            run_policy: LifecycleActivationRunPolicy::EveryStartup,
            input: json!({ "reason": "worker_startup" }),
            required: true,
        }],
    }
}

fn remote_error(
    status: StatusCode,
    code: &'static str,
    message: String,
    retryable: bool,
) -> Response {
    (
        status,
        Json(ErrorEnvelope {
            error: ErrorBody {
                code,
                message,
                retryable,
                details: vec![ErrorDetail {
                    field: Some("remote_status"),
                    reason: status.as_u16().to_string(),
                }],
            },
        }),
    )
        .into_response()
}
