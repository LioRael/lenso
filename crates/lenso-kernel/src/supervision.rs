use super::{
    ActivateContext, AppAdmission, BTreeMap, DeactivateContext, DeactivationReason,
    DiagnosticEvent, DiagnosticSource, Duration, Either, FutureExt, GenerationPreparationFailure,
    ManagedResourceScope, ManagedTask, ManagedTaskScope, NativeAppRuntime, NativeEventEndpoint,
    NativePluginGeneration, NativePluginRuntime, NativeRequestEndpoint, NativeStreamEndpoint,
    PluginDependencies, PluginLifecycle, PluginSupervision, PrepareContext, Rc, ResolvedAppPlan,
    RestartMode, RuntimeDiagnostics, RuntimeFailure, ShutdownOutcome,
    attach_managed_task_failure_handler, select, wait_until,
};

#[allow(
    clippy::too_many_arguments,
    reason = "startup rollback keeps every generation-owned cleanup input explicit"
)]
pub(super) async fn deactivate_in_reverse(
    plugins: &BTreeMap<String, NativePluginRuntime>,
    dependencies: &BTreeMap<String, PluginDependencies>,
    activation_order: &[String],
    reason: DeactivationReason,
    admission: &AppAdmission,
    diagnostics: &RuntimeDiagnostics,
    driver: &super::DriverControl,
    budget: Option<super::cleanup::CleanupBudget>,
) -> Option<RuntimeFailure> {
    let mut first_error = None;
    for instance_key in activation_order.iter().rev() {
        let plugin = plugins
            .get(instance_key)
            .expect("deactivation order only contains planned Plugin Instances");
        let Some(generation) = plugin.take_generation() else {
            continue;
        };
        match cleanup_generation(
            instance_key,
            generation,
            dependencies.get(instance_key).cloned().unwrap_or_default(),
            reason,
            admission.clone(),
            1,
            diagnostics,
            driver,
            budget.clone(),
        )
        .await
        {
            GenerationCleanupOutcome::Complete(Some(error)) if first_error.is_none() => {
                first_error = Some(error);
            }
            GenerationCleanupOutcome::TimedOut(generation) => {
                plugin.install_generation(generation);
                if first_error.is_none() {
                    first_error = Some(cleanup_timeout_failure());
                }
            }
            GenerationCleanupOutcome::Complete(_) => {}
        }
    }
    first_error
}

