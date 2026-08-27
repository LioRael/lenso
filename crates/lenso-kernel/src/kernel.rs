use super::{
    ActivateContext, AppAdmission, AppReadyGate, BTreeMap, CancellationToken, Cell,
    DeactivationReason, DriverControl, ExecutionAdapterCatalog, ManagedResourceScope,
    ManagedTaskScope, NativeApp, NativeAppRuntime, NativeBindingTable, NativeEndpointBinding,
    NativeEndpointState, NativeEndpointStateTable, NativeEventBindingTable,
    NativeEventEndpointStateTable, NativeExecutionAdapter, NativePluginGeneration,
    NativePluginRuntime, NativeStreamBindingTable, NativeStreamEndpointBinding,
    NativeStreamEndpointState, NativeStreamEndpointStateTable, PlanResolutionError,
    PluginDependencies, PluginDependency, PluginDependencyHandle, PluginEventDependencyHandle,
    PluginStreamDependencyHandle, PrepareContext, PreparedBinding, PreparedEventBinding,
    PreparedNativeApp, PreparedNativePlugin, PreparedStreamBinding, Rc, RefCell, RequestAdmission,
    ResolvedAppPlan, RuntimeDiagnostics, RuntimeDriver, RuntimeFailure, ShutdownCoordinator, Weak,
    begin_plugin_supervision, deactivate_in_reverse, event, handle_supervision_schedule_failure,
    plugin_supervision, schedule_plugin_supervision, validate_native_endpoint_set,
};

/// A reason the Kernel rejected a Resolved App Plan before boot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanValidationError {
    /// The Plan schema cannot be executed by this Kernel version.
    UnsupportedSchemaVersion { expected: u32, actual: u32 },
    /// The Plan graph is structurally invalid and cannot be booted.
    InvalidResolvedPlan { detail: String },
}

impl std::fmt::Display for PlanValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedSchemaVersion { expected, actual } => write!(
                formatter,
                "unsupported Resolved App Plan schema {actual}; expected {expected}"
            ),
            Self::InvalidResolvedPlan { detail } => {
                write!(formatter, "invalid Resolved App Plan: {detail}")
            }
        }
    }
}

impl std::error::Error for PlanValidationError {}

/// The portable App execution engine.
#[derive(Debug)]
pub struct Kernel;

impl Kernel {
    /// Starts one App backed by a single statically linked native Adapter package.
    pub async fn start_native<D: RuntimeDriver, A: NativeExecutionAdapter>(
        plan: ResolvedAppPlan,
        driver: D,
        adapter: A,
    ) -> Result<NativeApp, RuntimeFailure> {
        Self::start_native_with_diagnostics(plan, driver, adapter, RuntimeDiagnostics::new()).await
    }

    /// Starts one native Adapter package with an opt-in Runtime Diagnostics port.
    pub async fn start_native_with_diagnostics<D: RuntimeDriver, A: NativeExecutionAdapter>(
        plan: ResolvedAppPlan,
        driver: D,
        adapter: A,
        diagnostics: RuntimeDiagnostics,
    ) -> Result<NativeApp, RuntimeFailure> {
        Self::start_with_diagnostics(
            plan,
            driver,
            ExecutionAdapterCatalog::single(adapter),
            diagnostics,
        )
        .await
    }

    /// Starts Plugin Instances through the Adapter catalog assembled by the Runner.
    pub async fn start<D: RuntimeDriver>(
        plan: ResolvedAppPlan,
        driver: D,
        adapters: ExecutionAdapterCatalog,
    ) -> Result<NativeApp, RuntimeFailure> {
        Self::start_with_diagnostics(plan, driver, adapters, RuntimeDiagnostics::new()).await
    }

