use lenso_service::{
    CallPolicyCircuitBreaker, CallPolicyConcurrency, CallPolicyDeclaration, CallPolicyEvent,
    CallPolicyFailure, CallPolicyFallback, CallPolicyOverload, CallPolicyRuntime,
    ManualCallPolicyClock,
};
use std::sync::Arc;

fn policy() -> CallPolicyDeclaration {
    CallPolicyDeclaration {
        max_attempts: 2,
        circuit_breaker: Some(CallPolicyCircuitBreaker {
            failure_threshold: 2,
            open_for_ms: 100,
            half_open_max_calls: 1,
        }),
        concurrency: Some(CallPolicyConcurrency { max_in_flight: 1 }),
        overload: Some(CallPolicyOverload { max_in_flight: 1 }),
        fallback: Some(CallPolicyFallback {
            handler: "support.cached_sla".to_owned(),
            on: vec![CallPolicyFailure::CircuitOpen],
        }),
    }
}

#[test]
fn call_policy_validation_is_deterministic() {
    let invalid = CallPolicyDeclaration {
        max_attempts: 0,
        circuit_breaker: Some(CallPolicyCircuitBreaker {
            failure_threshold: 0,
            open_for_ms: 0,
            half_open_max_calls: 0,
        }),
        concurrency: Some(CallPolicyConcurrency { max_in_flight: 0 }),
        overload: Some(CallPolicyOverload { max_in_flight: 0 }),
        fallback: Some(CallPolicyFallback {
            handler: String::new(),
            on: vec![
                CallPolicyFailure::CircuitOpen,
                CallPolicyFailure::CircuitOpen,
            ],
        }),
    };

    assert_eq!(
        invalid
            .validate(false)
            .into_iter()
            .map(|issue| (issue.path, issue.code))
            .collect::<Vec<_>>(),
        vec![
            ("maxAttempts".to_owned(), "max_attempts_invalid"),
            (
                "circuitBreaker.failureThreshold".to_owned(),
                "circuit_failure_threshold_invalid"
            ),
            (
                "circuitBreaker.openForMs".to_owned(),
                "circuit_open_duration_invalid"
            ),
            (
                "circuitBreaker.halfOpenMaxCalls".to_owned(),
                "circuit_half_open_limit_invalid"
            ),
            (
                "concurrency.maxInFlight".to_owned(),
                "concurrency_limit_invalid"
            ),
            ("overload.maxInFlight".to_owned(), "overload_limit_invalid"),
            ("fallback.handler".to_owned(), "fallback_handler_required"),
            ("fallback.on".to_owned(), "fallback_trigger_duplicate"),
        ]
    );
}

#[test]
fn controlled_time_proves_open_half_open_and_recovery() {
    let clock = Arc::new(ManualCallPolicyClock::new(1_000));
    let runtime = CallPolicyRuntime::new(clock.clone());
    let policy = policy();

    let _ = runtime
        .begin_call("support:GetSla", &policy)
        .unwrap()
        .failure();
    let events = runtime
        .begin_call("support:GetSla", &policy)
        .unwrap()
        .failure();
    assert_eq!(events, vec![CallPolicyEvent::CircuitOpened]);
    assert_eq!(
        runtime.begin_call("support:GetSla", &policy).unwrap_err(),
        CallPolicyEvent::CircuitOpen
    );

    clock.advance_ms(100);
    let probe = runtime.begin_call("support:GetSla", &policy).unwrap();
    assert_eq!(probe.events(), &[CallPolicyEvent::CircuitHalfOpen]);
    assert_eq!(
        runtime.begin_call("support:GetSla", &policy).unwrap_err(),
        CallPolicyEvent::CircuitOpen
    );
    assert_eq!(
        probe.success(),
        vec![
            CallPolicyEvent::CircuitHalfOpen,
            CallPolicyEvent::CircuitRecovered
        ]
    );

    assert!(runtime.begin_call("support:GetSla", &policy).is_ok());
}

#[test]
fn bulkhead_and_receiver_overload_reject_without_machine_exhaustion() {
    let runtime = CallPolicyRuntime::new(Arc::new(ManualCallPolicyClock::new(0)));
    let policy = policy();

    let caller_permit = runtime.begin_call("support:GetSla", &policy).unwrap();
    assert_eq!(
        runtime.begin_call("support:GetSla", &policy).unwrap_err(),
        CallPolicyEvent::BulkheadSaturated
    );
    drop(caller_permit);

    let receiver_permit = runtime.admit("support:GetSla", &policy).unwrap();
    assert_eq!(
        runtime.admit("support:GetSla", &policy).unwrap_err(),
        CallPolicyEvent::OverloadRejected
    );
    drop(receiver_permit);
    assert!(runtime.admit("support:GetSla", &policy).is_ok());
}
