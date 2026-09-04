# Named Plugin authoring implementation status

This working implementation follows ADR 0073 and the executable specification
in [#699](https://github.com/LioRael/lenso/issues/699). It does not certify the
complete authoring V2 runtime profile or close #700 / #701.

## Implemented portable paths

- Plan schema 3 carries requirement identity, authoring version, runtime profile,
  and terminal policy. Schema 2 has an explicit decoding path that preserves old
  semantics and normalizes reserved requirement IDs. `host_essential` is rejected
  until its runtime implementation lands.
- Contracts, Descriptors, Host bindings, prepared endpoint bindings, dependency
  views, and invocation diagnostics preserve consumer-local requirement IDs.
  Existing generated clients accept `dependencies.requirement("source")` without
  a new client interface. Capability-only lookup rejects multiple declarations,
  including absent optional requirements and names selecting the same provider.
- Pure Root proposal and resolution support constrained, selectable one/optional
  dependencies. Startup requires materialized choices; invalid saved choices do
  not fall back. Explicit optional absence and dormant intent are retained.
- Version 2 request execution owns its admission permits independently of the
  caller. Pending work moves to a Driver-owned task before yielding. Cancellation
  and caller drop do not release execution capacity. An Adapter can retain an
  `ExecutionLease` beyond a terminal reply and explicitly settle it on observed
  termination. Dropping that lease is not proof of termination.
- Version 2 startup is Driver-owned after its first poll. Dropping the caller
  only cancels the attempt. Construction is a distinct generated lifecycle
  phase before activation; late success goes through startup rollback and stop
  exactly once. The controlled startup API carries the Host cleanup timeout;
  caller timeout, rollback, and late-result cleanup share one absolute deadline.
  Stop receives a fresh cleanup cancellation token and the remaining budget.
- Shutdown and replacement wait for tracked request, Stream, and Event execution
  to settle before stopping or replacing a generation. Named request handles
  share provider operation capacity as well as their individual binding limits.
  Dropped callers do not release execution ownership, and cleanup timeout keeps
  the generation retained and prevents a second stop or overlapping replacement.
- Managed task scopes discard completed handles during normal operation and keep
  incomplete join handles after a cleanup timeout. Primary lifecycle failures
  remain the reported failure when secondary cleanup also fails.
- `host_essential` validates sorted Host roots and an exact closure recomputed
  only through transitive `one` requirements. Runtime supervision uses that
  closure for terminal exhaustion while still activating every selected Plugin.

Old Adapters must explicitly opt into new authoring/profile pairs. The
conformance Adapter opts in to exercise these paths; that is not certification
of a complete production Adapter or SDK. Existing version 1 request cancellation
semantics remain unchanged.

## Remaining before the core cohort is releasable

- Add explicit acceptance mapping for the remaining Transition/dormant-choice
  diagnostics and shared physical-process fault-scope boundaries.
- Add all remaining CORE and LIFE acceptance evidence to #700 / #701. Keep both
  issues open and do not publish partial authoring V2 guarantees.

Focused tests live in the Plan and runtime-conformance `named_dependencies`
integration suites and the Kernel `settlement` unit suite. The existing complete
workspace regression and both portable Wasm target checks remain required.