    /// Starts an App with an opt-in Runtime Diagnostics port.
    #[allow(
        clippy::too_many_lines,
        reason = "startup remains linear so validation, preparation, and activation fail closed in order"
    )]
    pub async fn start_with_diagnostics<D: RuntimeDriver>(
        plan: ResolvedAppPlan,
        driver: D,
        adapters: ExecutionAdapterCatalog,
        diagnostics: RuntimeDiagnostics,
    ) -> Result<NativeApp, RuntimeFailure> {
        if let Err(error) = plan.validate() {
            let error = runtime_plan_error(&error);
            diagnostics.emit_runtime_failure(driver.now(), None, &error);
            return Err(error);
        }

        let activation_order = match plan.activation_order() {
            Ok(order) => order,
            Err(error) => {
                let error = runtime_plan_error(&error);
                diagnostics.emit_runtime_failure(driver.now(), None, &error);
                return Err(error);
            }
        };
        let adapters = Rc::new(adapters);
        let PreparedNativeApp {
            bindings: prepared_bindings,
            stream_bindings: prepared_stream_bindings,
            event_bindings: prepared_event_bindings,
            generations,
        } = match adapters.prepare(&plan) {
            Ok(prepared) => prepared,
            Err(error) => {
                diagnostics.emit_runtime_failure(driver.now(), None, &error);
                return Err(error);
            }
        };
        if let Err(error) = validate_prepared_native_app(
            &plan,
            &prepared_bindings,
            &prepared_stream_bindings,
            &prepared_event_bindings,
            &generations,
        ) {
            diagnostics.emit_runtime_failure(driver.now(), None, &error);
            return Err(error);
        }
        let (bindings, endpoint_states) = native_bindings(&plan, &prepared_bindings);
        let (stream_bindings, stream_endpoint_states) =
            native_stream_bindings(&plan, &prepared_stream_bindings);
        let (event_bindings, event_endpoint_states) =
            native_event_bindings(&plan, &prepared_event_bindings);
        let runtime_link = Rc::new(RefCell::new(Weak::new()));
        let dependencies = plugin_dependencies(
            &plan,
            &bindings,
            &stream_bindings,
            &event_bindings,
            &runtime_link,
        );
        let driver_control = DriverControl::new(&driver);
        let admission = AppAdmission::new();
        let plugin_runtimes = native_plugin_runtimes(&plan, &driver, generations);
        let ready_gate = AppReadyGate::new();
        let supervision = plugin_supervision(&plan);
        let runtime = Rc::new(NativeAppRuntime {
            plan,
            adapters,
            plugins: plugin_runtimes,
            dependencies,
            endpoint_states,
            stream_endpoint_states,
            event_endpoint_states,
            supervision: RefCell::new(supervision),
            supervision_tasks: RefCell::new(BTreeMap::new()),
            activation_order,
            ready_gate,
            admission,
            driver: driver_control,
            diagnostics: diagnostics.clone(),
            request_ids: Rc::new(Cell::new(1)),
            supervision_cancellation: CancellationToken::new(),
            shutdown_started: Cell::new(false),
            shutdown: ShutdownCoordinator::default(),
            shutdown_task: RefCell::new(None),
            terminal_failure: RefCell::new(None),
        });
        runtime_link.replace(Rc::downgrade(&runtime));
        attach_managed_task_failure_handlers(&runtime);
        runtime.diagnostics.emit(
            super::DiagnosticSource::Lifecycle,
            (runtime.driver.now)(),
            |_| super::DiagnosticEvent::AppStarted {
                plugin_count: runtime.plan.plugin_instances().len(),
            },
        );
        let prepared_instances = prepare_native_plugins(&runtime).await?;
        if let Err(error) = activate_native_plugins(&runtime).await {
            let _ = deactivate_in_reverse(
                &runtime.plugins,
                &runtime.dependencies,
                &prepared_instances,
                DeactivationReason::StartupRollback,
                &runtime.admission,
                &runtime.diagnostics,
                &runtime.driver,
            )
            .await;
            runtime
                .diagnostics
                .emit_runtime_failure((runtime.driver.now)(), None, &error);
            return Err(error);
        }
        open_native_readiness(&runtime).await;
        Ok(NativeApp {
            bindings,
            stream_bindings,
            event_bindings,
            diagnostics,
            runtime,
        })
    }
}

