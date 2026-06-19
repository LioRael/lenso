use crate::admin::AuthAdminData;
use crate::repositories::PostgresAuthUserRepository;
use platform_core::AppContext;
use platform_http::ApiOpenApiRouter;
use platform_module::{
    AdminAction, AdminActionDangerLevel, AdminActionInputField, AdminActionInputSchema,
    AdminDeclarativeComponent, AdminDeclarativePage, AdminDeclarativeSection,
    AdminDeclarativeSurface, AdminSchema, ConsoleArea, ConsoleNavigation, ConsolePackage,
    ConsoleSurface, ConsoleWorkspaceRef, EntitySchema, FieldSchema, FieldType, LinkedBinding,
    LinkedHttpContribution, Module, ModuleHttpMethod, ModuleHttpRoute, ModuleManifest,
};
use std::sync::Arc;

pub const MODULE_NAME: &str = "auth";
pub const AUTH_USERS_READ: &str = "auth.users.read";

pub fn http_routes() -> Vec<ModuleHttpRoute> {
    vec![
        ModuleHttpRoute {
            method: ModuleHttpMethod::Post,
            path: "/v1/auth/dev/sessions".to_owned(),
            capability: None,
            display_name: Some("Create Development Session".to_owned()),
            story_title: Some("Development Auth Session".to_owned()),
        },
        ModuleHttpRoute {
            method: ModuleHttpMethod::Post,
            path: "/v1/auth/sessions/revoke".to_owned(),
            capability: None,
            display_name: Some("Revoke Session".to_owned()),
            story_title: Some("Auth Session Revoked".to_owned()),
        },
    ]
}

pub fn user_schema() -> AdminSchema {
    AdminSchema {
        entities: vec![
            EntitySchema {
                name: "users".to_owned(),
                label: "Users".to_owned(),
                read_capability: AUTH_USERS_READ.to_owned(),
                fields: vec![
                    FieldSchema {
                        name: "id".to_owned(),
                        label: "ID".to_owned(),
                        field_type: FieldType::String,
                        nullable: false,
                    },
                    FieldSchema {
                        name: "created_at".to_owned(),
                        label: "Created".to_owned(),
                        field_type: FieldType::Timestamp,
                        nullable: false,
                    },
                    FieldSchema {
                        name: "disabled_at".to_owned(),
                        label: "Disabled".to_owned(),
                        field_type: FieldType::Timestamp,
                        nullable: true,
                    },
                    FieldSchema {
                        name: "disabled_reason".to_owned(),
                        label: "Reason".to_owned(),
                        field_type: FieldType::String,
                        nullable: true,
                    },
                    FieldSchema {
                        name: "disabled_until".to_owned(),
                        label: "Until".to_owned(),
                        field_type: FieldType::Timestamp,
                        nullable: true,
                    },
                ],
            },
            EntitySchema {
                name: "sessions".to_owned(),
                label: "Sessions".to_owned(),
                read_capability: AUTH_USERS_READ.to_owned(),
                fields: vec![
                    FieldSchema {
                        name: "id".to_owned(),
                        label: "ID".to_owned(),
                        field_type: FieldType::String,
                        nullable: false,
                    },
                    FieldSchema {
                        name: "user_id".to_owned(),
                        label: "User".to_owned(),
                        field_type: FieldType::String,
                        nullable: false,
                    },
                    FieldSchema {
                        name: "created_at".to_owned(),
                        label: "Created".to_owned(),
                        field_type: FieldType::Timestamp,
                        nullable: false,
                    },
                    FieldSchema {
                        name: "expires_at".to_owned(),
                        label: "Expires".to_owned(),
                        field_type: FieldType::Timestamp,
                        nullable: false,
                    },
                    FieldSchema {
                        name: "revoked_at".to_owned(),
                        label: "Revoked".to_owned(),
                        field_type: FieldType::Timestamp,
                        nullable: true,
                    },
                ],
            },
        ],
    }
}

pub fn admin_surface() -> AdminDeclarativeSurface {
    AdminDeclarativeSurface {
        pages: vec![AdminDeclarativePage {
            name: "sessions".to_owned(),
            label: "Sessions".to_owned(),
            sections: vec![AdminDeclarativeSection {
                name: "sessions".to_owned(),
                label: "Sessions".to_owned(),
                component: AdminDeclarativeComponent::EntityTable {
                    entity: "sessions".to_owned(),
                },
            }],
        }],
        actions: vec![
            action_with_string_input(
                "revoke_session",
                "Revoke session",
                "session_id",
                "Session",
                AdminActionDangerLevel::Medium,
            ),
            disable_user_action(),
            action_with_string_input(
                "enable_user",
                "Enable user",
                "user_id",
                "User",
                AdminActionDangerLevel::Low,
            ),
        ],
        fallback_schema: Some(user_schema()),
    }
}

