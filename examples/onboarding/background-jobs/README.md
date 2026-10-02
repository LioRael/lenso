# Local background jobs and notifications

This source-workspace example accepts a job, finishes it asynchronously, and
stores one notification in memory. Nothing sends email or calls an external
notification service. State is lost on restart. It is a small, bounded local
demonstration, not a durable queue.

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo build -p lenso-onboarding-background-jobs
python3 examples/onboarding/background-jobs/smoke.py
cargo run -p lenso-onboarding-background-jobs
```

Use the dynamically printed loopback URL:

```sh
curl -s -X POST "$APP_URL/jobs" -H 'content-type: application/json' \
  -d '{"message":"Your local report is ready","delay_ms":1000}'
curl -s "$APP_URL/jobs"
curl -s "$APP_URL/notifications"
```

POST returns `202` with a queued job before work completes. Poll `/jobs` for
`completed`, then `/notifications` for its message. Submit `"fail":true` to see
the asynchronous `failed` state and `requested_failure` code without a
notification. Empty messages and delays outside 20–5000 ms are rejected with
`400`; a generation retains at most 128 jobs. The example exposes only loopback
and does not add authentication.

The jobs Plugin owns job state, notification state, validation, and completion
policy. Its `LifecycleContext` supplies the generation's managed task scope.
Each task waits for readiness before running and observes cancellation during
its delay. Ctrl-C cancels pending work; lifecycle JSON on stdout records task
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

Dependencies deliberately use the same local workspace candidate as this
repository. These checks do not claim the npm CLI or registry-only onboarding
path passed. The durable Jobs Plugin has different persistence and authorization
requirements; consult <https://lenso.dev/docs/core/jobs> for that path.