pub(super) fn attach_managed_task_failure_handlers(runtime: &Rc<NativeAppRuntime>) {
    for (instance_key, plugin) in &runtime.plugins {
        let Some((_, tasks, _)) = plugin.generation_parts() else {
            continue;
        };
        attach_managed_task_failure_handler(runtime, instance_key, &tasks);
    }
}

pub(super) fn attach_managed_task_failure_handler(
    runtime: &Rc<NativeAppRuntime>,
    instance_key: &str,
    tasks: &ManagedTaskScope,
) {
    let task_runtime = Rc::downgrade(runtime);
    let task_instance_key = instance_key.to_owned();
    let handler: Rc<dyn Fn()> = Rc::new(move || {
        let Some(runtime) = task_runtime.upgrade() else {
            return;
        };
        if begin_plugin_supervision(&runtime, &task_instance_key).unwrap_or(false)
            && let Err(error) = schedule_plugin_supervision(&runtime, &task_instance_key)
        {
            let _ = handle_supervision_schedule_failure(&runtime, &task_instance_key, error);
        }
    });
    tasks.set_failure_handler(&handler);
}

pub(super) fn runtime_plan_error(error: &PlanResolutionError) -> RuntimeFailure {
    RuntimeFailure::InvalidResolvedPlan {
        detail: error.to_string(),
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "one fail-closed pass keeps request, stream, event, and generation validation aligned"
)]
pub(super) fn validate_prepared_native_app(
    plan: &ResolvedAppPlan,
    bindings: &[PreparedBinding],
    stream_bindings: &[PreparedStreamBinding],
    event_bindings: &[PreparedEventBinding],
    generations: &BTreeMap<String, PreparedNativePlugin>,
) -> Result<(), RuntimeFailure> {
    if generations.len() != plan.plugin_instances().len() {
        return Err(RuntimeFailure::InvalidResolvedPlan {
            detail: format!(
                "Execution Adapters prepared {} Plugin generations; expected {}",
                generations.len(),
                plan.plugin_instances().len()
            ),
        });
    }
    for instance in plan.plugin_instances() {
        let generation = generations.get(instance.instance_key()).ok_or_else(|| {
            RuntimeFailure::InvalidResolvedPlan {
                detail: format!(
                    "Execution Adapters did not prepare Plugin Instance `{}`",
                    instance.instance_key()
                ),
            }
        })?;
        validate_native_endpoint_set(
            instance.instance_key(),
            instance,
            generation.endpoints(),
            generation.stream_endpoints(),
            generation.event_endpoints(),
        )?;
    }
    if let Some(instance_key) = generations.keys().find(|instance_key| {
        !plan
            .plugin_instances()
            .iter()
            .any(|instance| instance.instance_key() == instance_key.as_str())
    }) {
        return Err(RuntimeFailure::InvalidResolvedPlan {
            detail: format!("Execution Adapter prepared unknown Plugin Instance `{instance_key}`"),
        });
    }

    let expected_request_bindings = plan
        .capability_bindings()
        .iter()
        .filter(|binding| {
            plan.plugin_instance(binding.provider_instance())
                .and_then(|provider| {
                    provider
                        .provided_capabilities()
                        .iter()
                        .find(|endpoint| endpoint.capability_id() == binding.capability_id())
                })
                .is_some_and(|endpoint| !endpoint.request_operations().is_empty())
        })
        .count();
    let expected_stream_bindings = plan
        .capability_bindings()
        .iter()
        .filter(|binding| {
            plan.plugin_instance(binding.provider_instance())
                .and_then(|provider| {
                    provider
                        .provided_capabilities()
                        .iter()
                        .find(|endpoint| endpoint.capability_id() == binding.capability_id())
                })
                .is_some_and(|endpoint| !endpoint.stream_operations().is_empty())
        })
        .count();
    let expected_event_bindings = plan
        .capability_bindings()
        .iter()
        .filter(|binding| {
            plan.plugin_instance(binding.provider_instance())
                .and_then(|provider| {
                    provider
                        .provided_capabilities()
                        .iter()
                        .find(|endpoint| endpoint.capability_id() == binding.capability_id())
                })
                .is_some_and(|endpoint| !endpoint.event_operations().is_empty())
        })
        .count();
    if bindings.len() != expected_request_bindings {
        return Err(RuntimeFailure::InvalidResolvedPlan {
            detail: if expected_stream_bindings == 0 && stream_bindings.is_empty() {
                format!(
                    "Execution Adapters prepared {} bindings; expected {}",
                    bindings.len(),
                    expected_request_bindings
                )
            } else {
                format!(
                    "Execution Adapters prepared {} request bindings; expected {}",
                    bindings.len(),
                    expected_request_bindings
                )
            },
        });
    }
    if stream_bindings.len() != expected_stream_bindings {
        return Err(RuntimeFailure::InvalidResolvedPlan {
            detail: format!(
                "Execution Adapters prepared {} stream bindings; expected {}",
                stream_bindings.len(),
                expected_stream_bindings
            ),
        });
    }
    if event_bindings.len() != expected_event_bindings {
        return Err(RuntimeFailure::InvalidResolvedPlan {
            detail: format!(
                "Execution Adapters prepared {} Event bindings; expected {}",
                event_bindings.len(),
                expected_event_bindings
            ),
        });
    }
    for planned in plan.capability_bindings() {
        let provider = generations
            .get(planned.provider_instance())
            .expect("the resolved Plan references one validated provider generation");
        let descriptor = plan
            .plugin_instance(planned.provider_instance())
            .and_then(|provider| {
                provider
                    .provided_capabilities()
                    .iter()
                    .find(|endpoint| endpoint.capability_id() == planned.capability_id())
            })
            .expect("the resolved Plan references one validated provider endpoint");
        if !descriptor.request_operations().is_empty() {
            let matching: Vec<_> = bindings
                .iter()
                .filter(|prepared| {
                    prepared.consumer_instance == planned.consumer_instance()
                        && prepared.provider_instance == planned.provider_instance()
                        && prepared.endpoint.capability_id() == planned.capability_id()
                        && prepared.endpoint.descriptor_version() == planned.descriptor_version()
                })
                .collect();
            if matching.len() != 1 {
                return Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!(
                        "Execution Adapters prepared {} request bindings for `{}:{}:{}`; expected 1",
                        matching.len(),
                        planned.consumer_instance(),
                        planned.capability_id(),
                        planned.provider_instance()
                    ),
                });
            }
            if !provider
                .endpoints()
                .iter()
                .any(|endpoint| Rc::ptr_eq(endpoint, &matching[0].endpoint))
            {
                return Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!(
                        "request binding `{}:{}:{}` does not reference its provider generation endpoint",
                        planned.consumer_instance(),
                        planned.capability_id(),
                        planned.provider_instance()
                    ),
                });
            }
        }
        if !descriptor.stream_operations().is_empty() {
            let matching: Vec<_> = stream_bindings
                .iter()
                .filter(|prepared| {
                    prepared.consumer_instance == planned.consumer_instance()
                        && prepared.provider_instance == planned.provider_instance()
                        && prepared.endpoint.capability_id() == planned.capability_id()
                        && prepared.endpoint.descriptor_version() == planned.descriptor_version()
                })
                .collect();
            if matching.len() != 1 {
                return Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!(
                        "Execution Adapters prepared {} stream bindings for `{}:{}:{}`; expected 1",
                        matching.len(),
                        planned.consumer_instance(),
                        planned.capability_id(),
                        planned.provider_instance()
                    ),
                });
            }
            if !provider
                .stream_endpoints()
                .iter()
                .any(|endpoint| Rc::ptr_eq(endpoint, &matching[0].endpoint))
            {
                return Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!(
                        "stream binding `{}:{}:{}` does not reference its provider generation endpoint",
                        planned.consumer_instance(),
                        planned.capability_id(),
                        planned.provider_instance()
                    ),
                });
            }
        }
        if !descriptor.event_operations().is_empty() {
            let matching: Vec<_> = event_bindings
                .iter()
                .filter(|prepared| {
                    prepared.consumer_instance == planned.consumer_instance()
                        && prepared.provider_instance == planned.provider_instance()
                        && prepared.endpoint.capability_id() == planned.capability_id()
                        && prepared.endpoint.descriptor_version() == planned.descriptor_version()
                })
                .collect();
            if matching.len() != 1 {
                return Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!(
                        "Execution Adapters prepared {} Event bindings for `{}:{}:{}`; expected 1",
                        matching.len(),
                        planned.consumer_instance(),
                        planned.capability_id(),
                        planned.provider_instance()
                    ),
                });
            }
            if !provider
                .event_endpoints()
                .iter()
                .any(|endpoint| Rc::ptr_eq(endpoint, &matching[0].endpoint))
            {
                return Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!(
                        "Event binding `{}:{}:{}` does not reference its provider generation endpoint",
                        planned.consumer_instance(),
                        planned.capability_id(),
                        planned.provider_instance()
                    ),
                });
            }
        }
    }
    Ok(())
}

