//! A small deterministic App harness for testing real native Plugin composition.
//!
//! `TestApp` keeps the same immutable Plan and native Adapter path as production,
//! while removing Tokio setup and manual Driver plumbing from Plugin tests.

use std::{future::Future, time::Duration};

use lenso_app_plan::ResolvedAppPlan;
use lenso_kernel::{Kernel, NativeApp, PluginDependencies, RuntimeFailure, ShutdownOutcome};
use lenso_native_adapter::{NativePluginFactory, NativePluginRegistry};
use lenso_plugin_authoring::CapabilityClient;

mod clock;
mod entropy;
mod faults;
mod receipt;
mod simulator;

pub use clock::TestWallClock;
pub use entropy::TestEntropy;
pub use faults::{FaultInjector, FaultPointError, ScenarioBoundary, SimulatorFault};
pub use receipt::{ScenarioReceipt, ScenarioReceiptEvent, ScenarioTerminal, ScenarioTransition};
pub use simulator::{SimulatorGate, SimulatorResource, TestSimulator};

/// Builder for one deterministic native Test App.
#[derive(Debug)]
pub struct TestAppBuilder {
    plan: ResolvedAppPlan,
    registry: NativePluginRegistry,
    simulator: TestSimulator,
}

impl TestAppBuilder {
    /// Starts with an exact immutable Plan.
    pub fn new(plan: ResolvedAppPlan) -> Self {
        Self {
            plan,
            registry: NativePluginRegistry::new(),
            simulator: TestSimulator::new(),
        }
    }

    /// Adds a native factory available to this test Host.
    #[must_use]
    pub fn with_factory(mut self, factory: impl NativePluginFactory) -> Self {
        self.registry = self.registry.with_factory(factory);
        self
    }

    /// Uses an exact Host-owned native factory registry for this test App.
    ///
    /// This is primarily for Adapter crates whose real Host composition builds
    /// a registry containing both Plugin and ingress factories. The immutable
    /// Plan remains the only source of Plugin dependency selection.
    #[must_use]
    pub fn with_registry(mut self, registry: NativePluginRegistry) -> Self {
        self.registry = registry;
        self
    }

    /// Adds factories linked into the test binary.
    #[must_use]
    pub fn with_linked_factories(mut self) -> Self {
        self.registry = self.registry.with_linked_factories();
        self
    }

    /// Uses one test-private Simulator for this exact App.
    #[must_use]
    pub fn with_simulator(mut self, simulator: TestSimulator) -> Self {
        self.simulator = simulator;
        self
    }

    /// Boots the exact Plan through Kernel and the native Adapter.
    pub fn start(self) -> Result<TestApp, RuntimeFailure> {
        let driver = self.simulator.driver();
        let app = driver.run(Kernel::start_native(
            self.plan,
            driver.clone(),
            self.registry,
        ))?;
        Ok(TestApp {
            simulator: self.simulator,
            app,
        })
    }
}

/// A running deterministic App that exposes the real Kernel handle to tests.
#[derive(Debug)]
pub struct TestApp {
    simulator: TestSimulator,
    app: NativeApp,
}

impl TestApp {
    /// Creates a builder for an exact immutable Plan.
    pub fn builder(plan: ResolvedAppPlan) -> TestAppBuilder {
        TestAppBuilder::new(plan)
    }

    /// Returns the running App for typed generated Client handles and diagnostics.
    pub fn app(&self) -> &NativeApp {
        &self.app
    }

    /// Returns this App's test-private simulated execution environment.
    pub fn simulator(&self) -> &TestSimulator {
        &self.simulator
    }

    /// Connects one generated Client from a consumer Instance's Plan-owned bindings.
    pub fn client<C>(&self, consumer_instance: &str) -> Result<C, RuntimeFailure>
    where
        C: CapabilityClient<Dependencies = PluginDependencies, Error = RuntimeFailure>,
    {
        let dependencies = self.app.dependencies(consumer_instance)?;
        C::from_dependencies(&dependencies)
    }

    /// Runs one future to completion on deterministic virtual time.
    pub fn run<F: Future>(&self, future: F) -> F::Output {
        self.simulator.run(future)
    }

    /// Advances deterministic virtual time and wakes elapsed timers.
    pub fn advance(&self, duration: Duration) {
        self.simulator.advance(duration);
    }

