# Canonical actual-workerd HTTP glue comparison, 2026-10-03

The same normally verified canonical Component App completed under actual packaged workerd, before/after HTTP glue, 64KiB bodies and one/eight loopback HTTP clients. Seven alternating AB/BA pairs per client count produced 28 valid cases, zero invalid or incomplete cases, in 218.4137 seconds. The allocation receipt records an outer timeout of 240 seconds and a total cleanup allocation of 300 seconds. The original outer shell/Python wrapper was not archived, so its implementation cannot be independently audited from this archive. Runner PID/PGID5466 and all57 owned process groups exited, with the CPU slot released before analysis. The new replay watchdog below is separately reviewable code, not a reconstruction of the historical wrapper.

| Clients | Before median req/s | After median req/s | Paired after/before median (observed range) | Before complete p99 median | After complete p99 median |
|---|---:|---:|---:|---:|---:|
| 1 | 162.123 | 178.787 | 1.164 (1.002–1.783) | 11.097 ms | 7.784 ms |
| 8 | 195.570 | 396.945 | 2.023 (1.882–2.119) | 56.422 ms | 36.487 ms |

All seven eight-client throughput pairs increased, with consistently about1.9–2.1 times the observed throughput for this target. Single-client throughput gains vary substantially, and its paired complete p50 ratio median is1.021: typical median latency did not improve consistently. Eight-client p99 has paired ratio median0.630 but range0.254–1.396; zero-based group1 regresses despite the throughput gain. Single-client throughput groups0 and4 have unusually large ratios1.783 and1.707. All outliers are retained without reruns or exclusions. These ranges are observed paired spread, not confidence intervals or a universal performance guarantee.

The existing full ten-case actual-workerd HTTP corpus passed on each variant, including runtime_failure and healthy_after_failure. Qualification server groups3982/4410 and corpus PIDs4001/4419 exited; elapsed windows1.8789/2.2106 seconds. SDK0.1.5 creates a fresh Guest per invoke, without Native Kernel supervision. Corpus and startup/readiness requests are excluded from performance samples. Each measured case has one first POST,200 warmup requests and1,000 measured requests with byte-for-byte64KiB response, status and request-ID checks. There are28,000 measured requests,5,600 warmup requests and28 first POSTs, plus readiness GETs outside those counts. Each case starts a fresh Wrangler/workerd process with warm OS caches; no true cache-cold machine or deployed-isolate startup claim is made. HTTP-ready time includes Wrangler startup and a readiness GET, so it is not pure Wasm/JIT or isolate cold-start time.

Frozen canonical build source is58a48391844b2c6fce80bb504232bc25a4630d53. Normal mise Cargo/MBX, fixed installed Rust1.98.1, jobs2, locked/offline and the existing exact ignored builder test executed one test, PASS, in211.1454 seconds; build PID/PGID81251 exited. No builder, guest, Descriptor, admission, SDK or Host rule was relaxed. Component SHA-256 remains d66a8655f6c54923cb0be0bae8dbab4eb1dcb2a8507cf6b238447464662e1afe. Jco core SHA-256 ce9a2ec2ce7b9b1cb8da49f875af5093326cae03f917a60f1cae693788629233 and bindings SHA-2569932e428e62cbc5eed956ab690b7a103d151aac979e63486ff6789389302f2a8 are retained in the normal build receipt.

The before module is immutable Git source9911e4979bb4bb97700f51bc277ba62d752ceb07, HTTP glue SHA-256ce552f355dd08daed61bcd73418cf784407aeb0f4547c2315432013fd394ccd2. After glue SHA-2562bfe1b31462cc97b513a51c3177640311b4a6791b7582372b0e31a7439a66609. Exactly one of17 frozen App files differs: workers-http.mjs. Before is an explicit benchmark-only glue overlay on normally built output; the original workers-build receipt is kept unmodified and is not presented as a separate before build. Component, core, bindings, SDK, Plan, Descriptor evidence and runtime are otherwise identical.

Official npm fixed Jco1.35.0 installation ran with ignore-scripts in a task-owned cache/prefix, taking184.0908 seconds. Initial npm prefix lock used unusual /tmp-to-/private/tmp relative keys and included an extraneous older package; normal offline lock-only generation from the real task cwd and one locked offline npm ci corrected this. Jco --version and npm dependency-tree checks passed. Lock SHA-256cca15807defbbccbd70b3f8a4eb7a0574f63d164b50ae2ad01f11c186b764f2d records487 platform-inclusive packages;143 physically installed tarballs had cache integrity verified, with4,986 installed-file identity entries. Setup wall time through verified manifest was426.733 seconds, exceeding the requested five-minute setup limit; that overrun is preserved, not relabelled as within budget. No install scripts, global configuration/cache/proxy/TLS changes or alternative Host were used.

