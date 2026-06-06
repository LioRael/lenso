use crate::admin::IdentityAdminData;
use crate::repositories::PostgresUserRepository;
use platform_core::{AppContext, StoryDisplayDescriptor, StoryDisplaySource};
use platform_http::ApiOpenApiRouter;
use platform_module::{
    AdminSchema, ConsoleArea, ConsolePackage, ConsoleSurface, EntitySchema, FieldSchema, FieldType,
    LinkedBinding, LinkedHttpContribution, Module, ModuleHttpMethod, ModuleHttpRoute,
    ModuleManifest, RuntimeFunctionDeclaration, RuntimeSurface,
};
use std::sync::Arc;

pub fn story_display() -> Vec<StoryDisplayDescriptor> {
    vec![
        StoryDisplayDescriptor {
            source: StoryDisplaySource::HttpRequest {
                method: "POST".to_owned(),
                path: "/v1/identity/users".to_owned(),
            },
            display_name: "Create User Request".to_owned(),
            story_title: Some("User Registration".to_owned()),
        },
        StoryDisplayDescriptor {
            source: StoryDisplaySource::ExecutionName {
                name: "identity.create_user".to_owned(),
            },
            display_name: "Create User".to_owned(),
            story_title: Some("User Registration".to_owned()),
        },
        StoryDisplayDescriptor {
            source: StoryDisplaySource::ExecutionName {
                name: "identity.user_registered.v1".to_owned(),
            },
            display_name: "User Registered".to_owned(),
            story_title: Some("User Registration".to_owned()),
        },
    ]
}

pub fn http_routes() -> Vec<ModuleHttpRoute> {
    vec![
        ModuleHttpRoute {
            method: ModuleHttpMethod::Post,
            path: "/v1/identity/users".to_owned(),
            capability: None,
            display_name: Some("Create User Request".to_owned()),
            story_title: Some("User Registration".to_owned()),
        },
        ModuleHttpRoute {
            method: ModuleHttpMethod::Get,
            path: "/v1/identity/me".to_owned(),
            capability: None,
            display_name: Some("Fetch Current User".to_owned()),
            story_title: Some("Fetch Current User".to_owned()),
        },
    ]
}

pub fn user_schema() -> AdminSchema {
    AdminSchema {
        entities: vec![EntitySchema {
            name: "users".to_owned(),
            label: "Users".to_owned(),
            read_capability: "identity.users.read".to_owned(),
            fields: vec![
                FieldSchema {
                    name: "id".into(),
                    label: "ID".into(),
                    field_type: FieldType::String,
                    nullable: false,
                },
                FieldSchema {
                    name: "email".into(),
                    label: "Email".into(),
                    field_type: FieldType::String,
                    nullable: false,
                },
                FieldSchema {
                    name: "display_name".into(),
                    label: "Display Name".into(),
                    field_type: FieldType::String,
                    nullable: true,
                },
                FieldSchema {
                    name: "created_at".into(),
                    label: "Created".into(),
                    field_type: FieldType::Timestamp,
                    nullable: false,
                },
                FieldSchema {
                    name: "updated_at".into(),
                    label: "Updated".into(),
                    field_type: FieldType::Timestamp,
                    nullable: false,
                },
            ],
        }],
    }
}

/// Context-free manifest: serializable metadata only (no AppContext needed).
pub fn manifest() -> ModuleManifest {
    ModuleManifest::builder("identity")
        .capabilities(vec!["identity.users.read".to_owned()])
        .story_display(story_display())
        .http_routes(http_routes())
        .console(vec![ConsoleSurface {
            name: "identity".to_owned(),
            label: "Identity".to_owned(),
            area: ConsoleArea::Data,
            route: "/data/identity".to_owned(),
            package: ConsolePackage {
                name: "@lenso/identity-console".to_owned(),
                export: "identityConsoleModule".to_owned(),
            },
            icon: Some("database".to_owned()),
            required_capabilities: vec!["identity.users.read".to_owned()],
            navigation: None,
        }])
        .runtime(RuntimeSurface {
            functions: vec![RuntimeFunctionDeclaration {
                name: "identity.cleanup_expired_sessions.v1".to_owned(),
                version: 1,
                queue: "identity".to_owned(),
                input_schema: Some("identity.cleanup_expired_sessions.v1".to_owned()),
                retry_policy: None,
            }],
        })
        .admin(user_schema())
        .build()
}

