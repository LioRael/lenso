# External configuration for a built App

The built Host normally reads its App's `plugins/` directory. An operator can
authorize one versioned external source before `app start` resolves that App.
The source supplies values only; it cannot grant itself a Plugin Instance,
permission, binding, or writable field. Keep the policy outside App-authored
source and protect it as Host deployment input.

For a local file source, create an absolute-path policy such as:

```json
{
  "schema": "lenso.configuration-source-policy.v1",
  "source_reference": "production-settings",
  "max_stale_seconds": 300,
  "source": { "type": "file", "path": "/etc/my-app/configuration-snapshot.json" },
  "objects": [
    { "plugin_id": "company.agent", "instance_key": "default", "fields": ["model"] }
  ]
}
```

The source file is a regular, non-symlink JSON document:

```json
{
  "schema": "lenso.plugin-configuration-snapshot.v1",
  "revision": 1,
  "configurations": [
    { "plugin_id": "company.agent", "instance_key": "default", "toml": "model = 'example/model'\n" }
  ]
}
```

Run `lenso app start --from dist --configuration-policy /etc/my-app/policy.json`.
With a policy, `app start` supervises the generated Host for its lifetime:
it polls the source, stops the current Host if source proof becomes too old or
the Host policy changes, and stops it before activating a changed accepted
revision. For File/HTTPS sources on the supported supervised-start path, a
normal stop releases managed resources so you can restart without manually
clearing a marker. An accepted update can start its replacement only after that
cleanup completes and the source and policy still authorize startup. A failed
or forced stop instead requires the recovery steps below; no replacement is
started automatically.

This automatic path applies to the current Native, Wasm Component, Process V1/V2,
and Bun Adapter profiles only when their lifetime cleanup evidence is clean.
It does not cover Windows or the bootstrap Configuration Source Plugin path
described below. `app dev` has the additional restrictions below. The generated
Host may perform Kernel side effects before its Ready
marker; stopping it is not an atomic cross-system rollback or a pre-activation
gate. One-shot `--check` and terminal `-- ...` arguments are rejected with
`--configuration-policy` because they cannot provide continuous supervision.
Without a policy, `app start` retains its ordinary one-shot behavior.

A newer accepted source revision that resolves to the exact same Plugin Root
under the same Host policy renews the running Host's source proof without a
restart. It does not forge a new activation receipt: `config-status` keeps the
new `desired_revision` separate from the historical `last_activated_revision`,
sets `desired_matches_last_activated_root_and_policy` to true, and still marks
`pending_activation`. That field compares stored Root and policy identities;
it is not a live-process health check. A changed Root or policy still requires
stopping the old Host; a policy change requires fresh authorization rather than
reusing the old policy's permission to start.

Only one supervised `app start` may own a built distribution at a time. Before
starting a Host, the supervisor durably writes
`dist/.lenso/supervised-start.uncertain`. On the automatic path, it removes this
crash fence only after the generated Host reports completed managed cleanup for
that launch, exits successfully, and leaves no live members in its original
process group. A failed spawn that created no Host can also clear the marker.

Managed cleanup covers framework-managed tasks, lifecycle work, and resources,
including Adapter-owned child processes. It does not cover arbitrary Native or
Bun code deliberately spawning or escaping descendants. The generated Host
reports cleanup only after Kernel shutdown is `Clean`, binding guards are
released, and its local task set and runtime are dropped. Historical managed
cleanup failures remain disqualifying even if a later Plugin generation succeeds;
Process and Bun Adapters also track owned children across their entire lifetime.
Its receipt carries a
private per-generation environment token to distinguish launches, not to
authenticate against trusted code. This is not cgroup isolation or a sandbox;
see [ADR 0078](../../../docs/adr/0078-bound-automatic-recovery-to-managed-resource-retirement.md).

If the receipt is missing or has the wrong token, the Host exits abnormally,
cleanup times out or requires force, or the supervisor crashes, the marker
remains and a later supervised start refuses to run. To recover, first verify
that **every Host process and descendant for this exact distribution** has
stopped; checking only the former leader PID is insufficient. Then manually
remove the marker and restart. The CLI does not automatically kill processes
identified only by an old PID or clear an uncertain marker. Exit status zero
or disappearance of the former process group alone is not enough.