pub(super) fn native_plugin_runtimes<D: RuntimeDriver>(
    plan: &ResolvedAppPlan,
    driver: &D,
    mut generations: BTreeMap<String, PreparedNativePlugin>,
) -> BTreeMap<String, NativePluginRuntime> {
    let mut runtimes = BTreeMap::new();
    for instance in plan.plugin_instances() {
        let lifecycle = generations
            .remove(instance.instance_key())
            .map(|generation| generation.lifecycle())
            .expect("prepared App validation requires one generation per planned Instance");
        runtimes.insert(
            instance.instance_key().to_owned(),
            NativePluginRuntime {
                generation: RefCell::new(Some(NativePluginGeneration {
                    lifecycle,
                    tasks: ManagedTaskScope::new(driver),
                    resources: ManagedResourceScope::new(),
                })),
            },
        );
    }
    runtimes
}

pub(super) async fn prepare_native_plugins(
    runtime: &Rc<NativeAppRuntime>,
) -> Result<Vec<String>, RuntimeFailure> {
    let mut prepared_instances = Vec::with_capacity(runtime.activation_order.len());
    for instance_key in &runtime.activation_order {
        let instance = runtime
            .plan
            .plugin_instances()
            .iter()
            .find(|instance| instance.instance_key() == instance_key)
            .expect("activation order only contains planned Plugin Instances");
        let plugin = runtime
            .plugins
            .get(instance_key)
            .expect("activation order only contains planned Plugin Instances");
        let (lifecycle, tasks, resources) = plugin
            .generation_parts()
            .expect("every startup Plugin Instance has a generation");
        let cancellation = tasks.cancellation();
        prepared_instances.push(instance_key.clone());
        let started_at = (runtime.driver.now)();
        runtime
            .diagnostics
            .emit(super::DiagnosticSource::Lifecycle, started_at, |_| {
                super::DiagnosticEvent::LifecycleStarted {
                    instance: instance_key.clone(),
                    generation: 1,
                    phase: super::PluginLifecyclePhase::Prepare,
                }
            });
        let context = PrepareContext {
            instance_key: instance_key.clone(),
            entrypoint: instance.entrypoint().to_owned(),
            configuration: instance.configuration().to_owned(),
            dependencies: runtime
                .dependencies
                .get(instance_key)
                .cloned()
                .unwrap_or_default(),
            resources,
            cancellation,
            admission: runtime.admission.clone(),
        };
        let result = lifecycle.prepare(context).await;
        let outcome = result.as_ref().map_or_else(
            |error| super::DiagnosticOutcome::RuntimeFailure(error.into()),
            |()| super::DiagnosticOutcome::Succeeded,
        );
        runtime.diagnostics.emit(
            super::DiagnosticSource::Lifecycle,
            (runtime.driver.now)(),
            |_| super::DiagnosticEvent::LifecycleCompleted {
                instance: instance_key.clone(),
                generation: 1,
                phase: super::PluginLifecyclePhase::Prepare,
                outcome,
                elapsed: (runtime.driver.now)().saturating_sub(started_at),
            },
        );
        if let Err(error) = result {
            let _ = deactivate_in_reverse(
                &runtime.plugins,
                &runtime.dependencies,
                &prepared_instances,
                DeactivationReason::StartupRollback,
                &runtime.admission,
                &runtime.diagnostics,
                &runtime.driver,
            )
            .await;
            runtime.diagnostics.emit_runtime_failure(
                (runtime.driver.now)(),
                Some(instance_key),
                &error,
            );
            return Err(error);
        }
    }
    Ok(prepared_instances)
}

