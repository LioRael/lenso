use std::{cell::RefCell, fmt, future::Future, pin::Pin, rc::Rc, time::Duration};

pub type JobFuture<T> = Pin<Box<dyn Future<Output = T> + 'static>>;
type DeliveryHook = Rc<dyn Fn(usize, u32) -> JobFuture<Result<(), String>>>;
type CommitHook = Rc<dyn Fn(usize) -> JobFuture<Result<(), String>>>;

/// Resource and delivery observations contain IDs and counters, never payloads.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProbeSnapshot {
    pub started: usize,
    pub dropped: usize,
    pub dropped_job_ids: Vec<usize>,
    pub shutdown_cancelled_job_ids: Vec<usize>,
    pub active: usize,
    pub stop_rejected: usize,
    pub receipts: usize,
    pub receipt_failures: usize,
    pub delivery_attempts: Vec<(usize, u32)>,
}

#[derive(Clone, Debug, Default)]
pub struct JobProbe(pub(crate) Rc<RefCell<ProbeSnapshot>>);

impl JobProbe {
    pub fn snapshot(&self) -> ProbeSnapshot {
        self.0.borrow().clone()
    }
}

/// Private Host bindings used by production Tokio and deterministic tests alike.
/// Hooks model local delivery boundaries; no external delivery is performed.
#[derive(Clone)]
pub struct JobRuntime {
    pub(crate) now: Rc<dyn Fn() -> Duration>,
    pub(crate) sleep_until: Rc<dyn Fn(Duration) -> JobFuture<()>>,
    pub(crate) before_delivery: DeliveryHook,
    pub(crate) after_commit: CommitHook,
    pub(crate) probe: JobProbe,
}

impl fmt::Debug for JobRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("JobRuntime")
            .field("probe", &self.probe)
            .finish_non_exhaustive()
    }
}

impl JobRuntime {
    pub fn new(
        now: impl Fn() -> Duration + 'static,
        sleep_until: impl Fn(Duration) -> JobFuture<()> + 'static,
    ) -> Self {
        Self {
            now: Rc::new(now),
            sleep_until: Rc::new(sleep_until),
            before_delivery: Rc::new(|_, _| Box::pin(async { Ok(()) })),
            after_commit: Rc::new(|_| Box::pin(async { Ok(()) })),
            probe: JobProbe::default(),
        }
    }

    #[must_use]
    pub fn with_before_delivery(
        mut self,
        hook: impl Fn(usize, u32) -> JobFuture<Result<(), String>> + 'static,
    ) -> Self {
        self.before_delivery = Rc::new(hook);
        self
    }

    #[must_use]
    pub fn with_after_commit(
        mut self,
        hook: impl Fn(usize) -> JobFuture<Result<(), String>> + 'static,
    ) -> Self {
        self.after_commit = Rc::new(hook);
        self
    }

    pub fn probe(&self) -> JobProbe {
        self.probe.clone()
    }

    pub(crate) fn sleep(&self, delay: Duration) -> JobFuture<()> {
        (self.sleep_until)((self.now)().saturating_add(delay))
    }
}

impl Default for JobRuntime {
    fn default() -> Self {
        let epoch = tokio::time::Instant::now();
        Self::new(
            move || epoch.elapsed(),
            move |deadline| Box::pin(tokio::time::sleep_until(epoch + deadline)),
        )
    }
}