`lenso app config-sync --root dist --policy /etc/my-app/policy.json` performs
only source reconciliation, without starting the Host. Check the distribution
with `lenso app check --root dist` and inspect its selected Plugin Instances
with `lenso app show --root dist/intent`; the generated Host reads `dist/intent`
as its Plugin Root.

`max_stale_seconds` is a Host-policy limit, currently defaulting to 300 seconds
when omitted and accepting explicit values from 1 through 86400. A successful
validated full fetch or an accepted HTTPS 304 renews the proof for the exact
source, policy, snapshot revision, and Plugin Root. Transient outages keep the
current Host only while that proof is fresh; an outage past the limit stops it
instead of serving indefinitely. A process restart cannot reuse an old
in-memory proof to start offline: it needs a new accepted source response.
The in-process limit uses a monotonic clock, including time spent building,
checking, and waiting for Host readiness. A proof that expires while preparing
a replacement cannot authorize it. A policy edit or unverifiable policy
stops the active Host at the next supervisor check, without waiting for the
stale deadline. Detection is bounded by the check interval, not atomic with
the edit.

For HTTPS, replace `source` with
`{"type":"https","url":"https://config.example/snapshot","admitted_origins":["https://config.example/"]}`.
The fetch rejects non-HTTPS, redirects, proxies, private DNS results, oversized
responses, and unapproved origins. Both sources use the same field authorization,
schema validation, Proposal, and Plugin Root compare-and-swap publication path.
An HTTPS ETag cursor is saved only after the corresponding desired revision has
been accepted and published. A 304 then confirms that exact source has not
changed; an interrupted publication discards the cursor and fetches a complete
snapshot on recovery. The cursor is private Host state, not App configuration.
Changing the Host-owned policy (including field scopes or admitted origins)
invalidates that cursor and requires a complete snapshot to be reauthorized.

A Host can instead admit one exact pre-App Process V2 Configuration Source
Plugin. The operator supplies a verified **directory** Bundle and pins both its
manifest and executable digest in the same protected policy:

```json
{
  "schema": "lenso.configuration-source-policy.v1",
  "source_reference": "production-settings",
  "source": {
    "type": "plugin",
    "bundle": "/opt/my-app/bootstrap/config-source.lenso-plugin",
    "plugin_id": "company.config-source",
    "release_version": "1.0.0",
    "manifest_digest": "sha256:<exact-manifest-digest>",
    "artifact_digest": "sha256:<exact-process-artifact-digest>",
    "configuration": {"path": "/etc/my-app/configuration-snapshot.json"}
  },
  "objects": [
    {"plugin_id": "company.agent", "instance_key": "default", "fields": ["model"]}
  ]
}
```

The path and configuration are examples, not values supplied by the Plugin.
The Host verifies the Bundle and exact pins before starting its selected
Process V2 Artifact. It constructs a separate, short-lived two-Instance Plan
(`lenso.configuration.source@1` provider plus Host client), invokes `fetch`,
closes that generation, then binds the returned revision and values to the
Host-issued source identity. The response has no source identity, trust root,
scope, or approval fields. The ordinary typed Plugin Root proposal still
validates the result against `objects` and Host ceilings before publication;
the source Plugin cannot change its own App configuration. This bootstrap
Plan does not depend on the business App Plan, which is resolved afterward.
It is a trusted native Process Plugin, **not an OS sandbox** or a claim that
arbitrary third-party code is safe. The Process V2 wire currently limits the
complete response frame to 1 MiB, tighter than the File/HTTPS source limit.
This path fetches a complete snapshot on each poll; ETag/304 and push
subscriptions are supported only where explicitly described above, not by
the Plugin contract. The Host policy itself is the trust root for the exact
release; the digest pins are not marketplace signature verification.

