# Incremental Bun implementations

Two Rust Plugins call two independently authored Bun Plugins through the public
Agent Tool Provider Capability. The saved Dependency choices bind Proof to Bun A
and Health to Bun B. The ordinary App resolver and Kernel own those calls.

From the repository root, with Cargo, Bun 1.4+ and registry access available:

```sh
cargo build --locked -p lenso-cli
cargo generate-lockfile --manifest-path examples/incremental-bun/Cargo.toml
cargo generate-lockfile --manifest-path examples/incremental-bun/app/health/Cargo.toml
(cd examples/incremental-bun/app/bun-a && bun install --ignore-scripts)
(cd examples/incremental-bun/app/bun-b && bun install --ignore-scripts)
target/debug/lenso app dev --root examples/incremental-bun
```

After activation, the Native consumer prints `NATIVE_BUN_RESULT` with
`"bun-baseline"`; the other consumer prints `NATIVE_HEALTH_RESULT` with
`"unrelated-bun"`. Edit Bun A's returned string in `app/bun-a/src/plugin.ts`.
Only its implementation packaging is rebuilt. Neither native compilation unit,
the linked Host, nor Bun B's bundle is rebuilt. No author-maintained Descriptor,
codec, Generation snapshot or artifact identity is required.

This path currently activates a fresh Host Generation and restarts its Instances
after the previous Host retires. It does **not** promise an in-place TS Instance
reload. Added/removed sources, dependency/lock/resource changes, or changed
contracts, codecs, configuration schemas and target profiles fall back to App
build. Failed packaging keeps the active preview. Selected convention compilers
retain their own packaging semantics.

The example uses current repository Rust SDK source and the published
`lenso-capability-agent-tool-provider@0.3.0`, `@lenso/bun-plugin@0.4.2` and
`@lenso/agent-tool-sdk@0.1.1`, with matching Capability identity and Descriptor
digest. It qualifies Native Request through the Bun process Adapter. It does not
qualify unpublished SDK changes, Workers, Stream, or mixed Bun HTTP ingress.

Optional measured regression probe, outside the ordinary build gates:

```sh
python3 crates/lenso-engine-app/tests/typescript-dev-smoke.py \
  --cli target/debug/lenso --root examples/incremental-bun \
  --native-call --expect-targeted --out /tmp/bun-dev-feedback.json
```

The probe waits for a real typed Request result **and** completed activation,
records native binary/rlib hashes and modification times, compares retained
packaging, requires only Bun A in development feedback, and restores its source
after clean shutdown. Initial startup and subsequent edit timing are separate.

The consumers use existing `consumer` and `plugin_impl` create/stop hooks, with
named Dependencies. Legacy `Lifecycle` authoring uses its older requirement
identity rules and cannot be mixed with these named choices.

Development feedback reports all `affected_instances` of the repackaged Bun
Plugin and the actual `activation_scope`. The current scope is
`host_generation`; instance selection alone does not claim a hot Transition.
Kernel activation must apply a validated transition between complete immutable
Plans before this path can keep unrelated running Instances.

One same-environment warm edit sample took 6.092 s with the previous dev-loop
and 2.217 s with targeted packaging. The latter packaged only Bun A, invoked
no Host build, preserved all native hash/mtime evidence and Bun B bytes, and
returned the edited value through the actual typed Request. This is one local
workload, not a cold-build or CI speedup. Raw evidence and the comparable
baseline recipe are in `docs/performance/dev-loop-bun-20261003.json`.
