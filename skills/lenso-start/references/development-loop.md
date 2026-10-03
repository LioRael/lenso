# Development loop

Start with the requested outcome, repository instructions, current source and
the nearest executable example. Choose one owner and implement the smallest
useful change. A simple request does not need a separate Plugin card, frozen
handoff, full review or report before editing. Ask only for a missing decision
that changes behavior or authority; continue independent work meanwhile.

## Use the existing tools

CLI and library functions own actual operations. MCP projects those same
operations with its existing authority and limits. Skills select operations;
they do not introduce another mandatory control surface. Inspect installed
`--help`, then reuse create/new, check, show/facts, configure, the affected
language's narrow tests, dev/start and diagnostics. Do not invent a
`narrow-test` command, a route DSL, or an MCP tool absent from current help.

Local source is an ordinary Plugin implementation. Package archives and signed
adoption have their own integrity boundaries; a local edit does not require
packaging, signing or freezing a release. Source, dependency and portable
origins retain the same Plugin/Instance semantics. Report registry/toolchain
failures separately from a workflow problem. A scaffold alone is not proof.

## Progressive authoring

Ordinary authors start with Plugin, Instance, Config, Dependency and Target.
Use existing Capabilities when available; author a new Capability only for a
real collaboration boundary. Internal helpers stay ordinary code modules.
Implement only the required target and storage. Rust and TypeScript are first
class authoring paths where the selected tooling supports them; unused
language runtimes, generators and portable packaging are optional. Cross a
language boundary only for a real, coarse-grained Capability interaction, and
retain same-language typed calls and optimizations.

One implementation may serve several Instances. Each Instance owns its
configuration, dependency binding, resources, mutable state and permissions;
sharing implementation code does not share authority or mutable state.

App Composition and semantic resolution remain shared. Validate support and
bind resources for each selected target before lowering and readiness. An
entrypoint or private platform binding may be necessary; an application-level
Native/Workers business Host fork is not the authoring path. Reusing resolution
does not skip target-specific admission or resource checks.

## Choose proof by risk

| Changed behavior | Minimum meaningful evidence |
| --- | --- |
| Documentation or skills | Relevant syntax, links, pack/configuration validation; exercise the changed workflow when command behavior is claimed |
| Frontend UI | Existing frontend checks, rendered screen and affected interactions |
| Business Plugin | Affected tests and one real consumer/request through the selected Host/Adapter; changed success and rejection paths |
| Capability contract | Compatibility against the accepted contract, generated freshness and affected published languages/consumers |
| Runtime or target | Affected target conformance and real Host/Adapter behavior; startup, cancellation and cleanup where changed |
| Money, Auth or migration | Explicit authority/trust boundary; positive and denied requests; affected concurrency, idempotency and recovery against real storage, including rollback/resume when claimed |

Lifecycle, replacement/removal, multiple implementations and durability need
additional proof when introduced, changed or explicitly claimed. Do not replay
unaffected suites after every edit. Broaden checks after failures or new risks.

Repository final review and the exact candidate gate still apply at delivery,
once the change is ready. Re-run them for a changed candidate or integration
base, not for each intermediate step. Publication, deployment, external
messages and credential/permission changes keep their explicit authority.
Never change platform permissions or treat a safety refusal as bypassable.

Finish with a concise result, evidence and limits. Record actual commands,
exit/status and useful response details, excluding secrets and private logs.
