//! Explicit Host clock input sharing the selected Runtime Driver's deadline domain.
use std::{fmt, rc::Rc, time::Duration};

use lenso_kernel::RuntimeDriver;

/// Monotonic time from the same Driver supplied to this App's Kernel.
#[derive(Clone)]
pub struct NativeHostClock {
    now: Rc<dyn Fn() -> Duration>,
}

impl NativeHostClock {
    /// Captures the selected Driver; it does not create an independent epoch.
    pub fn from_driver<D: RuntimeDriver>(driver: D) -> Self {
        Self {
            now: Rc::new(move || driver.now()),
        }
    }

    /// Returns time in the Kernel invocation deadline domain.
    pub fn now(&self) -> Duration {
        (self.now)()
    }
}

impl fmt::Debug for NativeHostClock {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeHostClock")
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{future::LocalBoxFuture, task::SpawnError};
    use lenso_kernel::{DriverTask, LocalTask};
    use std::cell::Cell;

    #[derive(Clone)]
    struct ControlledDriver(Rc<Cell<Duration>>);

    impl RuntimeDriver for ControlledDriver {
        fn now(&self) -> Duration {
            self.0.get()
        }
        fn sleep_until(&self, _deadline: Duration) -> LocalBoxFuture<'static, ()> {
            Box::pin(async {})
        }
        fn yield_now(&self) -> LocalBoxFuture<'static, ()> {
            Box::pin(async {})
        }
        fn spawn_local(&self, _task: LocalTask) -> Result<DriverTask, SpawnError> {
            Err(SpawnError::shutdown())
        }
        fn shutdown_requested(&self) -> bool {
            false
        }
    }

    #[test]
    fn owner_clock_and_driver_share_one_controlled_domain() {
        let driver = ControlledDriver(Rc::new(Cell::new(Duration::from_secs(7))));
        let owner = NativeHostClock::from_driver(driver.clone());
        let next_generation = owner.clone();
        assert_eq!(owner.now(), driver.now());
        driver.0.set(Duration::from_secs(23));
        assert_eq!(owner.now(), Duration::from_secs(23));
        assert_eq!(next_generation.now(), driver.now());
    }
}
