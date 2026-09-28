# CI feedback cost and coverage

## Measurement boundary

The first workflow cleanup did not materially accelerate the candidate gate:
the [direct baseline](https://github.com/LioRael/lenso/actions/runs/36402851020)
took 30m30s in `quality`, and the
[landed cleanup](https://github.com/LioRael/lenso/actions/runs/36411562960)
took 29m37s. Workspace test compilation plus execution changed from 26m03s
to 25m59s. Removing the native check and candidate cache upload explains
nearly all the job-level reduction.

Both runs lacked a reusable main Rust cache. Default-branch cache seeding is
described in [CONTRIBUTING.md](../../CONTRIBUTING.md#ci-feedback-and-dependency-caches);
seeding alone does not prove a subsequent candidate restored it.

The [main seed run](https://github.com/LioRael/lenso/actions/runs/36417494392)
succeeded at the same `62b41f43` SHA and created both Rust job caches under
`refs/heads/main`. It took 18m59s despite again reporting `No cache found` at
startup; configuration-source tests took 408.52s rather than 736.09s.
This run did not contain the test changes below. Its shorter duration cannot
be attributed to them or to restoring the cache it only saved afterward.
The same-SHA variation is another reason not to infer savings from a single
whole-job comparison.

## Coverage decisions

| Test group | Decision | Reason |
| --- | --- | --- |
| MCP explain/check equality | Reuse the authorized-build test's distribution | Exact CLI/MCP JSON comparisons were duplicated, but default distribution-root scope and explicit `built_distribution` scope are distinct. Retain both sessions with one build. |
| MCP build and supervised run | Retain real E2E | Authority, build provenance, process readiness, and shutdown are not serialization-only behavior. |
| Configuration-source Process Host | Retain real E2E | Missing-source recovery, pending versus active revisions, old Host continuity, shutdown, and verified restart cross process boundaries. |
| Configuration-source Web Host | Retain real E2E | Actual HTTP output must remain unchanged while pending and change only after restart; rejected fields and rollback must not change serving output. |
| Local workflow | Retain distinct readiness and toolchain-free scaffold cases | They establish different public outcomes. Larger ignored clean-room cases are not part of default CI cost. |
| V6 linked pack | Retain | Compiled-descriptor versus bundle-Contract validation is not covered by earlier adoption or root-Slot rejection. A replacement must exercise that exact comparison. |
| Portable Wasm fixtures | Retain execution tests | Target compilation does not prove bound imports, egress authorization, HTTP parity, streaming, deadlines, traps, or cancellation. |

Reducing build setup is preferable to dropping these boundaries. No test is
moved to `ignored` or a nightly gate by this change.

## Local measurements

Measurements used the configured Cargo/mbx toolchain, `TMPDIR=/tmp` for its
Unix socket path limit, and serial focused test runs without concurrent builds.
They are compiler-cache-warm local measurements, not cold Linux CI estimates.
Outer test-binary compilation is excluded.

The original MCP comparison and authorized-build pair took 5.10s (an earlier
sample was 5.25s). The consolidated test took 2.85s, 2.70s, and 2.74s:
median 2.74s, approximately 46% less for this pair, not for the whole suite.
The change eliminates one independent App creation/build and redundant CLI
queries while retaining default-scope and explicit-scope assertions.

For the configuration-source pair, a shared child `CARGO_TARGET_DIR` experiment
took 123.36s with a fresh directory and 79.64s warm, versus a phase-instrumented
original at 79.84s. This showed no meaningful warm-cache gain and was reverted.
The earlier uninstrumented 127.59s sample had different cache warmth and is
not a valid before/after comparison.

The retained configuration-source diagnostics print bounded phase summaries
directly to stderr so successful tests expose them under normal libtest capture.
They report elapsed time and visible Cargo activity, not raw child logs.
Generated Host stderr is visible, but other compiler subprocesses can capture
their own stderr: no visible Cargo completion is **not** proof that a phase did
no compilation. Use cold CI phase measurements before attributing residual
time to waits or proposing another cache change.

## Distribution hashing hotspot

Profiling the actual Process Host's `--prepare` path identified SHA-256,
not a compiler or a sleeping poll loop: all 799 samples on its active worker
were inside `Sha256::digest` / compression. The complete local distribution
contained two 216,619,064-byte debug CLI binaries (Host and resolver), plus
the guest. Repeated full-byte integrity checks amplified unoptimized hashing
cost across preparation, checking, activation, and supervision.

The workspace selectively sets `sha2` to optimization level 3 in the dev
profile, which the test profile inherits. Other dependencies, workspace
crates, release settings, and runtime verification behavior are unchanged.
The effective compiler flags retain `debug-assertions=on`.

On the same macOS machine with configured Cargo/mbx and no concurrent builds:

| Real workload | Before | After |
| --- | ---: | ---: |
| Complete distribution `--prepare`, three samples | 2.745 / 2.598 / 2.581s | 0.347 / 0.288 / 0.280s |
| Configuration-source test pair, warm compiler cache | 82.49s | 44.60s |
| Process initial activation | 10.83s | 1.44s |
| Process pending revision 2 | 2.86s | 1.22s |
| Process source outage | 2.38s | 0.625s |

The preparation median improved approximately 9x, and the focused pair took
approximately 46% less time. A baseline/fix repeat confirmed the preparation
result (baseline 2.553 / 2.474 / 2.677s; optimized 0.353 / 0.289 / 0.284s).
The dependent rebuild took 37.54s; a subsequent no-op took 0.71s, both excluded
from test-harness times.

The isolated preparation comparison used the same resolver, guest, and
configuration bytes. Only the Host executable and its matching lock entry
changed; the optimized Host grew by 79,840 bytes (0.037%), so reduced hashing
volume does not explain the gain. Both versions rejected an identical one-byte
resolver mutation with its length and modification time preserved, reporting
`runtime file changed: runtime/lenso-resolver`. No verification cache, weaker
metadata-only check, reduced timeout, or ignored test was introduced.

These local results establish the hotspot and the selective optimization's
effect, not an end-to-end Linux CI speedup. Candidate CI must confirm the
remaining compile cost, phase durations, and full native/Wasm gate.

## Focused reproduction

```sh
cargo test --locked -p lenso-cli --test mcp \
  stdio_authorized_build_reports_the_same_app_check -- --exact --test-threads=1
cargo test --locked -p lenso-cli --test configuration_source_dev -- --test-threads=1
```

Record toolchain, cache warmth, outer compilation, and test execution separately.
Keep input isolation and lifecycle assertions intact when comparing changes.
