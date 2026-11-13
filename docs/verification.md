# Local verification

Verified on 2026-11-13 (UTC+8), macOS arm64, Node.js 26.13.1, npm 11.20.0 and Rust 1.99.0.

| Check | Result | Evidence scope |
| --- | --- | --- |
| `npm run typecheck` | Passed | Strict checking and declaration builds for all three SDK packages and all three native TypeScript examples and the independent browser host; also verified with generated distributions removed |
| `npm run lint` | Passed | Oxlint checks package and example source with warnings rejected |
| `npm test` | 88 tests passed | Action/schema conformance, native values, policy composition, configured resource scopes, module contracts, Console operations/views, subscription lifetimes and durable queue behavior |
| `npm run test:package` | Passed | Packed all three npm packages and installed Zod, installed archives offline into a separate temporary consumer, checked inference and rejected invalid contracts, and executed native Node imports |
| `cargo check --workspace --all-targets --all-features` | Passed | SDK crates and all three Rust examples |
| `cargo test --workspace --all-features` | Passed | 27 Rust SDK integration tests, one native application test and two compiled/executed documentation examples; shared fixtures run inside the test functions |
| `cargo test -p lenso-app --no-default-features` | Passed | Native action/module/event behavior and the documentation example work without enabling JSON |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Passed | All SDK and example targets |
| `cargo doc --workspace --all-features --no-deps` | Passed | Public Rust SDK documentation and examples |
| `cargo fmt --all --check` | Passed | Rust workspace formatting |
| `npm run test:integration` | 19 tests passed | Private-files and Team Desk HTTP processes, workflow executables, mixed live TS/Rust processes and native module composition |
| `npm run test:browser` | Eight Chromium cases passed | Production browser bundle, real TS/Rust backends, 1440×1000 desktop and 390×844 mobile viewport |
| `CONSOLE_BROWSER=webkit npm run test:browser` | Eight WebKit cases passed | Same compiled pages and eight scenarios against both backends; Playwright engine, not a physical Safari/iPhone run |
| `git diff --check` | Passed | All new repository files included in the review diff |

Rust commands ran against `rust/Cargo.toml`. Native validation used the installed toolchain's Cargo binary directly, with `RUSTC_WRAPPER` and `RUSTC_WORKSPACE_WRAPPER` empty for each invocation. The local `cargo blaze` command resolves to another accelerator, so it was not used. No global Cargo configuration or toolchain was changed.

An additional advisory `clippy::pedantic` pass was reviewed. It reports stylistic suggestions, including unit patterns, constant string lifetimes and intentionally synchronous implementations of async traits; it is not warning-free. The configured strict Clippy check above passes without warnings.

## What the behavior checks establish

- Native actions preserve their language's types and use the same validation and authorization pipeline as external calls.
- TS schema inference rejects incompatible handlers and calls. Optional parser normalization is shared by native/external entrypoints, output invariant failures are internal errors, and async record sinks finish before the call returns.
- Ordered AND/OR policies share denial/error fixtures across languages. Internal failures and cancellation cannot be hidden by an alternative policy.
- Configured resources validate before opening, retain native client values and unwind cleanup. A failed operation and failed cleanup both remain observable.
- Two authenticated users cannot read each other's private files. Neither a forged token nor a request-supplied owner grants access.
- Sessions and file content survive an actual server restart. Session expiration is checked against persisted state.
- The independent Console client can read actual Users, Files and Requests through both backends. Anonymous and ordinary authenticated callers are denied management access; only an explicitly configured subject receives the read-only management scope.
- Both suites read the same action and task fixtures. Queue checks cover retry deadlines, delayed execution, attempt exhaustion, queued/running cancellation, successful effects after late cancellation, recovery from persisted expired leases, two-connection concurrency, heartbeat renewal, stale completion rejection and idempotency conflicts.
- Event fanout persists independent subscriber jobs atomically. Repeated event ids deduplicate deliveries, and a payload conflict rolls back all new jobs in that publication.
- Typed durable publishers do not name subscriber tasks. Local subscriptions have independent lifetimes even when their callbacks are identical.
- Task attempt history retains failures after later success; stale completions cannot overwrite history. Bounds checks reject unsafe or fractional timing, and valid large leases do not overflow heartbeat timers. Unit task payloads/results round-trip as JSON null in both languages.
- Console list/detail/custom-operation bindings reuse Action validation, policies and records. Compiled frontend page registration rejects duplicate contributions without partial installation.
- Native database transaction rollback removes both the domain write and the submitted job.
- Invitation and registration notification examples deliver to the local mailbox once across repeat runs. TS-to-Rust and Rust-to-TS workflow runs reopen the same data without duplicate messages. These examples do not send external email.

