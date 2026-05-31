use platform_core::EventHandler;
use platform_http::DomainHttp;
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
        name: "identity",
        runtime: crate::runtime::descriptor(),
        event_handlers: Vec::new(),
    }
}

pub fn http(ctx: platform_core::AppContext) -> DomainHttp {
    let _ctx = ctx;
    DomainHttp {
        name: "identity",
        router: crate::routes::router(),
    }
}