#[allow(
    clippy::too_many_lines,
    reason = "shutdown keeps one global deadline and reverse-order cleanup in one sequence"
)]
pub(super) async fn shutdown_native_plugins(
    runtime: &NativeAppRuntime,
    budget: super::cleanup::CleanupBudget,
) -> ShutdownOutcome {
    let deadline = budget.deadline();
    (runtime.driver.yield_now)().await;

    if !drain_supervision_until(runtime, deadline).await {
        terminate_remaining_cleanup(runtime);
        return ShutdownOutcome::Timeout;
    }

    // Caller cancellation and Adapter acknowledgement do not release execution
    // ownership. Never run stop against resources still reachable by old work.
    while !runtime.executions.is_settled(None) {
        if (runtime.driver.now)() >= deadline {
            return ShutdownOutcome::Timeout;
        }
        (runtime.driver.yield_now)().await;
    }

    for plugin in runtime.plugins.values() {
        let Some((_, tasks, _)) = plugin.generation_parts() else {
            continue;
        };
        if !tasks.drain_until(&runtime.driver, deadline).await {
            terminate_remaining_cleanup(runtime);
            return ShutdownOutcome::Timeout;
        }
    }

    let mut first_error = None;
    for instance_key in runtime.activation_order.iter().rev() {
        let plugin = runtime
            .plugins
            .get(instance_key)
            .expect("deactivation order only contains planned Plugin Instances");
        let Some((lifecycle, tasks, resources)) = plugin.generation_parts() else {
            continue;
        };
        let cancellation = tasks.cancellation();
        let generation = runtime
            .supervision
            .borrow()
            .get(instance_key)
            .map_or(1, |state| state.generation);
        let started_at = (runtime.driver.now)();
        runtime
            .diagnostics
            .emit(DiagnosticSource::Lifecycle, started_at, |_| {
                DiagnosticEvent::LifecycleStarted {
                    instance: instance_key.clone(),
                    generation,
                    phase: super::PluginLifecyclePhase::Deactivate,
                }
            });
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
                cleanup: Some(budget.clone()),
            }),
        )
        .await;
        let outcome = match &result {
            Some(Ok(())) => super::DiagnosticOutcome::Succeeded,
            Some(Err(error)) => super::DiagnosticOutcome::RuntimeFailure(error.into()),
            None => super::DiagnosticOutcome::RuntimeFailure(
                super::RuntimeFailureKind::DeadlineExceeded,
            ),
        };
        runtime
            .diagnostics
            .emit(DiagnosticSource::Lifecycle, (runtime.driver.now)(), |_| {
                DiagnosticEvent::LifecycleCompleted {
                    instance: instance_key.clone(),
                    generation,
                    phase: super::PluginLifecyclePhase::Deactivate,
                    outcome,
                    elapsed: (runtime.driver.now)().saturating_sub(started_at),
                }
            });
        match result {
            Some(Ok(())) => {}
            Some(Err(error)) => {
                runtime.diagnostics.emit_runtime_failure(
                    (runtime.driver.now)(),
                    Some(instance_key),
                    &error,
                );
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
                runtime.diagnostics.emit_runtime_failure(
                    (runtime.driver.now)(),
                    Some(instance_key),
                    &error,
                );
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
    for plugin in runtime.plugins.values() {
        if let Some((_, tasks, _)) = plugin.generation_parts() {
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

pub(super) fn plugin_supervision(plan: &ResolvedAppPlan) -> BTreeMap<String, PluginSupervision> {
    plan.plugin_instances()
        .iter()
        .map(|instance| {
            (
                instance.instance_key().to_owned(),
                PluginSupervision {
                    policy: instance.restart_policy(),
                    criticality: instance.criticality(),
                    required_path: plan.plugin_instance_is_terminal(instance.instance_key()),
                    generation: 1,
                    attempts: Vec::new(),
                    stable_since: Some(Duration::ZERO),
                    restarting: false,
                },
            )
        })
        .collect()
}

pub(super) fn begin_plugin_supervision(
    runtime: &Rc<NativeAppRuntime>,
    instance_key: &str,
) -> Result<bool, RuntimeFailure> {
    if runtime.shutdown_started.get() || runtime.terminal_failure.borrow().is_some() {
        return Err(RuntimeFailure::AdmissionClosed);
    }
    if !runtime.plugins.contains_key(instance_key) {
        return Err(RuntimeFailure::InvalidResolvedPlan {
            detail: format!("unknown Plugin Instance `{instance_key}`"),
        });
    }
    let mut supervision = runtime.supervision.borrow_mut();
    let state = supervision
        .get_mut(instance_key)
        .expect("every planned Plugin Instance has supervision state");
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
    let generation = state.generation;
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
    runtime.diagnostics.emit(
        DiagnosticSource::Supervision,
        (runtime.driver.now)(),
        |_| DiagnosticEvent::GenerationUnavailable {
            instance: instance_key.to_owned(),
            generation,
        },
    );
    Ok(true)
}

pub(super) fn schedule_plugin_supervision(
    runtime: &Rc<NativeAppRuntime>,
    instance_key: &str,
) -> Result<(), RuntimeFailure> {
    let task_runtime = runtime.clone();
    let task_instance_key = instance_key.to_owned();
    let task = (runtime.driver.spawn_local)(Box::pin(async move {
        let _ = supervise_plugin_instance(task_runtime, task_instance_key).await;
    }))
    .map_err(|error| {
        if let Some(state) = runtime.supervision.borrow_mut().get_mut(instance_key) {
            state.restarting = false;
        }
        RuntimeFailure::Internal {
            detail: format!("failed to schedule Plugin supervision: {error:?}"),
        }
    })?;
    runtime
        .supervision_tasks
        .borrow_mut()
        .insert(instance_key.to_owned(), ManagedTask::from_driver_task(task));
    Ok(())
}

#[allow(
    clippy::too_many_lines,
    reason = "the supervision generation transition remains linear and auditable"
)]
pub(super) async fn supervise_plugin_instance(
    runtime: Rc<NativeAppRuntime>,
    instance_key: String,
) -> Result<(), RuntimeFailure> {
    if runtime.shutdown_started.get() {
        return Err(RuntimeFailure::AdmissionClosed);
    }
    let current_generation = runtime
        .supervision
        .borrow()
        .get(&instance_key)
        .map_or(1, |state| state.generation);
    let cleanup_budget = runtime
        .cleanup_timeout
        .map(|timeout| super::cleanup::CleanupBudget::after(&runtime.driver, timeout));
    while !runtime.executions.is_settled(Some(&instance_key)) {
        if runtime.shutdown_started.get() {
            return Err(RuntimeFailure::AdmissionClosed);
        }
        if cleanup_budget
            .as_ref()
            .is_some_and(|budget| budget.remaining().is_zero())
        {
            return finish_plugin_cleanup_failure(
                &runtime,
                &instance_key,
                cleanup_timeout_failure(),
            );
        }
        (runtime.driver.yield_now)().await;
    }
    let generation = runtime
        .plugins
        .get(&instance_key)
        .and_then(NativePluginRuntime::take_generation);
    if let Some(generation) = generation
        && let Some(error) = cleanup_native_generation_with_budget(
            &runtime,
            &instance_key,
            generation,
            DeactivationReason::SupervisionRestart,
            current_generation,
            cleanup_budget,
        )
        .await
    {
        return finish_plugin_cleanup_failure(&runtime, &instance_key, error);
    }

    loop {
        let Some((attempt, delay)) = next_restart_attempt(&runtime, &instance_key) else {
            return finish_plugin_exhaustion(&runtime, &instance_key);
        };
        runtime.diagnostics.emit(
            DiagnosticSource::Supervision,
            (runtime.driver.now)(),
            |_| DiagnosticEvent::RestartScheduled {
                instance: instance_key.clone(),
                attempt,
                delay,
            },
        );
        if !wait_for_supervision_delay(&runtime, delay).await {
            return Err(RuntimeFailure::AdmissionClosed);
        }
        if runtime.shutdown_started.get() {
            return Err(RuntimeFailure::AdmissionClosed);
        }

        let Some(instance) = runtime
            .plan
            .plugin_instances()
            .iter()
            .find(|instance| instance.instance_key() == instance_key)
        else {
            return Err(RuntimeFailure::InvalidResolvedPlan {
                detail: format!("unknown Plugin Instance `{instance_key}`"),
            });
        };
        let Some(adapter) = runtime.adapters.adapter(instance.execution_class()) else {
            let error = RuntimeFailure::UnavailableExecutionClass {
                instance_key: instance_key.clone(),
                execution_class: instance.execution_class().to_string(),
            };
            runtime.diagnostics.emit_runtime_failure(
                (runtime.driver.now)(),
                Some(&instance_key),
                &error,
            );
            return Err(error);
        };
        let prepared = match adapter.recreate(&runtime.plan, &instance_key) {
            Ok(prepared) => prepared,
            Err(error) => {
                runtime.diagnostics.emit_runtime_failure(
                    (runtime.driver.now)(),
                    Some(&instance_key),
                    &error,
                );
                continue;
            }
        };
        let (endpoints, lifecycle) = prepared.into_parts();
        if let Err(error) = validate_native_endpoint_set(
            &instance_key,
            instance,
            endpoints.request(),
            endpoints.stream(),
            endpoints.event(),
        ) {
            runtime.diagnostics.emit_runtime_failure(
                (runtime.driver.now)(),
                Some(&instance_key),
                &error,
            );
            continue;
        }
        let generation_number = runtime
            .supervision
            .borrow()
            .get(&instance_key)
            .map_or(1, |state| state.generation.saturating_add(1));
        let generation = match prepare_and_activate_generation(
            &runtime,
            &instance_key,
            lifecycle,
            generation_number,
        )
        .await
        {
            Ok(generation) => generation,
            Err(GenerationPreparationFailure::Lifecycle) => continue,
            Err(GenerationPreparationFailure::Cleanup { primary }) => {
                return finish_plugin_cleanup_failure(&runtime, &instance_key, primary);
            }
        };

        if runtime.shutdown_started.get() {
            let _ = cleanup_native_generation(
                &runtime,
                &instance_key,
                generation,
                DeactivationReason::SupervisionRestart,
                generation_number,
            )
            .await;
            return Err(RuntimeFailure::AdmissionClosed);
        }

        if let Some(plugin) = runtime.plugins.get(&instance_key) {
            plugin.install_generation(generation);
        }
        install_plugin_endpoints(
            &runtime,
            &instance_key,
            endpoints.request().to_vec(),
            endpoints.stream().to_vec(),
            endpoints.event().to_vec(),
            generation_number,
        );
        if let Some(state) = runtime.supervision.borrow_mut().get_mut(&instance_key) {
            state.generation = generation_number;
            state.stable_since = Some((runtime.driver.now)());
            state.restarting = false;
        }
        runtime.diagnostics.emit(
            DiagnosticSource::Supervision,
            (runtime.driver.now)(),
            |_| DiagnosticEvent::GenerationReady {
                instance: instance_key.clone(),
                generation: generation_number,
            },
        );
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
        .expect("every planned Plugin Instance has supervision state");
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

pub(super) fn finish_plugin_exhaustion(
    runtime: &Rc<NativeAppRuntime>,
    instance_key: &str,
) -> Result<(), RuntimeFailure> {
    let (attempts, must_fail) = {
        let mut supervision = runtime.supervision.borrow_mut();
        let state = supervision
            .get_mut(instance_key)
            .expect("every planned Plugin Instance has supervision state");
        state.restarting = false;
        (
            state.attempts.len(),
            state.criticality.is_critical() || state.required_path,
        )
    };
    if !must_fail {
        runtime.diagnostics.emit(
            DiagnosticSource::Supervision,
            (runtime.driver.now)(),
            |_| DiagnosticEvent::RestartExhausted {
                instance: instance_key.to_owned(),
                attempts,
                terminal: false,
            },
        );
        return Ok(());
    }
    let error = RuntimeFailure::PluginRestartExhausted {
        instance: instance_key.to_owned(),
        attempts,
    };
    runtime.terminal_failure.replace(Some(error.clone()));
    runtime.begin_shutdown();
    runtime.diagnostics.emit(
        DiagnosticSource::Supervision,
        (runtime.driver.now)(),
        |_| DiagnosticEvent::RestartExhausted {
            instance: instance_key.to_owned(),
            attempts,
            terminal: true,
        },
    );
    runtime
        .diagnostics
        .emit_runtime_failure((runtime.driver.now)(), Some(instance_key), &error);
    Err(error)
}

pub(super) fn finish_plugin_cleanup_failure(
    runtime: &Rc<NativeAppRuntime>,
    instance_key: &str,
    error: RuntimeFailure,
) -> Result<(), RuntimeFailure> {
    let must_fail = {
        let mut supervision = runtime.supervision.borrow_mut();
        let state = supervision
            .get_mut(instance_key)
            .expect("every planned Plugin Instance has supervision state");
        state.restarting = false;
        state.criticality.is_critical() || state.required_path
    };
    if must_fail {
        runtime.terminal_failure.replace(Some(error.clone()));
        runtime.begin_shutdown();
    }
    runtime
        .diagnostics
        .emit_runtime_failure((runtime.driver.now)(), Some(instance_key), &error);
    Err(error)
}

#[allow(
    clippy::too_many_lines,
    reason = "prepare, activate, and rollback share one generation transaction"
)]
pub(super) async fn prepare_and_activate_generation(
    runtime: &Rc<NativeAppRuntime>,
    instance_key: &str,
    lifecycle: Rc<dyn PluginLifecycle>,
    generation_number: u64,
) -> Result<NativePluginGeneration, GenerationPreparationFailure> {
    let tasks = ManagedTaskScope::new_from_driver_control(&runtime.driver);
    attach_managed_task_failure_handler(runtime, instance_key, &tasks);
    let resources = ManagedResourceScope::new();
    let prepared = NativePluginGeneration {
        lifecycle: lifecycle.clone(),
        tasks: tasks.clone(),
        resources: resources.clone(),
        stop_attempted: false,
        cleanup_timed_out: false,
    };
    let dependencies = runtime
        .dependencies
        .get(instance_key)
        .cloned()
        .unwrap_or_default();
    let instance = runtime
        .plan
        .plugin_instances()
        .iter()
        .find(|instance| instance.instance_key() == instance_key)
        .expect("supervision only recreates planned Plugin Instances");
    let prepare_started_at = (runtime.driver.now)();
    runtime
        .diagnostics
        .emit(DiagnosticSource::Lifecycle, prepare_started_at, |_| {
            DiagnosticEvent::LifecycleStarted {
                instance: instance_key.to_owned(),
                generation: generation_number,
                phase: super::PluginLifecyclePhase::Prepare,
            }
        });
    let prepare_result = lifecycle
        .prepare(PrepareContext {
            instance_key: instance_key.to_owned(),
            entrypoint: instance.entrypoint().to_owned(),
            configuration: instance.configuration().to_owned(),
            dependencies: dependencies.clone(),
            resources: resources.clone(),
            cancellation: tasks.cancellation(),
            admission: runtime.admission.clone(),
        })
        .await;
    let prepare_outcome = prepare_result.as_ref().map_or_else(
        |error| super::DiagnosticOutcome::RuntimeFailure(error.into()),
        |()| super::DiagnosticOutcome::Succeeded,
    );
    runtime
        .diagnostics
        .emit(DiagnosticSource::Lifecycle, (runtime.driver.now)(), |_| {
            DiagnosticEvent::LifecycleCompleted {
                instance: instance_key.to_owned(),
                generation: generation_number,
                phase: super::PluginLifecyclePhase::Prepare,
                outcome: prepare_outcome,
                elapsed: (runtime.driver.now)().saturating_sub(prepare_started_at),
            }
        });
    if let Err(error) = prepare_result {
        runtime.diagnostics.emit_runtime_failure(
            (runtime.driver.now)(),
            Some(instance_key),
            &error,
        );
        let failure = if cleanup_native_generation(
            runtime,
            instance_key,
            prepared,
            DeactivationReason::SupervisionRestart,
            generation_number,
        )
        .await
        .is_some()
        {
            GenerationPreparationFailure::Cleanup { primary: error }
        } else {
            GenerationPreparationFailure::Lifecycle
        };
        return Err(failure);
    }
    let construct_result = run_construction_phase(
        runtime,
        instance,
        instance_key,
        generation_number,
        lifecycle.clone(),
        dependencies.clone(),
        tasks.clone(),
        resources.clone(),
    )
    .await;
    let activate_started_at = (runtime.driver.now)();
    runtime
        .diagnostics
        .emit(DiagnosticSource::Lifecycle, activate_started_at, |_| {
            DiagnosticEvent::LifecycleStarted {
                instance: instance_key.to_owned(),
                generation: generation_number,
                phase: super::PluginLifecyclePhase::Activate,
            }
        });
    let activate_context = ActivateContext {
        instance_key: instance_key.to_owned(),
        dependencies,
        ready_gate: runtime.ready_gate.clone(),
        tasks: tasks.clone(),
        resources: resources.clone(),
        cancellation: tasks.cancellation(),
        admission: runtime.admission.clone(),
    };
    let activate_result = match construct_result {
        Ok(()) => lifecycle.activate(activate_context).await,
        Err(error) => Err(error),
    };
    let activate_outcome = activate_result.as_ref().map_or_else(
        |error| super::DiagnosticOutcome::RuntimeFailure(error.into()),
        |()| super::DiagnosticOutcome::Succeeded,
    );
    runtime
        .diagnostics
        .emit(DiagnosticSource::Lifecycle, (runtime.driver.now)(), |_| {
            DiagnosticEvent::LifecycleCompleted {
                instance: instance_key.to_owned(),
                generation: generation_number,
                phase: super::PluginLifecyclePhase::Activate,
                outcome: activate_outcome,
                elapsed: (runtime.driver.now)().saturating_sub(activate_started_at),
            }
        });
    if let Err(error) = activate_result {
        runtime.diagnostics.emit_runtime_failure(
            (runtime.driver.now)(),
            Some(instance_key),
            &error,
        );
        let failure = if cleanup_native_generation(
            runtime,
            instance_key,
            prepared,
            DeactivationReason::SupervisionRestart,
            generation_number,
        )
        .await
        .is_some()
        {
            GenerationPreparationFailure::Cleanup { primary: error }
        } else {
            GenerationPreparationFailure::Lifecycle
        };
        return Err(failure);
    }
    Ok(prepared)
}

#[allow(
    clippy::too_many_arguments,
    reason = "construction diagnostics retain exact generation inputs"
)]
async fn run_construction_phase(
    runtime: &NativeAppRuntime,
    instance: &lenso_app_plan::PluginInstancePlan,
    instance_key: &str,
    generation_number: u64,
    lifecycle: Rc<dyn PluginLifecycle>,
    dependencies: PluginDependencies,
    tasks: ManagedTaskScope,
    resources: ManagedResourceScope,
) -> Result<(), RuntimeFailure> {
    if instance.authoring_version() == 1 {
        return Ok(());
    }
    let started_at = (runtime.driver.now)();
    runtime
        .diagnostics
        .emit(DiagnosticSource::Lifecycle, started_at, |_| {
            DiagnosticEvent::LifecycleStarted {
                instance: instance_key.to_owned(),
                generation: generation_number,
                phase: super::PluginLifecyclePhase::Construct,
            }
        });
    let result = lifecycle
        .construct(ActivateContext {
            instance_key: instance_key.to_owned(),
            dependencies,
            ready_gate: runtime.ready_gate.clone(),
            tasks: tasks.clone(),
            resources,
            cancellation: tasks.cancellation(),
            admission: runtime.admission.clone(),
        })
        .await
        .and_then(|()| {
            if runtime.shutdown_started.get() || tasks.cancellation().is_cancelled() {
                Err(RuntimeFailure::AdmissionClosed)
            } else {
                Ok(())
            }
        });
    let outcome = result.as_ref().map_or_else(
        |error| super::DiagnosticOutcome::RuntimeFailure(error.into()),
        |()| super::DiagnosticOutcome::Succeeded,
    );
    runtime
        .diagnostics
        .emit(DiagnosticSource::Lifecycle, (runtime.driver.now)(), |_| {
            DiagnosticEvent::LifecycleCompleted {
                instance: instance_key.to_owned(),
                generation: generation_number,
                phase: super::PluginLifecyclePhase::Construct,
                outcome,
                elapsed: (runtime.driver.now)().saturating_sub(started_at),
            }
        });
    result
}