pub(super) async fn activate_native_plugins(
    runtime: &Rc<NativeAppRuntime>,
) -> Result<(), RuntimeFailure> {
    for instance_key in &runtime.activation_order {
        let plugin = runtime
            .plugins
            .get(instance_key)
            .expect("activation order only contains planned Plugin Instances");
        let (lifecycle, tasks, resources) = plugin
            .generation_parts()
            .expect("every startup Plugin Instance has a generation");
        let cancellation = tasks.cancellation();
        let started_at = (runtime.driver.now)();
        runtime
            .diagnostics
            .emit(super::DiagnosticSource::Lifecycle, started_at, |_| {
                super::DiagnosticEvent::LifecycleStarted {
                    instance: instance_key.clone(),
                    generation: 1,
                    phase: super::PluginLifecyclePhase::Activate,
                }
            });
        let context = ActivateContext {
            instance_key: instance_key.clone(),
            dependencies: runtime
                .dependencies
                .get(instance_key)
                .cloned()
                .unwrap_or_default(),
            ready_gate: runtime.ready_gate.clone(),
            tasks,
            resources,
            cancellation,
            admission: runtime.admission.clone(),
        };
        let result = lifecycle.activate(context).await;
        let outcome = result.as_ref().map_or_else(
            |error| super::DiagnosticOutcome::RuntimeFailure(error.into()),
            |()| super::DiagnosticOutcome::Succeeded,
        );
        runtime.diagnostics.emit(
            super::DiagnosticSource::Lifecycle,
            (runtime.driver.now)(),
            |_| super::DiagnosticEvent::LifecycleCompleted {
                instance: instance_key.clone(),
                generation: 1,
                phase: super::PluginLifecyclePhase::Activate,
                outcome,
                elapsed: (runtime.driver.now)().saturating_sub(started_at),
            },
        );
        if let Err(error) = result {
            runtime.diagnostics.emit_runtime_failure(
                (runtime.driver.now)(),
                Some(instance_key),
                &error,
            );
            return Err(error);
        }
    }
    Ok(())
}

