use platform_core::{AppContext, StoryDisplayDescriptor, StoryDisplaySource};
use platform_domain::DomainDescriptor;

pub const STORY_DISPLAY: &[StoryDisplayDescriptor] = &[
    StoryDisplayDescriptor {
        source: StoryDisplaySource::HttpRequest {
            method: "POST",
            path: "/v1/identity/users",
        },
        display_name: "Create User Request",
        story_title: Some("User Registration"),
    },
    StoryDisplayDescriptor {
        source: StoryDisplaySource::ExecutionName("identity.create_user"),
        display_name: "Create User",
        story_title: Some("User Registration"),
    },
    StoryDisplayDescriptor {
        source: StoryDisplaySource::ExecutionName("identity.user_registered.v1"),
        display_name: "User Registered",
        story_title: Some("User Registration"),
    },
];

pub fn domain(_ctx: &AppContext) -> DomainDescriptor {
    DomainDescriptor::new("identity", crate::runtime::descriptor())
        .with_story_display(STORY_DISPLAY)
}
