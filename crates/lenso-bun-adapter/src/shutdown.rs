use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

/// Retirement evidence for every child started by one Bun Adapter.
///
/// A forced or unconfirmed retirement remains unclean across replacements.
#[derive(Clone, Debug, Default)]
pub struct ShutdownEvidence(Arc<State>);

#[derive(Debug, Default)]
struct State {
    outstanding: AtomicUsize,
    failed: AtomicBool,
}

impl ShutdownEvidence {
    /// Check after runtime teardown; detached native work is outside this evidence.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.0.outstanding.load(Ordering::Acquire) == 0 && !self.0.failed.load(Ordering::Acquire)
    }

    pub(crate) fn started(&self) {
        self.0.outstanding.fetch_add(1, Ordering::AcqRel);
    }

    pub(crate) fn reaped_cleanly(&self) {
        self.0.outstanding.fetch_sub(1, Ordering::AcqRel);
    }

    pub(crate) fn failed(&self) {
        self.0.failed.store(true, Ordering::Release);
    }
}
