# Background jobs and local notifications

This small App accepts jobs, finishes them asynchronously, and stores local
notifications. The generated jobs Plugin owns validation, bounded retries,
cancellation and state. A separate health Plugin keeps `/health` available when
jobs are removed. No email provider, paid API or credentials are involved.

## Run with virtual time

From this source checkout, with its pinned Rust toolchain:

```sh
cargo run --locked -p lenso-onboarding-background-jobs --example simulate
cargo test --locked -p lenso-onboarding-background-jobs --test simulator \
  smoke_corpus_replays_without_wall_clock_waits -- --exact
```

The example and deterministic tests use `NativeWebHost::prepare_simulated()` and pass its exact
Plan and Registry to `TestApp::builder(...).with_registry(...).with_simulator(...)`.
Requests travel through `SimulatedWebHost`, real Web Ingress, the real Kernel and
the generated Plugin factory. They open no socket. There is one job
implementation for both execution modes.

The shared [smoke corpus](tests/smoke-corpus.json) is also consumed by the
retained Python socket/process smoke. The full `--test simulator` target runs
that real sentinel and compares its observable receipt with simulation. It is
unconditionally included in the existing `cargo test --workspace` CI gate;
the filtered command above is a development loop, not provider qualification.
Drop and shutdown-cancellation IDs come from actual task lifecycle observations.
Intermediate queued/running states are an allowed set; their scheduling trace
need not be identical across environments. See the
[measurement report](../../../docs/performance/simulator-smoke.md) for separate
compile and execution costs and the remaining CI/Store work.

The private `JobRuntime` accepts a clock, a timer and explicit delivery hooks.
Tests supply `TestSimulator::now` and `sleep_until` from the same Simulator that
runs the App. `advance` moves time; `pump` lets newly ready work execute.
Neither method consults wall time, and waiting on a future timer does not advance
time automatically. Tests use existing Simulator gates, resource freezing,
one-shot faults and receipts at the private delivery boundary.

Regression scenarios check exact deadlines, retry timing and exhaustion,
cancellation while a resource is frozen, state-file recovery, cancellation
surviving recovery, and a committed notification whose acknowledgement is lost.
Each checks business state plus task drop counts, admission closure and clean
shutdown. Repeating the same retry scenario must produce identical receipts.

## Run as a local server

From the repository root:

```sh
cargo build --locked -p lenso-onboarding-background-jobs
python3 examples/onboarding/background-jobs/smoke.py
cargo run --locked -p lenso-onboarding-background-jobs --bin lenso-onboarding-background-jobs
```

Use the dynamically printed loopback URL:

```sh
curl -s -X POST "$APP_URL/jobs" -H 'content-type: application/json' \
  -d '{"message":"Your local report is ready","delay_ms":1000}'
curl -s "$APP_URL/jobs"
curl -s "$APP_URL/notifications"
curl -s -X DELETE "$APP_URL/jobs/1"
```

POST returns `202` with a queued job before work completes. Poll `/jobs` for
`completed`, then `/notifications` for its message. Submit `"fail":true` to see
the asynchronous `failed` state and `requested_failure` code without a
notification. Empty messages and delays outside 20–5000 ms are rejected with
`400`; at most 128 jobs are retained. A transient private delivery error retries
up to three total attempts, with 100ms and 200ms backoffs. Explicit cancellation
is terminal and generates no notification. Cancelling an already terminal job
leaves that outcome intact. The example exposes only loopback and does not add
authentication.

The jobs Plugin owns job state, notification state, validation, and completion
policy. Its `LifecycleContext` supplies the generation's managed task scope.
Each task waits for readiness before running and observes cancellation during
its delay. The default mode keeps state in memory and a fresh server starts
empty. Ctrl-C cancels pending work; lifecycle JSON on stdout records task
drops, rejection of new work after shutdown begins, and clean Host shutdown.
The Host contributes HTTP Ingress and its actual loopback address. A separate,
stateless health Plugin serves `GET /health` so that the remaining App stays
observable when jobs are removed. Removing the jobs Plugin with
`--without-jobs` removes `/jobs`, `/notifications`, and all background work;
`/health` continues to return `"ok"`. A Host with no Endpoint at all instead
rejects startup with `MissingEndpoint`.

The Python standard-library smoke starts and stops real Host processes. It
checks immediate acceptance, later completion and notification, invalid input,
asynchronous failure, pending-job cancellation, task cleanup, fresh restart,
and Plugin removal. Every process has a startup and shutdown deadline.

## Recover an App from a local state file

```sh
cargo run --locked -p lenso-onboarding-background-jobs --bin lenso-onboarding-background-jobs -- \
  --state-file /tmp/lenso-background-jobs.json
```

This explicit mode saves Plugin-owned JSON snapshots by replacing a temporary
file. Reopening the path creates fresh runtime objects from file bytes; it does
not retain an old generation's `Rc` store. Pending work is rescheduled on the new
clock, with its attempt budget preserved. Terminal jobs remain terminal. The
completed job and its one notification are written in one snapshot before the
acknowledgement hook, so losing an acknowledgement does not duplicate the local
notification after recovery. Invalid snapshots fail startup.

Use one App per state path. This example is a bounded, single-process store;
it does not implement concurrent writers, multi-node queues, power-loss
qualification, external delivery transactions or exactly-once email. Recovery
does not count downtime against a saved delay. Disk operations are synchronous
and deliberately small. Removing the jobs Plugin removes its routes and tasks;
an explicitly selected state file remains owned by the operator.

These are source-workspace checks against the repository's exact lockfile.
They do not claim registry-only onboarding, publication, deployment, remote
services, or CI qualification. The optional framework change is only the public
Simulator timer forwarding method; no product state enters the Kernel.
