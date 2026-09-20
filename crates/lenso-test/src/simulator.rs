//! Cooperative controls for deterministic native `TestApp` scenarios.
//!
//! A Simulator Gate is private test infrastructure. It only pauses futures
//! that voluntarily wait on it; it does not preempt arbitrary Plugin, network,
//! database, or CPU work.

use std::{cell::RefCell, future::Future, rc::Rc, time::Duration};

use futures::{channel::oneshot, task::SpawnError};
use lenso_kernel::{DeterministicDriver, DriverTask, RuntimeDriver};

use crate::FaultInjector;

/// A deterministic test-only execution environment around one Driver.
#[derive(Clone, Debug, Default)]
pub struct TestSimulator {
    driver: DeterministicDriver,
    faults: FaultInjector,
}

impl TestSimulator {
    /// Creates a Simulator at virtual monotonic instant zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a named cooperative checkpoint for one test scenario.
    pub fn gate(&self, name: impl Into<String>) -> SimulatorGate {
        SimulatorGate {
            name: name.into(),
            state: Rc::new(RefCell::new(GateState::default())),
        }
    }

    /// Creates a cooperative, test-owned resource availability control.
    ///
    /// Unlike a [`SimulatorGate`], a resource may be frozen and thawed more
    /// than once. Scenario code opts in by awaiting [`SimulatorResource::acquire`]
    /// at the same private resource boundary it uses in production; the
    /// simulator never substitutes a second business implementation.
    pub fn resource(&self, name: impl Into<String>) -> SimulatorResource {
        SimulatorResource {
            name: name.into(),
            state: Rc::new(RefCell::new(ResourceState::default())),
        }
    }

    /// Returns this Simulator's test-owned, explicit fault injector.
    ///
    /// Injected failures stay inert until scenario code checks the exact
    /// boundary. This avoids turning target behavior into an implicit global
    /// fault policy.
    pub fn faults(&self) -> FaultInjector {
        self.faults.clone()
    }

    /// Runs one root future on the deterministic Driver.
    ///
    /// This must remain the outermost Driver run for a scenario. Calling it
    /// from a future already running on this Simulator is recursive and is
    /// rejected by the underlying Driver.
    pub fn run<F: Future>(&self, future: F) -> F::Output {
        self.driver.run(future)
    }

    /// Cooperatively drives currently runnable local work.
    ///
    /// A pump is a scheduling boundary, not a single-step scheduler: the
    /// underlying local executor may poll more than one ready future before
    /// this call returns. Call it only outside a running [`Self::run`] future.
    pub fn pump(&self) {
        self.run(self.driver.yield_now());
    }

    /// Spawns voluntary scenario work on the deterministic local lane.
    pub fn spawn<F>(&self, future: F) -> Result<DriverTask, SpawnError>
    where
        F: Future<Output = ()> + 'static,
    {
        self.driver.spawn_local(Box::pin(future))
    }

    /// Advances virtual time and wakes elapsed deterministic timers.
    pub fn advance(&self, duration: Duration) {
        self.driver.advance(duration);
    }

    /// Returns the current virtual monotonic instant.
    pub fn now(&self) -> Duration {
        self.driver.now()
    }

    pub(crate) fn driver(&self) -> DeterministicDriver {
        self.driver.clone()
    }
}

#[derive(Debug, Default)]
struct ResourceState {
    acquisitions: usize,
    frozen: bool,
    thaw_notifiers: Vec<oneshot::Sender<()>>,
}

/// A named, cooperative resource owner that a test can freeze independently.
///
/// This represents ownership/availability only. It deliberately does not
/// provide a global service locator, connection pool, or implicit resource
/// selection mechanism.
#[derive(Clone, Debug)]
pub struct SimulatorResource {
    name: String,
    state: Rc<RefCell<ResourceState>>,
}

