use super::{
    ActivateContext, AppAdmission, BTreeMap, DeactivateContext, DeactivationReason, Duration,
    Either, FutureExt, GenerationPreparationFailure, ManagedResourceScope, ManagedTask,
    ManagedTaskScope, ModuleDependencies, ModuleLifecycle, ModuleSupervision, NativeAppRuntime,
    NativeEventEndpoint, NativeModuleGeneration, NativeModuleRuntime, NativeRequestEndpoint,
    NativeStreamEndpoint, PrepareContext, Rc, ResolvedAppPlan, RestartMode, RuntimeFailure,
    ShutdownOutcome, attach_managed_task_failure_handler, select, wait_until,
};

pub(super) async fn deactivate_in_reverse(
    modules: &BTreeMap<String, NativeModuleRuntime>,
    dependencies: &BTreeMap<String, ModuleDependencies>,
    activation_order: &[String],
    reason: DeactivationReason,
    admission: &AppAdmission,
) -> Option<RuntimeFailure> {
    let mut first_error = None;
    for instance_key in activation_order.iter().rev() {
        let module = modules
            .get(instance_key)
            .expect("deactivation order only contains planned Module Instances");
        let Some(generation) = module.take_generation() else {
            continue;
        };
        if let Some(error) = cleanup_generation(
            instance_key,
            generation,
            dependencies.get(instance_key).cloned().unwrap_or_default(),
            reason,
            admission.clone(),
        )
        .await
            && first_error.is_none()
        {
            first_error = Some(error);
        }
    }
    first_error
}

pub(super) async fn shutdown_native_modules(
    runtime: &NativeAppRuntime,
    timeout: Duration,
) -> ShutdownOutcome {
    let deadline = (runtime.driver.now)().saturating_add(timeout);
    (runtime.driver.yield_now)().await;

    if !drain_supervision_until(runtime, deadline).await {
        terminate_remaining_cleanup(runtime);
        return ShutdownOutcome::Timeout;
    }

    for module in runtime.modules.values() {
        let Some((_, tasks, _)) = module.generation_parts() else {
            continue;
        };
        if !tasks.drain_until(&runtime.driver, deadline).await {
            terminate_remaining_cleanup(runtime);
            return ShutdownOutcome::Timeout;
        }
    }

    let mut first_error = None;
    for instance_key in runtime.activation_order.iter().rev() {
        let module = runtime
            .modules
            .get(instance_key)
            .expect("deactivation order only contains planned Module Instances");
        let Some((lifecycle, tasks, resources)) = module.generation_parts() else {
            continue;
        };
        let cancellation = tasks.cancellation();
        let result = wait_until(
            &runtime.driver,
            deadline,
            lifecycle.deactivate(DeactivateContext {
                instance_key: instance_key.clone(),
                dependencies: runtime
                    .dependencies
                    .get(instance_key)
                    .cloned()
                    .unwrap_or_default(),
                reason: DeactivationReason::Shutdown,
                tasks,
                resources: resources.clone(),
                cancellation,
                admission: runtime.admission.clone(),
            }),
        )
        .await;
        match result {
            Some(Ok(())) => {}
            Some(Err(error)) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
            None => {
                terminate_remaining_cleanup(runtime);
                return ShutdownOutcome::Timeout;
            }
        }

        match resources.release_all_until(&runtime.driver, deadline).await {
            Ok(Some(error)) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
            Ok(None) => {}
            Err(()) => {
                terminate_remaining_cleanup(runtime);
                return ShutdownOutcome::Timeout;
            }
        }
    }

    first_error.map_or(ShutdownOutcome::Clean, |error| {
        ShutdownOutcome::RuntimeFailure { error }
    })
}

pub(super) fn terminate_remaining_cleanup(runtime: &NativeAppRuntime) {
    for module in runtime.modules.values() {
        if let Some((_, tasks, _)) = module.generation_parts() {
            tasks.abort_all();
        }
    }
    for task in runtime.supervision_tasks.borrow().values() {
        task.cancel();
    }
}

