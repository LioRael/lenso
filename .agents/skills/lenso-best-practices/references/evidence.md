# Source status and handoff

Checked 2026-10-15. These checkpoints identify inspected source, not registry publication or an instruction to downgrade installed dependencies.

| Owner | Evidence baseline | Status |
| --- | --- | --- |
| TS Lenso | `74139b0` (including DX Bun Web quickstart) | Existing develop/diagnose skills updated in place; current source exports govern |
| Native Rust | `e5dfafb` | Core admission/cancel-safe startup, Log, Cache and native Runtime adapter |
| Runtime | `765db7a` | TCP/stdio/WebSocket, cancellation, backpressure, generated TS/Rust clients |
| Go | local `lenso.local/go` core snapshot; see Go skill evidence | Prototype; verify delivery before using Runtime APIs |
| Console | `c8fff3e` and installed console-page-interactions skill | Host support and publication must be verified in the consumer |

Pending at initial drafting: Rust Limits/Authorization/Audit/Storage/Realtime; TS source-first Runtime authoring; Go Runtime SDK. TS DX recipes were separately delivered as local `b79aa1e`; their source/report was inspected and the develop skill has a gated recipe reference. They are not silently integrated or claimed published. A task report or branch can be referenced as candidate work, but update usable recipes only after checking commit, public exports, dependency provenance and focused validation. No API is inferred from its name.

For a handoff, state files and owner, installed/pinned source, what changed for the caller, exact verification commands/results, and unrun or unsupported boundaries. Keep executable examples beside the skill or point to existing runnable examples. Record actual tests, not desired tests. Do not claim runtime evidence from typechecking alone or persistence from in-memory fixtures.

For core/authorization/money/lifecycle/persistence work, preserve a focused failure proof that protects the actual invariant. Prefer extending existing coverage; skip tests that merely restate implementation or copy values. No heavy benchmark is required for a skill update.
