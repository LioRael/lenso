//! Contracts for a module's admin surface.

use crate::admin_schema::AdminSchema;
use serde::{Deserialize, Serialize};

/// A module's admin surface.
///
/// `Schema` is implemented today. Custom surface variants are data contracts
/// only until the Runtime Console implements their renderers/policies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum AdminSurface {
    /// Schema-driven CRUD: console renders a generic UI from this declaration.
    Schema(AdminSchema),
    /// Host-rendered custom UI built from trusted Runtime Console components.
    DeclarativeCustom(AdminDeclarativeSurface),
    /// Module-owned UI embedded behind a sandbox boundary.
    EmbeddedCustom(AdminEmbeddedSurface),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminDeclarativeSurface {
    #[serde(default)]
    pub pages: Vec<AdminDeclarativePage>,
    #[serde(default)]
    pub actions: Vec<AdminAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_schema: Option<AdminSchema>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminDeclarativePage {
    pub name: String,
    pub label: String,
    #[serde(default)]
    pub sections: Vec<AdminDeclarativeSection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminDeclarativeSection {
    pub name: String,
    pub label: String,
    pub component: AdminDeclarativeComponent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum AdminDeclarativeComponent {
    MetricStrip {
        #[serde(default)]
        metrics: Vec<AdminMetricBinding>,
    },
    EntityTable {
        entity: String,
    },
    EntityDetail {
        entity: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminMetricBinding {
    pub label: String,
    pub value_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminAction {
    pub name: String,
    pub label: String,
    pub capability: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminEmbeddedSurface {
    pub runtime: AdminEmbeddedRuntime,
    pub entry: AdminEmbeddedEntry,
    pub sandbox: AdminSandboxPolicy,
    #[serde(default)]
    pub permissions: Vec<AdminPermission>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_schema: Option<AdminSchema>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AdminEmbeddedRuntime {
    Iframe,
    Wasm,
    JsBundle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum AdminEmbeddedEntry {
    Url {
        url: String,
        #[serde(default)]
        allowed_origins: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminSandboxPolicy {
    #[serde(default)]
    pub allow_scripts: bool,
    #[serde(default)]
    pub allow_forms: bool,
    #[serde(default)]
    pub allow_popups: bool,
    #[serde(default)]
    pub allow_same_origin: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum AdminPermission {
    ReadEntity { entity: String },
    InvokeAction { action: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin_schema::{AdminSchema, EntitySchema};

    fn fallback_schema() -> AdminSchema {
        AdminSchema {
            entities: vec![EntitySchema {
                name: "contacts".to_owned(),
                label: "Contacts".to_owned(),
                fields: vec![],
                read_capability: "remote_crm.contacts.read".to_owned(),
            }],
        }
    }

    #[test]
    fn declarative_custom_surface_round_trips_through_json() {
        let surface = AdminSurface::DeclarativeCustom(AdminDeclarativeSurface {
            pages: vec![AdminDeclarativePage {
                name: "dashboard".to_owned(),
                label: "Dashboard".to_owned(),
                sections: vec![AdminDeclarativeSection {
                    name: "contacts".to_owned(),
                    label: "Contacts".to_owned(),
                    component: AdminDeclarativeComponent::EntityTable {
                        entity: "contacts".to_owned(),
                    },
                }],
            }],
            actions: vec![AdminAction {
                name: "sync_contacts".to_owned(),
                label: "Sync contacts".to_owned(),
                capability: "remote_crm.contacts.sync".to_owned(),
            }],
            fallback_schema: Some(fallback_schema()),
        });

        let json = serde_json::to_string(&surface).expect("serialize");
        assert!(
            json.contains(r#""kind":"declarative_custom""#),
            "got {json}"
        );
        let back: AdminSurface = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(surface, back);
    }

    #[test]
    fn embedded_custom_surface_round_trips_through_json() {
        let surface = AdminSurface::EmbeddedCustom(AdminEmbeddedSurface {
            runtime: AdminEmbeddedRuntime::Iframe,
            entry: AdminEmbeddedEntry::Url {
                url: "https://crm.example.test/admin".to_owned(),
                allowed_origins: vec!["https://crm.example.test".to_owned()],
            },
            sandbox: AdminSandboxPolicy {
                allow_scripts: true,
                allow_forms: false,
                allow_popups: false,
                allow_same_origin: false,
            },
            permissions: vec![AdminPermission::ReadEntity {
                entity: "contacts".to_owned(),
            }],
            fallback_schema: Some(fallback_schema()),
        });

        let json = serde_json::to_string(&surface).expect("serialize");
        assert!(json.contains(r#""kind":"embedded_custom""#), "got {json}");
        assert!(json.contains(r#""runtime":"iframe""#), "got {json}");
        let back: AdminSurface = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(surface, back);
    }
}