pub(super) async fn drain_supervision_until(
    runtime: &NativeAppRuntime,
    deadline: Duration,
) -> bool {
    let tasks = std::mem::take(&mut *runtime.supervision_tasks.borrow_mut());
    for (index, task) in tasks.values().enumerate() {
        if wait_until(&runtime.driver, deadline, task.join())
            .await
            .is_none()
        {
            for pending in tasks.values().skip(index) {
                pending.cancel();
            }
            return false;
        }
    }
    true
}

pub(super) fn module_supervision(plan: &ResolvedAppPlan) -> BTreeMap<String, ModuleSupervision> {
    plan.module_instances()
        .iter()
        .map(|instance| {
            (
                instance.instance_key().to_owned(),
                ModuleSupervision {
                    policy: instance.restart_policy(),
                    criticality: instance.criticality(),
                    required_path: plan.module_instance_is_required(instance.instance_key()),
                    generation: 1,
                    attempts: Vec::new(),
                    stable_since: Some(Duration::ZERO),
                    restarting: false,
                },
            )
        })
        .collect()
}

pub(super) fn begin_module_supervision(
    runtime: &Rc<NativeAppRuntime>,
    instance_key: &str,
) -> Result<bool, RuntimeFailure> {
    if runtime.shutdown_started.get() || runtime.terminal_failure.borrow().is_some() {
        return Err(RuntimeFailure::AdmissionClosed);
    }
    if !runtime.modules.contains_key(instance_key) {
        return Err(RuntimeFailure::InvalidResolvedPlan {
            detail: format!("unknown Module Instance `{instance_key}`"),
        });
    }
    let mut supervision = runtime.supervision.borrow_mut();
    let state = supervision
        .get_mut(instance_key)
        .expect("every planned Module Instance has supervision state");
    if state.restarting {
        return Ok(false);
    }
    let now = (runtime.driver.now)();
    if state.stable_since.is_some_and(|stable_since| {
        !state.policy.stability().is_zero()
            && now.saturating_sub(stable_since) >= state.policy.stability()
    }) {
        state.attempts.clear();
    } else {
        state
            .attempts
            .retain(|attempted_at| now.saturating_sub(*attempted_at) < state.policy.window());
    }
    state.stable_since = None;
    state.restarting = true;
    drop(supervision);
    for ((provider, _), endpoint) in &runtime.endpoint_states {
        if provider == instance_key {
            endpoint.mark_unavailable();
        }
    }
    for ((provider, _), endpoint) in &runtime.stream_endpoint_states {
        if provider == instance_key {
            endpoint.mark_unavailable();
        }
    }
    for ((provider, _), endpoint) in &runtime.event_endpoint_states {
        if provider == instance_key {
            endpoint.mark_unavailable();
        }
    }
    Ok(true)
}

pub(super) fn schedule_module_supervision(
    runtime: &Rc<NativeAppRuntime>,
    instance_key: &str,
) -> Result<(), RuntimeFailure> {
    let task_runtime = runtime.clone();
    let task_instance_key = instance_key.to_owned();
    let task = (runtime.driver.spawn_local)(Box::pin(async move {
        let _ = supervise_module_instance(task_runtime, task_instance_key).await;
    }))
    .map_err(|error| {
        if let Some(state) = runtime.supervision.borrow_mut().get_mut(instance_key) {
            state.restarting = false;
        }
        RuntimeFailure::Internal {
            detail: format!("failed to schedule Module supervision: {error:?}"),
        }
    })?;
    runtime
        .supervision_tasks
        .borrow_mut()
        .insert(instance_key.to_owned(), ManagedTask::from_driver_task(task));
    Ok(())
}