Wrangler4.143.1, Miniflare5.20260926.1-alpha and workerd1.20260926.1 Darwin-arm64 are copied into a task-owned closure with1,655 recorded file entries. Actual workerd executable SHA-256a428d3bde692f1fa80159ae02235c2e224424edf8fd145abf86c779b70abb919 reports workerd2026-09-26. The compatibility date is2026-07-08. Local-only startup disables Wrangler metrics for the process and retains task-local logs/state. Node and macOS metadata were collected after the window and are explicitly labelled as such in the receipt.

The client uses closed-loop HTTP/1 keepalive and includes byte validation in throughput. No coordinated-omission correction, production traffic, external upstream, deployment, formal confidence interval, server CPU/RSS sampling or leak evidence is claimed. These actual-workerd results are independently scoped; they are not pooled with earlier Node mock, Axum comparator or Native/Wasmtime1KiB results. The already completed Native/Wasmtime matrix was not rerun. No additional case, request matrix, third target or follow-up load experiment was executed.

[Complete hash manifest](web-audit-workerd-canonical-20261003-hashes.json), [raw case/ordering status](web-audit-workerd-canonical-20261003-raw/measurement/status.json), [analysis with all ratios/ranges](web-audit-workerd-canonical-20261003-raw/measurement/analysis.json), [allocation/release receipt](web-audit-workerd-canonical-20261003-raw/allocation/status.json), [build/artifact identity](web-audit-workerd-canonical-20261003-raw/build/artifact-manifest.json), and [setup/measurement-source identity](web-audit-workerd-canonical-20261003-raw/setup/measurement-ready.json) preserve the evidence. Raw samples, logs, both corpus receipts, lock/integrity and module manifests, and historical inner runners are included byte-for-byte; binaries are not embedded. Quantiles use sorted samples at zero-based index `floor(n * percentile / 100)`.

The [offline verifier and replay watchdog](../../tools/performance/workerd-replay.py) defaults to offline verification only:

```sh
python3 tools/performance/workerd-replay.py
```

It checks the complete 156-file archive, all 28,000 raw latency samples, throughput from elapsed time, AB/BA order, and all reported paired summary quantiles and ranges. Independent read-only review also checked all 34 actual variant files: only `workers-http.mjs` differs. No measurements were rerun for delivery.

A separately allocated replay can use the frozen binary closures at any explicit path:

```sh
python3 tools/performance/workerd-replay.py --replay \
  --tools /path/to/workerd-tools-frozen-20261003 \
  --variants /path/to/workerd-variants-frozen-20261003 \
  --out /path/to/new-replay-output
```

The default port is 63739; `--port` overrides it. The worker checks the port before starting each server, verifies all variant and tool hashes, and uses the archived byte-exact Node client. The independent supervisor enforces a 240-second main deadline and bounded TERM/KILL cleanup within a 300-second allocation; it records owned process groups and remaining processes. Each case retains independent client/server cleanup and atomic status writes. POSIX `ps`, Python 3.9 or newer and Node are needed; the recorded workerd closure is Darwin arm64, so replay on a different platform is a new experiment rather than the same runtime identity. A port race remains possible; use an exclusively allocated port.

The binary closures and Component are retained in the original task-owned paths in the manifests, rather than distributed in Git. If they are unavailable, replay is blocked; offline verification still works from a clean checkout. Do not rebuild with current main and treat it as this historical fixture. The build receipt identifies source `58a48391844b2c6fce80bb504232bc25a4630d53`, Rust 1.98.1, Jco 1.35.0 and SDK 0.1.5; any replacement must reproduce every recorded hash and qualification or be labelled as a new experiment. The historical scripts in `raw/setup/` remain evidence snapshots with original machine paths; use the parameterized entry above for replay.

This additive evidence candidate is based on landed Core `8d5a125d7c57bb867ec15df4d8e9ebe852292077`. It carries no runtime, HTTP glue, Native/Wasmtime, NODELAY or workflow changes. Publication and deployment are outside its scope.
