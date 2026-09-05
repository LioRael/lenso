# Named Plugin authoring implementation status

This implementation follows ADRs 0073/0074 and the executable specification in
[#699](https://github.com/LioRael/lenso/issues/699). The first Rust Process and
TypeScript Bun Request delivery completed on 2026-09-05; [#695](https://github.com/LioRael/lenso/issues/695)
records its cross-repository releases and executable evidence. Authoring V2 is
profile-specific: this delivery does not claim TypeScript Stream/Event authoring
or support from an Adapter that has not explicitly adopted the profile.

## Implemented portable paths

- Plan schema 3 carries requirement identity, authoring version, runtime profile,
  and terminal policy. Schema 2 has an explicit decoding path that preserves old
  semantics and normalizes reserved requirement IDs. Supporting executors apply
  `host_essential`; other executors reject it before activation.
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

Old Adapters must explicitly opt into new authoring/profile pairs. Production
Rust Process and Bun Request profiles have owner-local execution evidence;
conformance alone does not certify another Adapter or interaction kind. Existing
version 1 request cancellation semantics remain unchanged.

## Delivery record

- [#700](https://github.com/LioRael/lenso/issues/700) records named requirement,
  selection, Transition, dormant-choice, compatibility, and routing evidence.
- [#701](https://github.com/LioRael/lenso/issues/701) records portable result,
  settlement, construction, and cleanup evidence.
- [#702](https://github.com/LioRael/lenso/issues/702) records Host-essential
  conformance and the supported Process fault-scope boundary.
- [lenso-examples#69](https://github.com/LioRael/lenso-examples/issues/69)
  records the installable Rust/TypeScript document-sync proof through Agent
  ToolProvider, two named Native Store accounts, and normal CLI operations.

Focused tests live in the Plan and runtime-conformance `named_dependencies`
integration suites and the Kernel `settlement` unit suite. Owner repositories
retain their complete workspace, portable Wasm compile, Adapter, package, and
clean-room release gates.