pub(super) async fn supervise_module_instance(
    runtime: Rc<NativeAppRuntime>,
    instance_key: String,
) -> Result<(), RuntimeFailure> {
    if runtime.shutdown_started.get() {
        return Err(RuntimeFailure::AdmissionClosed);
    }
    let generation = runtime
        .modules
        .get(&instance_key)
        .and_then(NativeModuleRuntime::take_generation);
    if let Some(generation) = generation
        && let Some(error) = cleanup_native_generation(
            &runtime,
            &instance_key,
            generation,
            DeactivationReason::SupervisionRestart,
        )
        .await
    {
        return finish_module_cleanup_failure(&runtime, &instance_key, error);
    }

    loop {
        let Some((_attempt, delay)) = next_restart_attempt(&runtime, &instance_key) else {
            return finish_module_exhaustion(&runtime, &instance_key);
        };
        if !wait_for_supervision_delay(&runtime, delay).await {
            return Err(RuntimeFailure::AdmissionClosed);
        }
        if runtime.shutdown_started.get() {
            return Err(RuntimeFailure::AdmissionClosed);
        }

        let Some(instance) = runtime
            .plan
            .module_instances()
            .iter()
            .find(|instance| instance.instance_key() == instance_key)
        else {
            return Err(RuntimeFailure::InvalidResolvedPlan {
                detail: format!("unknown Module Instance `{instance_key}`"),
            });
        };
        let Some(adapter) = runtime.adapters.adapter(instance.execution_class()) else {
            return Err(RuntimeFailure::UnavailableExecutionClass {
                instance_key: instance_key.clone(),
                execution_class: instance.execution_class().to_string(),
            });
        };
        let Ok(prepared) = adapter.recreate(&runtime.plan, &instance_key) else {
            continue;
        };
        let (endpoints, stream_endpoints, event_endpoints, lifecycle) = prepared.into_parts();
        if validate_native_endpoint_set(
            &instance_key,
            instance,
            &endpoints,
            &stream_endpoints,
            &event_endpoints,
        )
        .is_err()
        {
            continue;
        }
        let generation_number = runtime
            .supervision
            .borrow()
            .get(&instance_key)
            .map_or(1, |state| state.generation.saturating_add(1));
        let generation =
            match prepare_and_activate_generation(&runtime, &instance_key, lifecycle).await {
                Ok(generation) => generation,
                Err(GenerationPreparationFailure::Lifecycle) => continue,
                Err(GenerationPreparationFailure::Cleanup(error)) => {
                    return finish_module_cleanup_failure(&runtime, &instance_key, error);
                }
            };

        if runtime.shutdown_started.get() {
            let _ = cleanup_native_generation(
                &runtime,
                &instance_key,
                generation,
                DeactivationReason::SupervisionRestart,
            )
            .await;
            return Err(RuntimeFailure::AdmissionClosed);
        }

        if let Some(module) = runtime.modules.get(&instance_key) {
            module.install_generation(generation);
        }
        install_module_endpoints(
            &runtime,
            &instance_key,
            endpoints,
            stream_endpoints,
            event_endpoints,
            generation_number,
        );
        if let Some(state) = runtime.supervision.borrow_mut().get_mut(&instance_key) {
            state.generation = generation_number;
            state.stable_since = Some((runtime.driver.now)());
            state.restarting = false;
        }
        return Ok(());
    }
}

pub(super) async fn wait_for_supervision_delay(
    runtime: &NativeAppRuntime,
    delay: Duration,
) -> bool {
    if delay.is_zero() {
        return true;
    }
    let deadline = (runtime.driver.now)().saturating_add(delay);
    let timer = (runtime.driver.sleep_until)(deadline).fuse();
    let cancellation = runtime.supervision_cancellation.cancelled().fuse();
    futures::pin_mut!(timer, cancellation);
    matches!(select(timer, cancellation).await, Either::Left(((), _)))
}

