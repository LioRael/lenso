# Plugin verification

Prove the Plugin at four layers:

1. **Package:** format, lint, unit tests, schema/default validation, generated
   descriptor freshness, and exact dependency locks.
2. **Runtime:** success, Domain Error, Runtime Failure or startup rejection,
   cancellation, limits, and lifecycle cleanup through the real Adapter.
3. **Product:** `lenso app check`, a real consumer invocation through the
   Host-derived Plan, and visible Plugin Root configuration.
4. **Deletion:** disable or remove the Plugin, resolve again, and prove the
   remaining App starts without a Kernel branch or hidden registration.

For a portable package, `lenso plugin pack` validates created bytes and `lenso
plugins add` validates received bytes. Do not add a separate `plugin verify`
step. A live Host must keep current routing unchanged until the candidate
Generation passes readiness.