pub(super) async fn open_native_readiness(runtime: &Rc<NativeAppRuntime>) {
    runtime.ready_gate.open();
    runtime.admission.open();
    runtime.diagnostics.emit(
        super::DiagnosticSource::Lifecycle,
        (runtime.driver.now)(),
        |_| super::DiagnosticEvent::AppReady,
    );
    (runtime.driver.yield_now)().await;
}

pub(super) fn native_bindings(
    plan: &ResolvedAppPlan,
    prepared: &[PreparedBinding],
) -> (NativeBindingTable, NativeEndpointStateTable) {
    let mut bindings = BTreeMap::new();
    let mut endpoint_states = BTreeMap::new();
    for binding in plan.capability_bindings() {
        let Some(descriptor) =
            plan.plugin_instance(binding.provider_instance())
                .and_then(|provider| {
                    provider
                        .provided_capabilities()
                        .iter()
                        .find(|endpoint| endpoint.capability_id() == binding.capability_id())
                })
        else {
            continue;
        };
        if descriptor.request_operations().is_empty() {
            continue;
        }
        let Some(endpoint) = prepared.iter().find_map(|prepared| {
            (prepared.consumer_instance == binding.consumer_instance()
                && prepared.provider_instance == binding.provider_instance()
                && prepared.endpoint.capability_id() == binding.capability_id())
            .then_some(&prepared.endpoint)
        }) else {
            continue;
        };
        let state = endpoint_states
            .entry((
                binding.provider_instance().to_owned(),
                endpoint.capability_id().to_owned(),
            ))
            .or_insert_with(|| Rc::new(NativeEndpointState::new(endpoint.clone(), 1)))
            .clone();
        let admissions = endpoint
            .operations()
            .iter()
            .map(|operation| {
                (
                    (*operation).to_owned(),
                    RequestAdmission::new(plan.request_admission_for(binding, operation)),
                )
            })
            .collect();
        bindings
            .entry((
                binding.consumer_instance().to_owned(),
                endpoint.capability_id(),
            ))
            .or_insert_with(Vec::new)
            .push(NativeEndpointBinding {
                plugin_instance: binding.provider_instance().to_owned(),
                state,
                admissions,
            });
    }
    (bindings, endpoint_states)
}

