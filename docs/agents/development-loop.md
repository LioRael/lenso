# Lightweight agent development

State one observable result and let the agent implement it through the existing
CLI/library. For example:

> Add a local Rust `POST /greet` Plugin that trims a name, returns a greeting, rejects empty names, and prove it with affected tests and a real request.

The runnable [source example](../../examples/agent-flow/README.md) includes
the implementation, commands and a real HTTP smoke. Use `lenso-start` when the
owner is unclear; otherwise invoke the matching owner Skill directly. A clear
request does not require a planning card, freeze, handoff, full review or long
report before each edit. Continue through ownership seams in the same task;
handoff only when another owner must actually act.

Start with Plugin, Instance, Config, Dependency and Target. Internal helpers
stay ordinary modules. Add a Capability when a real collaboration boundary
needs a stable role, and reuse existing contracts first. Rust and TypeScript
are first class where selected tooling supports them; pay for only the target,
storage and language tools required now. Preserve same-language optimizations
and use coarse-grained Capability calls for genuine cross-language edges.

One implementation can create multiple Instances, each with independent
config, bindings, resources, state and permissions. Source, dependency and
portable packages are origins of the same Plugin semantics. A local source
edit does not require packing or freezing an immutable release.

Shared App Composition and semantic resolution do not remove target-specific
support validation, resource binding or lowering. Necessary entrypoints and
private platform bindings are allowed; Native/Workers business Host forks are
not the default authoring path.

## CLI, library and MCP

The CLI and library are the capability owners. MCP is a projection of those
operations with its existing fixed roots and permissions. Skills choose the
next relevant operation rather than add a control plane.

| Need | Existing operation |
| --- | --- |
| Start source | `app create` or `plugin new`; inspect selected `--help` |
| Inspect source | `app discover --json`; discovery is not activation |
| Inspect built state | `app check`, `app show`, `app explain`, `doctor`; MCP `project_check`, `project_facts`, `project_explain` where available |
| Change an Instance | `plugins configure`, enable/disable/remove; existing MCP preview/apply when that authority is enabled |
| Narrow proof | Actual affected Cargo/Bun/frontend test or check; there is no required new `narrow-test` command |
| Build/run | `app build`, `app dev`, `app start`; existing MCP build/run tools when separately enabled |
| Diagnose | First failing tool's diagnostic, current `--help`, and bounded facts pages |

By default the MCP bridge is read-only. Existing build/run/change flags express
local application authority, not permission to change platform security. A
source App must be built before inspecting its generated Host authority; use
`scope: "built_distribution"` where current MCP help supports it. Do not assume
all CLI commands have MCP counterparts. The
[CLI MCP guide](../../crates/lenso-cli/README.md#inspect-an-app-through-mcp)
documents current projections and constraints. No MCP changes are needed for
the source example.

## Validate the changed risk

| Change | Development proof |
| --- | --- |
| Docs/skills | Focused syntax, links and pack checks; real commands when the workflow changes |
| UI | Frontend checks plus rendered result and affected interactions |
| Business | Affected tests plus a real consumer/request and changed rejection paths |
| Contract | Compatibility, generated freshness and affected published languages/consumers |
| Runtime/target | Affected target conformance plus real Host/Adapter paths |
| Money/Auth/migration | Explicit authority and trust boundary, allow/deny, affected concurrency, idempotency and recovery with real storage; rollback/resume when claimed |

Add lifecycle, cancellation, removal/replacement, multi-implementation and
durability proof when changed or claimed. Broaden testing when new evidence
demands it, rather than repeat unaffected suites after every step. Existing
repository final review and exact candidate CI remain required at delivery and
repeat for a changed candidate/base. Publication and deployment remain separate
authorizations. Never treat a security rejection as bypassable.

Record actual commands, exits and useful responses, excluding credentials and
private logs. Separate source-toolchain setup, registry availability, compile
cost, runtime failure and workflow overhead. The six canonical Skills are
routing aids; they do not replace real execution evidence or impose six stages.