pub(super) async fn cleanup_native_generation(
    runtime: &NativeAppRuntime,
    instance_key: &str,
    generation: NativePluginGeneration,
    reason: DeactivationReason,
    generation_number: u64,
) -> Option<RuntimeFailure> {
    let budget = runtime
        .cleanup_timeout
        .map(|timeout| super::cleanup::CleanupBudget::after(&runtime.driver, timeout));
    cleanup_native_generation_with_budget(
        runtime,
        instance_key,
        generation,
        reason,
        generation_number,
        budget,
    )
    .await
}

async fn cleanup_native_generation_with_budget(
    runtime: &NativeAppRuntime,
    instance_key: &str,
    generation: NativePluginGeneration,
    reason: DeactivationReason,
    generation_number: u64,
    budget: Option<super::cleanup::CleanupBudget>,
) -> Option<RuntimeFailure> {
    let dependencies = runtime
        .dependencies
        .get(instance_key)
        .cloned()
        .unwrap_or_default();
    match cleanup_generation(
        instance_key,
        generation,
        dependencies,
        reason,
        runtime.admission.clone(),
        generation_number,
        &runtime.diagnostics,
        &runtime.driver,
        budget,
    )
    .await
    {
        GenerationCleanupOutcome::Complete(error) => error,
        GenerationCleanupOutcome::TimedOut(generation) => {
            let error = cleanup_timeout_failure();
            runtime.diagnostics.emit_runtime_failure(
                (runtime.driver.now)(),
                Some(instance_key),
                &error,
            );
            if let Some(plugin) = runtime.plugins.get(instance_key) {
                plugin.install_generation(generation);
            }
            Some(error)
        }
    }
}