pub(super) fn next_restart_attempt(
    runtime: &NativeAppRuntime,
    instance_key: &str,
) -> Option<(usize, Duration)> {
    let now = (runtime.driver.now)();
    let mut supervision = runtime.supervision.borrow_mut();
    let state = supervision
        .get_mut(instance_key)
        .expect("every planned Module Instance has supervision state");
    if state.policy.mode() != RestartMode::OnFailure {
        return None;
    }
    // Keep all attempts from this supervision episode together. Pruning here
    // would turn a backoff longer than the rolling window into an unbounded
    // restart loop.
    if state.attempts.len() >= state.policy.max_attempts() {
        return None;
    }
    let attempt = state.attempts.len().saturating_add(1);
    state.attempts.push(now);
    let exponent = u32::try_from(attempt.saturating_sub(1).min(31)).unwrap_or(31);
    let backoff = state.policy.backoff().saturating_mul(1_u32 << exponent);
    let jitter = (runtime.driver.jitter)(state.policy.jitter()).min(state.policy.jitter());
    Some((attempt, backoff.saturating_add(jitter)))
}

pub(super) fn finish_module_exhaustion(
    runtime: &Rc<NativeAppRuntime>,
    instance_key: &str,
) -> Result<(), RuntimeFailure> {
    let (attempts, must_fail) = {
        let mut supervision = runtime.supervision.borrow_mut();
        let state = supervision
            .get_mut(instance_key)
            .expect("every planned Module Instance has supervision state");
        state.restarting = false;
        (
            state.attempts.len(),
            state.criticality.is_critical() || state.required_path,
        )
    };
    if !must_fail {
        return Ok(());
    }
    let error = RuntimeFailure::ModuleRestartExhausted {
        instance: instance_key.to_owned(),
        attempts,
    };
    runtime.terminal_failure.replace(Some(error.clone()));
    runtime.begin_shutdown();
    Err(error)
}

pub(super) fn finish_module_cleanup_failure(
    runtime: &Rc<NativeAppRuntime>,
    instance_key: &str,
    error: RuntimeFailure,
) -> Result<(), RuntimeFailure> {
    let must_fail = {
        let mut supervision = runtime.supervision.borrow_mut();
        let state = supervision
            .get_mut(instance_key)
            .expect("every planned Module Instance has supervision state");
        state.restarting = false;
        state.criticality.is_critical() || state.required_path
    };
    if must_fail {
        runtime.terminal_failure.replace(Some(error.clone()));
        runtime.begin_shutdown();
    }
    Err(error)
}

pub(super) async fn prepare_and_activate_generation(
    runtime: &Rc<NativeAppRuntime>,
    instance_key: &str,
    lifecycle: Rc<dyn ModuleLifecycle>,
) -> Result<NativeModuleGeneration, GenerationPreparationFailure> {
    let tasks = ManagedTaskScope::new_from_driver_control(&runtime.driver);
    attach_managed_task_failure_handler(runtime, instance_key, &tasks);
    let resources = ManagedResourceScope::new();
    let prepared = NativeModuleGeneration {
        lifecycle: lifecycle.clone(),
        tasks: tasks.clone(),
        resources: resources.clone(),
    };
    let dependencies = runtime
        .dependencies
        .get(instance_key)
        .cloned()
        .unwrap_or_default();
    let instance = runtime
        .plan
        .module_instances()
        .iter()
        .find(|instance| instance.instance_key() == instance_key)
        .expect("supervision only recreates planned Module Instances");
    if lifecycle
        .prepare(PrepareContext {
            instance_key: instance_key.to_owned(),
            entrypoint: instance.entrypoint().to_owned(),
            configuration: instance.configuration().to_owned(),
            dependencies: dependencies.clone(),
            resources: resources.clone(),
            cancellation: tasks.cancellation(),
            admission: runtime.admission.clone(),
        })
        .await
        .is_err()
    {
        let failure = if let Some(cleanup_error) = cleanup_native_generation(
            runtime,
            instance_key,
            prepared,
            DeactivationReason::SupervisionRestart,
        )
        .await
        {
            GenerationPreparationFailure::Cleanup(cleanup_error)
        } else {
            GenerationPreparationFailure::Lifecycle
        };
        return Err(failure);
    }
    if lifecycle
        .activate(ActivateContext {
            instance_key: instance_key.to_owned(),
            dependencies,
            ready_gate: runtime.ready_gate.clone(),
            tasks: tasks.clone(),
            resources: resources.clone(),
            cancellation: tasks.cancellation(),
            admission: runtime.admission.clone(),
        })
        .await
        .is_err()
    {
        let failure = if let Some(cleanup_error) = cleanup_native_generation(
            runtime,
            instance_key,
            prepared,
            DeactivationReason::SupervisionRestart,
        )
        .await
        {
            GenerationPreparationFailure::Cleanup(cleanup_error)
        } else {
            GenerationPreparationFailure::Lifecycle
        };
        return Err(failure);
    }
    Ok(prepared)
}

