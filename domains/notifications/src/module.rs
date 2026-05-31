use platform_core::EventHandler;
use platform_runtime::RuntimeDescriptor;
use std::sync::Arc;

#[derive(Clone)]
pub struct DomainDescriptor {
    pub name: &'static str,
    pub runtime: RuntimeDescriptor,
    pub event_handlers: Vec<Arc<dyn EventHandler>>,
}

impl std::fmt::Debug for DomainDescriptor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DomainDescriptor")
            .field("name", &self.name)
            .field("runtime", &self.runtime)
            .field("event_handlers", &self.event_handlers.len())
            .finish()
    }
}

pub fn domain() -> DomainDescriptor {
    DomainDescriptor {
        name: "notifications",
        runtime: RuntimeDescriptor {
            module: "notifications",
            ..RuntimeDescriptor::default()
        },
        event_handlers: vec![Arc::new(crate::events::WelcomeEmailRequestedHandler)],
    }
}
