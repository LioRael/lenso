//! A module's behavior contract: only what varies across loading sources.

use platform_core::EventHandlerRegistry;
use platform_runtime::FunctionRegistry;

/// Narrow by design — pure data lives in [`crate::ModuleManifest`], read
/// directly by upper layers, never through this trait.
///
/// HTTP routing is deliberately EXCLUDED: it carries utoipa `OpenApiRouter`
/// types that out-of-process/Wasm sources cannot produce, so its cross-source
/// shape is the Remote spec's problem. Two clean seams now.
pub trait ModuleBinding: std::fmt::Debug + Send + Sync {
    /// Register this module's runtime functions into the shared registry.
    fn register_functions(&self, registry: &mut FunctionRegistry);

    /// Register this module's in-process event handlers.
    fn register_event_handlers(&self, registry: &mut EventHandlerRegistry);
}
