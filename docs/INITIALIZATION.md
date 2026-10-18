# Explicit application initialization

This is an opt-in source candidate, not a released API. `@lenso/core/initialization`
provides the small first-run coordinator; `lenso setup` uses the selected trusted
application config and never starts the incomplete application or Engine. The
[runnable local example](../packages/cli/examples/first-run/README.md) commits a
real SQLite first owner/configuration, without per-install scripts.

## Host and Console contract

Reuse `ConfigContract` / `ConfigSource`, `resolveConfig`, and the existing safe
config diagnostics. Initialization owns neither a generic user table nor a
new secret provider. A host opts in explicitly:

```ts
export const initialization = defineInitialization({
  input: existingFirstRunConfigContract,
  authorize, // verifies actual host evidence for each action, never a JSON actor
  read, // read-only durable marker, null only for a positively known new instance
  beforeCommit, // DB and host capability checks, optional for local atomic storage
  commit, // atomic first-owner claim + config references + pending marker
  check, // current selected environment/config, no writes
  activate, // optional idempotent host activation, no business/model/payment effect
  complete, // compare-and-set the same qualified revision
});
export function initializationBinding(action) {
  return trustedNativeLaunchEvidence(action);
}
export default defineApp({ plugins, initialization });
```

`initialization` on the default app and the named export must be the same exact
object. Normal `startApp` refuses an absent/incomplete durable marker before
plugin config/setup, using `initialization-required` / `initialization-not-ready`.
Apps that do not opt in retain their existing lifecycle. Inspect remains static;
setup status performs explicitly authorized runtime reads.

The entry supports `status(evidence)`,
`apply({requestId,input}, evidence)` and `activate(evidence)`. Console supplies a
verified bootstrap-host binding to these ordinary methods; it must not expose
`initializationBinding` itself or use normal Auth that cannot yet start. An
independent minimal bootstrap route can serve the setting shell while business
plugins remain closed. Route admission, short authorization, current scope and
final first-owner policy remain with the host. No route is automatically mounted.

The safe result is `schemaVersion: 1`, `state`, `revision`, `blockers` and
`nextAction` only. The request ID is a non-secret idempotency identifier, not an
actor or permission. Input and arbitrary callback messages/extra fields never
enter the status DTO.

| state                 | Meaning                                                        | nextAction                                                  |
| --------------------- | -------------------------------------------------------------- | ----------------------------------------------------------- |
| `uninitialized`       | Positively known new instance; no committed first owner/config | `apply`                                                     |
| `blocked`             | Required config, DB or host capability is not available        | `external_configuration` (or the relevant activation retry) |
| `awaiting_activation` | Owner/config transaction exists but is not qualified/active    | `activate`                                                  |
| `ready`               | Same revision qualified by host, ready for business startup    | `none`                                                      |

Blocker codes are `database-unavailable`, `external-configuration-required`,
`activation-failed`, or `configuration-invalid` with existing safe field
`ConfigDiagnostic`s. An unavailable/uncertain marker read is an
`initialization-unavailable` error, never guessed to mean a new instance. A known
existing owner without a marker must fail closed for manual owner recovery.

`apply` validates with the existing config resolver, commits once, then attempts
separately authorized activation. Interrupted/unknown commit returns
`initialization-outcome-unknown`; query status before any retry. A pending
revision resumes through activate without resubmitting credentials. Status/repair
and safe identical-request replay can remain usable after completion; first-owner
creation is durably closed. The coordinator does not automatically reset markers,
retry arbitrary writes, compensate external effects or undo completed business.

## Required atomic boundary

`commit` is the app-owned transaction boundary. It must acquire a DB lock/CAS,
verify that no first owner exists, verify current initialization authorization at
the write boundary, and commit the first-owner claim + config references + marker
together in the authoritative owner store. The native sample also commits its
password hash and secret values in that same local transaction. It must serialize concurrent claimants, compare identical retry
input without logging it, reject changed input for an existing request, and close
new requests after completion. Secret generation happens once inside that owned
boundary; retries read the committed values. SDK callbacks are contracts, not a
claim of cross-provider atomicity.

A secret backend and account DB are separate resources on many hosts. Reuse their
existing version/reference and idempotency support, store a durable pending
checkpoint, and do not label the owner or active environment ready until that
checkpoint can be proven. A separate password store needs a durable hash/reference
and an inactive claim until it is ready; do not pretend multiple D1 databases or
a secret provider share one transaction. If a provider never stored an entered
credential, report the missing external input rather than claiming it can be
reconstructed on resume. Keep raw passwords transient, provider keys in the
existing provider and output secret-free. Do not mint a platform credential or
a privileged role to work around missing deployment authorization.

## Platform boundary

The native local example is executable and tested with real SQLite/child
processes. Its opaque local authority and private filesystem are explicit; they
are not a remotely callable bootstrap scheme.

Workers do not get an ambient self-secret writer. The host's `beforeCommit` and
`check` report missing external configuration, and `activate` must observe the
intended active deployment/config revision. The portable core tests exercise this
boundary; this change supplies no live D1 migration/claim adapter, Cloudflare API
writer or production deployment controller. Those must belong to the app's
existing owners, not an implicit runtime fallback.

Cloudflare distinguishes versions from active deployments. Its current docs say
`secret put` creates/deploys a version, whereas `versions secret put` creates only
a version requiring later deployment. UI must show the actual selected result,
not infer activation from a successful secret upload. See the official
[secrets documentation](https://developers.cloudflare.com/workers/configuration/secrets/)
and [versions/deployments](https://developers.cloudflare.com/workers/versions-and-deployments/).

Console owns the settings shell and plugin forms. It can use this DTO for a single
flow and preserve non-secret drafts/request IDs across reconnects. Passwords,
provider keys and initialization credentials must not enter URL parameters,
persisted browser drafts, logs, diagnostics or status polling. Failed activation
must offer the exact existing repair owner and resume the same revision; no
silent business call/replay is authorized.
