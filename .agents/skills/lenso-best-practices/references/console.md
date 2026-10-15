# Console: reuse the existing host and interaction contract

Use the existing **console-page-interactions** skill when changing page UI, controls, selection or host projections. Do not copy its design rules into this skill. In the reviewed environment it lives at `~/.agents/skills/console-page-interactions/SKILL.md`; locate the installed skill rather than assuming that absolute path on another machine. Its `references/runtime-support.md` distinguishes local and compiled/admitted hosts. If that skill is unavailable, read the consuming Console repository's `docs/design/console-interaction-direction.md` and the relevant public package README; do not fabricate missing host APIs.

Current evidence: Console `c8fff3e`, `packages/console/README.md`, `packages/console-react/README.md`, `packages/console-authoring/README.md`, `examples/ts-console`. This is a source checkpoint, not a release assurance.

Console composes into the application's existing TS plugin graph. The application owns Web/listener and exact Auth provider; Console owns explicitly selected targets, pages and management admission. Pages alone do not enable RPC; management operations are explicitly selected. Do not introduce a parallel container, user database, global method scan or marketplace service inside Console.

Business services remain authoritative for permissions and effects. Bind real trusted request evidence into selected Manage operations; a Console actor is not automatically a business-audience actor. Browser bundles receive browser-safe exports only, never credentials/config readers. Errors crossing UI/HTTP entries use safe public classification and retain internal causes privately.

Local `ConsoleLocalPageProps` and SDK `PageProps` have different navigation/lifetime contracts. Public exports alone do not prove the host renders a contribution. Consult the interaction skill's host support reference before promising a page Command/Context Bar or local Dock view. Reuse the existing guarded action instead of another UI execution store; keep persistent business state with its page and presentation with its host.

For actual interactive changes, follow the existing Console direction and verify in a real browser. Documentation-only edits need no artificial browser fixture. Reuse focused visual/keyboard/accessibility checks for the changed surface, not a full UI matrix or static-copy snapshots. This skill authorizes no Console refactor or publication by itself.
