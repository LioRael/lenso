# Real CI consumer dependency reuse

This slice reduces repeated child compilation in an existing, unignored native
CI test. It does not replace a real provider or process gate with simulation.
The same registered-snapshot consumer still resolves its dependencies, compiles
the current macro, runs code generation and executes every original assertion.
The existing `cargo test --locked --workspace` invokes it without a new gate.

## Why the slow real E2E gates remain

The successful [baseline CI](https://github.com/LioRael/lenso/actions/runs/37102233391)
at `c28dce56cfffe639523b6ef07f797943a431ace0` took 22m50s from workflow
creation to final update. Its native phase took 21m18.6s: approximately 8m59s
in outer compilation/lint and 12m20s in workspace execution. Execution includes
child builds; it is not a measurement of sleeping or SQL time. These are
observed intervals, not a claimed speedup from the earlier Simulator slice.

| Existing case / observed baseline time | Observable boundary | Existing Simulator equivalent | Decision |
| --- | --- | --- | --- |
| Configuration-source Web and Process group / 192.06s | Real file watcher; missing-source recovery; actual HTTP output; pending versus active revision; rejection/outage continuity; verified process restart and retirement | Kernel scheduling and cancellation do not establish file watching, child readiness, HTTP output or OS receipt fencing | Retain all three tests. Web initial activation includes visible 73s child Cargo; Process build includes 41.83s child Cargo. Prior shared-child-target experiment already found no meaningful warm benefit. |
| Typed Process Notes group / 118.24s | Registry scaffold, production Host bytes, HTTP CRUD/input problems, signal shutdown receipt, supervised update, edited guest and second distribution after source removal | No equivalent proof of production build provenance, generated distribution or OS behavior | Retain both tests and the two distinct distributions; the second builds changed application code. |
| Configuration-source retirement / 12.16s | Actual child/PID fencing; missing, wrong, nonexact or old receipt; nonzero exit and killed supervisor | Simulator receipt diagnostics cannot qualify the real supervisor fence | Retain all six default tests. |
| Local Host bindings / 47.40s | Linked host and real binding behavior | Outside this worktree's ownership; no established equivalent | Retain unchanged. |
| MCP / 57.63s | Real authority, distribution provenance and supervised run | Not replaced by Kernel-only simulation | Retain; existing build consolidation is already in baseline. |
| Registered snapshot / 12.00s | Fresh external Cargo consumer of published authoring/codegen plus current local macro; generated descriptor checks | Compiler/codegen acceptance is a real consumer boundary | Retain actual Cargo/test and assertions; reuse compiled dependencies only. |

No real test moves to `ignored`, loses an assertion, or stops running on relevant
candidate changes. The shared background-job simulation and real sentinel remain
unchanged. A production Store adapter's SQL/bindings/migrations, Cloudflare network
boundary and process retirement cannot be qualified by the test-owned Store replay.

## Implementation and cache boundary

`LENSO_CARGO_FIXTURE_CACHE_DIR` opts the macro test into an absolute,
checkout-owned child target at `registered-snapshot/`. Without it the previous
temporary-output behavior remains. It is separate from the outer Cargo target
and from `wasm-guests`, avoiding an outer-test target lock. Every invocation
creates a fresh consumer source tree and invokes the same actual `cargo test`.
Do not share one cache directory concurrently between different checkouts.

The quality job places this directory under its existing fixture Actions cache.
The key retains OS, architecture, compiler and original manifest/lockfile inputs,
and adds the macro source, test harness and consumer fixture. Restore fallbacks
remain within that platform/compiler boundary. Only successful main runs save
the cache. Existing native, Wasm and Bun commands are unchanged. The second
Store slice, integrated separately by the Core owner, must retain its mandatory
real-provider prerequisite when these workflow edits are combined.

Cargo still validates source and dependency fingerprints. This stores artifacts,
not a cached qualification result. The consumer's existing dynamic dependency
resolution has no committed lockfile; this slice does not claim to pin that graph.
Bounded elapsed/visible-compilation diagnostics write directly to stderr so normal
successful libtest capture does not hide them.

## Actual before/after measurements

Linux cloud, Rust 1.94.0, two Cargo build jobs, prefetched registry sources,
no compiler wrapper, no other local builds. Exact test-binary execution includes
child Cargo, fresh-source preparation and all assertions. Outer harness builds
are recorded separately and excluded from the following process times.

The checked-in measurement harness ran clean source commits:

| Source / cache state | Process seconds |
| --- | ---: |
| Baseline `c28dce56`, fresh child target every process | 13.317938 / 13.307936 |
| Candidate source `0e0d26d5`, initially empty child target | 13.067747 |
| Same candidate, reused child target | 0.642470 / 0.668337 / 0.639011 |

The warm median is 0.642470s versus baseline median 13.312937s: 12.670467s
less in this existing serial CI component. The empty-cache result establishes
no meaningful cold-build improvement. Independent initial measurements gave
baseline 14.220581 / 13.226182s, candidate cold 14.563117s, and warm
0.739298 / 0.738650 / 0.701233s. Initial diagnostics counted 47 compilations
cold and one fresh-consumer compilation warm.

A real negative probe changed the fresh consumer by adding `compile_error!`.
The same warm target returned Cargo 101 and the intended compiler error; old
passing artifacts did not qualify changed source. Raw reproduction, initial
samples and the negative probe are under `registered-snapshot-ci-raw/`.
Log copies normalize trailing blank lines for repository whitespace checks.

The child target initially occupied about 274MiB on this machine. Actions archive
transfer and extraction are not included in local process measurements. The first
candidate lacking a main-seeded consumer cache has no cross-run benefit. A main
seed and subsequent observed restore hit are necessary to measure total CI
critical-path savings, including additional cache transfer. The 12.67s local
component reduction is **not** an established end-to-end GitHub CI reduction.
No reduction is claimed for Cargo's outer workspace build, Host generation, SQL,
real-provider qualification, or other retained E2E startup.
An additional local default-zstd tar probe produced a 74,743,327-byte archive,
taking 1.144s to pack and 0.402s to extract. This excludes network and does not
establish equivalence with Actions' archive settings or total cache overhead.

Reproduce with independent clean worktrees and a new build root:

```sh
python3 .github/scripts/measure-registered-snapshot.py \
  --baseline /path/to/baseline-c28dce56 \
  --candidate /path/to/candidate \
  --build-root /tmp/new-consumer-build-root \
  --output /tmp/consumer-evidence
```

Validation: original exact real consumer test passes cold and warm; a changed
source fails against warm artifacts; targeted Clippy with `-D warnings`, workspace
format check and whitespace check pass. Independent read-only source review
passed `584355c2afc4d2ee12d1a4bc6c83c73c081b93c7`; final review binds the delivery
commit separately. Real source and workflow qualification still requires the
Core owner's exact-candidate CI and integration.
