use platform_module::ModuleBinding;
use platform_core::EventHandlerRegistry;
use platform_runtime::FunctionRegistry;

#[derive(Debug, Default)]
pub struct RemoteBinding;

impl ModuleBinding for RemoteBinding {
    fn register_functions(&self, _registry: &mut FunctionRegistry) {}

    fn register_event_handlers(&self, _registry: &mut EventHandlerRegistry) {}
}