pub(super) async fn cleanup_native_generation(
    runtime: &NativeAppRuntime,
    instance_key: &str,
    generation: NativeModuleGeneration,
    reason: DeactivationReason,
) -> Option<RuntimeFailure> {
    let dependencies = runtime
        .dependencies
        .get(instance_key)
        .cloned()
        .unwrap_or_default();
    cleanup_generation(
        instance_key,
        generation,
        dependencies,
        reason,
        runtime.admission.clone(),
    )
    .await
}

pub(super) async fn cleanup_generation(
    instance_key: &str,
    generation: NativeModuleGeneration,
    dependencies: ModuleDependencies,
    reason: DeactivationReason,
    admission: AppAdmission,
) -> Option<RuntimeFailure> {
    generation.tasks.close();
    generation.resources.close();
    generation.tasks.cancel_all().await;
    let mut first_error = None;
    if let Err(error) = generation
        .lifecycle
        .deactivate(DeactivateContext {
            instance_key: instance_key.to_owned(),
            dependencies,
            reason,
            tasks: generation.tasks.clone(),
            resources: generation.resources.clone(),
            cancellation: generation.tasks.cancellation(),
            admission,
        })
        .await
    {
        first_error = Some(error);
    }
    if let Some(error) = generation.resources.release_all().await
        && first_error.is_none()
    {
        first_error = Some(error);
    }
    first_error
}

pub(super) fn install_module_endpoints(
    runtime: &NativeAppRuntime,
    instance_key: &str,
    endpoints: Vec<Rc<dyn NativeRequestEndpoint>>,
    stream_endpoints: Vec<Rc<dyn NativeStreamEndpoint>>,
    event_endpoints: Vec<Rc<dyn NativeEventEndpoint>>,
    generation: u64,
) {
    for endpoint in endpoints {
        if let Some(state) = runtime
            .endpoint_states
            .get(&(instance_key.to_owned(), endpoint.capability_id().to_owned()))
        {
            state.install(endpoint, generation);
        }
    }
    for endpoint in stream_endpoints {
        if let Some(state) = runtime
            .stream_endpoint_states
            .get(&(instance_key.to_owned(), endpoint.capability_id().to_owned()))
        {
            state.install(endpoint, generation);
        }
    }
    for endpoint in event_endpoints {
        if let Some(state) = runtime
            .event_endpoint_states
            .get(&(instance_key.to_owned(), endpoint.capability_id().to_owned()))
        {
            state.install(endpoint, generation);
        }
    }
}