## Team Desk application verification

The [Team Desk guide](../examples/team-desk/README.md) describes its domain and local identity model. The initial eight additional integration cases cover four independently authored modules, Console create/assign/status/archive operations, live role changes, foreign-team isolation, competing versions, domain rollback on injected queue failure, persisted tasks after restart, interchange in both directions and simultaneous TS/Rust processes. One case exercises typed TS native exports and independently installed projects; the Rust application test separately exercises typed native inputs and independent Console installation.

The SDK regression checks also cover direct TS Action bindings, shared bindings across pages, conflicting handlers, heterogeneous module registration in the packed consumer, codec/resource type rejection and atomic Rust extension merging. The configured Clippy pass includes the new application.

## Post-Team Desk review regressions

The [follow-up review](post-team-sdk-review-2026-11-13.md) records failed-first cases and fixes. Five additional TS cases cover awaited async invariants, stale unsubscribe handles, owned manifests, publication clock snapshots and lease-loss cleanup. Three additional Rust cases cover typed transaction submission, the shared clock rule and awaited cleanup after lease loss. The native Team Desk tests now prepend an unrelated scope; two additional HTTP cases use shared Unicode text fixtures.

At that stage the integration suite had 17 cases, including ten Team Desk cases. The later browser-consumer review adds two query-routing cases, bringing the current total to 19. Packed-consumer checks, strict compiler/lint/Clippy checks and Rust documentation examples pass with the new interfaces. The unreleased Rust raw transaction helper was renamed to `enqueue_serialized_on`; `enqueue_on` now accepts the Task and native input. Repository consumers were migrated.

## Independent browser host

The [Team Desk Console host](../typescript/examples/team-desk-console/README.md) is a separate package and process. Its compiled page factories use `ConsolePages<View>` and `ConsoleClient`, shared browser-safe application contracts and fixed configured same-origin proxies. The local launcher starts the built host with independent TS/Rust databases. The full HTTP suite now has 19 passing cases, including query-bearing base URLs against both backends.

Playwright 1.65.0 with its matched Chromium 158.0.8127.18 and WebKit build 2388 executes the same eight cases in each engine: two full read/write flows (one per backend), two live-revocation flows, backend reconnect isolation, editor replacement during a delayed committed write, malformed write responses and out-of-order session refreshes. Coverage includes empty/invalid form inputs, keyboard submit, disabled/pending submission state, create/assign/status/archive, role writes, read-only controls, notifications, stale versions, draft preservation, explicit refresh/retry and prevention of obsolete responses repainting a new view. Desktop and 390px mobile screenshots were inspected outside the repository. No relevant page errors or unexplained console warnings/errors were observed; 401/403/409 request diagnostics are expected in denial/conflict cases.

The SDK regressions cover default and injected native fetch receiver preservation, standalone operations, duplicate ids, authorization and missing catalog references. The browser default transport failed before the receiver correction, despite passing earlier Node-only integration flows. The new current-session and notification-worker bindings continue to use the ordinary Executor and do not appear as fake page controls.

The Browser plugin/skill was not listed in this session, so regular Playwright was used. The CI workflow now includes browser installation and the Chromium suite; this records local results only, not a remote CI execution.

## Browser-consumer SDK review

The [review](browser-consumer-sdk-review-2026-11-13.md) records ten new client cases for parsed responses, server-error preservation, pre-dispatch cancellation, body-read cancellation, base URL/query handling, malformed JSON and invalid manifest invariants. The packed consumer checks inferred parsed result types. Two added browser cases run in both engines: an already committed write with an invalid result does not mark user fields invalid, and an older session response cannot replace a newer revocation. TS server and client share typed manifest semantics; Rust has one private reference builder for both normal and standalone bindings. The latest suite has 88 TS cases, 28 Rust cases plus two documentation examples, 19 HTTP cases and eight cases per browser engine.

## Limits

These runs establish the browser host workflows only in the specifically verified local engines and viewports. They do not establish Firefox, actual Safari or iPhone execution, other JavaScript runtimes, other operating systems, Rust minimum-version compatibility, performance gains or production identity-provider readiness. Lease recovery tests reopen explicitly persisted expired worker state; they do not claim an exhaustive operating-system crash or storage-failure test.

The tables above record local verification. Remote workflow results are tracked separately in [GitHub Actions](https://github.com/LioRael/lenso/actions). GitHub source delivery does not constitute an npm/crates.io release, application deployment or external email delivery. CLI Framework, Official Registry and the full Official Console product remain separate work; the new browser host is a tested reference consumer.
