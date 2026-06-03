use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, routing::get};
use platform_module::{
    AdminEmbeddedEntry, AdminEmbeddedRuntime, AdminEmbeddedSurface, AdminSandboxPolicy,
    AdminSchema, EntitySchema, FieldSchema, FieldType, ModuleManifest,
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
        .route("/lenso/module/v1/embedded-manifest", get(embedded_manifest))
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

async fn embedded_manifest() -> Json<ModuleManifest> {
    Json(
        ModuleManifest::builder("remote-crm-embedded")
            .embedded_admin(AdminEmbeddedSurface {
                runtime: AdminEmbeddedRuntime::Iframe,
                entry: AdminEmbeddedEntry::Url {
                    url: "https://remote-crm.example.test/admin".to_owned(),
                    allowed_origins: vec!["https://remote-crm.example.test".to_owned()],
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
