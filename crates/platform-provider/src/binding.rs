use crate::config::ProviderConfig;
use crate::event::{
    ProviderEventHandler, ProviderEventHostActionRunner, validate_event_handler_name,
    validate_event_name,
};
use crate::runtime::{ProviderRuntimeFunction, validate_function_name};
use platform_core::{AppResult, EventHandlerRegistry};
use platform_module::{
    EventHandlerRegistrationContext, EventSurface, ModuleBinding, RuntimeSurface,
};
use platform_runtime::{FunctionDefinition, FunctionRegistry, RetryPolicy};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct ProviderBinding {
    config: Option<ProviderConfig>,
    functions: Vec<FunctionDefinition>,
    event_handlers: Vec<ProviderEventHandlerRegistration>,
}

#[derive(Debug, Clone)]
struct ProviderEventHandlerRegistration {
    name: String,
    event_name: String,
}

impl ProviderBinding {
    pub fn from_surfaces(
        config: ProviderConfig,
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
                    handler: Arc::new(ProviderRuntimeFunction::new(
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
                validate_event_handler_name(&declaration.name)?;
                validate_event_name(&declaration.event_name)?;
                Ok(ProviderEventHandlerRegistration {
                    name: declaration.name.clone(),
                    event_name: declaration.event_name.clone(),
                })
            })
            .collect::<AppResult<Vec<_>>>()?;

        Ok(Self {
            config: Some(config),
            functions,
            event_handlers,
        })
    }
}

impl ModuleBinding for ProviderBinding {
    fn register_functions(&self, registry: &mut FunctionRegistry) {
        for function in self.functions.iter().cloned() {
            registry.register(function);
        }
    }

    fn register_event_handlers(
        &self,
        registry: &mut EventHandlerRegistry,
        context: &EventHandlerRegistrationContext,
    ) {
        let Some(config) = &self.config else {
            return;
        };
        let allowed_function_names = self
            .functions
            .iter()
            .map(|function| function.name.clone())
            .collect::<Vec<_>>();

        for declaration in &self.event_handlers {
            let mut handler = ProviderEventHandler::new(
                config.clone(),
                declaration.name.clone(),
                declaration.event_name.clone(),
            )
            .expect("provider event handler declaration was validated");

            if let Some(runtime) = context.runtime() {
                handler = handler.with_host_action_runner(ProviderEventHostActionRunner::new(
                    runtime.runtime_client.clone(),
                    runtime.function_registry.clone(),
                    allowed_function_names.clone(),
                ));
            }

            registry.register(std::sync::Arc::new(handler));
        }
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
    fn provider_binding_registers_declared_functions() {
        let binding = ProviderBinding::from_surfaces(
            ProviderConfig::new("provider-crm", "http://127.0.0.1:4100/lenso/provider/v1"),
            Some(&RuntimeSurface {
                functions: vec![RuntimeFunctionDeclaration {
                    name: "provider_crm.sync_contact.v1".to_owned(),
                    version: 1,
                    queue: "provider-crm".to_owned(),
                    input_schema: Some("provider_crm.sync_contact.v1".to_owned()),
                    retry_policy: Some(RuntimeRetryPolicyDeclaration {
                        max_attempts: 3,
                        initial_delay_ms: 1000,
                    }),
                    operation: None,
                }],
                schedules: vec![],
                workflows: vec![],
            }),
            None,
        )
        .expect("provider binding should build");

        let mut registry = FunctionRegistry::default();
        binding.register_functions(&mut registry);

        let definition = registry
            .get("provider_crm.sync_contact.v1")
            .expect("provider function should register");
        assert_eq!(definition.version, 1);
        assert_eq!(definition.queue, "provider-crm");
        assert_eq!(definition.retry_policy.max_attempts, 3);
        assert_eq!(
            definition.retry_policy.initial_delay,
            Duration::from_millis(1000)
        );
    }

    #[test]
    fn provider_binding_rejects_invalid_function_name() {
        let error = ProviderBinding::from_surfaces(
            ProviderConfig::new("provider-crm", "http://127.0.0.1:4100/lenso/provider/v1"),
            Some(&RuntimeSurface {
                functions: vec![RuntimeFunctionDeclaration {
                    name: "provider_crm/sync_contact.v1".to_owned(),
                    version: 1,
                    queue: "provider-crm".to_owned(),
                    input_schema: None,
                    retry_policy: None,
                    operation: None,
                }],
                schedules: vec![],
                workflows: vec![],
            }),
            None,
        )
        .expect_err("invalid function name should fail");

        assert_eq!(error.code, platform_core::ErrorCode::Validation);
    }

    #[test]
    fn provider_binding_registers_declared_event_handlers() {
        let binding = ProviderBinding::from_surfaces(
            ProviderConfig::new("provider-crm", "http://127.0.0.1:4100/lenso/provider/v1"),
            None,
            Some(&EventSurface {
                handlers: vec![EventHandlerDeclaration {
                    name: "sync_contact_on_user_registered".to_owned(),
                    event_name: "identity.user_registered.v1".to_owned(),
                    operation: None,
                }],
            }),
        )
        .expect("provider binding should build");

        let mut registry = EventHandlerRegistry::default();
        binding.register_event_handlers(&mut registry, &EventHandlerRegistrationContext::empty());

        assert_eq!(registry.handler_count("identity.user_registered.v1"), 1);
    }

    #[test]
    fn provider_binding_rejects_invalid_event_handler_name() {
        let error = ProviderBinding::from_surfaces(
            ProviderConfig::new("provider-crm", "http://127.0.0.1:4100/lenso/provider/v1"),
            None,
            Some(&EventSurface {
                handlers: vec![EventHandlerDeclaration {
                    name: "sync/contact".to_owned(),
                    event_name: "identity.user_registered.v1".to_owned(),
                    operation: None,
                }],
            }),
        )
        .expect_err("invalid event handler name should fail");

        assert_eq!(error.code, platform_core::ErrorCode::Validation);
    }
}