impl SimulatorResource {
    /// Returns the stable test-only owner name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Waits until this resource owner is available, then records one access.
    pub async fn acquire(&self) {
        loop {
            let thaw = {
                let mut state = self.state.borrow_mut();
                if state.frozen {
                    let (sender, receiver) = oneshot::channel();
                    state.thaw_notifiers.push(sender);
                    Some(receiver)
                } else {
                    state.acquisitions += 1;
                    None
                }
            };

            let Some(thaw) = thaw else {
                return;
            };
            let _ = thaw.await;
        }
    }

    /// Freezes future cooperative acquisitions of this specific owner.
    ///
    /// Returns true only for the transition from available to frozen.
    #[must_use]
    pub fn freeze(&self) -> bool {
        let mut state = self.state.borrow_mut();
        if state.frozen {
            return false;
        }
        state.frozen = true;
        true
    }

    /// Makes this owner available and releases all currently blocked accessors.
    ///
    /// Returns true only for the transition from frozen to available.
    #[must_use]
    pub fn thaw(&self) -> bool {
        let notifiers = {
            let mut state = self.state.borrow_mut();
            if !state.frozen {
                return false;
            }
            state.frozen = false;
            std::mem::take(&mut state.thaw_notifiers)
        };
        for notifier in notifiers {
            let _ = notifier.send(());
        }
        true
    }

    /// Returns the number of completed cooperative acquisitions.
    pub fn acquisition_count(&self) -> usize {
        self.state.borrow().acquisitions
    }

    /// Returns whether this owner is currently frozen.
    pub fn is_frozen(&self) -> bool {
        self.state.borrow().frozen
    }
}

#[derive(Debug, Default)]
struct GateState {
    reached: usize,
    released: bool,
    reached_notifiers: Vec<oneshot::Sender<()>>,
    release_notifiers: Vec<oneshot::Sender<()>>,
}

/// A named checkpoint that a test explicitly releases.
#[derive(Clone, Debug)]
pub struct SimulatorGate {
    name: String,
    state: Rc<RefCell<GateState>>,
}

impl SimulatorGate {
    /// Returns the stable test-only checkpoint name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Waits until the test releases this checkpoint.
    pub async fn wait(&self) {
        let release = {
            let mut state = self.state.borrow_mut();
            state.reached += 1;
            for notifier in state.reached_notifiers.drain(..) {
                let _ = notifier.send(());
            }
            if state.released {
                None
            } else {
                let (sender, receiver) = oneshot::channel();
                state.release_notifiers.push(sender);
                Some(receiver)
            }
        };

        if let Some(release) = release {
            let _ = release.await;
        }
    }

    /// Waits until at least one operation has reached this checkpoint.
    pub async fn reached(&self) {
        let notification = {
            let mut state = self.state.borrow_mut();
            if state.reached > 0 {
                None
            } else {
                let (sender, receiver) = oneshot::channel();
                state.reached_notifiers.push(sender);
                Some(receiver)
            }
        };

        if let Some(notification) = notification {
            let _ = notification.await;
        }
    }

    /// Releases every operation currently waiting at this checkpoint.
    ///
    /// Returns true only for the transition from blocked to released.
    #[must_use]
    pub fn release(&self) -> bool {
        let notifiers = {
            let mut state = self.state.borrow_mut();
            if state.released {
                return false;
            }
            state.released = true;
            std::mem::take(&mut state.release_notifiers)
        };
        for notifier in notifiers {
            let _ = notifier.send(());
        }
        true
    }

    /// Returns how many voluntary waits have reached this checkpoint.
    pub fn reached_count(&self) -> usize {
        self.state.borrow().reached
    }

