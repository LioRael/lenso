use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::{Rc, Weak},
    time::Duration,
};

use super::{ModuleLifecyclePhase, RuntimeFailure};

/// The Kernel subsystem that produced one Runtime Diagnostic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum DiagnosticSource {
    /// Module generation preparation, activation, readiness, or deactivation.
    Lifecycle = 0,
    /// A Request or Stream-open operation entered or left the Kernel.
    Invocation = 1,
    /// Bounded work admission or Event delivery was accepted or rejected.
    Admission = 2,
    /// Provider generation replacement and restart-budget decisions.
    Supervision = 3,
    /// App shutdown admission and cleanup.
    Shutdown = 4,
    /// A sanitized Runtime Failure fact.
    RuntimeFailure = 5,
}

impl DiagnosticSource {
    const COUNT: u8 = 6;

    const fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

/// A compact source allowlist for one observer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiagnosticFilter {
    mask: u8,
}

impl DiagnosticFilter {
    /// Matches no diagnostic source.
    pub const fn none() -> Self {
        Self { mask: 0 }
    }

    /// Matches every Kernel diagnostic source.
    pub const fn all() -> Self {
        Self {
            mask: (1 << DiagnosticSource::COUNT) - 1,
        }
    }

    /// Matches exactly one diagnostic source.
    pub const fn only(source: DiagnosticSource) -> Self {
        Self { mask: source.bit() }
    }

    /// Returns a filter that also matches `source`.
    #[must_use]
    pub const fn with_source(self, source: DiagnosticSource) -> Self {
        Self {
            mask: self.mask | source.bit(),
        }
    }

    /// Returns whether this filter accepts `source`.
    pub const fn includes(self, source: DiagnosticSource) -> bool {
        self.mask & source.bit() != 0
    }
}

impl Default for DiagnosticFilter {
    fn default() -> Self {
        Self::all()
    }
}

/// A sanitized category of Runtime Failure.
///
/// Details, payloads, configuration, and opaque values are intentionally not
/// represented. Observers can use the category with structural fields from a
/// [`DiagnosticEvent`] without receiving business data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeFailureKind {
    /// No current provider generation is available.
    Unavailable,
    /// The requested Operation is not in the resolved Descriptor.
    UnknownOperation,
    /// A singular handle was used for several providers.
    AmbiguousBinding,
    /// The generated contract and endpoint disagreed.
    ProtocolViolation,
    /// A selected Module factory was not linked.
    MissingModuleFactory,
    /// The selected Execution Adapter is unavailable.
    UnavailableExecutionClass,
    /// The resolved Plan or prepared endpoint set is invalid.
    InvalidResolvedPlan,
    /// New work was rejected because App admission is closed.
    AdmissionClosed,
    /// A bounded admission queue was full.
    ResourceExhausted,
    /// A monotonic invocation deadline expired.
    DeadlineExceeded,
    /// Invocation cancellation won the race.
    Cancelled,
    /// The Driver or Adapter reported an internal failure.
    Internal,
    /// A Module generation reported a failure.
    ModuleFailure,
    /// A finite Module restart budget was exhausted.
    ModuleRestartExhausted,
}

impl From<&RuntimeFailure> for RuntimeFailureKind {
    fn from(error: &RuntimeFailure) -> Self {
        match error {
            RuntimeFailure::Unavailable { .. } => Self::Unavailable,
            RuntimeFailure::UnknownOperation { .. } => Self::UnknownOperation,
            RuntimeFailure::AmbiguousBinding { .. } => Self::AmbiguousBinding,
            RuntimeFailure::ProtocolViolation { .. } => Self::ProtocolViolation,
            RuntimeFailure::MissingModuleFactory { .. } => Self::MissingModuleFactory,
            RuntimeFailure::UnavailableExecutionClass { .. } => Self::UnavailableExecutionClass,
            RuntimeFailure::InvalidResolvedPlan { .. } => Self::InvalidResolvedPlan,
            RuntimeFailure::AdmissionClosed => Self::AdmissionClosed,
            RuntimeFailure::ResourceExhausted { .. } => Self::ResourceExhausted,
            RuntimeFailure::DeadlineExceeded { .. } => Self::DeadlineExceeded,
            RuntimeFailure::Cancelled { .. } => Self::Cancelled,
            RuntimeFailure::Internal { .. } => Self::Internal,
            RuntimeFailure::ModuleFailure { .. } => Self::ModuleFailure,
            RuntimeFailure::ModuleRestartExhausted { .. } => Self::ModuleRestartExhausted,
        }
    }
}

/// An outcome that is safe to expose without including a Domain Error body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticOutcome {
    /// The operation or lifecycle phase completed successfully.
    Succeeded,
    /// The Capability returned a Domain Error; its body is deliberately absent.
    DomainError,
    /// The Kernel returned a sanitized Runtime Failure category.
    RuntimeFailure(RuntimeFailureKind),
}

/// A bounded admission outcome safe to expose to a diagnostic observer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticAdmission {
    /// The value or operation entered the selected bounded queue.
    Accepted,
    /// The selected provider or subscriber generation is unavailable.
    Unavailable,
    /// The selected bounded queue is full.
    Exhausted,
    /// App admission was already closed.
    Closed,
}

