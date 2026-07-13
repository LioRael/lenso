use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CallPolicyDeclaration {
    pub max_attempts: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub circuit_breaker: Option<CallPolicyCircuitBreaker>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concurrency: Option<CallPolicyConcurrency>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overload: Option<CallPolicyOverload>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<CallPolicyFallback>,
}

impl Default for CallPolicyDeclaration {
    fn default() -> Self {
        Self {
            max_attempts: 2,
            circuit_breaker: None,
            concurrency: None,
            overload: None,
            fallback: None,
        }
    }
}

impl CallPolicyDeclaration {
    #[must_use]
    pub fn validate(&self, retry_safe: bool) -> Vec<CallPolicyValidationIssue> {
        let mut issues = Vec::new();
        if self.max_attempts == 0 {
            issues.push(CallPolicyValidationIssue::new(
                "maxAttempts",
                "max_attempts_invalid",
            ));
        } else if self.max_attempts > 1 && !retry_safe {
            issues.push(CallPolicyValidationIssue::new(
                "maxAttempts",
                "unsafe_retry_policy",
            ));
        }
        if let Some(circuit) = &self.circuit_breaker {
            if circuit.failure_threshold == 0 {
                issues.push(CallPolicyValidationIssue::new(
                    "circuitBreaker.failureThreshold",
                    "circuit_failure_threshold_invalid",
                ));
            }
            if circuit.open_for_ms == 0 {
                issues.push(CallPolicyValidationIssue::new(
                    "circuitBreaker.openForMs",
                    "circuit_open_duration_invalid",
                ));
            }
            if circuit.half_open_max_calls == 0 {
                issues.push(CallPolicyValidationIssue::new(
                    "circuitBreaker.halfOpenMaxCalls",
                    "circuit_half_open_limit_invalid",
                ));
            }
        }
        if self
            .concurrency
            .as_ref()
            .is_some_and(|value| value.max_in_flight == 0)
        {
            issues.push(CallPolicyValidationIssue::new(
                "concurrency.maxInFlight",
                "concurrency_limit_invalid",
            ));
        }
        if self
            .overload
            .as_ref()
            .is_some_and(|value| value.max_in_flight == 0)
        {
            issues.push(CallPolicyValidationIssue::new(
                "overload.maxInFlight",
                "overload_limit_invalid",
            ));
        }
        if let Some(fallback) = &self.fallback {
            if fallback.handler.trim().is_empty() {
                issues.push(CallPolicyValidationIssue::new(
                    "fallback.handler",
                    "fallback_handler_required",
                ));
            }
            let unique = fallback.on.iter().copied().collect::<BTreeSet<_>>();
            if unique.len() != fallback.on.len() {
                issues.push(CallPolicyValidationIssue::new(
                    "fallback.on",
                    "fallback_trigger_duplicate",
                ));
            }
            if fallback.on.is_empty() {
                issues.push(CallPolicyValidationIssue::new(
                    "fallback.on",
                    "fallback_trigger_required",
                ));
            }
        }
        issues
    }

