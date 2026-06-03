use platform_core::{AppContext, StoryDisplayDescriptor, StoryDisplaySource};
use platform_domain::DomainDescriptor;

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

pub fn domain(_ctx: &AppContext) -> DomainDescriptor {
    DomainDescriptor::new("identity", crate::runtime::descriptor())
        .with_story_display(story_display())
        .with_runtime_config(crate::config::RUNTIME_CONFIG.as_slice())
}