pub(super) fn validate_native_endpoint_set(
    instance_key: &str,
    expected: &lenso_app_plan::ModuleInstancePlan,
    actual: &[Rc<dyn NativeRequestEndpoint>],
    actual_streams: &[Rc<dyn NativeStreamEndpoint>],
    actual_events: &[Rc<dyn NativeEventEndpoint>],
) -> Result<(), RuntimeFailure> {
    let expected_requests = expected
        .provided_capabilities()
        .iter()
        .filter(|descriptor| !descriptor.request_operations().is_empty())
        .count();
    let expected_streams = expected
        .provided_capabilities()
        .iter()
        .filter(|descriptor| !descriptor.stream_operations().is_empty())
        .count();
    let expected_events = expected
        .provided_capabilities()
        .iter()
        .filter(|descriptor| !descriptor.event_operations().is_empty())
        .count();
    if expected_requests != actual.len()
        || expected_streams != actual_streams.len()
        || expected_events != actual_events.len()
    {
        return Err(RuntimeFailure::InvalidResolvedPlan {
            detail: format!(
                "Module Instance `{instance_key}` prepared {} request, {} stream, and {} Event endpoints; expected {} request, {} stream, and {} Event endpoints",
                actual.len(),
                actual_streams.len(),
                actual_events.len(),
                expected_requests,
                expected_streams,
                expected_events
            ),
        });
    }
    for descriptor in expected.provided_capabilities() {
        let request_operations = descriptor.request_operations();
        if !request_operations.is_empty() {
            let matching: Vec<_> = actual
                .iter()
                .filter(|endpoint| endpoint.capability_id() == descriptor.capability_id())
                .collect();
            if matching.len() != 1 {
                return Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!(
                        "Module Instance `{instance_key}` prepared {} request endpoints for Capability `{}`",
                        matching.len(),
                        descriptor.capability_id()
                    ),
                });
            }
            validate_endpoint_operations(
                instance_key,
                descriptor.capability_id(),
                descriptor.descriptor_version(),
                &request_operations,
                matching[0].descriptor_version(),
                matching[0].operations(),
            )?;
        }
        let stream_operations = descriptor.stream_operations();
        if !stream_operations.is_empty() {
            let matching: Vec<_> = actual_streams
                .iter()
                .filter(|endpoint| endpoint.capability_id() == descriptor.capability_id())
                .collect();
            if matching.len() != 1 {
                return Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!(
                        "Module Instance `{instance_key}` prepared {} stream endpoints for Capability `{}`",
                        matching.len(),
                        descriptor.capability_id()
                    ),
                });
            }
            validate_endpoint_operations(
                instance_key,
                descriptor.capability_id(),
                descriptor.descriptor_version(),
                &stream_operations,
                matching[0].descriptor_version(),
                matching[0].operations(),
            )?;
        }
        let event_operations = descriptor.event_operations();
        if !event_operations.is_empty() {
            let matching: Vec<_> = actual_events
                .iter()
                .filter(|endpoint| endpoint.capability_id() == descriptor.capability_id())
                .collect();
            if matching.len() != 1 {
                return Err(RuntimeFailure::InvalidResolvedPlan {
                    detail: format!(
                        "Module Instance `{instance_key}` prepared {} Event endpoints for Capability `{}`",
                        matching.len(),
                        descriptor.capability_id()
                    ),
                });
            }
            validate_endpoint_operations(
                instance_key,
                descriptor.capability_id(),
                descriptor.descriptor_version(),
                &event_operations,
                matching[0].descriptor_version(),
                matching[0].operations(),
            )?;
        }
    }
    Ok(())
}

pub(super) fn validate_endpoint_operations(
    instance_key: &str,
    capability_id: &str,
    expected_version: &str,
    expected_operations: &[&str],
    actual_version: &str,
    actual_operations: &[&str],
) -> Result<(), RuntimeFailure> {
    if actual_version != expected_version || actual_operations != expected_operations {
        return Err(RuntimeFailure::InvalidResolvedPlan {
            detail: format!(
                "Module Instance `{instance_key}` endpoint `{capability_id}` differs from its resolved Descriptor"
            ),
        });
    }
    let mut unique = actual_operations.to_vec();
    unique.sort_unstable();
    unique.dedup();
    if unique.len() != actual_operations.len() {
        return Err(RuntimeFailure::InvalidResolvedPlan {
            detail: format!(
                "Module Instance `{instance_key}` endpoint `{capability_id}` has duplicate Operations"
            ),
        });
    }
    Ok(())
}