This bootstrap Plugin runs under a separate owner from the generated App Host.
Until that owner implements retirement, supervised starts using a Plugin source
retain the crash fence even after a normal stop. Before restarting or activating
an update, verify all Host and bootstrap Plugin descendants have stopped, then
manually clear the marker as described above. App Host cleanup alone cannot
authorize automatic recovery of this source path.

For a local preview, run `lenso app dev --configuration-policy POLICY`. On Unix,
File/HTTPS sources without a separate frontend dev process use the same managed
retirement rule:

- Ctrl+C stops the Host and clears the recovery marker only after confirmed
  cleanup, so another `app dev` can start normally.
- An accepted changed Root first passes static `--prepare` checks while the old
  preview remains available. The supervisor then retires the old Host before
  launching its replacement. The actual new Host's Ready Gate is the only
  dynamic startup; there is no extra `--check` generation.
- A newer source revision resolving to the same Root renews authority without
  restarting or replacing the original activation receipt.
- Static preparation failures can be retried. Failed dynamic startup, missing
  cleanup confirmation, forced stop, and supervisor crashes retain the marker
  at the source App root's `.lenso/supervised-dev.uncertain`, outside disposable
  build generations. Before clearing it manually, verify all processes from the
  prior preview have stopped. SIGTERM is not the normal dev shutdown path.

The supervisor shortens a longer requested source poll interval to at most half
the stale limit. Ordinary source failures do not stop a still-fresh preview.
Expiry or detected policy revocation force-stops the active process groups,
withdraws any published backend URL, and retains the recovery marker.

A separately configured frontend dev process still supports initial startup,
but does not provide the required retirement acknowledgement. Such sessions
remain conservative: no automatic replacement, and the root recovery marker
remains even after normal shutdown. Bootstrap Configuration Source Plugins and
unsupported platforms also retain manual recovery. These limits are not OS
containment or atomic rollback of Plugin side effects.

The accepted desired revision is persisted in the built App's private
`intent/.lenso/configuration-source-state.json` before publication. A repeated or stale
revision cannot silently replace newer content; an interrupted publication is
reconciled only against the same snapshot. Once a distribution has a source,
`app start` requires its policy on every subsequent start, including after a
network outage. A failed initial fetch prevents startup. The state separates
accepted `desired` configuration from `last_activated`: the latter is written
only after supervised Host readiness, never by `--check` or source reconciliation.
It records the last successful activation, **not** a claim that a Generation
is still running. A new desired revision preserves the previous activation
record until the new Host is ready.
Use `lenso app config-status --root dist --json` to inspect those revision
numbers and whether a proposal is still pending publication or an accepted
update is pending activation. `lenso app facts --root dist --json` includes the
same status alongside the resolved Plugin and binding facts from `dist/intent`.
The status identifies the source kind, but omits configuration values, source
addresses, and digests; it does not report whether the last activated Host
process is currently running.
The receipt is historical and does not establish current process health or
freshness. Keep secret material with its provider:
the snapshot contains only authorized references for schema-marked sensitive
fields, and normal diagnostics do not print values.

An external Plugin Root used by the prepared TypeScript Host has a separate
operator path: `lenso app config-sync --root APP --host-build
DIST/.lenso/host-build.json --policy POLICY`. This copies only the exact
distribution Host authority into the external Root before publishing approved
fields. `lenso-host-runtime --configuration-policy POLICY` performs that sync
before its initial resolution and before each revision-fenced `reconcile`
request. A failed source sync or candidate transition does not report success;
the current healthy Generation remains selected. This private control path
does not automatically poll the source, and it is not yet wired into the
generated native Host or a public App-handle update operation.
After the prepared Host passes its Ready Gate or switches to a new Generation,
it asks the bundled resolver to record the exact resolved Plugin Root revision
as `last_activated`. The receipt is fenced against the currently accepted
desired revision, so a newer source update cannot be mistaken for an already
active Generation. If receipt writing fails, the healthy Generation stays
active, but the control response reports `activation_recorded: false`; without
a prior matching receipt, `config-status` remains pending. A later `reconcile`
can retry the receipt.
Inspect an external Root with `lenso app config-status --root APP --host-build
DIST/.lenso/host-build.json --json`. This is historical activation evidence,
not a live-process health assertion.
