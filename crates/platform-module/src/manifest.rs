//! A module's pure-data contract: serializable metadata describable without
//! behavior. Owned + serde so every loading source produces the same shape.

use crate::admin::{
    AdminDeclarativeComponent, AdminDeclarativeSurface, AdminEmbeddedEntry, AdminEmbeddedRuntime,
    AdminEmbeddedSurface, AdminPermission, AdminSurface,
};
use crate::admin_schema::AdminSchema;
use crate::http::{ModuleHttpRoute, lint_module_http_routes};
use crate::module::ModuleSource;
use platform_core::StoryDisplayDescriptor;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use utoipa::ToSchema;

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

    /// Admin surface: `Some(AdminSurface::Schema(_))` for schema-driven CRUD,
    /// future custom surfaces for richer module admin UI, or `None` for modules
    /// with no admin surface (e.g. notifications).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admin: Option<AdminSurface>,

    /// Declared module-owned HTTP routes. These are metadata only until a
    /// loading-source-specific mount/proxy protocol exists.
    #[serde(default)]
    pub http_routes: Vec<ModuleHttpRoute>,

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
                http_routes: Vec::new(),
                capabilities: Vec::new(),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModuleManifestLintSeverity {
    Ok,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ModuleManifestLint {
    pub severity: ModuleManifestLintSeverity,
    pub subject: String,
    pub message: String,
    pub suggestion: String,
}

pub fn lint_module_manifest(
    source: ModuleSource,
    manifest: &ModuleManifest,
) -> Vec<ModuleManifestLint> {
    lint_module_manifest_parts(
        source,
        &manifest.name,
        manifest.admin.as_ref(),
        &manifest.http_routes,
        &manifest.capabilities,
    )
}

pub fn lint_module_manifest_parts(
    source: ModuleSource,
    name: &str,
    admin: Option<&AdminSurface>,
    http_routes: &[ModuleHttpRoute],
    capabilities: &[String],
) -> Vec<ModuleManifestLint> {
    let mut lints = Vec::new();

    if !present(name) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Error,
            subject: "module.name".to_owned(),
            message: "Missing module manifest name.".to_owned(),
            suggestion: "Set ModuleManifest.name to the stable module identifier.".to_owned(),
        });
    }

    for capability in capabilities {
        if !valid_capability(capability) {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Warning,
                subject: format!("capability {capability}"),
                message: "Capability name should use dot-separated lowercase identifiers."
                    .to_owned(),
                suggestion: "Use a stable capability name such as module.entity.read.".to_owned(),
            });
        }
    }

    for route_lint in lint_module_http_routes(source, http_routes) {
        lints.push(ModuleManifestLint {
            severity: match route_lint.severity {
                crate::ModuleRouteLintSeverity::Ok => ModuleManifestLintSeverity::Ok,
                crate::ModuleRouteLintSeverity::Warning => ModuleManifestLintSeverity::Warning,
                crate::ModuleRouteLintSeverity::Error => ModuleManifestLintSeverity::Error,
            },
            subject: route_lint.subject,
            message: route_lint.message,
            suggestion: route_lint.suggestion,
        });
    }

    if let Some(admin) = admin {
        lint_admin_surface(admin, &mut lints);
    }

    if lints.is_empty() {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Ok,
            subject: "manifest".to_owned(),
            message: "Module manifest metadata is complete.".to_owned(),
            suggestion: "No action needed.".to_owned(),
        });
    }

    lints
}

