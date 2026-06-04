//! Schema-admin data contracts: a module's declared manageable entities.

use serde::{Deserialize, Serialize};

/// A module's declared admin surface: which entities it exposes for management.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AdminSchema {
    pub entities: Vec<EntitySchema>,
}

/// One manageable entity (e.g. identity's "users").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct EntitySchema {
    /// Stable entity key, unique within the module, e.g. "users".
    pub name: String,
    /// Human label for the console, e.g. "Users".
    pub label: String,
    /// Ordered field descriptors driving list columns / detail rows.
    pub fields: Vec<FieldSchema>,
    /// Capability required to read this entity. Declared now; gated only
    /// coarsely (AdminActor) this step. Fine-grained RBAC is a later spec.
    pub read_capability: String,
}

/// One field of an entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct FieldSchema {
    /// Key in the record's JSON object, e.g. "email".
    pub name: String,
    /// Human label, e.g. "Email".
    pub label: String,
    /// Rendering hint for the console's display layer.
    pub field_type: FieldType,
    /// Whether the value may be null/absent.
    #[serde(default)]
    pub nullable: bool,
}

/// Minimal field-type vocabulary. `Json` is the catch-all so any field renders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum FieldType {
    String,
    Integer,
    Boolean,
    Timestamp,
    Json,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> AdminSchema {
        AdminSchema {
            entities: vec![EntitySchema {
                name: "users".to_owned(),
                label: "Users".to_owned(),
                read_capability: "identity.users.read".to_owned(),
                fields: vec![
                    FieldSchema {
                        name: "email".into(),
                        label: "Email".into(),
                        field_type: FieldType::String,
                        nullable: false,
                    },
                    FieldSchema {
                        name: "created_at".into(),
                        label: "Created".into(),
                        field_type: FieldType::Timestamp,
                        nullable: false,
                    },
                ],
            }],
        }
    }

    #[test]
    fn admin_schema_round_trips_through_json() {
        let schema = sample();
        let json = serde_json::to_string(&schema).expect("serialize");
        let back: AdminSchema = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(schema, back);
    }

    #[test]
    fn field_type_serializes_with_kind_tag() {
        let json = serde_json::to_string(&FieldType::Timestamp).expect("serialize");
        assert_eq!(json, r#"{"kind":"timestamp"}"#);
    }
}
