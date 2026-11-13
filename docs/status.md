# Implementation status

This is a local SDK baseline, not a released backend platform. TypeScript and Rust implementations share behavioral acceptance fixtures and external Console description semantics. Their native interfaces intentionally differ.

| Area | Implemented baseline | Further product work |
| --- | --- | --- |
| Action | Native execution, inferred TS definitions, shared native/external validation, optional external codecs, structured errors, cancellation and owned call-record completion | More HTTP/Agent integrations, transactional audit provider |
| Access | Trusted principals, exact scopes, ownership, ordered AND/OR policy composition; dependency failures cannot become permission grants | Organization/tenant policy modules and production role management |
| Resource | Configuration validation before connection, native values, explicit lifecycle scopes and asynchronous cleanup, startup unwinding and test replacement | Provider-specific health integrations |
| Module | Actual typed native operation exports, resource requirements, introspection and atomic collision rejection | Registry packaging and version compatibility tooling |
| Task | SQLite persistence, retry/delay, attempts, lease recovery and fencing, per-kind concurrency, cancellation, results and per-attempt history | Additional providers, worker hosting, operational retention |
| Events | Independent local subscriptions; typed durable event subscriptions map to task inputs; atomic fanout integrates with native transactions | Subscription distribution, replay and ordering policies |
| Console | Actual Action bindings for lists/details/custom controls, compiled frontend page registration, headless client, an independent Team Desk browser host, and protected Users/Files/Requests operations | Independent Console application, built-in UI views and production write-operation UX |

## Verified environments

The local development environment is macOS arm64, Node.js 26 and Rust 1.99. Core TypeScript source uses standard JavaScript and web platform types. The SQLite package and examples use Node's native SQLite module. Other JavaScript hosts, Windows/Linux execution, a Rust minimum-version build and broader browser support need their own runs before support is claimed.

## Example scope

The private file applications are local reference implementations with actual scrypt password verification, SHA-256-hashed expiring session tokens, persisted file bytes and explicit Console authorization. They are not production identity providers: rate limiting, recovery flows, MFA, revocation administration, external storage and deployment hardening are outside these examples. SDK Access does not implement password authentication itself.

Task and event behavior is exercised against real SQLite connections, including native transaction rollback and multiple worker connections. The workflow examples validate invitation mail and registration notifications using a local SQLite mailbox resource. Domain writes and task submission share a transaction, and delivery uses the job id for idempotency. Both languages can reopen the same example data. No external email is sent; integrating a real mail provider remains application-owned.

The seven SDK responsibilities have separate acceptance coverage. A type declaration or metadata list alone is not evidence that its corresponding mechanism is complete. See the [design review](review-2026-11-13.md) for the implementation gaps corrected against the product baseline.

## Multi-module application evidence

Team Desk adds independent members, projects, work-items and notifications modules, with real Console writes in both languages. Tests exercise stored roles, tenant isolation, optimistic concurrency, native transaction rollback, restart and mixed-language live processes. TS page bindings accept Actions directly and heterogeneous modules compose in a loop; Rust extensions compose atomically and generate their own module descriptions. See the [application guide](../examples/team-desk/README.md) and [authoring assessment](team-desk-assessment-2026-11-13.md). The independent Team Desk browser host now exercises the same compiled pages against both backends, including forms, permissions and conflict recovery. Production deployment and the full Official Console product are not established by this reference host.

The [follow-up SDK review](post-team-sdk-review-2026-11-13.md) additionally corrected async invariant ownership, stale subscription removal, manifest snapshots, publication clock consistency and lease-loss cleanup. Rust transaction submission now retains its typed Task/input interface. Team Desk scopes no longer depend on order and Unicode normalization shares acceptance fixtures. The current qualitative assessment is in the [revised judgment](team-desk-assessment-2026-11-13.md#revised-judgment).

## Repository and release policy

Language builds and checks run independently. Cross-language integration is an additional check. Native package versions may advance independently; only an actual external protocol change needs coordinated compatibility work. No RPC, FFI or code generation connects the language implementations.

CLI Framework, Official Registry and the full Official Console application remain separate products. This SDK source is delivered at [LioRael/lenso](https://github.com/LioRael/lenso); the predecessor retains its history at [LioRael/lenso-previous](https://github.com/LioRael/lenso-previous). The SDK packages are released to npm and crates.io through GitHub Trusted Publishing; see the [release receipts](publishing.md#verified-release). Registry publication does not deploy an application.

The [browser-consumer SDK review](browser-consumer-sdk-review-2026-11-13.md) adds parsed-result ownership, cancellation preflight, URL/query correctness and shared manifest invariants. The host's session ordering remains application-owned. The revised assessment is a useful preview SDK, not a claim of production platform maturity.