pub fn merge_http(base: ApiOpenApiRouter) -> ApiOpenApiRouter {
    base.merge(crate::routes::router())
}

pub fn binding() -> LinkedBinding {
    LinkedBinding::builder()
        .runtime(crate::runtime::descriptor())
        .http(LinkedHttpContribution {
            public_prefixes: &["/v1/identity/"],
            merge: merge_http,
        })
        .build()
}

/// The loaded module: manifest + linked behavior + internal config.
pub fn module(ctx: &AppContext) -> Module {
    let repository = Arc::new(PostgresUserRepository::new(ctx.db.clone()));
    Module::linked(manifest(), binding())
        .with_runtime_config(crate::config::RUNTIME_CONFIG.as_slice())
        .with_admin_data(Arc::new(IdentityAdminData::new(repository)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_declares_linked_http_routes() {
        let manifest = manifest();

        assert_eq!(manifest.http_routes.len(), 2);
        assert_eq!(manifest.http_routes[0].method, ModuleHttpMethod::Post);
        assert_eq!(manifest.http_routes[0].path, "/v1/identity/users");
        assert_eq!(
            manifest.http_routes[0].display_name.as_deref(),
            Some("Create User Request")
        );
        assert_eq!(
            manifest.http_routes[0].story_title.as_deref(),
            Some("User Registration")
        );
        assert_eq!(manifest.http_routes[1].method, ModuleHttpMethod::Get);
        assert_eq!(manifest.http_routes[1].path, "/v1/identity/me");
        assert_eq!(
            manifest.http_routes[1].display_name.as_deref(),
            Some("Fetch Current User")
        );
    }

    #[test]
    fn manifest_declares_runtime_functions() {
        let manifest = manifest();
        let runtime = manifest.runtime.expect("runtime surface");

        assert_eq!(runtime.functions.len(), 1);
        assert_eq!(
            runtime.functions[0].name,
            "identity.cleanup_expired_sessions.v1"
        );
        assert_eq!(runtime.functions[0].queue, "identity");
        assert_eq!(
            runtime.functions[0].input_schema.as_deref(),
            Some("identity.cleanup_expired_sessions.v1")
        );
    }

    #[test]
    fn manifest_declares_identity_console_surface() {
        let manifest = manifest();
        let console_surface_contract: serde_json::Value = serde_json::from_str(include_str!(
            "../../../apps/runtime-console/packages/identity-console/console-surface.json"
        ))
        .expect("identity console surface contract should be valid json");

        assert_eq!(manifest.name, console_surface_contract["id"]);
        assert_eq!(
            manifest.capabilities,
            required_capabilities_from_contract(&console_surface_contract)
        );
        assert_eq!(manifest.console.len(), 1);
        let surface = &manifest.console[0];
        let surface_json =
            serde_json::to_value(surface).expect("identity console surface should serialize");
        assert_eq!(surface.name, console_surface_contract["surfaceName"]);
        assert_eq!(surface.label, console_surface_contract["label"]);
        assert_eq!(surface.area, platform_module::ConsoleArea::Data);
        assert_eq!(surface_json["area"], console_surface_contract["area"]);
        assert_eq!(surface.route, console_surface_contract["route"]);
        assert_eq!(
            surface.package.name,
            console_surface_contract["packageName"]
        );
        assert_eq!(
            surface.package.export,
            console_surface_contract["exportName"]
        );
        assert_eq!(surface_json["icon"], console_surface_contract["icon"]);
        assert_eq!(
            surface.required_capabilities,
            required_capabilities_from_contract(&console_surface_contract)
        );
    }

    fn required_capabilities_from_contract(contract: &serde_json::Value) -> Vec<String> {
        contract["requiredCapabilities"]
            .as_array()
            .expect("requiredCapabilities should be an array")
            .iter()
            .map(|capability| {
                capability
                    .as_str()
                    .expect("requiredCapabilities should contain strings")
                    .to_owned()
            })
            .collect()
    }

    #[test]
    fn linked_http_routes_pass_manifest_lint() {
        let manifest = manifest();

        assert_eq!(
            platform_module::lint_module_http_routes(
                platform_module::ModuleSource::Linked,
                &manifest.http_routes,
            ),
            vec![platform_module::ModuleRouteLint {
                severity: platform_module::ModuleRouteLintSeverity::Ok,
                subject: "routes".to_owned(),
                message: "Declared routes include display and story metadata.".to_owned(),
                suggestion: "No action needed.".to_owned(),
            }]
        );
    }
}