    #[must_use]
    pub fn fallback_for(&self, failure: CallPolicyFailure) -> Option<&CallPolicyFallback> {
        self.fallback
            .as_ref()
            .filter(|fallback| fallback.on.contains(&failure))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CallPolicyCircuitBreaker {
    pub failure_threshold: u32,
    pub open_for_ms: u64,
    pub half_open_max_calls: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CallPolicyConcurrency {
    pub max_in_flight: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CallPolicyOverload {
    pub max_in_flight: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CallPolicyFallback {
    pub handler: String,
    pub on: Vec<CallPolicyFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallPolicyFailure {
    CircuitOpen,
    BulkheadSaturated,
    OverloadRejected,
    DeadlineExpired,
    RetryableFailure,
    NonRetryableFailure,
    TransportFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallPolicyEvent {
    CircuitOpen,
    CircuitHalfOpen,
    CircuitOpened,
    CircuitRecovered,
    BulkheadSaturated,
    OverloadRejected,
    DeadlineExpired,
    FallbackApplied,
    RetryScheduled,
    CallCompleted,
    CallFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallPolicyTerminalOutcome {
    Completed,
    Failed,
    Rejected,
    Fallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallPolicyEvidence {
    pub events: Vec<CallPolicyEvent>,
    pub attempts: u32,
    pub terminal_outcome: CallPolicyTerminalOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_handler: Option<String>,
}

impl CallPolicyEvent {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CircuitOpen => "circuit_open",
            Self::CircuitHalfOpen => "circuit_half_open",
            Self::CircuitOpened => "circuit_opened",
            Self::CircuitRecovered => "circuit_recovered",
            Self::BulkheadSaturated => "bulkhead_saturated",
            Self::OverloadRejected => "overload_rejected",
            Self::DeadlineExpired => "deadline_expired",
            Self::FallbackApplied => "fallback_applied",
            Self::RetryScheduled => "retry_scheduled",
            Self::CallCompleted => "call_completed",
            Self::CallFailed => "call_failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallPolicyValidationIssue {
    pub path: String,
    pub code: &'static str,
}

impl CallPolicyValidationIssue {
    fn new(path: impl Into<String>, code: &'static str) -> Self {
        Self {
            path: path.into(),
            code,
        }
    }
}

pub trait CallPolicyClock: Send + Sync {
    fn now_ms(&self) -> u64;
}

#[derive(Debug, Default)]
pub struct SystemCallPolicyClock;

impl CallPolicyClock for SystemCallPolicyClock {
    fn now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

#[derive(Debug)]
pub struct ManualCallPolicyClock {
    now_ms: AtomicU64,
}

impl ManualCallPolicyClock {
    #[must_use]
    pub const fn new(now_ms: u64) -> Self {
        Self {
            now_ms: AtomicU64::new(now_ms),
        }
    }

    pub fn advance_ms(&self, duration_ms: u64) {
        self.now_ms.fetch_add(duration_ms, Ordering::SeqCst);
    }
}

impl CallPolicyClock for ManualCallPolicyClock {
    fn now_ms(&self) -> u64 {
        self.now_ms.load(Ordering::SeqCst)
    }
}

#[derive(Clone)]
pub struct CallPolicyRuntime {
    inner: Arc<RuntimeInner>,
}

struct RuntimeInner {
    clock: Arc<dyn CallPolicyClock>,
    states: Mutex<BTreeMap<String, OperationState>>,
}

#[derive(Debug, Default)]
struct OperationState {
    consecutive_failures: u32,
    circuit_open_until_ms: Option<u64>,
    half_open_in_flight: u32,
    caller_in_flight: u32,
    receiver_in_flight: u32,
}

impl std::fmt::Debug for CallPolicyRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CallPolicyRuntime")
            .finish_non_exhaustive()
    }
}

impl Default for CallPolicyRuntime {
    fn default() -> Self {
        Self::new(Arc::new(SystemCallPolicyClock))
    }
}

impl CallPolicyRuntime {
    #[must_use]
    pub fn new(clock: Arc<dyn CallPolicyClock>) -> Self {
        Self {
            inner: Arc::new(RuntimeInner {
                clock,
                states: Mutex::new(BTreeMap::new()),
            }),
        }
    }

    #[must_use]
    pub fn now_ms(&self) -> u64 {
        self.inner.clock.now_ms()
    }

    pub fn begin_call(
        &self,
        operation_key: impl Into<String>,
        policy: &CallPolicyDeclaration,
    ) -> Result<CallPolicyPermit, CallPolicyEvent> {
        let operation_key = operation_key.into();
        let mut states = self
            .inner
            .states
            .lock()
            .expect("call policy state poisoned");
        let state = states.entry(operation_key.clone()).or_default();
        let now_ms = self.inner.clock.now_ms();
        let mut half_open = false;
        let mut events = Vec::new();
        if let Some(circuit) = &policy.circuit_breaker {
            if let Some(open_until) = state.circuit_open_until_ms {
                if now_ms < open_until || state.half_open_in_flight >= circuit.half_open_max_calls {
                    return Err(CallPolicyEvent::CircuitOpen);
                }
                half_open = true;
                state.half_open_in_flight += 1;
                events.push(CallPolicyEvent::CircuitHalfOpen);
            }
        }
        if policy
            .concurrency
            .as_ref()
            .is_some_and(|limit| state.caller_in_flight >= limit.max_in_flight)
        {
            if half_open {
                state.half_open_in_flight -= 1;
            }
            return Err(CallPolicyEvent::BulkheadSaturated);
        }
        state.caller_in_flight += 1;
        drop(states);
        Ok(CallPolicyPermit {
            runtime: self.clone(),
            operation_key,
            policy: policy.clone(),
            mode: PermitMode::Caller { half_open },
            events,
            released: false,
        })
    }

    pub fn admit(
        &self,
        operation_key: impl Into<String>,
        policy: &CallPolicyDeclaration,
    ) -> Result<CallPolicyPermit, CallPolicyEvent> {
        let operation_key = operation_key.into();
        let mut states = self
            .inner
            .states
            .lock()
            .expect("call policy state poisoned");
        let state = states.entry(operation_key.clone()).or_default();
        if policy
            .overload
            .as_ref()
            .is_some_and(|limit| state.receiver_in_flight >= limit.max_in_flight)
        {
            return Err(CallPolicyEvent::OverloadRejected);
        }
        state.receiver_in_flight += 1;
        drop(states);
        Ok(CallPolicyPermit {
            runtime: self.clone(),
            operation_key,
            policy: policy.clone(),
            mode: PermitMode::Receiver,
            events: Vec::new(),
            released: false,
        })
    }
}

#[derive(Debug, Clone, Copy)]
enum PermitMode {
    Caller { half_open: bool },
    Receiver,
}

#[derive(Debug)]
pub struct CallPolicyPermit {
    runtime: CallPolicyRuntime,
    operation_key: String,
    policy: CallPolicyDeclaration,
    mode: PermitMode,
    events: Vec<CallPolicyEvent>,
    released: bool,
}

impl CallPolicyPermit {
    #[must_use]
    pub fn events(&self) -> &[CallPolicyEvent] {
        &self.events
    }

    #[must_use]
    pub fn success(mut self) -> Vec<CallPolicyEvent> {
        if let PermitMode::Caller { half_open } = self.mode {
            let mut states = self
                .runtime
                .inner
                .states
                .lock()
                .expect("call policy state poisoned");
            let state = states
                .get_mut(&self.operation_key)
                .expect("permit state exists");
            state.consecutive_failures = 0;
            if half_open {
                state.circuit_open_until_ms = None;
                self.events.push(CallPolicyEvent::CircuitRecovered);
            }
        }
        self.release();
        self.events.clone()
    }

    #[must_use]
    pub fn success_after(self, earlier_events: Vec<CallPolicyEvent>) -> Vec<CallPolicyEvent> {
        ordered_events(self.success(), earlier_events)
    }

    #[must_use]
    pub fn failure(mut self) -> Vec<CallPolicyEvent> {
        if let (PermitMode::Caller { half_open }, Some(circuit)) =
            (self.mode, self.policy.circuit_breaker.as_ref())
        {
            let mut states = self
                .runtime
                .inner
                .states
                .lock()
                .expect("call policy state poisoned");
            let state = states
                .get_mut(&self.operation_key)
                .expect("permit state exists");
            state.consecutive_failures = state.consecutive_failures.saturating_add(1);
            if half_open || state.consecutive_failures >= circuit.failure_threshold {
                state.circuit_open_until_ms = Some(
                    self.runtime
                        .inner
                        .clock
                        .now_ms()
                        .saturating_add(circuit.open_for_ms),
                );
                self.events.push(CallPolicyEvent::CircuitOpened);
            }
        }
        self.release();
        self.events.clone()
    }

    #[must_use]
    pub fn failure_after(self, earlier_events: Vec<CallPolicyEvent>) -> Vec<CallPolicyEvent> {
        ordered_events(self.failure(), earlier_events)
    }

    fn release(&mut self) {
        if self.released {
            return;
        }
        let mut states = self
            .runtime
            .inner
            .states
            .lock()
            .expect("call policy state poisoned");
        let state = states
            .get_mut(&self.operation_key)
            .expect("permit state exists");
        match self.mode {
            PermitMode::Caller { half_open } => {
                state.caller_in_flight = state.caller_in_flight.saturating_sub(1);
                if half_open {
                    state.half_open_in_flight = state.half_open_in_flight.saturating_sub(1);
                }
            }
            PermitMode::Receiver => {
                state.receiver_in_flight = state.receiver_in_flight.saturating_sub(1);
            }
        }
        self.released = true;
    }
}

fn ordered_events(
    mut state_events: Vec<CallPolicyEvent>,
    mut earlier_events: Vec<CallPolicyEvent>,
) -> Vec<CallPolicyEvent> {
    let terminal_transition = state_events.last().copied().filter(|event| {
        matches!(
            event,
            CallPolicyEvent::CircuitOpened | CallPolicyEvent::CircuitRecovered
        )
    });
    if terminal_transition.is_some() {
        state_events.pop();
    }
    state_events.append(&mut earlier_events);
    if let Some(event) = terminal_transition {
        state_events.push(event);
    }
    state_events
}

impl Drop for CallPolicyPermit {
    fn drop(&mut self) {
        self.release();
    }
}
