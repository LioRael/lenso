# Make resilience an explicit Service Contract

> **Status: superseded for vNext.** Retained as historical and v0.3.x
> maintenance context. [ADR 0030](0030-rebuild-lenso-as-a-local-first-modular-runtime.md)
> and ADRs 0031 onward are normative for vNext.

Every cross-Service operation will carry an explicit Call Policy rather than inheriting hidden global retry behavior. Policies define the end-to-end Deadline, retry eligibility, idempotency, circuit breaking, concurrency isolation, overload handling, and any business fallback; undeclared write operations are not retried automatically, and receiving or calling Services enforce policies locally without synchronous System Plane decisions.

## Consequences

- Service Contract checks reject unsafe combinations such as retryable writes without idempotency semantics.
- Each hop propagates the remaining Deadline instead of starting a new timeout budget.
- Circuit, concurrency, and rate state remain local to the Data Plane participant.
- Runtime Console records timeout, retry, circuit-open, overload, and fallback evidence against the business operation.
- The System Plane may evaluate policy conformance during planning or release without executing the policy at runtime.
