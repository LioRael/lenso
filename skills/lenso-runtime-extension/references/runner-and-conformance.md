# Host, Runner, and conformance recipe

A product Host owns its generated Host Catalog, exact admitted Plugin Releases,
root Slots, and implementation-selection policy. Resolution produces an exact
Plan; a Runner chooses concrete Drivers and Execution Adapters, translates host
shutdown, drives Kernel, and reports the terminal outcome. Neither surface is
Plugin behavior or an App-owned Plan-file workflow.

## Select implementations before resolution

One Plugin Release has one runtime-independent Contract and one or more exact
implementations. Host policy filters by target and admitted Execution Classes,
then selects exactly one implementation before the resolver materializes a
`PluginDescriptor` and Plan. Reject no-match and ambiguous policy outcomes.
Ordering may be a deliberate Host preference; filesystem or discovery order is
never policy.

The selected implementation's entrypoint, target, runtime package identity,
and Execution Class become immutable Plan input. An Adapter receives that
selection; it never benchmarks, negotiates, or falls back. Changing the
selection is a structural App Generation change. Every published
implementation must pass the same Contract conformance vectors, and a stateful
switch requires explicit state-compatibility evidence.

## Assemble execution explicitly

Inspect the selected runtime packages and assemble one unambiguous Adapter per
execution class:

```rust,ignore
let native = NativePluginRegistry::new().with_linked_factories();

let adapters = ExecutionAdapterCatalog::new()
    .with_adapter(native)?
    .with_adapter(BunAdapter::production("bun").with_codec(GreetingCodec))?;

let driver = TokioDriver::new();
let outcome = lenso_runner::run(
    approved_plan,
    driver,
    adapters,
    Duration::from_secs(10),
).await?;
```

The example is an internal Runner assembly shape. Product Hosts use the current
`lenso::host::HostBuilder` facade for durable Generation control. `HostBuilder`
takes the exact App identity, `GenerationRuntime`, and `ControlStateStore`;
product code remains responsible for resolution, Plugin policy, and recovery
authority. Start it on the same Tokio `LocalSet` as the lane-local Host and
finish with the exact `suspend` or `shutdown` handshake.

The Adapter catalog rejects duplicate execution classes. Kernel rejects an Instance
whose class is unavailable. The Runner may expose configuration for host paths,
limits, shutdown timeout, lane count, or Adapter selection, but cannot rewrite
the Plan or invent bindings. Direct `with_factory(...)` registration remains an
implementation-level compatibility escape hatch; ordinary Rust Plugins use the
public facade and linked generated registration.

For host shutdown, set Driver shutdown state and wake the lane. Report startup
failure, runtime failure, cleanup failure, and shutdown timeout as distinct
terminal outcomes. Do not call process exit from portable Kernel.

## Replicated lanes

For Plan-declared lanes, create one single-owner Kernel replica per lane and
place only the Instances assigned to it. Cross-lane request transfer uses the
explicit transfer catalog and only contracts marked transferable. Preserve
request identity, deadlines, cancellation, typed Domain Errors/Runtime
Failures, and diagnostics. No work stealing, live Instance migration, or
shared mutable Plugin state is implied.

Use `LioRael/lenso-runtime-rust/crates/lenso-runner` as the current native and
replicated-lane source anchor.

## Conformance matrix

Choose evidence by changed seam:

| Change | Required proof |
| --- | --- |
| Driver | portable lifecycle/invocation/supervision suite plus real timer/task/shutdown smoke |
| Adapter | endpoint/binding preparation plus real process/wire/isolation and recreation paths |
| Host policy | deterministic implementation selection, no-match/ambiguity rejection, identical Contract projection, no runtime fallback |
| Runner/Host | catalog validation, resolved-Plan loading, `HostBuilder` recovery/suspend/shutdown, terminal outcomes |
| Lane transfer | same-lane and cross-lane request, deadline, cancellation, restart, diagnostics |
| Portable core | deterministic tests plus every supported target compile and at least two host implementations or equivalent product-neutral evidence |

Run the owning repository's format/check/test gates and the core
`lenso-runtime-conformance` surface. Record ignored/real-host tests explicitly;
a skipped process/browser/WASIp2 test is not passing host evidence.

## Completion

The branch is complete when Host policy resolves one implementation
deterministically, the Runner loads exact canonical bytes, only installed
execution classes are assembled, shutdown does not mutate the graph, every
terminal outcome is truthful, and the conformance row for each changed seam
passes.