/// A sanitized App shutdown outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticShutdownOutcome {
    /// All managed work and resources were released.
    Clean,
    /// Cleanup reported a Runtime Failure.
    RuntimeFailure,
    /// The global cleanup deadline expired.
    Timeout,
}

/// Structural, lossy metadata emitted by the Kernel.
///
/// This enum intentionally has no payload, configuration, secret, opaque
/// extension, `ActorAssertion`, or Domain Error fields. Delivery of these
/// records is not itself observed, so exporting a record cannot recurse into
/// the diagnostic feed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiagnosticEvent {
    /// The Kernel created a running App runtime.
    AppStarted { module_count: usize },
    /// Every selected Module generation has activated and App admission opened.
    AppReady,
    /// A Module lifecycle phase began.
    LifecycleStarted {
        instance: String,
        generation: u64,
        phase: ModuleLifecyclePhase,
    },
    /// A Module lifecycle phase completed with a sanitized outcome and duration.
    LifecycleCompleted {
        instance: String,
        generation: u64,
        phase: ModuleLifecyclePhase,
        outcome: DiagnosticOutcome,
        elapsed: Duration,
    },
    /// A typed request or stream operation began.
    InvocationStarted {
        request_id: u64,
        caller_instance: String,
        provider_instance: Option<String>,
        capability: &'static str,
        operation: Option<&'static str>,
    },
    /// A typed request or stream operation completed.
    InvocationCompleted {
        request_id: u64,
        caller_instance: String,
        provider_instance: Option<String>,
        capability: &'static str,
        operation: Option<&'static str>,
        outcome: DiagnosticOutcome,
        elapsed: Duration,
    },
    /// Bounded request admission rejected an operation.
    AdmissionRejected {
        request_id: u64,
        caller_instance: String,
        provider_instance: Option<String>,
        capability: &'static str,
        operation: Option<&'static str>,
        outcome: DiagnosticAdmission,
    },
    /// One Event subscriber received an independent admission outcome.
    EventAdmission {
        request_id: u64,
        publisher_instance: String,
        subscriber_instance: String,
        capability: &'static str,
        operation: Option<&'static str>,
        outcome: DiagnosticAdmission,
    },
    /// A provider generation became unavailable.
    GenerationUnavailable { instance: String, generation: u64 },
    /// A replacement provider generation became ready.
    GenerationReady { instance: String, generation: u64 },
    /// Supervision scheduled one bounded restart attempt.
    RestartScheduled {
        instance: String,
        attempt: usize,
        delay: Duration,
    },
    /// Supervision exhausted its finite restart budget.
    RestartExhausted {
        instance: String,
        attempts: usize,
        terminal: bool,
    },
    /// A Runtime Failure category was observed without its detail or payload.
    RuntimeFailure {
        instance: Option<String>,
        kind: RuntimeFailureKind,
    },
    /// App shutdown admission closed and cleanup started.
    ShutdownStarted { timeout: Duration },
    /// App cleanup completed with a sanitized outcome and duration.
    ShutdownCompleted {
        outcome: DiagnosticShutdownOutcome,
        elapsed: Duration,
    },
}

/// One sequenced, timestamped Runtime Diagnostic record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticRecord {
    /// Monotonic sequence within the supplied diagnostics port.
    pub sequence: u64,
    /// Driver-monotonic timestamp at emission.
    pub timestamp: Duration,
    /// Kernel subsystem that emitted the record.
    pub source: DiagnosticSource,
    /// Sanitized structural metadata.
    pub event: DiagnosticEvent,
}

/// Error returned when an observer queue cannot be created.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticSubscribeError {
    /// Every observer queue must have at least one slot.
    ZeroCapacity,
}

#[derive(Debug, Default)]
struct RuntimeDiagnosticsState {
    observers: RefCell<Vec<Weak<DiagnosticObserverState>>>,
    next_sequence: Cell<u64>,
}

/// An opt-in Runtime Diagnostics port.
///
/// The port only stores local, ephemeral, best-effort records. It never calls
/// observer code and never waits for a consumer. It is therefore unsuitable
/// for audit, durable Story correctness, persistence, replay, or redelivery.
#[derive(Clone, Debug)]
pub struct RuntimeDiagnostics {
    state: Rc<RuntimeDiagnosticsState>,
}

impl RuntimeDiagnostics {
    /// Creates an empty diagnostics port with no observers.
    pub fn new() -> Self {
        Self {
            state: Rc::new(RuntimeDiagnosticsState::default()),
        }
    }

    /// Adds an independently bounded, source-filtered observer queue.
    pub fn subscribe(
        &self,
        filter: DiagnosticFilter,
        capacity: usize,
    ) -> Result<DiagnosticObserver, DiagnosticSubscribeError> {
        if capacity == 0 {
            return Err(DiagnosticSubscribeError::ZeroCapacity);
        }
        let observer = Rc::new(DiagnosticObserverState {
            filter,
            capacity,
            queue: RefCell::new(VecDeque::with_capacity(capacity)),
            dropped: Cell::new(0),
        });
        let mut observers = self.state.observers.borrow_mut();
        observers.retain(|observer| observer.upgrade().is_some());
        observers.push(Rc::downgrade(&observer));
        Ok(DiagnosticObserver { state: observer })
    }

