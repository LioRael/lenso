# Plan and Kernel startup attribution

The `startup_attribution` benchmark characterizes four operations on
the same deterministic, valid Resolved App Plan semantics:

1. fail-closed `ResolvedAppPlan::validate`;
2. deterministic `ResolvedAppPlan::activation_order` derivation;
3. Execution Adapter preparation of exact Plugin generations and bindings; and
4. complete `Kernel::start_native`, including validation, order derivation,
   Adapter preparation, endpoint/binding checks, lifecycle preparation,
   activation, and readiness.

Schema v2 additionally measures validation, activation order, and Adapter
preparation on an already checked snapshot. Each cold sample constructs a fresh
unchecked Plan outside the timed interval, so resolver work or an earlier sample
cannot prewarm startup. The fixture Adapter calls public `validate()` before
constructing fresh endpoints, generations, and bindings.

The fixture is a linear required-request dependency chain. Every Plugin
Instance has a deterministic, zero-padded identity, exact package revision,
configuration, endpoint, and explicit binding to its predecessor. The default
small, medium, and larger graph sizes are 10, 100, and 500 Instances.

## Run

Run the optimized Cargo benchmark profile through the workspace Cargo wrapper:

```sh
LENSO_STARTUP_BUILD_ID='<commit plus source identity>' \
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
profile, assertion mode, target, package version, iterations, warmup, graph
sizes, and the compile-time `LENSO_STARTUP_BUILD_ID` (or `unspecified`). Inputs
and outputs pass through `std::hint::black_box`; complete starts
also verify readiness and shut down outside the measured interval.

These values are local characterization evidence, not production latency or a
performance budget. Preserve the JSON output with machine, load, and commit
context when comparing runs. Use identical arguments and a stable host, and
compare distributions rather than one sample.

## Attribution boundary

The cold stages call the public fail-closed Plan and Adapter Interfaces
independently. The checked stages reuse the same immutable snapshot; Adapter
construction still creates fresh generations for every sample. Cold stages
exclude fixture construction and result destruction; checked stages include
result destruction. These boundaries are identical in the before/after runs.
Subtracting stages from complete startup is only directional: allocations,
cache state, Kernel-owned prepared-endpoint validation, lifecycle work, and
timer overhead are not strictly additive across independent samples.

## W07 follow-up: reuse immutable validation

`ResolvedAppPlan` privately retains a `OnceLock` containing either the complete
validation error or an opaque `CheckedTopology` with the activation order.
Resolution retains the order it already computed while rejecting cycles. Kernel
startup asks for that order, which validates a cold snapshot completely; existing
Adapter `prepare(&ResolvedAppPlan)` implementations continue calling public
`validate()` and reuse the result automatically. No public unchecked entry point
or new Adapter trait method is introduced. Adapters remain responsible for
validating their own factories, endpoints, configuration, and host inputs.

The memo contains only derived immutable topology, never Plugin state, event
queues, credentials, resources, lifecycle state, or runtime generations. Plan
bytes and equality exclude it; Debug omits it using the standard non-exhaustive
form. Cloning the identical snapshot may copy its proof. Wire decoding starts
unchecked, and `with_terminal_policy` clears the proof because it creates changed
input. Future snapshot builders must do the same. `AppComposition::resolve`
retains its successful proof, so that in-memory result needs no additional
resolution at startup. Structural error precedence, Kernel diagnostic emission,
prepared-output validation, and lifecycle rollback paths remain intact.

The following counts concern complete graph resolution and topological sorting,
not endpoint checking, lifecycle work, or independent Adapter-specific checks:

| Stage | Before resolution / topology passes | After resolution / topology passes |
| --- | ---: | ---: |
| Cold validation | 1 / 1 | 1 / 1 |
| Cold activation order | 1 / 2 | 1 / 1 |
| Cold public Adapter preparation | 1 / 1 | 1 / 1 |
| Checked validation | 1 / 1 | 0 / 0 |
| Checked activation order | 1 / 2 | 0 / 0 |
| Checked public Adapter preparation | 1 / 1 | 0 / 0 |
| Complete cold startup, one validating Adapter | 3 / 4 | 1 / 1 |

The new Plan unit test instruments the actual resolution and sorting functions
on 10/100/500-instance chains. It asserts one pass of each across activation
order plus repeated public validation/order calls. Additional tests cover wire
tampering, exact errors, policy invalidation, and identity. Conformance tests
exercise direct public Adapter rejection before factories run, cached errors at
Kernel entry, and independent generation state across checked-Plan starts.

### Local before/after evidence

The paired run uses 20 samples and 3 warmups on each deterministic chain in the
optimized bench profile. Raw distributions, compiler details, and source and
executable hashes are retained in:

- [Before JSON](validated-plan-reuse/before.json)
- [After JSON](validated-plan-reuse/after.json)
- [Build identity and source manifest](validated-plan-reuse/build-identity.json)
- [Baseline harness patch](validated-plan-reuse/baseline-harness.patch)

Baseline production code is commit
`c3fb3f3fa9c5df9bd35d8f1583d3c2ebf6a0a6a8`, with only the recorded benchmark
harness patch applied. That patch adds cold/checked stages and public Adapter
validation to the original fixture; the old fixture omitted validation. Thus
the paired results are comparable to each other, but should not be directly
compared to older v1 fixture timings. The after build is identified by the same
base plus source-manifest SHA-256
`684fcd2b6713633617d5df2b633af60d0e6f569f6d109ffdce5c006bb2bd0529`.
The manifest hashes all crate Rust/TOML files plus root Cargo files; its digest
is SHA-256 of the sorted, compact JSON `files` map. Both builds embed their
identity in the benchmark binary; executable hashes identify the measured
binaries without relying on the worktree's later state.

Median durations in microseconds, **before → after**:

| Stage | 10 Instances | 100 Instances | 500 Instances |
| --- | ---: | ---: | ---: |
| Cold validation | 14.500 → 12.625 | 140.333 → 128.125 | 754.375 → 692.416 |
| Cold activation order | 16.583 → 12.542 | 165.792 → 127.084 | 959.125 → 698.666 |
| Cold public Adapter preparation | 14.375 → 13.125 | 154.208 → 143.500 | 869.000 → 789.083 |
| Checked validation | 12.042 → 0.041 | 133.792 → 0.000 | 750.750 → 0.000 |
| Checked activation order | 14.292 → 0.208 | 161.000 → 1.542 | 923.250 → 7.542 |
| Checked public Adapter preparation | 14.750 → 2.583 | 161.625 → 23.583 | 862.416 → 119.792 |
| Complete cold startup | 62.958 → 37.291 | 937.375 → 593.541 | 8678.458 → 6880.667 |

Complete-start medians decreased about 41%, 37%, and 21% in this paired run.
Zero-nanosecond checked-validation samples reflect timer resolution, not zero
work. Order copying and fresh Adapter generation construction remain measurable;
prepared binding checks and lifecycle work still dominate larger graphs. The
memo retains an additional order vector per checked snapshot (and its clones).
These measurements use a no-op fixture on one host with uncontrolled background
activity. They establish neither an SLA nor production Adapter latency.

### Validation of the measured source

All checks below passed through `lenso-cargo` on the source manifest above:

- `fmt --all -- --check`
- `check --locked --workspace --all-targets`
- `test --locked --workspace`: 169 passed, 0 failed, 0 ignored
- `clippy --locked --workspace --all-targets -- -D warnings`
- `check --locked -p lenso-app-plan -p lenso-kernel -p lenso-runtime-conformance`
  for each of `--target wasm32-unknown-unknown` and `--target wasm32-wasip2`

The sandbox denied writes to the default shared Cargo cache, so both benchmark
runs and successful gates used the wrapper's
`LENSO_CARGO_CACHE_ROOT=/private/tmp/lenso-perf-validated-plan-target` override
and `RUSTC_WRAPPER=/usr/bin/env`. No check arguments or repository validation
rules were weakened. CPU model inspection was also denied; target, OS, compiler,
and executable identity remain recorded. Concrete external Adapter packages
were not rebuilt; their traits and validation entry points are unchanged, and
the portable conformance Adapter exercises the existing public path.
