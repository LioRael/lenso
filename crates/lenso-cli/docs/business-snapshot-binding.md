# Plugin-owned business snapshot binding

A linked Native Plugin can bind a changing business policy without changing the
App's resolved configuration or Plan. The Plugin owns the policy schema, source
authorization, request-pinned values, and source availability behavior. The
generated Host only connects that implementation to startup and shutdown.

This is an advanced source-authoring contract for the current CLI checkout,
not a new App configuration requirement. See [installation](../README.md#install)
for the source/release distinction. It does not add Workers, Process, or Wasm
support, nor replace [external App configuration](configuration-sources.md).

## Declare the binding in Plugin source

In the linked Plugin's Cargo manifest:

```toml
[package.metadata.lenso]
plugin-id = "company.policy"
root-slot = "web"
host-bindings = ["business-snapshot@1"]
```

The Plugin exports a public `business_snapshot` module with:

- `bind(registry: NativePluginRegistry, plan: &ResolvedAppPlan, policy_path: &Path)`
  returning `anyhow::Result<(NativePluginRegistry, Poller)>`;
- `Poller::recheck(&self)`, an async method returning `anyhow::Result<()>`;
- `Poller::spawn(self)`, returning a guard held by the Host until shutdown or
  startup failure. Dropping it must revoke availability and stop scheduled work.

`NativePluginRegistry` comes from `lenso-native-adapter`; `ResolvedAppPlan` comes
from `lenso-app-plan`. `Poller` and its guard are Plugin-owned types, not framework
traits. Their implementation is compiled as an ordinary locked Plugin dependency.
The Host does not copy arbitrary authored Rust files or accept a function path.

Only one linked source candidate may declare this binding in a generated Host.
Absent or empty `host-bindings` selects none; unknown versions, malformed values,
duplicate declarations, and competing owners fail the build. The Plugin ID does
not select a special implementation. A build must migrate an old
`attachment-policy@1` declaration and export the binding rather than silently
retaining the former built-in knowledge-base behavior.

## Start and stop

```sh
lenso app start --from dist --business-snapshot-policy /etc/my-app/business-policy.json
```

The policy is protected Host input, not App-authored configuration. The Plugin's
`bind` must validate the selected Plan Instance, object identity, authorized
source, field/schema bounds, and initial snapshot before installing its factory
override. The generic `BusinessSnapshotAuthority` and file/HTTPS source helpers
remain available from `lenso-engine-authoring`; a Plugin using them owns that
dependency.

After Kernel activation, but before the Host publishes readiness, the Host calls
`recheck`. Failure triggers Kernel shutdown and prevents readiness; it does not
roll back side effects already performed during activation. Success starts the
poller. The Plugin keeps requests fail-closed when its source is unavailable and
pins each request's snapshot for that request's lifetime.

Without the policy argument the Host does not call the binding. With the argument
but no declared binding, startup fails. `--check` exercises binding, activation,
recheck, and shutdown; static preparation does not invoke the binding.
