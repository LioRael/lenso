//! Host-owned absolute cleanup deadlines.

use super::{CancellationToken, Cell, DriverControl, Duration, Rc};

/// One absolute deadline shared by every phase of a cleanup attempt.
#[derive(Clone, Debug)]
pub(super) struct CleanupBudget {
    driver: DriverControl,
    deadline: Duration,
    cancellation: CancellationToken,
}

impl CleanupBudget {
    pub(super) fn after(driver: &DriverControl, timeout: Duration) -> Self {
        Self::at(driver, (driver.now)().saturating_add(timeout))
    }

    pub(super) fn at(driver: &DriverControl, deadline: Duration) -> Self {
        Self {
            driver: driver.clone(),
            deadline,
            cancellation: CancellationToken::new(),
        }
    }

    pub(super) const fn deadline(&self) -> Duration {
        self.deadline
    }

    pub(super) fn remaining(&self) -> Duration {
        self.deadline.saturating_sub((self.driver.now)())
    }

    pub(super) fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }
}

/// Lazily fixes the cleanup deadline for one startup attempt.
///
/// Caller timeout/drop and late constructor completion share this cell, so a
/// late result cannot reset the Host's cleanup clock.
#[derive(Clone, Debug)]
pub(super) struct StartupCleanupBudget {
    driver: DriverControl,
    timeout: Duration,
    deadline: Rc<Cell<Option<Duration>>>,
}

impl StartupCleanupBudget {
    pub(super) fn new(driver: &DriverControl, timeout: Duration) -> Self {
        Self {
            driver: driver.clone(),
            timeout,
            deadline: Rc::new(Cell::new(None)),
        }
    }

    pub(super) fn establish(&self) -> CleanupBudget {
        self.establish_at((self.driver.now)())
    }

    pub(super) fn establish_at(&self, cleanup_started_at: Duration) -> CleanupBudget {
        let deadline = self.deadline.get().unwrap_or_else(|| {
            let deadline = cleanup_started_at.saturating_add(self.timeout);
            self.deadline.set(Some(deadline));
            deadline
        });
        CleanupBudget::at(&self.driver, deadline)
    }

    pub(super) const fn timeout(&self) -> Duration {
        self.timeout
    }
}
