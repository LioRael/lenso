use platform_core::{AppContext, StoryDisplayDescriptor, StoryDisplaySource};
use platform_domain::DomainDescriptor;
use platform_runtime::RuntimeClient;
use std::sync::Arc;

pub fn story_display() -> Vec<StoryDisplayDescriptor> {
    vec![
        StoryDisplayDescriptor {
            source: StoryDisplaySource::ExecutionName {
                name: "notifications.handle_user_registered".to_owned(),
            },
            display_name: "Handle User Registered".to_owned(),
            story_title: None,
        },
        StoryDisplayDescriptor {
            source: StoryDisplaySource::ExecutionName {
                name: "notifications.send_welcome_email.v1".to_owned(),
            },
            display_name: "Send Welcome Email".to_owned(),
            story_title: None,
        },
    ]
}

pub fn domain(ctx: &AppContext) -> DomainDescriptor {
    let runtime_client = RuntimeClient::new(ctx.db.clone());
    DomainDescriptor::new("notifications", crate::runtime::descriptor())
        .with_story_display(story_display())
        .with_event_handlers(vec![Arc::new(
            crate::events::WelcomeEmailRequestedHandler::new(runtime_client),
        )])
}
