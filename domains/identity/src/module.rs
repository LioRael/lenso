use platform_core::{EventHandler, StoryDisplayDescriptor, StoryDisplaySource};
use platform_http::DomainHttp;
use platform_runtime::RuntimeDescriptor;
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

pub fn domain() -> DomainDescriptor {
    DomainDescriptor {
        name: "identity",
        runtime: crate::runtime::descriptor(),
        event_handlers: Vec::new(),
        story_display: STORY_DISPLAY,
    }
}

pub fn http(ctx: platform_core::AppContext) -> DomainHttp {
    let _ctx = ctx;
    DomainHttp {
        name: "identity",
        router: crate::routes::router(),
    }
}
