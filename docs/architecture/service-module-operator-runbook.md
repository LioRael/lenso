# Service Operator Runbook

Use this runbook when a Lenso service is installed but its runtime state is
unclear. A Provider-mode Service exposes its protocol endpoint and Modules while
the Host owns auth, proxy policy, runtime queues, Runtime Story, Remote Calls,
and Technical Operations.

## Fast Path

```sh
lenso service list
lenso service status <provider> <service>
lenso service logs <provider> <service> --tail 100
lenso service export --module <provider> --format compose
lenso service doctor <module> --json
```

If the service is not running:

```sh
lenso service start <provider> <service>
lenso service logs <provider> <service> --tail 100
```

After applying a reviewed Module or Service Installation plan, restart the API
and worker so the Host recompiles the Provider Runtime Plan from the exact
target-owned artifacts.

For a non-static endpoint source, the embedding Host must register an endpoint
resolver under the exact source or adapter ID in `HostComposition`. For an
identity policy with credential references, it must register a credential
resolver under the exact trust profile. Production composition includes only
the `bearer_env` profile: configure exactly one `env://UPPERCASE_ENV_NAME`
reference and provision that variable in the API and worker environments.
Missing adapters, unsafe environment names, empty credentials, and endpoint
candidates outside the plan's allowed bindings all fail startup. Doctor and
Debug output show only `auth_configured`; they never print the resolved value.

## Status Table

| Status | Meaning | CLI check | Console evidence | Fix |
| --- | --- | --- | --- | --- |
| `ready` | The source is configured, the manifest is loaded, and service readiness checks pass. | `lenso service doctor <module> --json` | Modules shows service ready. | None. |
| `restart_pending` | Desired config changed after the current API/worker process started. | Doctor shows desired vs running source. | Modules shows restart pending. | Restart API and worker. |
| `configured_not_loaded` | The host has a configured source but did not load module metadata. | Doctor source exists; module metadata absent. | Modules shows configured but not loaded. | Restart; then inspect manifest errors. |
| `manifest_unreachable` | The host cannot fetch the module manifest. | Doctor manifest status is unreachable. | Modules shows manifest unreachable. | Start the service or fix the base URL. |
| `service_not_ready` | A declared service process is not passing its ready URL. | `lenso service status <provider> <service>` and `lenso service logs <provider> <service> --tail 100` | Modules shows service not ready. | Start the service or inspect local logs. |
| `missing_config` | A host-started service declares required env that is absent from `.env`. | `lenso service verify <manifest> --env-file .env --json` | Services shows missing config. | Set the env value and restart API/worker. |
| `stale_state` | Lock or pid files exist but the ready URL is failing. | Doctor lists lock or pid paths. | Modules shows stale state. | Stop the service, then remove stale files if needed. |
| `not_configured` | The target has no exact active Service Installation for the locked Module export. | Doctor has no installation entry. | Module is absent or install state is empty. | Apply the reviewed Service Installation plan. |
| `resolver_missing` | The Provider plan names an endpoint source ID or trust profile that the Host did not compose. | Startup error names the missing adapter ID. | Provider Service remains unavailable. | Register the exact adapter in `HostComposition`, then restart API and worker. |

Services can also declare `compatibility`, `statusUrl` or `statusPath`,
`deployment`, and `install.services` metadata. The host records standard
status checks in `.lenso/service-health.json` and Console shows the recent
health history without taking over process orchestration.

Local logs are only captured for services started by `lenso service start` or
host-started managed services. They live under
`.lenso/service-logs/<provider>/<service>.log` in the host repo and are not a
deployment log store.

Provider upgrades should go through a release plan when the service is already
installed:

```sh
lenso service release plan <provider> <manifest-or-package> --output .lenso/<provider>.release-plan.json
lenso service policy check .lenso/<provider>.release-plan.json --fail-on breaking
lenso service release apply .lenso/<provider>.release-plan.json
```

The plan records current and candidate manifest references, module/capability
and operation diffs, env/config changes, risk, restart requirement, and next
action. Apply writes `.lenso/service-releases.json`; Console Services renders
the latest release and the recent provider history next to health, lifecycle,
operations, and config state.

Module composition operations initiated from Runtime Console can also own the
deployment step. The target materializes exact Service actions in
`.lenso/module-planning-context.json`; plan preview copies the selected adapter
and action into the immutable Module Change Plan before review.

The desired Service topology is stored independently at
`.lenso/environments/<environment-id>/service-installations.json` using
`lenso.service-installations.v1`. Runtime Console and other management clients
use the same backend API to read the set, preview an immutable install or
uninstall plan, and apply it with `service.manage`. Apply uses revision plus
state-digest CAS and writes an idempotent receipt below
`.lenso/environments/<environment-id>/service-install-receipts/`. A receipt
with `applied_needs_attention` means desired state was committed but fresh
identity and readiness observations are still required.

An embedding Host composes this same manager into its target-owned System Plane
with `lenso_bootstrap::compose_host_system_plane_runtime`, then passes the
result to `lenso_api::try_build_router_with_composition_and_system_plane`.
The runtime is scoped to that Router and registers `service-installations` as a
real Capability Provider. Production deployments must supply their own Workload
Identity, delegated-context, and authenticated TLS transport adapters; the
development sandbox providers are never selected implicitly.

