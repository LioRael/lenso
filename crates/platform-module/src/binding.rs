//! A module's behavior contract: only what varies across loading sources.

use platform_core::EventHandlerRegistry;
use platform_runtime::{FunctionRegistry, RuntimeClient};
use std::sync::Arc;

#[derive(Debug, Clone, Default)]
pub struct EventHandlerRegistrationContext {
    runtime: Option<EventHandlerRuntimeContext>,
}

impl EventHandlerRegistrationContext {
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_runtime(
        runtime_client: RuntimeClient,
        function_registry: Arc<FunctionRegistry>,
    ) -> Self {
        Self {
            runtime: Some(EventHandlerRuntimeContext {
                runtime_client,
                function_registry,
            }),
        }
    }

    #[must_use]
    pub fn runtime(&self) -> Option<&EventHandlerRuntimeContext> {
        self.runtime.as_ref()
    }
}

#[derive(Debug, Clone)]
pub struct EventHandlerRuntimeContext {
    pub runtime_client: RuntimeClient,
    pub function_registry: Arc<FunctionRegistry>,
}

/// Narrow by design — pure data lives in [`crate::ModuleManifest`], read
/// directly by upper layers, never through this trait.
///
/// HTTP routing is deliberately EXCLUDED from this cross-source trait: it
/// carries utoipa `OpenApiRouter` types that out-of-process/Wasm sources cannot
/// produce. Linked modules can still carry source-specific HTTP behavior on
/// [`crate::LinkedBinding`].
pub trait ModuleBinding: std::fmt::Debug + Send + Sync {
    /// Register this module's runtime functions into the shared registry.
    fn register_functions(&self, registry: &mut FunctionRegistry);

    /// Register this module's in-process event handlers.
    fn register_event_handlers(
        &self,
        registry: &mut EventHandlerRegistry,
        context: &EventHandlerRegistrationContext,
    );
}
