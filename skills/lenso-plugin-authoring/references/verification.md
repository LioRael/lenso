# Plugin verification

Prove the Plugin at four layers:

1. **Contract:** Schema/default validation, Capability identities, requirement
   cardinality, lifecycle/state semantics, and generated projection freshness.
2. **Implementation:** format, lint, unit tests, exact dependency locks,
   executable Artifact/linked factory validation, and target compile or
   typecheck for every published implementation.
3. **Runtime:** success, Domain Error, Runtime Failure or startup rejection,
   cancellation, limits, and lifecycle cleanup through the real Adapter.
4. **Product:** `lenso app check`, a real consumer invocation through the
   Host-derived Plan, and visible Plugin Root configuration.
5. **Deletion:** disable or remove the Plugin, resolve again, and prove the
   remaining App starts without a Kernel branch or hidden registration.

When one Release publishes multiple implementations, run the same Contract
vectors against each: success, every changed Domain Error, Runtime Failure
classification, cancellation/deadline, configuration, lifecycle, and state
compatibility. Prove Host selection is deterministic, unsupported targets fail
before readiness, and failure of the selected implementation does not trigger
fallback.

For a portable package, `lenso plugin pack` validates created bytes and `lenso
plugins add` validates received bytes. Do not add a separate `plugin verify`
step. A live Host must keep current routing unchanged until the candidate
Generation passes readiness.
