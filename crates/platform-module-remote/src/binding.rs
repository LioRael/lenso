use crate::config::RemoteModuleConfig;
use crate::event::RemoteEventHandler;
use crate::runtime::{RemoteRuntimeFunction, validate_function_name};
use platform_core::{AppResult, EventHandler, EventHandlerRegistry};
use platform_module::{EventSurface, ModuleBinding, RuntimeSurface};
use platform_runtime::{FunctionDefinition, FunctionRegistry, RetryPolicy};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct RemoteBinding {
    functions: Vec<FunctionDefinition>,
    event_handlers: Vec<Arc<dyn EventHandler>>,
}

impl RemoteBinding {
    pub fn from_surfaces(
        config: RemoteModuleConfig,
        runtime: Option<&RuntimeSurface>,
        events: Option<&EventSurface>,
    ) -> AppResult<Self> {
        let functions = runtime
            .into_iter()
            .flat_map(|surface| surface.functions.iter())
            .map(|declaration| {
                validate_function_name(&declaration.name)?;
                Ok(FunctionDefinition {
                    name: declaration.name.clone(),
                    version: declaration.version,
                    queue: declaration.queue.clone(),
                    retry_policy: declaration
                        .retry_policy
                        .as_ref()
                        .map(|policy| {
                            RetryPolicy::fixed(
                                policy.max_attempts,
                                Duration::from_millis(policy.initial_delay_ms),
                            )
                        })
                        .unwrap_or_default(),
                    handler: Arc::new(RemoteRuntimeFunction::new(
                        config.clone(),
                        declaration.name.clone(),
                    )?),
                })
            })
            .collect::<AppResult<Vec<_>>>()?;

        let event_handlers = events
            .into_iter()
            .flat_map(|surface| surface.handlers.iter())
            .map(|declaration| {
                Ok(Arc::new(RemoteEventHandler::new(
                    config.clone(),
                    declaration.name.clone(),
                    declaration.event_name.clone(),
                )?) as Arc<dyn EventHandler>)
            })
            .collect::<AppResult<Vec<_>>>()?;

        Ok(Self {
            functions,
            event_handlers,
        })
    }
}

impl ModuleBinding for RemoteBinding {
    fn register_functions(&self, registry: &mut FunctionRegistry) {
        for function in self.functions.iter().cloned() {
            registry.register(function);
        }
    }

    fn register_event_handlers(&self, registry: &mut EventHandlerRegistry) {
        registry.register_all(self.event_handlers.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use platform_module::{
        EventHandlerDeclaration, EventSurface, RuntimeFunctionDeclaration,
        RuntimeRetryPolicyDeclaration, RuntimeSurface,
    };

    #[test]
    fn remote_binding_registers_declared_functions() {
        let binding = RemoteBinding::from_surfaces(
            RemoteModuleConfig::new("remote-crm", "http://127.0.0.1:4100/lenso/module/v1"),
            Some(&RuntimeSurface {
                functions: vec![RuntimeFunctionDeclaration {
                    name: "remote_crm.sync_contact.v1".to_owned(),
                    version: 1,
                    queue: "remote-crm".to_owned(),
                    input_schema: Some("remote_crm.sync_contact.v1".to_owned()),
                    retry_policy: Some(RuntimeRetryPolicyDeclaration {
                        max_attempts: 3,
                        initial_delay_ms: 1000,
                    }),
                }],
            }),
            None,
        )
        .expect("remote binding should build");

        let mut registry = FunctionRegistry::default();
        binding.register_functions(&mut registry);

        let definition = registry
            .get("remote_crm.sync_contact.v1")
            .expect("remote function should register");
        assert_eq!(definition.version, 1);
        assert_eq!(definition.queue, "remote-crm");
        assert_eq!(definition.retry_policy.max_attempts, 3);
        assert_eq!(
            definition.retry_policy.initial_delay,
            Duration::from_millis(1000)
        );
    }

    #[test]
    fn remote_binding_rejects_invalid_function_name() {
        let error = RemoteBinding::from_surfaces(
            RemoteModuleConfig::new("remote-crm", "http://127.0.0.1:4100/lenso/module/v1"),
            Some(&RuntimeSurface {
                functions: vec![RuntimeFunctionDeclaration {
                    name: "remote_crm/sync_contact.v1".to_owned(),
                    version: 1,
                    queue: "remote-crm".to_owned(),
                    input_schema: None,
                    retry_policy: None,
                }],
            }),
            None,
        )
        .expect_err("invalid function name should fail");

        assert_eq!(error.code, platform_core::ErrorCode::Validation);
    }

    #[test]
    fn remote_binding_registers_declared_event_handlers() {
        let binding = RemoteBinding::from_surfaces(
            RemoteModuleConfig::new("remote-crm", "http://127.0.0.1:4100/lenso/module/v1"),
            None,
            Some(&EventSurface {
                handlers: vec![EventHandlerDeclaration {
                    name: "sync_contact_on_user_registered".to_owned(),
                    event_name: "identity.user_registered.v1".to_owned(),
                }],
            }),
        )
        .expect("remote binding should build");

        let mut registry = EventHandlerRegistry::default();
        binding.register_event_handlers(&mut registry);

        assert_eq!(registry.handler_count("identity.user_registered.v1"), 1);
    }

    #[test]
    fn remote_binding_rejects_invalid_event_handler_name() {
        let error = RemoteBinding::from_surfaces(
            RemoteModuleConfig::new("remote-crm", "http://127.0.0.1:4100/lenso/module/v1"),
            None,
            Some(&EventSurface {
                handlers: vec![EventHandlerDeclaration {
                    name: "sync/contact".to_owned(),
                    event_name: "identity.user_registered.v1".to_owned(),
                }],
            }),
        )
        .expect_err("invalid event handler name should fail");

        assert_eq!(error.code, platform_core::ErrorCode::Validation);
    }
}
