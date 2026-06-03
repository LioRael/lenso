use axum::extract::{Path, Query};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::Html;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, routing::get};
use platform_module::{
    AdminDeclarativeComponent, AdminDeclarativePage, AdminDeclarativeSection,
    AdminDeclarativeSurface, AdminEmbeddedEntry, AdminEmbeddedRuntime, AdminEmbeddedSurface,
    AdminMetricBinding, AdminSandboxPolicy, AdminSchema, EntitySchema, FieldSchema, FieldType,
    ModuleManifest,
};

use serde::Deserialize;
use serde_json::{Value, json};

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

#[must_use]
pub fn router() -> Router {
    Router::new()
        .route("/123", get(manifest))
        .route("/lenso/module/v1/manifest", get(manifest))
        .route(
            "/lenso/module/v1/declarative/manifest",
            get(declarative_manifest),
        )
        .route("/lenso/module/v1/embedded/manifest", get(embedded_manifest))
        .route("/lenso/module/v1/embedded/admin", get(embedded_admin))
        .route("/lenso/module/v1/admin/contacts", get(list_contacts))
        .route("/lenso/module/v1/admin/contacts/{id}", get(get_contact))
}

async fn manifest() -> Json<ModuleManifest> {
    Json(
        ModuleManifest::builder("remote-crm")
            .admin(contacts_schema())
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
                    ],
                }],
                actions: vec![],
                fallback_schema: Some(contacts_schema()),
            })
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