    /// Shuts down the App and returns the exact cleanup outcome.
    pub fn shutdown(&self, timeout: Duration) -> ShutdownOutcome {
        self.simulator.run(self.app.shutdown(timeout))
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use futures::{FutureExt, future::Either};
    use lenso_app_plan::PluginInstancePlan;
    use lenso_kernel::{ActivateContext, PluginFuture, PluginLifecycle};
    use lenso_native_adapter::{NativePluginFactoryContext, NativePluginInstance};

    use super::*;

    #[derive(Debug)]
    struct GateLifecycle {
        gate: SimulatorGate,
        completed: Rc<Cell<bool>>,
        cancelled: Rc<Cell<bool>>,
    }

    impl PluginLifecycle for GateLifecycle {
        fn activate(&self, context: ActivateContext) -> PluginFuture {
            let gate = self.gate.clone();
            let completed = self.completed.clone();
            let cancelled = self.cancelled.clone();
            let cancellation = context.cancellation();
            let spawned = context.tasks().spawn_local(Box::pin(async move {
                let gate_wait = gate.wait().fuse();
                let cancellation_wait = cancellation.cancelled().fuse();
                futures::pin_mut!(gate_wait, cancellation_wait);
                match futures::future::select(gate_wait, cancellation_wait).await {
                    Either::Left(((), _)) => completed.set(true),
                    Either::Right(((), _)) => cancelled.set(true),
                }
            }));
            Box::pin(async move {
                spawned
                    .map(|_| ())
                    .map_err(|error| RuntimeFailure::PluginFailure {
                        detail: format!("could not spawn simulator fixture task: {error:?}"),
                    })
            })
        }
    }

    #[derive(Debug)]
    struct GateFactory {
        gate: SimulatorGate,
        completed: Rc<Cell<bool>>,
        cancelled: Rc<Cell<bool>>,
    }

    impl NativePluginFactory for GateFactory {
        fn package_id(&self) -> &'static str {
            "test.simulator-gate"
        }

        fn instantiate(
            &self,
            _context: NativePluginFactoryContext<'_>,
        ) -> Result<NativePluginInstance, RuntimeFailure> {
            Ok(NativePluginInstance::with_lifecycle(
                Vec::new(),
                GateLifecycle {
                    gate: self.gate.clone(),
                    completed: self.completed.clone(),
                    cancelled: self.cancelled.clone(),
                },
            ))
        }
    }

    fn gate_plan() -> ResolvedAppPlan {
        ResolvedAppPlan::new(
            vec![PluginInstancePlan::new("gate", "test.simulator-gate")],
            vec![],
        )
    }

    #[test]
    fn empty_test_app_starts_ready_and_shuts_down_cleanly() {
        let app = TestApp::builder(ResolvedAppPlan::empty()).start().unwrap();

        assert!(app.app().is_ready());
        assert_eq!(app.shutdown(Duration::from_secs(1)), ShutdownOutcome::Clean);
    }

    #[test]
    fn plan_selected_plugin_without_a_factory_fails_closed() {
        let plan = ResolvedAppPlan::new(
            vec![PluginInstancePlan::new("missing", "test.missing")],
            vec![],
        );

        let error = TestApp::builder(plan).start().unwrap_err();
        assert!(
            matches!(error, RuntimeFailure::MissingPluginFactory { instance, .. } if instance == "missing")
        );
    }

    #[test]
    fn builder_keeps_the_supplied_simulator_with_the_real_native_app() {
        let simulator = TestSimulator::new();
        let app = TestApp::builder(ResolvedAppPlan::empty())
            .with_simulator(simulator.clone())
            .start()
            .unwrap();

        simulator.advance(Duration::from_millis(5));
        assert_eq!(app.simulator().now(), Duration::from_millis(5));
        assert_eq!(app.shutdown(Duration::from_secs(1)), ShutdownOutcome::Clean);
    }

    #[test]
    fn gate_controls_managed_native_plugin_work_after_app_start() {
        let simulator = TestSimulator::new();
        let gate = simulator.gate("fixture.after-activate");
        let completed = Rc::new(Cell::new(false));
        let cancelled = Rc::new(Cell::new(false));
        let app = TestApp::builder(gate_plan())
            .with_simulator(simulator.clone())
            .with_factory(GateFactory {
                gate: gate.clone(),
                completed: completed.clone(),
                cancelled: cancelled.clone(),
            })
            .start()
            .unwrap();

        simulator.pump();
        assert_eq!(gate.reached_count(), 1);
        assert!(!completed.get());

        assert!(gate.release());
        simulator.pump();
        assert!(completed.get());
        assert!(!cancelled.get());
        assert_eq!(app.shutdown(Duration::from_secs(1)), ShutdownOutcome::Clean);
    }

    #[test]
    fn shutdown_cancels_unreleased_managed_native_plugin_work() {
        let simulator = TestSimulator::new();
        let gate = simulator.gate("fixture.awaiting-shutdown");
        let completed = Rc::new(Cell::new(false));
        let cancelled = Rc::new(Cell::new(false));
        let app = TestApp::builder(gate_plan())
            .with_simulator(simulator.clone())
            .with_factory(GateFactory {
                gate: gate.clone(),
                completed: completed.clone(),
                cancelled: cancelled.clone(),
            })
            .start()
            .unwrap();

        simulator.pump();
        assert_eq!(gate.reached_count(), 1);
        assert!(!completed.get());
        assert_eq!(app.shutdown(Duration::from_secs(1)), ShutdownOutcome::Clean);
        assert!(!completed.get());
        assert!(cancelled.get());
    }
}
