//! The compile-time loading source: behavior is linked Rust code.

use crate::binding::ModuleBinding;
use platform_core::{EventHandler, EventHandlerRegistry};
use platform_http::ApiOpenApiRouter;
use platform_runtime::{FunctionRegistry, RuntimeDescriptor};
use std::sync::Arc;

pub type LinkedHttpRouteMerger = fn(ApiOpenApiRouter) -> ApiOpenApiRouter;

#[derive(Debug, Clone, Copy)]
pub struct LinkedHttpContribution {
    pub public_prefixes: &'static [&'static str],
    pub merge: LinkedHttpRouteMerger,
}

/// The only [`ModuleBinding`] impl in Step 1. Remote/Wasm impls arrive in their
/// own specs without touching this one (open extension point). Only forwards to
/// the existing registration logic — no logic moves here.
#[derive(Debug)]
pub struct LinkedBinding {
    pub runtime: RuntimeDescriptor,
    pub event_handlers: Vec<Arc<dyn EventHandler>>,
    pub http: Option<LinkedHttpContribution>,
}

impl LinkedBinding {
    /// Start building a linked binding.
    #[must_use]
    pub fn builder() -> LinkedBindingBuilder {
        LinkedBindingBuilder {
            runtime: RuntimeDescriptor::default(),
            event_handlers: Vec::new(),
            http: None,
        }
    }
}

impl ModuleBinding for LinkedBinding {
    fn register_functions(&self, registry: &mut FunctionRegistry) {
        self.runtime.register_into(registry);
    }

    fn register_event_handlers(&self, registry: &mut EventHandlerRegistry) {
        registry.register_all(self.event_handlers.clone());
    }
}

/// Fluent builder for [`LinkedBinding`]. Source-specific (Linked only).
#[derive(Debug)]
pub struct LinkedBindingBuilder {
    runtime: RuntimeDescriptor,
    event_handlers: Vec<Arc<dyn EventHandler>>,
    http: Option<LinkedHttpContribution>,
}

impl LinkedBindingBuilder {
    /// Set the runtime descriptor (functions, queues, triggers, flows).
    #[must_use]
    pub fn runtime(mut self, runtime: RuntimeDescriptor) -> Self {
        self.runtime = runtime;
        self
    }

    /// Set the in-process event handlers.
    #[must_use]
    pub fn event_handlers(mut self, handlers: Vec<Arc<dyn EventHandler>>) -> Self {
        self.event_handlers = handlers;
        self
    }

    /// Set this linked module's in-process HTTP router contribution.
    #[must_use]
    pub fn http(mut self, contribution: LinkedHttpContribution) -> Self {
        self.http = Some(contribution);
        self
    }

    /// Finish building.
    #[must_use]
    pub fn build(self) -> LinkedBinding {
        LinkedBinding {
            runtime: self.runtime,
            event_handlers: self.event_handlers,
            http: self.http,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use platform_core::{ClaimedOutboxEvent, ExecutionContext};
    use platform_runtime::{FunctionDefinition, FunctionHandler, RetryPolicy};
    use serde_json::Value;

    // Minimal no-op handler to register one function.
    #[derive(Debug)]
    struct NoopHandler;

    #[async_trait]
    impl FunctionHandler for NoopHandler {
        async fn call(
            &self,
            _ctx: ExecutionContext,
            _input: Value,
        ) -> platform_core::AppResult<Value> {
            Ok(Value::Null)
        }
    }

    // Minimal no-op event handler bound to a fixed event name.
    #[derive(Debug)]
    struct NoopEventHandler;

    #[async_trait]
    impl EventHandler for NoopEventHandler {
        fn event_name(&self) -> &'static str {
            "test.event"
        }

        async fn handle(&self, _event: &ClaimedOutboxEvent) -> platform_core::AppResult<()> {
            Ok(())
        }
    }

    #[test]
    fn linked_binding_registers_its_functions() {
        let runtime = RuntimeDescriptor {
            module: "test",
            functions: vec![FunctionDefinition {
                name: "test.noop".to_owned(),
                version: 1,
                queue: "test".to_owned(),
                retry_policy: RetryPolicy::default(),
                handler: Arc::new(NoopHandler),
            }],
            ..RuntimeDescriptor::default()
        };
        let binding = LinkedBinding::builder().runtime(runtime).build();

        let mut registry = FunctionRegistry::default();
        binding.register_functions(&mut registry);

        assert!(registry.get("test.noop").is_some());
    }

    #[test]
    fn linked_binding_registers_its_event_handlers() {
        let binding = LinkedBinding::builder()
            .event_handlers(vec![Arc::new(NoopEventHandler)])
            .build();

        let mut registry = EventHandlerRegistry::default();
        binding.register_event_handlers(&mut registry);

        assert_eq!(registry.handler_count("test.event"), 1);
    }
}
