# Optional file-convention authoring

Use this branch for `cli.ts` / `cli.rs`, Console pages/services or
`agent/tools.ts` / `agent/tools.rs`. Inspect current support metadata, compiler
and executable examples before selecting API spellings. These additions are
local implementations as of 2026-09-20, not a registry release claim.

## Ownership and package boundaries

A support Plugin makes a convention available; App selection activates it.
Engine owns generic discovery/processing, the support package owns filenames
and lowering, and the business Plugin owns state and final authorization.
Unselected independent surface packages must not be parsed, installed, compiled
or bundled. Dependencies already imported by the core or its Cargo workspace
remain part of that build. Use one manifest for a simple Plugin; use independent
surface packages when optional dependencies need separate build closures.

Rust and Bun retain their own SDKs. Generic Engine subprocess processors may
use other languages, but that is not evidence of a new business Plugin Adapter.
App source discovery is not arbitrary live Kernel graph mutation.

## Select the concrete path

- CLI: inspect Engine `crates/lenso-engine-app/assets/terminal/` and CLI
  `docs/convention-authoring.md`. TS uses `command` from `@lenso/cli`;
  Rust uses `#[command]` from `lenso_cli_support`, typed arguments and optional
  output context. Exercise Stream commands through `app dev -- ...` or
  `app start --from dist -- ...`, not Request-only standalone invocation.
- Console: inspect Console `packages/console-sdk/README.md` and
  `examples/app-console/`. `console/page.tsx` and nested pages use
  `PageProps`, scoped navigation and the Shell's React singleton. Supported
  files include layout/loading/error and root not-found. Catch-all params are
  arrays; ordinary params are strings. Strict source/signature checks run before
  bundling. This is the application's Console, not the Agent Web console.
- Services: optional `console/services.ts` uses
  `@lenso/console-sdk/server` with `defineServices` and `operation`.
  Every operation parses and explicitly authorizes before handling. The
  generated adapter and contribution share one Instance and owner-scoped
  aliases. Browser bundles must exclude server source. This helper is
  request-only; cross-Plugin business access still uses explicit generated
  Capability dependencies and the domain owner's authorization.
- Agent: inspect Agent `packages/agent-tool-convention/` and
  `examples/app-tools/` / `examples/app-tools-rust/`. TS uses
  `tool` / `tools` from `@lenso/agent-tool-sdk`; Rust uses
  `#[lenso_agent_tool_sdk::tool_provider]` and `#[tool]`.
  Compilation produces ordinary Tool providers. It neither starts an Agent
  nor grants tools to a Model.

## Proof

Exercise the selected consumer, then remove support and repeat. Console needs
a real App with no Agent, route rendering, authorized/denied service requests
and server-source exclusion. Agent needs an actual Turn with allowed execution,
an out-of-scope call rejected before execution, and removal from the catalog;
`examples/app-tools/verify-agent.py` supplies deterministic fixture evidence.
Record any local Adapter patch separately from released dependency availability.
