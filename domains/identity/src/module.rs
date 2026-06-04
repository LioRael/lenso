use crate::admin::IdentityAdminData;
use crate::repositories::PostgresUserRepository;
use platform_core::{AppContext, StoryDisplayDescriptor, StoryDisplaySource};
use platform_module::{
    AdminSchema, EntitySchema, FieldSchema, FieldType, LinkedBinding, Module, ModuleHttpMethod,
    ModuleHttpRoute, ModuleManifest,
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
        .story_display(story_display())
        .http_routes(http_routes())
        .admin(user_schema())
        .build()
}

/// The loaded module: manifest + linked behavior + internal config.
pub fn module(ctx: &AppContext) -> Module {
    let repository = Arc::new(PostgresUserRepository::new(ctx.db.clone()));
    let binding = LinkedBinding::builder()
        .runtime(crate::runtime::descriptor())
        .build();
    Module::linked(manifest(), binding)
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
}
