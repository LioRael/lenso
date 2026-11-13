# SDK review after browser-host integration — 2026-11-13

Scope: the TS and Rust SDKs, their native applications, and the independent Team Desk browser consumer. This is a whole-source review of the local unreleased baseline, not a commit comparison. The prior successful browser flows established usability, but did not establish all malformed-response, cancellation or concurrent-context behavior.

## Findings fixed

| Priority | Finding and reproduction | Correction and owning source |
| --- | --- | --- |
| P2 | A successful write with a malformed result produced a ZodError in the host's result wrapper. The form treated it as invalid user input, even though the write was already committed. | `ConsoleClient.callParsed` owns result parsing and classifies parser rejection as `internal`, preserving its private cause. Server ActionErrors keep their original code. The host's `query` wrapper and manual session-result parsing were removed. `typescript/packages/console/src/index.ts:89`; browser page consumers. |
| P2 | Already-aborted calls still acquired credentials; a call aborted while awaiting its token could still invoke an injected transport. A cancelled body read could also reach result parsing if the transport ignored the signal. | The client checks cancellation before acquiring credentials, after acquisition, after transport completion and after body reading. Original abort reasons are preserved. `typescript/packages/console/src/index.ts:95`. |
| P2 | Appending a slash to the complete base URL instead of its pathname corrupted routing when a configured query was present. TS example routers also matched the raw request URL while Rust routed by pathname. | Normalize the URL pathname, preserve the configured query on child requests, and route both TS HTTP examples by pathname. Shared native HTTP acceptance covers a query-bearing Console client against TS and Rust. |
| P2 | A non-JSON response escaped as SyntaxError; structurally shaped manifests with duplicate pages/modules/operations or unknown action references were accepted by the client. | Malformed bodies become safe internal protocol errors. Shape decoding precedes shared typed manifest validation. Invalid server metadata is `internal`, while native manifest construction keeps its conflict/not-found classifications. `typescript/packages/console/src/index.ts:66`; `typescript/packages/app/src/console.ts:89`. |
| P2 | Two session refreshes could arrive out of order: an older editor role response replaced a newer viewer response after revocation. The backend still denied writes, but the UI incorrectly re-enabled them. | The host applies only its newest initiated session refresh within the current connection. One local sequence counter owns this ordering; no SDK identity cache was added. `typescript/examples/team-desk-console/src/main.ts:68`. |
| P3 | Standalone-operation validation repeated page validation's action lookup policy. Rust additionally scanned every module for every standalone binding. | Both native manifest entrypoints now share one internal reference-validation path per language. Rust builds one action set for page and standalone references. `rust/crates/lenso-app/src/console.rs:274`; TS `makeManifest`/`validateManifest`. No public Rust API changed and no speedup is claimed. |

The client cases have failed-first reproductions against the old transport, and the two new browser cases separately reproduced input/result error conflation and stale session context. HTTP query conformance was added with the route correction. The packed consumer verifies inferred parsed-result types, including rejection of an incompatible result assignment.

## Interface changes and restraint

`call(action, input, signal)` still returns unknown. `callParsed(action, resultParser, input, signal)` is the optional short path when the page owns a known result contract. It returns the parser's inferred result, does not validate input or infer a server Action type from its string id, and never retries a write. This is more than moving parsing syntax: it gives the response phase one error/async-completion owner, so form input errors cannot be confused with committed-write response failures.

`validateConsoleManifest` accepts an already shape-checked, typed description. It is not an unknown-JSON decoder. TS native manifest construction and the client share its module uniqueness and reference semantics, including the checks needed for standalone bindings. Rust continues to obtain module uniqueness from its native Catalog and uses one private builder for its Console references. The transport protocol version remains 1.

No Application builder, universal CRUD layer, generated typed RPC client, global session store or framework-specific View base class was introduced. The existing native Action, Resource, Module, Task and Event seams remain useful. HTTP adapters, identity, application forms, record versions and view lifetimes stay with their actual owners. In particular, response failures cannot imply rollback, aborting a request cannot undo a committed write, and a token-provider Promise has its own lifecycle: the SDK prevents later dispatch after cancellation but cannot forcibly stop arbitrary provider work.

Repeated Rust trait implementations still make native associated types and ownership explicit. The browser host's element/dispose/ready contract remains a consumer choice rather than a new SDK execution model. There was no supported reason to delete an entire SDK layer or enforce identical native syntax across languages.

## Verification and assessment

The current [verification record](verification.md) contains executed environments and counts. It includes the regular TS/Rust checks, packed consumers, 19 HTTP cases and eight scenarios in each of Chromium and Playwright WebKit. The new browser scenarios prove that malformed results do not mark input fields invalid and that stale session responses cannot re-enable revoked write controls. Existing success, denial, version conflict, late page/editor response and mobile checks continue to pass.

The [five-dimension assessment](team-desk-assessment-2026-11-13.md#revised-judgment) is updated with these findings. The SDK is a useful preview foundation with a cleaner common path; provider breadth, production identity/operations, Rust MSRV, real Safari/iPhone and performance remain unproven.