fn lint_admin_surface(admin: &AdminSurface, lints: &mut Vec<ModuleManifestLint>) {
    match admin {
        AdminSurface::Schema(schema) => lint_schema_entities("admin.schema", schema, lints),
        AdminSurface::DeclarativeCustom(surface) => {
            if surface.pages.is_empty() {
                lints.push(ModuleManifestLint {
                    severity: ModuleManifestLintSeverity::Warning,
                    subject: "admin.declarative.pages".to_owned(),
                    message: "Declarative admin surface declares no pages.".to_owned(),
                    suggestion: "Add at least one page or omit the declarative admin surface."
                        .to_owned(),
                });
            }
            if let Some(schema) = &surface.fallback_schema {
                lint_schema_entities("admin.declarative.fallback_schema", schema, lints);
            }
            let fallback_entities = surface
                .fallback_schema
                .as_ref()
                .map(schema_entity_names)
                .unwrap_or_default();
            for page in &surface.pages {
                for section in &page.sections {
                    match &section.component {
                        AdminDeclarativeComponent::EntityTable { entity }
                        | AdminDeclarativeComponent::EntityDetail { entity } => {
                            if !fallback_entities.contains(entity) {
                                lints.push(ModuleManifestLint {
                                    severity: ModuleManifestLintSeverity::Warning,
                                    subject: format!("admin.declarative.section.{}", section.name),
                                    message: format!(
                                        "Declarative section references unknown fallback entity `{entity}`."
                                    ),
                                    suggestion:
                                        "Declare the entity in fallback_schema or update the section binding."
                                            .to_owned(),
                                });
                            }
                        }
                        AdminDeclarativeComponent::MetricStrip { .. } => {}
                    }
                }
            }
        }
        AdminSurface::EmbeddedCustom(surface) => {
            if surface.runtime != AdminEmbeddedRuntime::Iframe {
                lints.push(ModuleManifestLint {
                    severity: ModuleManifestLintSeverity::Warning,
                    subject: "admin.embedded.runtime".to_owned(),
                    message: "Embedded admin runtime is reserved for a future host policy."
                        .to_owned(),
                    suggestion: "Use iframe for the current embedded admin slice.".to_owned(),
                });
            }
            match &surface.entry {
                AdminEmbeddedEntry::Url {
                    url,
                    allowed_origins,
                } => {
                    if !url.starts_with("https://") && !url.starts_with("http://localhost") {
                        lints.push(ModuleManifestLint {
                            severity: ModuleManifestLintSeverity::Warning,
                            subject: "admin.embedded.entry.url".to_owned(),
                            message:
                                "Embedded admin URL should use HTTPS outside local development."
                                    .to_owned(),
                            suggestion: "Use an HTTPS URL and list its origin in allowed_origins."
                                .to_owned(),
                        });
                    }
                    if allowed_origins.is_empty() {
                        lints.push(ModuleManifestLint {
                            severity: ModuleManifestLintSeverity::Warning,
                            subject: "admin.embedded.entry.allowed_origins".to_owned(),
                            message: "Embedded admin surface declares no allowed origins."
                                .to_owned(),
                            suggestion:
                                "Declare the iframe origin allowlist before enabling the surface."
                                    .to_owned(),
                        });
                    }
                }
            }
            if let Some(schema) = &surface.fallback_schema {
                lint_schema_entities("admin.embedded.fallback_schema", schema, lints);
                let fallback_entities = schema_entity_names(schema);
                for permission in &surface.permissions {
                    if let AdminPermission::ReadEntity { entity } = permission
                        && !fallback_entities.contains(entity)
                    {
                        lints.push(ModuleManifestLint {
                            severity: ModuleManifestLintSeverity::Warning,
                            subject: format!("admin.embedded.permission.{entity}"),
                            message: format!(
                                "Embedded admin permission references unknown fallback entity `{entity}`."
                            ),
                            suggestion:
                                "Declare the entity in fallback_schema or remove the permission."
                                    .to_owned(),
                        });
                    }
                }
            }
        }
    }
}

fn lint_schema_entities(prefix: &str, schema: &AdminSchema, lints: &mut Vec<ModuleManifestLint>) {
    if schema.entities.is_empty() {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Warning,
            subject: prefix.to_owned(),
            message: "Admin schema declares no entities.".to_owned(),
            suggestion: "Add at least one entity or omit the admin schema surface.".to_owned(),
        });
    }
    for entity in &schema.entities {
        if !present(&entity.read_capability) {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Warning,
                subject: format!("{prefix}.{}", entity.name),
                message: "Admin entity is missing read capability.".to_owned(),
                suggestion: "Declare the capability required to read this entity.".to_owned(),
            });
        }
    }
}

fn schema_entity_names(schema: &AdminSchema) -> HashSet<String> {
    schema
        .entities
        .iter()
        .map(|entity| entity.name.clone())
        .collect()
}

fn present(value: &str) -> bool {
    !value.trim().is_empty()
}