pub(super) fn native_stream_bindings(
    plan: &ResolvedAppPlan,
    prepared: &[PreparedStreamBinding],
) -> (NativeStreamBindingTable, NativeStreamEndpointStateTable) {
    let mut bindings = BTreeMap::new();
    let mut endpoint_states = BTreeMap::new();
    for binding in plan.capability_bindings() {
        let Some(descriptor) =
            plan.plugin_instance(binding.provider_instance())
                .and_then(|provider| {
                    provider
                        .provided_capabilities()
                        .iter()
                        .find(|endpoint| endpoint.capability_id() == binding.capability_id())
                })
        else {
            continue;
        };
        if descriptor.stream_operations().is_empty() {
            continue;
        }
        let Some(endpoint) = prepared.iter().find_map(|prepared| {
            (prepared.consumer_instance == binding.consumer_instance()
                && prepared.provider_instance == binding.provider_instance()
                && prepared.endpoint.capability_id() == binding.capability_id())
            .then_some(&prepared.endpoint)
        }) else {
            continue;
        };
        let state = endpoint_states
            .entry((
                binding.provider_instance().to_owned(),
                endpoint.capability_id().to_owned(),
            ))
            .or_insert_with(|| Rc::new(NativeStreamEndpointState::new(endpoint.clone(), 1)))
            .clone();
        let admissions = endpoint
            .operations()
            .iter()
            .map(|operation| {
                (
                    (*operation).to_owned(),
                    RequestAdmission::new(plan.request_admission_for(binding, operation)),
                )
            })
            .collect();
        bindings
            .entry((
                binding.consumer_instance().to_owned(),
                endpoint.capability_id(),
            ))
            .or_insert_with(Vec::new)
            .push(NativeStreamEndpointBinding {
                plugin_instance: binding.provider_instance().to_owned(),
                state,
                admissions,
            });
    }
    (bindings, endpoint_states)
}

