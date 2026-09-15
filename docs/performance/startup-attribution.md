# Plan and Kernel startup attribution

The `startup_attribution` benchmark characterizes four distinct operations on
the same deterministic, valid Resolved App Plan semantics:

1. fail-closed `ResolvedAppPlan::validate`;
2. deterministic `ResolvedAppPlan::activation_order` derivation;
3. Execution Adapter preparation of exact Plugin generations and bindings; and
4. complete `Kernel::start_native`, including validation, order derivation,
   Adapter preparation, endpoint/binding checks, lifecycle preparation,
   activation, and readiness.

The fixture is a linear required-request dependency chain. Every Plugin
Instance has a deterministic, zero-padded identity, exact package revision,
configuration, endpoint, and explicit binding to its predecessor. The default
small, medium, and larger graph sizes are 10, 100, and 500 Instances.

## Run

Run the optimized Cargo benchmark profile through the workspace Cargo wrapper:

```sh
/Users/leosouthey/Projects/framework/.lenso-tools/bin/lenso-cargo bench \
  --locked -p lenso-kernel --bench startup_attribution -- \
  --iterations 20 --warmup 3 --sizes 10,100,500 \
  > startup-attribution.json
```

The benchmark writes one JSON document to standard output. Cargo build messages
remain on standard error. For a quick harness smoke test, use one iteration and
small graphs; do not compare those smoke-test values:

```sh
/Users/leosouthey/Projects/framework/.lenso-tools/bin/lenso-cargo bench \
  --locked -p lenso-kernel --bench startup_attribution -- \
  --iterations 1 --warmup 0 --sizes 3,10,30
```

Each stage reports raw nanosecond samples plus minimum, mean, median/p50, p95,
and maximum. Metadata includes the Rust compiler and Cargo versions, benchmark
profile, assertion mode, target, package version, iterations, warmup, and graph
sizes. Inputs and outputs pass through `std::hint::black_box`; complete starts
also verify readiness and shut down outside the measured interval.

These values are local characterization evidence, not production latency or a
performance budget. Preserve the JSON output with machine, load, and commit
context when comparing runs. Use identical arguments and a stable host, and
compare distributions rather than one sample.

## Attribution boundary

The first three stages intentionally call the current public fail-closed Plan
and Adapter Interfaces independently. The complete-start stage measures the
unchanged production startup path and therefore repeats validation, activation
order derivation, and Adapter preparation. Subtracting stage values from the
complete duration is only directional: allocations, cache state, Kernel-owned
prepared-endpoint validation, lifecycle work, and timer overhead are not
strictly additive across independently sampled operations.
