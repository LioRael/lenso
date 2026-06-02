use platform_core::{EventHandler, StoryDisplayDescriptor, StoryDisplaySource};
use platform_runtime::{RuntimeClient, RuntimeDescriptor};
use std::sync::Arc;

#[derive(Clone)]
pub struct DomainDescriptor {
    pub name: &'static str,
    pub runtime: RuntimeDescriptor,
    pub event_handlers: Vec<Arc<dyn EventHandler>>,
    pub story_display: &'static [StoryDisplayDescriptor],
}

impl std::fmt::Debug for DomainDescriptor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DomainDescriptor")
            .field("name", &self.name)
            .field("runtime", &self.runtime)
            .field("event_handlers", &self.event_handlers.len())
            .field("story_display", &self.story_display.len())
            .finish()
    }
}

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

pub fn domain(pool: platform_core::DbPool) -> DomainDescriptor {
    let runtime_client = RuntimeClient::new(pool);
    DomainDescriptor {
        name: "notifications",
        runtime: crate::runtime::descriptor(),
        event_handlers: vec![Arc::new(crate::events::WelcomeEmailRequestedHandler::new(
            runtime_client,
        ))],
        story_display: STORY_DISPLAY,
    }
}