enum GenerationCleanupOutcome {
    Complete(Option<RuntimeFailure>),
    TimedOut(NativePluginGeneration),
}

fn cleanup_timeout_failure() -> RuntimeFailure {
    RuntimeFailure::DeadlineExceeded { request_id: 0 }
}

#[allow(
    clippy::too_many_arguments,
    reason = "cleanup spells out every generation-owned input without hidden runtime state"
)]
async fn cleanup_generation(
    instance_key: &str,
    mut generation: NativePluginGeneration,
    dependencies: PluginDependencies,
    reason: DeactivationReason,
    admission: AppAdmission,
    generation_number: u64,
    diagnostics: &RuntimeDiagnostics,
    driver: &super::DriverControl,
    budget: Option<super::cleanup::CleanupBudget>,
) -> GenerationCleanupOutcome {
    generation.tasks.close();
    generation.resources.close();
    if generation.cleanup_timed_out || generation.stop_attempted {
        return GenerationCleanupOutcome::TimedOut(generation);
    }
    if let Some(budget) = &budget {
        if !generation
            .tasks
            .drain_until(driver, budget.deadline())
            .await
        {
            generation.cleanup_timed_out = true;
            return GenerationCleanupOutcome::TimedOut(generation);
        }
        if budget.remaining().is_zero() {
            generation.cleanup_timed_out = true;
            return GenerationCleanupOutcome::TimedOut(generation);
        }
    } else {
        generation.tasks.cancel_all().await;
    }
    let mut first_error = None;
    let started_at = (driver.now)();
    diagnostics.emit(super::DiagnosticSource::Lifecycle, started_at, |_| {
        super::DiagnosticEvent::LifecycleStarted {
            instance: instance_key.to_owned(),
            generation: generation_number,
            phase: super::PluginLifecyclePhase::Deactivate,
        }
    });
    generation.stop_attempted = true;
    let deactivate = generation.lifecycle.deactivate(DeactivateContext {
        instance_key: instance_key.to_owned(),
        dependencies,
        reason,
        tasks: generation.tasks.clone(),
        resources: generation.resources.clone(),
        cancellation: generation.tasks.cancellation(),
        admission,
        cleanup: budget.clone(),
    });
    let result = if let Some(budget) = &budget {
        if let Some(result) = wait_until(driver, budget.deadline(), deactivate).await {
            result
        } else {
            generation.cleanup_timed_out = true;
            return GenerationCleanupOutcome::TimedOut(generation);
        }
    } else {
        deactivate.await
    };
    let outcome = result.as_ref().map_or_else(
        |error| super::DiagnosticOutcome::RuntimeFailure(error.into()),
        |()| super::DiagnosticOutcome::Succeeded,
    );
    diagnostics.emit(super::DiagnosticSource::Lifecycle, (driver.now)(), |_| {
        super::DiagnosticEvent::LifecycleCompleted {
            instance: instance_key.to_owned(),
            generation: generation_number,
            phase: super::PluginLifecyclePhase::Deactivate,
            outcome,
            elapsed: (driver.now)().saturating_sub(started_at),
        }
    });
    if let Err(error) = result {
        diagnostics.emit_runtime_failure((driver.now)(), Some(instance_key), &error);
        first_error = Some(error);
    }
    if let Some(budget) = &budget {
        match generation
            .resources
            .release_all_until(driver, budget.deadline())
            .await
        {
            Ok(Some(error)) if first_error.is_none() => {
                diagnostics.emit_runtime_failure((driver.now)(), Some(instance_key), &error);
                first_error = Some(error);
            }
            Err(()) => {
                generation.cleanup_timed_out = true;
                return GenerationCleanupOutcome::TimedOut(generation);
            }
            Ok(_) => {}
        }
    } else if let Some(error) = generation.resources.release_all().await
        && first_error.is_none()
    {
        diagnostics.emit_runtime_failure((driver.now)(), Some(instance_key), &error);
        first_error = Some(error);
    }
    GenerationCleanupOutcome::Complete(first_error)
}

