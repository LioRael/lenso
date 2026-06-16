use crate::admin::AuthAdminData;
use crate::repositories::PostgresAuthUserRepository;
use platform_core::AppContext;
use platform_http::ApiOpenApiRouter;
use platform_module::{
    AdminSchema, EntitySchema, FieldSchema, FieldType, LinkedBinding, LinkedHttpContribution,
    Module, ModuleHttpMethod, ModuleHttpRoute, ModuleManifest,
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
        entities: vec![EntitySchema {
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
            ],
        }],
    }
}

pub fn manifest() -> ModuleManifest {
    ModuleManifest::builder(MODULE_NAME)
        .capabilities(vec![AUTH_USERS_READ.to_owned()])
        .http_routes(http_routes())
        .admin(user_schema())
        .build()
}

pub fn merge_http(base: ApiOpenApiRouter) -> ApiOpenApiRouter {
    base.merge(crate::routes::router())
}

pub fn binding() -> LinkedBinding {
    LinkedBinding::builder()
        .http(LinkedHttpContribution {
            public_prefixes: &["/v1/auth/"],
            merge: merge_http,
        })
        .build()
}

pub fn module(ctx: &AppContext) -> Module {
    let repository = Arc::new(PostgresAuthUserRepository::new(ctx.db.clone()));
    Module::linked(manifest(), binding()).with_admin_data(Arc::new(AuthAdminData::new(repository)))
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
            Some(platform_module::AdminSurface::Schema(user_schema()))
        );

        let lints = lint_module_manifest(ModuleSource::Linked, &manifest);
        assert!(
            lints
                .iter()
                .all(|lint| lint.severity == ModuleManifestLintSeverity::Ok),
            "auth manifest should not have warning/error lints: {lints:?}"
        );
    }
}
