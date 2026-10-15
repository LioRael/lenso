# Skills delivery evidence

Date: 2026-10-15 UTC. Local documentation/skill delivery only. No core/SDK/generator source or root lockfiles changed; no global skill installation, push, merge, publication or deployment. Skill entrypoints use progressive references and passed skill-creator quick_validate; relative links resolve.

## Ownership and triggers

| Skill | When it applies |
| --- | --- |
| lenso-develop (updated) | Consumer TS plugin/service, explicit adapters/exposure and Engine authoring |
| lenso-diagnose (updated) | Existing TS assembly/config/lifecycle/Runtime symptoms |
| lenso-migrate (new) | Actual TS↔Rust feature port and semantic parity assessment |
| lenso-best-practices (new) | Repository/application structure and cross-repo ownership decisions |

All files are under `.agents/skills` plus this evidence document. Existing platform.md from DX commit 74139b0 is unchanged. No source recipe is duplicated. Delivered follow-up b79aa1e was read in the DX owner's checkout; source signatures and its verification report informed the gated recipe reference. Those integration tests are attributed to that delivery, not rerun here.

## Actual checks

Source baseline: `74139b0585ff0769ff55229769970f2bf33f08b6`; paired Rust source: `e5dfafbf9e7e96ec7593c66ae108b227e6629d74`. Bun 1.4.3, TypeScript 7.0.2, rustc 1.101.0-nightly (e9bbadf68 2026-10-12), Darwin arm64.

- Pinned `bun scripts/prepare-runtime.ts`, frozen `bun install --frozen-lockfile --ignore-scripts --backend=copyfile`: passed. Sandbox cache/network operations required approved execution; no lock was changed.
- `bun run --cwd packages/lenso build`: passed, normal public dist exports.
- `python3 .agents/skills/lenso-migrate/scripts/verify-native.py --ts-root <this-checkout> --rust-root <selected-rust-checkout> --bun <Bun-1.4.3> --cargo <installed-cargo> --offline --output-dir <new-owned-directory>`: passed. The helper typechecks TS and compiles/runs both native programs in normal and partial-setup-failure modes.
- All four TS skill frontmatters and relative references passed validation; all seven skills across the delivery passed the same check.

The first run exposed an incorrect example assertion equating independent-node topological order across languages. It was replaced with the actual invariant: each instance closes once and each dependent closes before its own provider. Final evidence:

```json
{
  "normal": {
    "typescript": {
      "values": [
        "North Ada",
        "South Grace"
      ],
      "closed": [
        "query-south",
        "south",
        "query-north",
        "north"
      ]
    },
    "rust": {
      "closed": [
        "query-south",
        "query-north",
        "south",
        "north"
      ],
      "values": [
        "North Ada",
        "South Grace"
      ]
    }
  },
  "rollback": {
    "typescript": {
      "rollback": [
        "query-south",
        "south",
        "query-north",
        "north"
      ]
    },
    "rust": {
      "rollback": [
        "query-south",
        "query-north",
        "south",
        "north"
      ]
    }
  },
  "scope": "Native exact instances, successful calls, retained-service close, partial-setup rollback; no network/persistence proof."
}

```

Runtime candidate 765db7a adds SDK dependencies absent from this TS root lock. Preparing that candidate made frozen install reject, as expected; the root lock was preserved and pinned preparation restored for the native example. This is a source-integration gap, not a Core example failure. The Runtime reference documents it.

## Remaining boundaries

No new network, UI, authorization, persistence or performance claim comes from the paired native fixture. Existing Runtime build/typecheck and independent Rust acceptance are recorded in their owning deliveries. Rust five-plugin candidates, Go Runtime and source-first authoring remain explicitly status-gated. Global installation requires a separate request/approval. Go skill is staged in its independent docs-only bundle until integration ownership is assigned.