fn action_with_string_input(
    name: &str,
    label: &str,
    input_name: &str,
    input_label: &str,
    danger_level: AdminActionDangerLevel,
) -> AdminAction {
    AdminAction {
        name: name.to_owned(),
        label: label.to_owned(),
        capability: AUTH_USERS_READ.to_owned(),
        input_schema: Some(AdminActionInputSchema {
            fields: vec![AdminActionInputField {
                name: input_name.to_owned(),
                label: input_label.to_owned(),
                field_type: FieldType::String,
                required: true,
                description: None,
            }],
        }),
        confirmation: None,
        danger_level,
    }
}

fn disable_user_action() -> AdminAction {
    AdminAction {
        name: "disable_user".to_owned(),
        label: "Disable user".to_owned(),
        capability: AUTH_USERS_READ.to_owned(),
        input_schema: Some(AdminActionInputSchema {
            fields: vec![
                AdminActionInputField {
                    name: "user_id".to_owned(),
                    label: "User".to_owned(),
                    field_type: FieldType::String,
                    required: true,
                    description: None,
                },
                AdminActionInputField {
                    name: "reason".to_owned(),
                    label: "Reason".to_owned(),
                    field_type: FieldType::String,
                    required: false,
                    description: None,
                },
                AdminActionInputField {
                    name: "disabled_until".to_owned(),
                    label: "Until".to_owned(),
                    field_type: FieldType::Timestamp,
                    required: false,
                    description: Some("RFC3339 timestamp; omit for permanent".to_owned()),
                },
            ],
        }),
        confirmation: None,
        danger_level: AdminActionDangerLevel::Medium,
    }
}

fn auth_workspace() -> ConsoleWorkspaceRef {
    ConsoleWorkspaceRef {
        id: "auth".to_owned(),
        label: "Auth".to_owned(),
        icon: Some("shield".to_owned()),
    }
}

pub fn console_surfaces() -> Vec<ConsoleSurface> {
    vec![
        ConsoleSurface {
            name: "sessions".to_owned(),
            label: "Sessions".to_owned(),
            area: ConsoleArea::Data,
            route: "/data/auth/sessions".to_owned(),
            package: ConsolePackage {
                name: "@lenso/auth-console".to_owned(),
                export: "authConsoleModule".to_owned(),
            },
            icon: Some("shield".to_owned()),
            required_capabilities: vec![AUTH_USERS_READ.to_owned()],
            navigation: Some(ConsoleNavigation {
                workspace: auth_workspace(),
                group: None,
                order: Some(50),
            }),
        },
        ConsoleSurface {
            name: "users".to_owned(),
            label: "Users".to_owned(),
            area: ConsoleArea::Data,
            route: "/data/auth/users".to_owned(),
            package: ConsolePackage {
                name: "@lenso/auth-console".to_owned(),
                export: "authConsoleModule".to_owned(),
            },
            icon: Some("shield".to_owned()),
            required_capabilities: vec![AUTH_USERS_READ.to_owned()],
            navigation: Some(ConsoleNavigation {
                workspace: auth_workspace(),
                group: None,
                order: Some(60),
            }),
        },
    ]
}

pub fn manifest() -> ModuleManifest {
    ModuleManifest::builder(MODULE_NAME)
        .capabilities(vec![AUTH_USERS_READ.to_owned()])
        .http_routes(http_routes())
        .declarative_admin(admin_surface())
        .console(console_surfaces())
        .build()
}

pub fn merge_http(base: ApiOpenApiRouter) -> ApiOpenApiRouter {
    base.merge(crate::routes::router())
}

pub fn binding() -> LinkedBinding {
    LinkedBinding::builder()
        .http(LinkedHttpContribution {
            public_prefixes: &["/v1/auth/dev/", "/v1/auth/sessions/"],
            merge: merge_http,
        })
        .build()
}

pub fn module(ctx: &AppContext) -> Module {
    let repository = Arc::new(PostgresAuthUserRepository::new(ctx.db.clone()));
    let admin = Arc::new(AuthAdminData::new(repository));
    Module::linked(manifest(), binding())
        .with_admin_data(admin.clone())
        .with_admin_actions(admin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use platform_module::{ModuleManifestLintSeverity, ModuleSource, lint_module_manifest};

    #[test]
    fn manifest_declares_auth_user_anchor() {
        let manifest = manifest();

        assert_eq!(manifest.name, MODULE_NAME);
        assert_eq!(manifest.capabilities, vec![AUTH_USERS_READ]);
        assert_eq!(manifest.http_routes, http_routes());
        assert_eq!(
            manifest.admin,
            Some(platform_module::AdminSurface::DeclarativeCustom(
                admin_surface()
            ))
        );
        assert_eq!(manifest.console, console_surfaces());

        let lints = lint_module_manifest(ModuleSource::Linked, &manifest);
        assert!(
            lints
                .iter()
                .all(|lint| lint.severity == ModuleManifestLintSeverity::Ok),
            "auth manifest should not have warning/error lints: {lints:?}"
        );
    }
}
