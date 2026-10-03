# Shared durable-operation corpus and real provider gates

## Exact scope and delivery

Base: `c28dce56cfffe639523b6ef07f797943a431ace0`, the landed first slice.
Measured, independently reviewed source:
`91750a046985e52c48266b0a271f2bf45bb90019`.
The follow-up candidate adds only this report and measured evidence to that
source. It is a local candidate: it has not been pushed or qualified by GitHub
CI. Core's integration owner coordinates any later candidate push and landing.
Linked Workers/local_host and the Stream owner's files are untouched.

The portable Kernel remains independent of databases. `lenso-test` owns one
generic, test-only `DurableFaultFacade` around a delegated finite operation;
it supplies no SQL parser, database, transaction, retry, or business Store.
The Node fixture owns actual PostgreSQL and official local D1 execution.
Rust gains only existing serde dev dependencies, not a SQL client or Tokio.

There is no general production Store interface or PG/D1 adapter in this Core
checkout. The shared Store is explicitly a **test-owned contract fixture**.
This slice calibrates that fixture and the fault/replay boundary, not existing
product Stores, product migrations, management/security owners, or production
provider adapters. The first slice continues to exercise the existing real
background-jobs application Plugin; no copy of that business code was added.
No D1 security audit or production operation was performed.

## Evidence and observable contract

The [corpus](../../crates/lenso-test/tests/store-contract/corpus.json) is consumed
by the same native Plugin fixture and real Kernel in live and replay modes.
The private Store delegates one bounded typed `apply` operation: insert a
receipt and its effect atomically, deduplicated by operation ID. Both rows must
agree, parameter binding must preserve the literal ID, and retries with a
changed amount must preserve an already committed result.

| Case | Actual boundary | Caller ACK | Commit evidence | Independent read-back before retry |
| --- | --- | --- | --- | --- |
| Success | Real finite operation | Received | Committed | Both rows, original value |
| Not dispatched | Fault before invoking delegate | Lost | NotExecuted | Neither row |
| Confirmed rollback | Deliberate real second-statement constraint failure | Error | RolledBack | Neither row, including first insert |
| Committed ACK lost | Real success, fault after commit | Lost | Committed, **test oracle only** | Both rows, original value |
| Unknown, committed | Real success, response evidence hidden | Lost | Unknown | Both rows, original value |
| Unknown, rolled back | Real confirmed rollback, response evidence hidden | Lost | Unknown | Neither row |

A timeout is never used to infer rollback. PostgreSQL classifies only the
deliberate SQLSTATE 23514 plus successful explicit rollback as RolledBack;
other backend failures are Unknown, including an error after COMMIT. Failed
rollback discards the connection. D1 classifies only the deliberate real CHECK
failure in its documented atomic batch as RolledBack; other failures, including
post-commit receipt-read failures, are Unknown. The facade's committed lost-ACK
classification is test-oracle evidence, not knowledge available to a real
disconnected caller. Its scenario terminal is Uncertain, not success.

