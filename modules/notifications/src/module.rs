use platform_core::{AppContext, StoryDisplayDescriptor, StoryDisplaySource};
use platform_module::{
    EventHandlerDeclaration, EventSurface, LinkedBinding, Module, ModuleManifest,
    RuntimeFunctionDeclaration, RuntimeSurface,
};
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

/// Context-free manifest: serializable metadata only.
pub fn manifest() -> ModuleManifest {
    ModuleManifest::builder("notifications")
        .story_display(story_display())
        .runtime(RuntimeSurface {
            functions: vec![RuntimeFunctionDeclaration {
                name: crate::runtime::SEND_WELCOME_EMAIL.to_owned(),
                version: 1,
                queue: "notifications".to_owned(),
                input_schema: Some(crate::runtime::SEND_WELCOME_EMAIL.to_owned()),
                retry_policy: None,
            }],
        })
        .events(EventSurface {
            handlers: vec![EventHandlerDeclaration {
                name: "notifications.handle_user_registered".to_owned(),
                event_name: crate::events::USER_REGISTERED.to_owned(),
            }],
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_declares_runtime_functions() {
        let manifest = manifest();
        let runtime = manifest.runtime.expect("runtime surface");

        assert_eq!(runtime.functions.len(), 1);
        assert_eq!(
            runtime.functions[0].name,
            "notifications.send_welcome_email.v1"
        );
        assert_eq!(runtime.functions[0].queue, "notifications");
        assert_eq!(
            runtime.functions[0].input_schema.as_deref(),
            Some("notifications.send_welcome_email.v1")
        );
        let events = manifest.events.expect("events surface");
        assert_eq!(events.handlers.len(), 1);
        assert_eq!(
            events.handlers[0].name,
            "notifications.handle_user_registered"
        );
        assert_eq!(events.handlers[0].event_name, "identity.user_registered.v1");
    }
}