    /// Returns whether the test has released this checkpoint.
    pub fn is_released(&self) -> bool {
        self.state.borrow().released
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use lenso_kernel::TaskOutcome;

    use super::*;

    #[test]
    fn gate_blocks_work_until_the_test_releases_it() {
        let simulator = TestSimulator::new();
        let gate = simulator.gate("provider.before-response");
        let task = simulator
            .spawn({
                let gate = gate.clone();
                async move {
                    gate.wait().await;
                }
            })
            .unwrap();

        simulator.run(gate.reached());
        assert_eq!(gate.name(), "provider.before-response");
        assert_eq!(gate.reached_count(), 1);
        assert!(!gate.is_released());
        assert!(gate.release());
        assert!(!gate.release());
        assert_eq!(simulator.run(task), TaskOutcome::Completed);
    }

    #[test]
    fn gates_allow_test_chosen_completion_order() {
        let simulator = TestSimulator::new();
        let first = simulator.gate("operation.first");
        let second = simulator.gate("operation.second");
        let completed = Rc::new(RefCell::new(Vec::new()));

        let first_task = simulator
            .spawn({
                let first = first.clone();
                let completed = completed.clone();
                async move {
                    first.wait().await;
                    completed.borrow_mut().push("first");
                }
            })
            .unwrap();
        let second_task = simulator
            .spawn({
                let second = second.clone();
                let completed = completed.clone();
                async move {
                    second.wait().await;
                    completed.borrow_mut().push("second");
                }
            })
            .unwrap();

        simulator.run(async {
            first.reached().await;
            second.reached().await;
        });
        assert!(second.release());
        assert_eq!(simulator.run(second_task), TaskOutcome::Completed);
        assert_eq!(&*completed.borrow(), &["second"]);

        assert!(first.release());
        assert_eq!(simulator.run(first_task), TaskOutcome::Completed);
        assert_eq!(&*completed.borrow(), &["second", "first"]);
    }

    #[test]
    fn cancelling_one_blocked_operation_does_not_release_its_peer() {
        let simulator = TestSimulator::new();
        let cancelled = simulator.gate("operation.cancelled");
        let peer = simulator.gate("operation.peer");
        let cancelled_task = simulator
            .spawn({
                let cancelled = cancelled.clone();
                async move {
                    cancelled.wait().await;
                }
            })
            .unwrap();
        let peer_task = simulator
            .spawn({
                let peer = peer.clone();
                async move {
                    peer.wait().await;
                }
            })
            .unwrap();

        simulator.run(async {
            cancelled.reached().await;
            peer.reached().await;
        });
        cancelled_task.cancel();
        assert_eq!(simulator.run(cancelled_task), TaskOutcome::Cancelled);
        assert!(!peer.is_released());

        assert!(peer.release());
        assert_eq!(simulator.run(peer_task), TaskOutcome::Completed);
    }

    #[test]
    fn advancing_virtual_time_wakes_elapsed_work_when_the_test_pumps() {
        let simulator = TestSimulator::new();
        let completed = Rc::new(RefCell::new(false));
        let driver = simulator.driver();
        let task = simulator
            .spawn({
                let completed = completed.clone();
                async move {
                    driver.sleep_until(Duration::from_millis(5)).await;
                    *completed.borrow_mut() = true;
                }
            })
            .unwrap();

        simulator.pump();
        assert!(!*completed.borrow());
        simulator.advance(Duration::from_millis(5));
        simulator.pump();
        assert!(*completed.borrow());
        assert_eq!(simulator.run(task), TaskOutcome::Completed);
    }

    #[test]
    fn freezing_one_resource_owner_does_not_block_another() {
        let simulator = TestSimulator::new();
        let auth_store = simulator.resource("auth-store");
        let object_store = simulator.resource("object-store");
        assert!(auth_store.freeze());
        assert!(!auth_store.freeze());

        let blocked = simulator
            .spawn({
                let auth_store = auth_store.clone();
                async move {
                    auth_store.acquire().await;
                }
            })
            .unwrap();
        let independent = simulator
            .spawn({
                let object_store = object_store.clone();
                async move {
                    object_store.acquire().await;
                }
            })
            .unwrap();

        simulator.pump();
        assert_eq!(auth_store.acquisition_count(), 0);
        assert_eq!(object_store.acquisition_count(), 1);
        assert_eq!(simulator.run(independent), TaskOutcome::Completed);
        assert!(auth_store.is_frozen());

        assert!(auth_store.thaw());
        assert!(!auth_store.thaw());
        assert_eq!(simulator.run(blocked), TaskOutcome::Completed);
        assert_eq!(auth_store.acquisition_count(), 1);
    }
}