D1 uses the actual `env.DB` binding, prepared `.bind()` statements and
`withSession("first-primary").batch()`. It has no shared BEGIN/COMMIT or
interactive transaction API. The batch rollback boundary follows the
[official D1 contract](https://developers.cloudflare.com/d1/worker-api/d1-database/#batch).
PG uses PostgreSQL 17.6, actual parameterized SQL and transactions, with no
SQLite substitute. D1 uses locked Miniflare 5.20260926.1-alpha and workerd
1.20260926.1. Ambient runtime replacement and configuration variables are
rejected. D1's disallowed `sqlite_version()` is not emulated; runtime identity
comes from installed pinned packages.

Each provider also runs two concurrent writes for one ID. Both callers must
observe the same complete winner; either input value is allowed. The contract
does not require the PG and D1 concurrent traces or winners to match. PG then
closes and recreates its connection pool, while D1 disposes and recreates its
local runtime with the same persistence directory. Subsequent reads must
preserve every effect. PG server restart/crash recovery is not exercised.

## Recorded completion replay and bounded exploration

Live execution launches one provider process per entire corpus, and reuses one
Rust test binary for both providers. No generated Host or Cargo build occurs
per operation. The resulting trace records typed requests and actual provider
responses, including independent read-backs. Replay checks every request in
order, reports the first mismatching index, and rejects unused/exhausted
completions. It never matches SQL strings or computes results from expected
corpus values. Stored traces carry the corpus SHA-256 and provider identity.

The same Kernel/factory/lifecycle executes around that boundary in both modes.
External completions are published at explicit virtual instants; the last
millisecond before completion remains pending. Seed 31 must reproduce the full
receipt exactly twice. Seeds 32–34 add bounded timing perturbations: four seeds
and six cases per provider, plus one exact replay repeat. These seeds change
completion publication times, not provider concurrency scheduling. A negative
test proves that a changed external request fails at its first divergence.
No database qualification is inferred from a recorded trace alone.

The new CI `store-contract` job runs **both real providers**, then replays the
fresh traces it just recorded. `quality` depends on that job; provider failure,
zero-test filters, transcript mismatch, or replay failure prevents a successful
quality result. SQL/schema/binding/atomicity/error-mapping/persistence assertions
remain in every candidate gate. The original workspace/native/WASM gates are
retained. All test subprocess gates have 90-second limits and a five-second
kill escalation; measurements also clean up the process group on deadlines.

## Actual measurements

Linux x86_64, Rust 1.94.0, Node 24.19.0, two Cargo build jobs, one previously
absent target directory. Registry sources, npm packages and the dedicated PG
server were already available. No simultaneous local builds ran during the
measurement. This is an empty-target compilation boundary, not a claim about
cold OS, network, npm installation, or PG server startup.

| Boundary | Actual time |
| --- | ---: |
| Thin Store test empty-target compilation | 20.641s |
| Warm no-op compilation | 0.192s |
| First real PG corpus process | 0.1198s |
| Repeated real PG processes | 0.1315 / 0.1304 / 0.1382s |
| First real D1 corpus process | 5.5645s |
| Repeated real D1 processes | 5.5907 / 5.5773 / 5.6516s |
| First replay process, **both** provider transcripts | 0.00969s |
| Repeated replay processes, **both** transcripts | 0.00857 / 0.00635 / 0.00716s |

Every real sample runs the same six cases, concurrency check and reconnect/
runtime-recreation checks for one provider. Every replay sample runs both
six-case traces with the same invariant checks. Repeated medians are PG 0.1315s,
D1 5.5907s, and combined replay 0.00716s. These are mode comparisons for a new
fixture, not before/after default-CI savings or production SQL throughput.
The 20.641s thin-target build cannot be compared as an optimization of the
first slice's different 43s background-jobs target.

Provider stderr reports initialization, delegated-call, restart and cleanup
wall time separately. Delegated calls include client/HTTP/workerd/SQL round
trips; they are not server SQL CPU measurements. See the archived process
outputs for each actual phase sample. In D1, awaiting runtime readiness moves
deferred restart startup into the restart phase instead of blaming the first
post-restart SELECT. Runtime/process overhead dominates that fixture; PG SQL
calls are small. This agrees with treating the previously supplied PG31
0.54-second result as a small execution cost, not the main CI bottleneck.

[Exact measurement summary](simulator-store-samples.json).
The handoff evidence archive contains every build/process output and fresh
provider trace. Reproduce with the locked dependencies installed and a dedicated
local PostgreSQL fixture:

```sh
bash .github/scripts/check-store-contract.sh
python3 .github/scripts/measure-store-contract.py \
  --build-root /tmp/unused-store-target --output /tmp/new-store-evidence
```

## Default CI critical path and coverage mapping

The landed [first-slice CI](https://github.com/LioRael/lenso/actions/runs/37102233391)
passed quality and Bun at exact SHA `c28dce56`. Workflow created-to-final-updated
elapsed time was 22m50s. Its [timestamped excerpt](simulator-store-ci-log-extract.txt)
shows a partial Rust cache restore and a Wasm-fixture cache miss.

| Current run boundary | Observed wall time | Meaning |
| --- | ---: | --- |
| Native phase | about 21m19s | Default critical path |
| First narrow engine test build | Cargo 2m13s | Real compilation before workspace gates |
| Workspace Clippy | Cargo 1m56s | Compilation/linting |
| Workspace test no-run | Cargo 4m45s | Compilation |
| Outer native build/lint section | about 8m59s, 42% of native phase | Simulator cannot remove it |
| Workspace execution section | about 12m20s, 58% | Includes child compilation, not merely sleeps |
| Configuration-source group | 192.06s | Web initial activation 93.29s contains visible 73s Cargo; Process build 46.92s contains visible 41.83s Cargo |
| Portable target checks after native | about 31s | Separate compile proof retained |

This current run is not comparable as an isolated speedup against the older
29m37s run: cache state and other already-landed code differ. Historical outer
Clippy plus test compilation was 6m47s versus about 21m25s execution; its
configuration-source group was 736.09s. No whole-job reduction is attributed
to either Simulator slice. No SQL cost is inferred from those mixed phases.

| Existing background-jobs socket case | Fast shared-corpus counterpart | Retained real sentinel |
| --- | --- | --- |
| 1500ms success, bounded acceptance, no early notification | Real Plugin/Kernel, exact virtual deadline | Original socket/process assertions |
| 50ms asynchronous failure, no additional notification | Same failure input and observable result | Original asynchronous-failure assertions |
| 5000ms work cancelled by shutdown | Cooperative cancellation plus 10s virtual late-effect check | SIGINT, drain, closed admission, process exit |
| Each task dropped/cancelled once | Actual per-task observed IDs | Actual production lifecycle IDs |
| Fresh restart and Plugin removal | Fresh real App and composition | Real restart and health socket |

The original Python smoke was **not in default CI**. The first slice therefore
adds coverage and about 1.7s of real sentinel execution; it does not replace an
old default slow group. All 13 pre-existing Simulator scenarios remain. The
second slice likewise adds a new contract gate rather than deleting/moving
an old slow group. Its real-provider dependency adds setup and qualification
cost before quality. A measured default-CI acceleration remains outstanding.

The implemented dependency reduction is limited and concrete: the Store target
uses the small `lenso-test` graph, performs one build for both providers, and
keeps SQL clients out of Rust/Kernel. Next CI work should verify an exact main
cache seed/restore and reuse checked Cargo fixture outputs, not cache admission
or integrity results. The existing Wasm reuse path and the prior no-benefit
shared-child-target experiment are documented in [CI feedback](ci-feedback.md).
CLI/Host dependency changes need separate attribution against the actual child
build phases. No unmeasured cache or dependency experiment is included here.

## Validation, review and remaining work

Local validation passed 19 unit tests, five durable-fault tests, three replay/
negative tests, nine receipt-diagnostic tests and one doc test. The separate
real PG and D1 corpus runs and replay of their fresh completions passed.
Package all-target Clippy with `-D warnings`, formatting, script syntax and
workflow dependency checks passed. Independent GPT-6.1 Sol high review binds
the exact measured source SHA above with no remaining blockers; the reviewer
did not rerun tests. Deadline/process cleanup and pinned runtime override
findings were fixed before measurement.

Remaining scope: integrate a real product Store owner and its actual adapter;
product-specific SQL/migration/error qualification; PG server crash recovery;
Cloudflare network boundaries; counterexample shrinking and concurrent
schedule exploration. Current first-divergence diagnostics and bounded timing
replay do not claim those capabilities. Failed abrupt test termination can
leave a schema in the disposable PG fixture database; successful runs remove
their schema. No protection setting, release or production deployment changes.