Production Hosts call `lenso_api::run_production_system_plane` with a bound TCP
listener, Host context and composition, production configuration, and shutdown
future. Configuration supplies the Workload API endpoint, trust domain, stable
Service Principal, and one or more exact `(issuer, verification method,
Ed25519 public key)` records. The helper composes the runtime, builds the Router,
serves rotating X.509-SVID mTLS, derives `AuthenticatedTransportBinding` from
the verified peer certificate, restricts the listener to `/system-plane/v1/*`,
and shuts down its Workload API sources after graceful server termination.
`compose_host_production_system_plane_runtime` and
`try_build_router_with_production_system_plane` remain lower-level embedding
seams, but their returned runtime must be kept alive and explicitly shut down.
The normal API listener deliberately does not mount a production System Plane,
and proxy or caller headers are not accepted as transport proof.

The default `lenso::host::run_api_from_env_with_composition` startup can own
both listeners. It remains Data Plane-only unless
`LENSO_SYSTEM_PLANE_ENABLED=true`. When enabled, configure these required
values:

```dotenv
LENSO_SYSTEM_PLANE_BIND=127.0.0.1:3443
LENSO_SYSTEM_PLANE_SYSTEM_ID=system-production
LENSO_SPIFFE_TRUST_DOMAIN=example.org
SPIFFE_ENDPOINT_SOCKET=unix:///run/spire/sockets/agent.sock
LENSO_SYSTEM_PLANE_DELEGATED_KEYS_JSON=[{"issuer":"console","verificationMethod":"key-1","publicKeyBase64url":"..."}]
```

The delegated-key array must be non-empty and rejects unknown fields. Optional
values select the stable Service ID and Principal, positive Service revision,
positive plan lifetime, runtime and Enrollment state paths, and workspace root;
see `.env.example` for their exact names and defaults. Service environment
defaults to `APP_ENV`. The core protocol version, canonical schema digest, and
supported capability features are framework-owned rather than caller-authored.
Startup rejects incomplete or malformed configuration before connecting to the
database. It binds the Data Plane first, then the independent System Plane
socket, and starts neither server if either bind fails. A process signal or an
exit from either server requests graceful shutdown of both and closes the live
SPIFFE Workload API sources.

A newly managed Service can import an owner-approved Enrollment during startup
without exposing enrollment over the network:

```dotenv
LENSO_SYSTEM_PLANE_ENROLLMENT_RECEIPT_FILE=/run/secrets/lenso/enrollment-receipt.json
LENSO_SYSTEM_PLANE_ENROLLMENT_VERIFICATION_EVIDENCE_DIGEST=sha256:...
```

Provision both values together only after the Service owner has verified the
Receipt signature and approval through the owning trust process. Startup parses
the strict Receipt contract and checks its exact Service identity, configured
SPIFFE trust domain, and every delegated issuer/key-method pair before writing
the Service-owned Enrollment Record atomically. Replaying the exact Receipt and
evidence is safe across restarts. A changed evidence digest, same-revision
replacement, conflicting Console authority, or missing local verification key
fails closed. Authority transfer still requires explicit local revocation and a
newer verified Receipt; no remote System Plane endpoint can activate, transfer,
or revoke Enrollment.

The production delegated-context adapter accepts URL-safe, unpadded Base64
encodings of raw 32-byte Ed25519 public keys. Configure each key under the exact
Console issuer and verification method admitted by the Service Enrollment.
During rotation, install both public-key pairs before issuing with the new
method, then remove the retired pair after its last credential has expired. The
Service receives verification keys only; the Console signing key never crosses
the System Plane boundary.

During Service startup, System Plane runtime composition resumes all durably
accepted or running management Operations before exposing its Router. Providers
must therefore treat `operation_id` as their effect idempotency identity. Once
the kernel has recorded its internal completion checkpoint, recovery only
rebuilds terminal evidence and Operation state; it does not invoke the Provider
again. Missing capability registration, unreadable state, or an invalid
checkpoint fails startup instead of silently discarding accepted work.

Each capability has its own monotonic Operation Evidence sequence. Consumers
must persist the returned opaque `nextCursor` and continue until it equals the
page `watermark`; pages are intentionally bounded. A typed sequence gap requires
a fresh capability snapshot; the returned last-verified cursor identifies the
proven continuity boundary. Payload or identity verification failure is Service
state corruption and requires local repair—it must not be interpreted as
observed business drift.

- `local` and `kubernetes` actions execute a target-owned program plus argv
  without a shell. An optional working directory must remain inside the Host
  workspace.
- `externally_managed` actions do not run infrastructure commands. They verify
  an exact content-addressed deployment receipt produced by the owning platform.
- A missing action, failed command, stale receipt, or digest mismatch blocks the
  durable Module operation after preserving the reviewed desired Service
  installation. After correcting target-owned state, the operator retries the
  same operation instead of recording a successful no-op.
- Removing a Module only changes Module composition. It deliberately preserves
  the Service installation and its observations; uninstall the Service through
  a separate reviewed Service Installation plan.

Successful actions append content-addressed evidence below
`.lenso/module-management/effect-evidence/`. That evidence is part of the
operation journal and prevents a crash-resume from repeating a completed
deployment action.

## Boundaries

The service may own its process, language, deployment package, and module-local
storage. It should not write host runtime tables, consume host
outbox rows directly, receive browser bearer tokens, or bypass host capability
checks. All user-facing evidence should still flow through the host: Runtime
Console, Remote Calls, Runtime Story, and Technical Operations.

## Minimal Proof

The recommended proof path is the support-ticket service in
`lenso-examples`:

```sh
pnpm start:support-ticket
lenso service install http://127.0.0.1:4110/lenso/service/v1/manifest
lenso service doctor support-suite-provider --json
```

Use `pnpm host-api-smoke:support-ticket` for the one full host proof when
validating a release slice.
