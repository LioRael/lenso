use crate::config::RemoteModuleConfig;
use crate::runtime::{RemoteRuntimeFunction, validate_function_name};
use platform_core::{AppResult, EventHandlerRegistry};
use platform_module::{ModuleBinding, RuntimeSurface};
use platform_runtime::{FunctionDefinition, FunctionRegistry, RetryPolicy};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct RemoteBinding {
    functions: Vec<FunctionDefinition>,
}

impl RemoteBinding {
    pub fn from_runtime_surface(
        config: RemoteModuleConfig,
        runtime: Option<&RuntimeSurface>,
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

        Ok(Self { functions })
    }
}

impl ModuleBinding for RemoteBinding {
    fn register_functions(&self, registry: &mut FunctionRegistry) {
        for function in self.functions.iter().cloned() {
            registry.register(function);
        }
    }

    fn register_event_handlers(&self, _registry: &mut EventHandlerRegistry) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use platform_module::{
        RuntimeFunctionDeclaration, RuntimeRetryPolicyDeclaration, RuntimeSurface,
    };

    #[test]
    fn remote_binding_registers_declared_functions() {
        let binding = RemoteBinding::from_runtime_surface(
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
        let error = RemoteBinding::from_runtime_surface(
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
        )
        .expect_err("invalid function name should fail");

        assert_eq!(error.code, platform_core::ErrorCode::Validation);
    }
}
