use platform_core::{AppContext, StoryDisplayDescriptor, StoryDisplaySource};
use platform_domain::DomainDescriptor;
use platform_module::{LinkedBinding, Module, ModuleManifest};
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

/// Context-free manifest: serializable metadata only.
pub fn manifest() -> ModuleManifest {
    ModuleManifest::builder("notifications")
        .story_display(story_display())
        .build()
}

/// The loaded module: manifest + linked behavior (event handler, no config).
pub fn module(ctx: &AppContext) -> Module {
    let runtime_client = RuntimeClient::new(ctx.db.clone());
    let binding = LinkedBinding::builder()
        .runtime(crate::runtime::descriptor())
        .event_handlers(vec![Arc::new(
            crate::events::WelcomeEmailRequestedHandler::new(runtime_client),
        )])
        .build();
    Module::linked(manifest(), binding)
}
