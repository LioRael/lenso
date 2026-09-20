# Workers target qualification harness

This directory contains no passing target receipt. It documents the executable
qualification boundary in
[`experiments/workers-g2/target-qualification.manifest.json`](../../../experiments/workers-g2/target-qualification.manifest.json).

The harness executes two real local `workerd` suites against one generated G2
Rust/Wasm artifact:

- W02 retains stream and WebSocket leases while short-request generations rotate,
  and covers cancellation, quarantine and cleanup failures.
- The target-local suite traps the generated Wasm module while a Host callback
  remains pending, proves it cannot deliver late into the abandoned generation,
  then proves a fresh generated instance handles a real ingress request. It also
  exercises a real `ReadableStream` body deadline and a Workerd service-binding
  callback's failure/timeout path.

The service binding is a controlled lifecycle backend, not PostgreSQL or
Hyperdrive. Runtime deliberately forwards Auth's private callback JSON as opaque
strings; Auth does not receive a binding name, credential or connection detail.

The output report labels the result `local-workerd-passed-external-gates-pending`
until it receives source-backed external receipts for Auth callback composition,
real D1 failure, external client disconnect, and real Hyperdrive/PostgreSQL
failure. Those receipts need their own task-owned deployed Worker resources and
must not contain credentials, connection strings, binding names, request bodies
or tokens.

`auth-postgres-cohort.mjs` provides the repeatable temporary cross-repository
step: it accepts an explicit Auth source path and an Auth-owned local-workerd
composition command, records the source snapshot, and refuses to emit a receipt
unless that command reports real create/consume/revoke/factory-secrecy cases.
It creates no permanent path dependency and does not substitute a JavaScript
transport for Auth's generated factory.

Environment labels are non-interchangeable:

- **Miniflare:** no claim from this harness.
- **local workerd:** only the executable local cases above.
- **deployed Worker:** only a receipt from that task-owned deployed configuration.
- **production deployment:** separate rollout, monitoring, capacity/resource
  budget, and operations approval.