fn valid_capability(value: &str) -> bool {
    let mut parts = value.split('.');
    let Some(first) = parts.next() else {
        return false;
    };
    present(first)
        && value.contains('.')
        && std::iter::once(first).chain(parts).all(|part| {
            present(part)
                && part.chars().all(|character| {
                    character.is_ascii_lowercase() || character == '_' || character.is_ascii_digit()
                })
        })
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

    /// Attach declared module-owned HTTP routes.
    #[must_use]
    pub fn http_routes(mut self, routes: Vec<ModuleHttpRoute>) -> Self {
        self.manifest.http_routes = routes;
        self
    }

    /// Attach a schema-driven admin surface.
    #[must_use]
    pub fn admin(mut self, schema: AdminSchema) -> Self {
        self.manifest.admin = Some(AdminSurface::Schema(schema));
        self
    }

    /// Attach a host-rendered custom admin surface declaration.
    #[must_use]
    pub fn declarative_admin(mut self, surface: AdminDeclarativeSurface) -> Self {
        self.manifest.admin = Some(AdminSurface::DeclarativeCustom(surface));
        self
    }

    /// Attach a sandboxed module-owned admin surface declaration.
    #[must_use]
    pub fn embedded_admin(mut self, surface: AdminEmbeddedSurface) -> Self {
        self.manifest.admin = Some(AdminSurface::EmbeddedCustom(surface));
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
    use crate::admin::{
        AdminDeclarativeComponent, AdminDeclarativePage, AdminDeclarativeSection,
        AdminDeclarativeSurface,
    };
    use crate::{
        AdminEmbeddedEntry, AdminEmbeddedRuntime, AdminEmbeddedSurface, AdminSandboxPolicy,
    };
    use crate::{ModuleHttpMethod, ModuleHttpRoute};
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
        assert!(
            !json.contains("admin"),
            "admin: None must be skipped, got {json}"
        );
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
                    name: "email".into(),
                    label: "Email".into(),
                    field_type: FieldType::String,
                    nullable: false,
                }],
            }],
        };
        let manifest = ModuleManifest::builder("identity").admin(schema).build();
        let json = serde_json::to_string(&manifest).expect("serialize");
        assert!(json.contains(r#""kind":"schema""#), "got {json}");
    }

    #[test]
    fn manifest_with_declarative_admin_serializes_kind() {
        use crate::admin::AdminDeclarativeSurface;

        let manifest = ModuleManifest::builder("remote-crm")
            .declarative_admin(AdminDeclarativeSurface {
                pages: vec![],
                actions: vec![],
                fallback_schema: None,
            })
            .build();
        let json = serde_json::to_string(&manifest).expect("serialize");
        assert!(
            json.contains(r#""kind":"declarative_custom""#),
            "got {json}"
        );
    }

    #[test]
    fn manifest_with_embedded_admin_serializes_kind() {
        use crate::admin::{
            AdminEmbeddedEntry, AdminEmbeddedRuntime, AdminEmbeddedSurface, AdminSandboxPolicy,
        };

        let manifest = ModuleManifest::builder("remote-crm")
            .embedded_admin(AdminEmbeddedSurface {
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
                permissions: vec![],
                fallback_schema: None,
            })
            .build();
        let json = serde_json::to_string(&manifest).expect("serialize");
        assert!(json.contains(r#""kind":"embedded_custom""#), "got {json}");
    }

    #[test]
    fn manifest_with_http_routes_round_trips_through_json() {
        let manifest = ModuleManifest::builder("remote-crm")
            .http_routes(vec![
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
                    capability: Some("remote_crm.contacts.write".to_owned()),
                    display_name: None,
                    story_title: None,
                },
            ])
            .build();

        let json = serde_json::to_string(&manifest).expect("serialize");
        assert!(json.contains(r#""http_routes""#), "got {json}");
        assert!(json.contains(r#""method":"GET""#), "got {json}");
        assert!(
            json.contains(r#""display_name":"List Contacts""#),
            "got {json}"
        );
        let back: ModuleManifest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(manifest, back);
    }

    #[test]
    fn manifest_lint_warns_for_invalid_capability_names() {
        let manifest = ModuleManifest::builder("remote-crm")
            .capabilities(vec!["RemoteCRM Contacts Read".to_owned()])
            .build();

        assert!(
            lint_module_manifest(ModuleSource::Remote, &manifest)
                .iter()
                .any(|lint| lint.subject == "capability RemoteCRM Contacts Read"
                    && lint.severity == ModuleManifestLintSeverity::Warning)
        );
    }

    #[test]
    fn manifest_lint_warns_for_unknown_declarative_fallback_entities() {
        let manifest = ModuleManifest::builder("remote-crm")
            .declarative_admin(AdminDeclarativeSurface {
                pages: vec![AdminDeclarativePage {
                    name: "dashboard".to_owned(),
                    label: "Dashboard".to_owned(),
                    sections: vec![AdminDeclarativeSection {
                        name: "missing".to_owned(),
                        label: "Missing".to_owned(),
                        component: AdminDeclarativeComponent::EntityTable {
                            entity: "contacts".to_owned(),
                        },
                    }],
                }],
                actions: vec![],
                fallback_schema: None,
            })
            .build();

        assert!(
            lint_module_manifest(ModuleSource::Remote, &manifest)
                .iter()
                .any(|lint| lint.subject == "admin.declarative.section.missing"
                    && lint.severity == ModuleManifestLintSeverity::Warning)
        );
    }

    #[test]
    fn manifest_lint_warns_for_embedded_origin_policy() {
        let manifest = ModuleManifest::builder("remote-crm")
            .embedded_admin(AdminEmbeddedSurface {
                runtime: AdminEmbeddedRuntime::Iframe,
                entry: AdminEmbeddedEntry::Url {
                    url: "http://crm.example.test/admin".to_owned(),
                    allowed_origins: vec![],
                },
                sandbox: AdminSandboxPolicy {
                    allow_scripts: true,
                    allow_forms: false,
                    allow_popups: false,
                    allow_same_origin: false,
                },
                permissions: vec![],
                fallback_schema: None,
            })
            .build();

        let lints = lint_module_manifest(ModuleSource::Remote, &manifest);

        assert!(
            lints
                .iter()
                .any(|lint| lint.subject == "admin.embedded.entry.url")
        );
        assert!(
            lints
                .iter()
                .any(|lint| lint.subject == "admin.embedded.entry.allowed_origins")
        );
    }
}
