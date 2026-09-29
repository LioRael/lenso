use std::{
    io,
    ops::{Deref, DerefMut},
    process::{Child, ExitStatus},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

/// Read-only retirement evidence for every child started by one Adapter.
///
/// Check after dropping the runtime. This covers owned children, not detached
/// native work or processes deliberately launched outside the Adapter.
#[derive(Clone, Debug, Default)]
pub struct ShutdownEvidence(Arc<State>);

#[derive(Debug, Default)]
struct State {
    outstanding: AtomicUsize,
    uncertain: AtomicBool,
}

impl ShutdownEvidence {
    /// True only when every owned child was reaped without forced cleanup.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.0.outstanding.load(Ordering::Acquire) == 0 && !self.0.uncertain.load(Ordering::Acquire)
    }

    pub(crate) fn track(&self, child: Child) -> ManagedChild {
        self.0.outstanding.fetch_add(1, Ordering::AcqRel);
        ManagedChild {
            child,
            evidence: self.clone(),
            reaped: false,
        }
    }

    pub(crate) fn uncertain(&self) {
        self.0.uncertain.store(true, Ordering::Release);
    }
}

#[derive(Debug)]
pub(crate) struct ManagedChild {
    child: Child,
    evidence: ShutdownEvidence,
    reaped: bool,
}

impl ManagedChild {
    /// Reap an already failed generation, without claiming graceful retirement.
    pub(crate) fn reap_failed(&mut self, timeout: Duration) -> io::Result<()> {
        self.evidence.uncertain();
        let deadline = Instant::now() + timeout;
        loop {
            match self.try_wait()? {
                Some(_) => return Ok(()),
                None if Instant::now() >= deadline => {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "failed Process generation termination is unconfirmed",
                    ));
                }
                None => std::thread::sleep(Duration::from_millis(2)),
            }
        }
    }

    pub(crate) fn kill(&mut self) -> io::Result<()> {
        self.evidence.uncertain();
        self.child.kill()
    }

    pub(crate) fn wait(&mut self) -> io::Result<ExitStatus> {
        let result = self.child.wait();
        self.record(result.as_ref().map(Some));
        result
    }

    pub(crate) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        let result = self.child.try_wait();
        self.record(result.as_ref().map(Option::as_ref));
        result
    }

    fn record(&mut self, result: Result<Option<&ExitStatus>, &io::Error>) {
        match result {
            Ok(Some(status)) => {
                if !status.success() {
                    self.evidence.uncertain();
                }
                if !self.reaped {
                    self.reaped = true;
                    self.evidence.0.outstanding.fetch_sub(1, Ordering::AcqRel);
                }
            }
            Err(_) => self.evidence.uncertain(),
            Ok(None) => {}
        }
    }
}

impl Deref for ManagedChild {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.child
    }
}

impl DerefMut for ManagedChild {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.child
    }
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        if self.reaped {
            return;
        }
        let _ = self.kill();
        let deadline = Instant::now() + Duration::from_secs(2);
        while matches!(self.try_wait(), Ok(None)) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn spawn(evidence: &ShutdownEvidence, script: &str) -> ManagedChild {
        evidence.track(
            std::process::Command::new("sh")
                .args(["-c", script])
                .spawn()
                .unwrap(),
        )
    }

    #[test]
    fn clean_requires_every_owned_child_to_be_reaped() {
        let evidence = ShutdownEvidence::default();
        let mut first = spawn(&evidence, "exit 0");
        let mut second = spawn(&evidence, "exit 0");
        assert!(!evidence.is_clean());
        first.wait().unwrap();
        assert!(!evidence.is_clean());
        second.wait().unwrap();
        assert!(evidence.is_clean());
        drop((first, second));
        assert!(evidence.is_clean());
    }

    #[test]
    fn forced_retirement_survives_a_successful_replacement() {
        let evidence = ShutdownEvidence::default();
        let mut first = spawn(&evidence, "exec sleep 30");
        first.kill().unwrap();
        first.wait().unwrap();
        let mut replacement = spawn(&evidence, "exit 0");
        replacement.wait().unwrap();
        drop((first, replacement));
        assert!(!evidence.is_clean());
        assert_eq!(evidence.0.outstanding.load(Ordering::Acquire), 0);
    }

    #[test]
    fn startup_abandonment_and_nonzero_exit_remain_unclean() {
        let evidence = ShutdownEvidence::default();
        drop(spawn(&evidence, "exec sleep 30"));
        assert!(!evidence.is_clean());
        let other = ShutdownEvidence::default();
        spawn(&other, "exit 9").wait().unwrap();
        assert!(!other.is_clean());
        assert_eq!(other.0.outstanding.load(Ordering::Acquire), 0);
    }

    #[test]
    fn stop_hook_failure_after_successful_reap_remains_unclean() {
        let evidence = ShutdownEvidence::default();
        spawn(&evidence, "exit 0").wait().unwrap();
        assert!(evidence.is_clean());
        evidence.uncertain();
        spawn(&evidence, "exit 0").wait().unwrap();
        assert!(!evidence.is_clean());
    }

    #[test]
    fn failed_generation_reap_allows_replacement_but_never_cleans_lifetime_evidence() {
        for script in ["exit 0", "exit 23"] {
            let evidence = ShutdownEvidence::default();
            let mut failed = spawn(&evidence, script);
            failed.reap_failed(Duration::from_secs(1)).unwrap();
            assert!(failed.try_wait().unwrap().is_some());
            spawn(&evidence, "exit 0").wait().unwrap();
            assert!(!evidence.is_clean());
            assert_eq!(evidence.0.outstanding.load(Ordering::Acquire), 0);
        }
    }

    #[test]
    fn failed_generation_still_alive_is_not_confirmed_cleanup() {
        let evidence = ShutdownEvidence::default();
        let mut failed = spawn(&evidence, "exec sleep 30");
        let error = failed.reap_failed(Duration::from_millis(10)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(failed.try_wait().unwrap().is_none());
        assert!(!evidence.is_clean());
        failed.kill().unwrap();
        failed.wait().unwrap();
        assert!(!evidence.is_clean());
    }
}