pub(super) fn native_event_bindings(
    plan: &ResolvedAppPlan,
    prepared: &[PreparedEventBinding],
) -> (NativeEventBindingTable, NativeEventEndpointStateTable) {
    let mut bindings = BTreeMap::new();
    let mut endpoint_states = BTreeMap::new();
    for binding in plan.capability_bindings() {
        let Some(descriptor) =
            plan.plugin_instance(binding.provider_instance())
                .and_then(|provider| {
                    provider
                        .provided_capabilities()
                        .iter()
                        .find(|endpoint| endpoint.capability_id() == binding.capability_id())
                })
        else {
            continue;
        };
        if descriptor.event_operations().is_empty() {
            continue;
        }
        let Some(endpoint) = prepared.iter().find_map(|prepared| {
            (prepared.consumer_instance == binding.consumer_instance()
                && prepared.provider_instance == binding.provider_instance()
                && prepared.endpoint.capability_id() == binding.capability_id())
            .then_some(&prepared.endpoint)
        }) else {
            continue;
        };
        let state = endpoint_states
            .entry((
                binding.provider_instance().to_owned(),
                endpoint.capability_id().to_owned(),
            ))
            .or_insert_with(|| Rc::new(event::NativeEventEndpointState::new(endpoint.clone(), 1)))
            .clone();
        let queue = event::NativeEventQueue::new(plan.event_admission_for(binding));
        state.register_queue(&queue);
        bindings
            .entry((
                binding.consumer_instance().to_owned(),
                endpoint.capability_id(),
            ))
            .or_insert_with(Vec::new)
            .push(event::NativeEventEndpointBinding {
                plugin_instance: binding.provider_instance().to_owned(),
                state,
                queue,
            });
    }
    (bindings, endpoint_states)
}

pub(super) fn plugin_dependencies(
    plan: &ResolvedAppPlan,
    endpoints: &BTreeMap<(String, &'static str), Vec<NativeEndpointBinding>>,
    stream_endpoints: &NativeStreamBindingTable,
    event_endpoints: &NativeEventBindingTable,
    runtime: &Rc<RefCell<Weak<NativeAppRuntime>>>,
) -> BTreeMap<String, PluginDependencies> {
    let mut dependencies: BTreeMap<String, PluginDependencies> = plan
        .plugin_instances()
        .iter()
        .map(|instance| {
            (
                instance.instance_key().to_owned(),
                PluginDependencies::new(instance.instance_key(), runtime.clone()),
            )
        })
        .collect();
    for binding in plan.capability_bindings() {
        dependencies
            .get_mut(binding.consumer_instance())
            .expect("every resolved binding consumer has Plugin dependencies")
            .bindings
            .push(PluginDependency::new(
                binding.capability_id(),
                binding.provider_instance(),
                binding.provider_order(),
                endpoints
                    .iter()
                    .find(|((consumer, capability), _)| {
                        consumer == binding.consumer_instance()
                            && *capability == binding.capability_id()
                    })
                    .and_then(|(_, endpoints)| endpoints.get(binding.provider_order()))
                    .map(|endpoint| PluginDependencyHandle {
                        binding: endpoint.clone(),
                        caller_instance: binding.consumer_instance().to_owned(),
                        runtime: runtime.clone(),
                    }),
                stream_endpoints
                    .iter()
                    .find(|((consumer, capability), _)| {
                        consumer == binding.consumer_instance()
                            && *capability == binding.capability_id()
                    })
                    .and_then(|(_, endpoints)| endpoints.get(binding.provider_order()))
                    .map(|endpoint| PluginStreamDependencyHandle {
                        binding: endpoint.clone(),
                        caller_instance: binding.consumer_instance().to_owned(),
                        runtime: runtime.clone(),
                    }),
                event_endpoints
                    .iter()
                    .find(|((consumer, capability), _)| {
                        consumer == binding.consumer_instance()
                            && *capability == binding.capability_id()
                    })
                    .and_then(|(_, endpoints)| endpoints.get(binding.provider_order()))
                    .map(|endpoint| PluginEventDependencyHandle {
                        binding: endpoint.clone(),
                        caller_instance: binding.consumer_instance().to_owned(),
                        runtime: runtime.clone(),
                    }),
            ));
    }
    dependencies
}
