//! A module's pure-data contract: serializable metadata describable without
//! behavior. Owned + serde so every loading source produces the same shape.

use crate::admin::AdminSurface;
use crate::admin_schema::AdminSchema;
use platform_core::StoryDisplayDescriptor;
use serde::{Deserialize, Serialize};

/// The serializable metadata a module exposes. Runtime config is deliberately
/// NOT here — it stays an internal `&'static` field on [`crate::Module`]
/// because the config registry needs the real (non-serde) `RuntimeConfigType`
/// to validate. Only round-trippable fields belong here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ModuleManifest {
    /// Stable module name, e.g. `"identity"`.
    pub name: String,

    /// Console story-display metadata.
    #[serde(default)]
    pub story_display: Vec<StoryDisplayDescriptor>,

    /// Admin surface: `Some(AdminSurface::Schema(_))` for schema-driven CRUD
    /// modules; `None` for modules with no managed entities (e.g. notifications).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admin: Option<AdminSurface>,

    /// RESERVED SEAM — capabilities the module declares (perms/tenancy).
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl ModuleManifest {
    /// Start building a manifest for `name`.
    #[must_use]
    pub fn builder(name: impl Into<String>) -> ModuleManifestBuilder {
        ModuleManifestBuilder {
            manifest: ModuleManifest {
                name: name.into(),
                story_display: Vec::new(),
                admin: None,
                capabilities: Vec::new(),
            },
        }
    }
}

/// Fluent builder for [`ModuleManifest`]. Reusable by every loading source.
#[derive(Debug)]
pub struct ModuleManifestBuilder {
    manifest: ModuleManifest,
}

impl ModuleManifestBuilder {
    /// Attach console story-display metadata.
    #[must_use]
    pub fn story_display(mut self, story_display: Vec<StoryDisplayDescriptor>) -> Self {
        self.manifest.story_display = story_display;
        self
    }

    /// Attach declared capabilities.
    #[must_use]
    pub fn capabilities(mut self, capabilities: Vec<String>) -> Self {
        self.manifest.capabilities = capabilities;
        self
    }

    /// Attach a schema-driven admin surface.
    #[must_use]
    pub fn admin(mut self, schema: AdminSchema) -> Self {
        self.manifest.admin = Some(AdminSurface::Schema(schema));
        self
    }

    /// Finish building.
    #[must_use]
    pub fn build(self) -> ModuleManifest {
        self.manifest
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use platform_core::{StoryDisplayDescriptor, StoryDisplaySource};

    #[test]
    fn manifest_round_trips_through_json() {
        let manifest = ModuleManifest::builder("identity")
            .story_display(vec![StoryDisplayDescriptor {
                source: StoryDisplaySource::ExecutionName {
                    name: "identity.create_user".to_owned(),
                },
                display_name: "Create User".to_owned(),
                story_title: Some("User Registration".to_owned()),
            }])
            .build();

        let json = serde_json::to_string(&manifest).expect("serialize");
        let back: ModuleManifest = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(manifest, back);
    }

    #[test]
    fn empty_admin_is_skipped_in_json() {
        let manifest = ModuleManifest::builder("notifications").build();
        let json = serde_json::to_string(&manifest).expect("serialize");
        assert!(!json.contains("admin"), "admin: None must be skipped, got {json}");
    }

    #[test]
    fn manifest_with_admin_serializes_schema_kind() {
        use crate::admin_schema::{AdminSchema, EntitySchema, FieldSchema, FieldType};
        let schema = AdminSchema {
            entities: vec![EntitySchema {
                name: "users".to_owned(),
                label: "Users".to_owned(),
                read_capability: "identity.users.read".to_owned(),
                fields: vec![FieldSchema {
                    name: "email".into(), label: "Email".into(),
                    field_type: FieldType::String, nullable: false,
                }],
            }],
        };
        let manifest = ModuleManifest::builder("identity").admin(schema).build();
        let json = serde_json::to_string(&manifest).expect("serialize");
        assert!(json.contains(r#""kind":"schema""#), "got {json}");
    }
}
