# Workerd evidence candidate review

Base: `8d5a125d7c57bb867ec15df4d8e9ebe852292077` (origin/main, independently refreshed).

Independent reviewer: `/root/workerd_evidence_review`. Final verdict: PASS,
no remaining blockers after the replay cleanup and optimization-mode fixes.

The reviewer independently verified the original 156 file hashes and sizes,
all 28 cases and 28,000 samples, quantiles, throughput, summary ranges and
paired ratios. The 34 frozen App files were also checked: HTTP glue is the
only differing file. Original evidence remains byte-exact.

The new replay entry received static review of parameter paths, tool/artifact
identity, signal handling, per-group TERM/KILL cleanup, bounded `ps` calls,
allocation receipts and Python optimization rejection. It was not executed
against a server. A mocked check verified that one group signaling error does
not skip subsequent groups and that process inspection sets a timeout.

Focused validation: offline verifier PASS; Python syntax PASS; report local
links PASS; authored-file whitespace check PASS. Raw stdout/stderr have original
trailing blank lines; these were retained deliberately to preserve their hashes.
No local Rust build, new load, Native/Wasmtime supplement or performance rerun
was used for this delivery.

Remote candidate CI and exact-SHA main landing must be recorded separately
after the unique Core integration owner schedules the candidate. This review
does not claim CI, landing, publication or deployment.