pub(super) fn install_plugin_endpoints(
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

#[allow(
    clippy::too_many_lines,
    reason = "request, stream, and event endpoint validation must remain symmetric"
)]
pub(super) fn validate_native_endpoint_set(
    instance_key: &str,
    expected: &lenso_app_plan::PluginInstancePlan,
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
                "Plugin Instance `{instance_key}` prepared {} request, {} stream, and {} Event endpoints; expected {} request, {} stream, and {} Event endpoints",
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
                        "Plugin Instance `{instance_key}` prepared {} request endpoints for Capability `{}`",
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
                        "Plugin Instance `{instance_key}` prepared {} stream endpoints for Capability `{}`",
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
                        "Plugin Instance `{instance_key}` prepared {} Event endpoints for Capability `{}`",
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
                "Plugin Instance `{instance_key}` endpoint `{capability_id}` differs from its resolved Descriptor"
            ),
        });
    }
    let mut unique = actual_operations.to_vec();
    unique.sort_unstable();
    unique.dedup();
    if unique.len() != actual_operations.len() {
        return Err(RuntimeFailure::InvalidResolvedPlan {
            detail: format!(
                "Plugin Instance `{instance_key}` endpoint `{capability_id}` has duplicate Operations"
            ),
        });
    }
    Ok(())
}
