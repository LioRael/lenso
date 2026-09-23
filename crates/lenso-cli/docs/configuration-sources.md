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
Use `--check` to exercise Host readiness and exit. `lenso app config-sync --root
dist --policy /etc/my-app/policy.json` performs only the source reconciliation,
without starting the Host. Inspect the runtime App with
`app check/show --root dist/intent`; the generated Host reads this same Root.

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

The accepted desired revision is persisted in the built App's private
`intent/.lenso/configuration-source-state.json` before publication. A repeated or stale
revision cannot silently replace newer content; an interrupted publication is
reconciled only against the same snapshot. Once a distribution has a source,
`app start` requires its policy on every subsequent start, including after a
network outage. A failed initial fetch prevents startup. The state separates
accepted `desired` configuration from `last_activated`: the latter is written
only after native Kernel startup, never by `--check` or source reconciliation.
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
There is no background polling or live switch yet; rerun the reconciliation and
Host readiness flow to consume an update. Keep secret material with its provider:
the snapshot contains only authorized references for schema-marked sensitive
fields, and normal diagnostics do not print values.