    /// Adds an all-source observer queue.
    pub fn subscribe_all(
        &self,
        capacity: usize,
    ) -> Result<DiagnosticObserver, DiagnosticSubscribeError> {
        self.subscribe(DiagnosticFilter::all(), capacity)
    }

    /// Returns the number of observers that are still connected to this port.
    pub fn observer_count(&self) -> usize {
        let mut observers = self.state.observers.borrow_mut();
        observers.retain(|observer| observer.upgrade().is_some());
        observers.len()
    }

    pub(crate) fn emit<F>(&self, source: DiagnosticSource, timestamp: Duration, build: F)
    where
        F: FnOnce(u64) -> DiagnosticEvent,
    {
        let interested = self
            .state
            .observers
            .borrow()
            .iter()
            .filter_map(Weak::upgrade)
            .any(|observer| observer.filter.includes(source));
        if !interested {
            return;
        }

        let sequence = self.state.next_sequence.get();
        self.state.next_sequence.set(sequence.saturating_add(1));
        let record = DiagnosticRecord {
            sequence,
            timestamp,
            source,
            event: build(sequence),
        };
        self.state.observers.borrow_mut().retain(|observer| {
            let Some(observer) = observer.upgrade() else {
                return false;
            };
            if observer.filter.includes(source) {
                observer.enqueue(record.clone());
            }
            true
        });
    }

    pub(crate) fn emit_runtime_failure(
        &self,
        timestamp: Duration,
        instance: Option<&str>,
        error: &RuntimeFailure,
    ) {
        let kind = RuntimeFailureKind::from(error);
        self.emit(DiagnosticSource::RuntimeFailure, timestamp, |_| {
            DiagnosticEvent::RuntimeFailure {
                instance: instance.map(str::to_owned),
                kind,
            }
        });
    }
}

impl Default for RuntimeDiagnostics {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
struct DiagnosticObserverState {
    filter: DiagnosticFilter,
    capacity: usize,
    queue: RefCell<VecDeque<DiagnosticRecord>>,
    dropped: Cell<u64>,
}

impl DiagnosticObserverState {
    fn enqueue(&self, record: DiagnosticRecord) {
        let mut queue = self.queue.borrow_mut();
        if queue.len() >= self.capacity {
            self.dropped.set(self.dropped.get().saturating_add(1));
            return;
        }
        queue.push_back(record);
    }
}

/// The receiving side of one independently bounded diagnostics queue.
#[derive(Clone, Debug)]
pub struct DiagnosticObserver {
    state: Rc<DiagnosticObserverState>,
}

impl DiagnosticObserver {
    /// Removes and returns the oldest pending record without waiting.
    pub fn try_recv(&self) -> Option<DiagnosticRecord> {
        self.state.queue.borrow_mut().pop_front()
    }

    /// Alias for [`Self::try_recv`].
    pub fn try_next(&self) -> Option<DiagnosticRecord> {
        self.try_recv()
    }

    /// Returns the number of records dropped because this queue was full.
    pub fn dropped_count(&self) -> u64 {
        self.state.dropped.get()
    }

    /// Returns the monotonic sequence-gap count for this observer.
    pub fn gap_count(&self) -> u64 {
        self.dropped_count()
    }

    /// Returns the number of records currently buffered.
    pub fn pending_count(&self) -> usize {
        self.state.queue.borrow().len()
    }

    /// Returns the fixed queue capacity.
    pub fn capacity(&self) -> usize {
        self.state.capacity
    }

    /// Returns this observer's source filter.
    pub fn filter(&self) -> DiagnosticFilter {
        self.state.filter
    }
}

#[cfg(test)]
mod tests {
    use super::{DiagnosticEvent, DiagnosticFilter, DiagnosticSource, RuntimeDiagnostics};
    use std::time::Duration;

    #[test]
    fn does_not_build_a_record_without_an_interested_observer() {
        let diagnostics = RuntimeDiagnostics::new();
        let built = std::cell::Cell::new(false);

        diagnostics.emit(DiagnosticSource::Lifecycle, Duration::ZERO, |_| {
            built.set(true);
            DiagnosticEvent::AppReady
        });

        assert!(!built.get());
    }

    #[test]
    fn filters_sources_before_building_a_record() {
        let diagnostics = RuntimeDiagnostics::new();
        let observer = diagnostics
            .subscribe(DiagnosticFilter::only(DiagnosticSource::Invocation), 1)
            .expect("observer capacity is positive");
        let built = std::cell::Cell::new(false);

        diagnostics.emit(DiagnosticSource::Lifecycle, Duration::ZERO, |_| {
            built.set(true);
            DiagnosticEvent::AppReady
        });

        assert!(!built.get());
        assert!(observer.try_recv().is_none());
    }
}
