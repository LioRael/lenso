use platform_core::{AppContext, StoryDisplayDescriptor, StoryDisplaySource};
use platform_domain::DomainDescriptor;
use platform_runtime::RuntimeClient;
use std::sync::Arc;

pub const STORY_DISPLAY: &[StoryDisplayDescriptor] = &[
    StoryDisplayDescriptor {
        source: StoryDisplaySource::ExecutionName("notifications.handle_user_registered"),
        display_name: "Handle User Registered",
        story_title: None,
    },
    StoryDisplayDescriptor {
        source: StoryDisplaySource::ExecutionName("notifications.send_welcome_email.v1"),
        display_name: "Send Welcome Email",
        story_title: None,
    },
];

pub fn domain(ctx: &AppContext) -> DomainDescriptor {
    let runtime_client = RuntimeClient::new(ctx.db.clone());
    DomainDescriptor::new("notifications", crate::runtime::descriptor())
        .with_story_display(STORY_DISPLAY)
        .with_event_handlers(vec![Arc::new(
            crate::events::WelcomeEmailRequestedHandler::new(runtime_client),
        )])
}
