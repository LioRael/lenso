//! Test-only wall-clock projection from deterministic virtual time.

use std::time::SystemTime;

use crate::simulator::TestSimulator;

/// An explicit wall-clock projection of one [`TestSimulator`]'s virtual time.
///
/// Production code must supply its own clock. This type is intentionally not
/// installed into a Driver or selected from a Plan; a test passes it only to a
/// test-support seam that explicitly accepts a clock.
#[derive(Clone, Debug)]
pub struct TestWallClock {
    simulator: TestSimulator,
    epoch: SystemTime,
}

impl TestWallClock {
    pub(crate) fn new(simulator: TestSimulator, epoch: SystemTime) -> Self {
        Self { simulator, epoch }
    }

    /// Returns the explicitly configured instant at virtual instant zero.
    pub const fn epoch(&self) -> SystemTime {
        self.epoch
    }

    /// Returns the current wall-clock instant derived from virtual time.
    ///
    /// `None` means the explicit epoch plus virtual duration is out of range;
    /// this method never consults the host wall clock.
    pub fn now(&self) -> Option<SystemTime> {
        self.epoch.checked_add(self.simulator.now())
    }
}

impl TestSimulator {
    /// Projects this simulator's virtual monotonic clock onto a chosen epoch.
    ///
    /// The supplied epoch is part of the test scenario, not Host configuration
    /// or production security policy.
    pub fn wall_clock(&self, epoch: SystemTime) -> TestWallClock {
        TestWallClock::new(self.clone(), epoch)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn wall_clock_advances_only_with_its_simulator() {
        let simulator = TestSimulator::new();
        let epoch = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let clock = simulator.wall_clock(epoch);

        assert_eq!(clock.epoch(), epoch);
        assert_eq!(clock.now(), Some(epoch));
        simulator.advance(Duration::from_millis(250));
        assert_eq!(clock.now(), Some(epoch + Duration::from_millis(250)),);
    }
}
