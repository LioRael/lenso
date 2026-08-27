# Runner and conformance recipe

A Runner chooses concrete Drivers and Execution Adapters, loads an already
approved Plan, translates host shutdown, drives Kernel, and reports the
terminal outcome. It is a app configuration root, not a package manager or product
service.

## Assemble explicitly

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

The catalog rejects duplicate execution classes. Kernel rejects an Instance
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
| Runner | catalog validation, approved-Plan loading, host shutdown, terminal outcomes |
| Lane transfer | same-lane and cross-lane request, deadline, cancellation, restart, diagnostics |
| Portable core | deterministic tests plus every supported target compile and at least two host implementations or equivalent product-neutral evidence |

Run the owning repository's format/check/test gates and the core
`lenso-runtime-conformance` surface. Record ignored/real-host tests explicitly;
a skipped process/browser/WASIp2 test is not passing host evidence.

## Completion

The Runner branch is complete when it loads exact canonical bytes, assembles
only installed execution classes, translates shutdown without graph mutation,
returns a truthful terminal outcome, and passes the conformance row for every
changed seam.
