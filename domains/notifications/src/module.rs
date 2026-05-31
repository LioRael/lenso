use platform_core::EventHandler;
use platform_runtime::{RuntimeClient, RuntimeDescriptor};
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

pub fn domain(pool: platform_core::DbPool) -> DomainDescriptor {
    let runtime_client = RuntimeClient::new(pool);
    DomainDescriptor {
        name: "notifications",
        runtime: crate::runtime::descriptor(),
        event_handlers: vec![Arc::new(crate::events::WelcomeEmailRequestedHandler::new(
            runtime_client,
        ))],
    }
}
