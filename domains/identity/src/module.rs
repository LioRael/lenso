use platform_core::{AppContext, StoryDisplayDescriptor, StoryDisplaySource};
use platform_module::{LinkedBinding, Module, ModuleManifest};

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
            source: StoryDisplaySource::ExecutionName { name: "identity.create_user".to_owned() },
            display_name: "Create User".to_owned(),
            story_title: Some("User Registration".to_owned()),
        },
        StoryDisplayDescriptor {
            source: StoryDisplaySource::ExecutionName { name: "identity.user_registered.v1".to_owned() },
            display_name: "User Registered".to_owned(),
            story_title: Some("User Registration".to_owned()),
        },
    ]
}

/// Context-free manifest: serializable metadata only (no AppContext needed).
pub fn manifest() -> ModuleManifest {
    ModuleManifest::builder("identity")
        .story_display(story_display())
        .build()
}

/// The loaded module: manifest + linked behavior + internal config.
pub fn module(_ctx: &AppContext) -> Module {
    let binding = LinkedBinding::builder()
        .runtime(crate::runtime::descriptor())
        .build();
    Module::linked(manifest(), binding)
        .with_runtime_config(crate::config::RUNTIME_CONFIG.as_slice())
}
