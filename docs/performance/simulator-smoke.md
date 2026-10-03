# Shared background-job corpus: execution cost and retained real gate

## Scope and revisions

The first slice reuses the existing TestSimulator, JobRuntime, real Kernel,
generated Plugin factory, and Web Ingress. No new Kernel or Simulator API,
business implementation, database emulator, or SQL response fixture is added.
The existing background-jobs socket smoke supplies a non-Relay workload with
a real 1500ms wait. Its requests and allowed intermediate states now come from
one JSON corpus consumed by simulation and the real smoke.

Base: `75ff83c6150cc7ce38d00933925912a0d2ec0788`.
Measured and independently reviewed source:
`8457308b867ececa877197fd13b3d152eb3f5234`.
The final candidate adds only this report, raw evidence, and a phase-observation
script to that source revision. The latest remote main was read back at the
base above. Nothing was pushed, landed, published, or deployed.

## Actual CI attribution

The [historical candidate CI run](https://github.com/LioRael/lenso/actions/runs/36411562960)
used source `62b41f43e57558b1dbb989b7549c2b1ea992653a`, candidate ref
`candidate/ci-feedback/20260928-3`, quality job `108892723083`, attempt 1.
Decoded logs were fetched through the GitHub connector. The local GitHub CLI
API returned Forbidden; it was not used as evidence. See the
[timestamped excerpt](simulator-ci-log-extract.txt).

| Boundary in that run | Observed time | Attribution limit |
| --- | ---: | --- |
| Workspace Clippy | 2m22s | Compilation/linting; cannot be accelerated by virtual time |
| Workspace test compilation | 4m25s | Separate from test execution |
| Workspace test execution | about 21m25s | Includes child Cargo work, host creation, sockets, waits and other tests |
| Configuration-source E2E pair | 736.09s | Logs do not separate generated Host compilation, activation and waits |
| Host runtime control E2E | 29.34s | Real distribution/process/integrity boundary, not shown to be a timer bottleneck |
| HTTP egress native corpus | 0.12s | Too small to justify treating this as the primary hotspot |

That run reported no restored Rust cache. These are historical observations,
not current-main timings. The existing [CI feedback report](ci-feedback.md)
already records Host hashing and Wasm build-reuse work. None of those savings
are attributed to this slice. No PostgreSQL query-cost attribution is made.

## Controlled local measurements

Linux x86_64, Rust 1.94.0, two build jobs, no simultaneous builds in the final
comparison. Each source revision used its own new, empty Cargo target. Registry
sources were already fetched. No wrapper environment variables or Cargo wrapper
configuration were present in this environment. The empty target is the cold
boundary; neither OS caches nor registry sources are claimed globally cold.

The [measurement script](../../.github/scripts/measure-simulator-smoke.py) invokes
Cargo directly with the configured toolchain and `--locked`, then executes the
already-built workload. It rejects an exact filter that ran zero tests. All
samples must pass all assertions. First-process samples are separate from the
three repeated-process samples; they are not claims about OS page-cache state.

| Boundary | Baseline | Shared-corpus simulation |
| --- | ---: | ---: |
| Empty-target test compilation, including Cargo invocation | 43.260s | 43.102s |
| Warm no-op compilation | 0.215s | 0.211s |
| First workload process | 1.680s | 0.0124s |
| Repeated workload processes | 1.668 / 1.658 / 1.662s | 0.0137 / 0.0121 / 0.0126s |
| Retained candidate socket/process sentinel | — | 1.682s |

Baseline executes the original three-Host socket smoke. Simulation runs the
same observable corpus twice through fresh real Apps, including fresh-state
restart and Plugin removal. Repeated-process medians are 1.662s and 0.0126s.
This is an execution-loop improvement; cold compilation is effectively equal.
There is no demonstrated candidate CI total-time improvement: the original
Python smoke was outside default CI, and this slice adds its real sentinel to
the existing unfiltered workspace test gate. It strengthens that gate and costs
about 1.7s locally. No existing safety/data-correctness test was removed,
ignored, filtered in CI, or moved to a periodic gate.

The optional [phase observer](trace-socket-smoke.py) wraps the actual Python
smoke without changing its assertions. Separate phase samples attribute the
selected workload to process readiness, polling for delayed jobs, and shutdown.
No Host generation subprocess, SQL statement, database connection, or retry
occurs in this corpus. The current example is in-memory; database correctness
is not established by these results. Phase samples are separate runs and are
not subtracted from the uninstrumented comparison.

| Observed socket-smoke phase | Baseline sample | Candidate real sample |
| --- | ---: | ---: |
| Three process spawns through readiness, summed | 0.0261s | 0.0261s |
| Poll successful job until completed | 1.5064s | 1.5072s |
| Poll requested-failure job until failed | 0.0635s | 0.0630s |
| Poll shutdown job until running | 0.0005s | 0.0005s |
| Three SIGINT/exit/drain handshakes, summed | 0.0034s | 0.0034s |

The wait is the dominant measured execution cost for this selected corpus.
The phase observer starts its total timer after Python imports; the outer
uninstrumented workload samples include interpreter startup and imports.

Raw [summary](simulator-smoke-final-raw/summary.json), Cargo build logs,
four executions per mode, real sentinel output, and phase samples are archived
in [the evidence directory](simulator-smoke-final-raw/).

To reproduce with two checkouts and a previously unused build root:

```sh
python3 .github/scripts/measure-simulator-smoke.py \
  --baseline /path/to/base-checkout --candidate /path/to/source-checkout \
  --output /tmp/lenso-smoke-evidence --build-root /tmp/lenso-smoke-new-targets
```

## Invariants, review, and remaining scope

The real sentinel retains immediate bounded acceptance, notification absence
before completion, asynchronous failure without extra notification, SIGINT
cancellation, each task dropped exactly once, admission closure, clean process
exit, fresh restart, and Plugin removal. It emits a receipt from real results
and lifecycle events. Simulation compares the same observable results and
actual per-task drop/cancellation IDs, checks the last millisecond before the
deadline, and advances ten seconds after shutdown to reject late delivery.
Intermediate queued/running observations are an allowed set, not a required
unique concurrent trace. The simulation replay budget is exactly two fresh
executions; no unbounded search or new scheduler is introduced.

Independent review by a separate GPT-6.1 Sol agent at high reasoning confirmed
the exact source commit above with no remaining source blockers. Initial
findings about deriving IDs from submissions and accepting zero-test
measurements were fixed before the final source commit and remeasurement.
The reviewer performed a read-only code review and `git diff --check`; it did
not rerun tests. Local validation by the main executor passed all 15 integration
tests including the real sentinel, all-target package Clippy with `-D warnings`,
and formatting. Candidate GitHub quality/native/WASM proof remains pending
the parent's batch authorization to push an exact candidate.

The next slice must identify a real Store owner and reuse its application
code and shared contract corpus. Facade faults must distinguish not executed,
confirmed rollback, committed with lost ACK, and unknown commit; a timeout is
not rollback. D1 requires official local Miniflare/workerd with real SQLite
semantics, and PG adapter acceptance requires real PostgreSQL. The common
contract cannot grant D1 PG interactive-transaction guarantees. Relevant SQL,
binding, migration, atomic-operation, error-mapping and persistence gates must
remain mandatory. No D1 security audit was attempted. Cloudflare network
boundaries, Store/PG/D1 adapter correctness, counterexample shrinking, broader
seed/event exploration and end-to-end CI acceleration remain unimplemented by
this slice. Linked Workers and local_host files owned by the Stream task were
untouched.
