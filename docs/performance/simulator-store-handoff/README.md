# Exact Store candidate evidence transport

This branch is a transport for review and raw evidence. The Core owner lands
only a verified candidate, not this handoff commit.
Its base is `c28dce56cfffe639523b6ef07f797943a431ace0`.

Candidate remote ref: `candidate/simulator-store-facade/20261003-1`.
Exact candidate CI: https://github.com/LioRael/lenso/actions/runs/37104848033.
The normal Git push and remote read-back succeeded. Main was not written.

The ZIP is the same approved Library handoff, mirrored because Library transfer
reported download failures in the consuming environment. SHA-256:
`7dfec6c4495ac772d4ba6177962893e1855f8b28d6f39f36d06d0765169b14bb`.

Unzip it to obtain the verified Git bundle, binary patch, raw evidence archive,
index with every hash, and exact-candidate independent review. The archive
includes all measured compile/process outputs and final fresh PG/D1 traces.
The index's local-only delivery status records the pre-push snapshot; this README
and the remote candidate/CI above supersede that status, not its source hashes.

No CI is triggered by this `handoff/` ref; the frozen candidate's CI remains
the only qualification run for that exact candidate. No release or deployment.

## Portable gate correction

Attempt 1 failed after real PG passed because Ubuntu had no `rg`; D1 did not
run and quality was skipped. That run is not qualification evidence.

The two-line correction is independently reviewed at
`225c0d27a0c3ae0181109041409a6f1149359702`:
`candidate/simulator-store-facade/20261003-2`.
Exact CI: https://github.com/LioRael/lenso/actions/runs/37105216415.
It replaces `rg` with `grep -Fxq`, preserving full-line success matching,
zero-test rejection and fail-closed propagation. No Rust or provider changes.

Local validation ran the complete real PG/D1/fresh-replay gate with a PATH
containing only its required tools and **no rg**. All passed; empty/wrong-test
outputs were separately rejected and the exact real-test success accepted.
Fresh records are mirrored in `no-rg-evidence/`. The correction's separate
read-only independent review passed; that reviewer did not rerun tests.
