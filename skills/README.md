# Plugin-first Lenso skills

These public skills follow the vNext product model on `main`: selected product
behavior is built from ordinary Plugins, while Capabilities, App configuration,
and the runtime seams each keep one distinct responsibility.

This catalog is project-level agent documentation. It routes work to the
repository that owns each implementation; its location does not move CLI,
protocol, Driver, Adapter, Plugin, or example ownership into the portable core.

```text
product outcome
      |
      v
Plugin map -> Capability contracts -> Plugin Contract + implementations
                                      |
                                      +--> optional portable package
                                      |        |
                                      v
                    Host Catalog + plugins/ -> Resolved App Plan
                                      |
                                      v
                    Driver + Adapters + App Generations
```

“Everything is a Plugin” is the product rule, not a reason to turn every
technical object into one. A Plugin owns removable product behavior. A
Capability is its collaboration Interface. One Plugin Contract may have
several executable implementations with identical product semantics. The Host
Catalog supplies defaults and implementation policy; App configuration records
only differences in `plugins/`; resolution selects one exact implementation
and derives Instances and bindings. Runtime Drivers, Execution Adapters,
Runners, and the portable Kernel make those Plugins run.

| Skill | Use it for |
| --- | --- |
| `lenso-start` | Explicitly route a request to one primary vNext workflow. |
| `lenso-business-planning` | Turn a product outcome into a vertical Plugin map. |
| `lenso-capability-authoring` | Design or evolve a versioned Capability contract and bindings. |
| `lenso-plugin-authoring` | Implement one Plugin Contract through a supported linked Rust, portable Rust, or Bun path. |
| `lenso-app-configuration` | Configure `plugins/`, inspect the Host-derived App, and prove behavior/removal. |
| `lenso-runtime-extension` | Extend the Driver, Execution Adapter, Runner, App Generation controller, or other host mechanism. |

## How to use the pack

Start with `lenso-start` when the owner is unclear. Once routed, load one
primary workflow and only the branch references that match the task:

```text
unclear outcome/owner
        |
        v
business planning
        |
        +--> Capability source + generated bindings
        +--> Plugin behavior + factory/entrypoint + lifecycle
        +--> Plugin Root differences + derived Plan
        `--> Driver/Adapter/Runner host mechanics
```

The entrypoint skills stay short enough to preserve their ordered workflow.
Detailed branch references contain the package layouts, code/config examples,
failure cases, verification matrix, and completion evidence needed to perform
the work. Exact current APIs and commands still come from the selected package
versions, repository instructions, manifests, `--help`, and CI.

The support-ticket planning example shows how the workflows hand off to each
other. Plugin authoring distinguishes the current CLI Rust scaffold, linked
native Rust facade, and Bun request SDK instead of presenting one fictional
universal generator. Capability and App configuration document typed contracts
and the visible Plugin Root.

The old Service, Provider, Host, Console Surface, and API-client workflows are
not peer vNext authoring models. Out-of-process behavior, UI Contributions,
Auth, State, Story, Audit, OpenTelemetry, Web ingress, and similar product
concerns route through Plugin authoring. Generated consumers and providers
route through Capability authoring. Process, transport, and endpoint mechanics
route through runtime extension.

Plugin authoring owns behavior, the runtime-independent Contract, and published
implementations. App configuration owns only visible `plugins/` differences.
Host Catalog generation owns product defaults, root Slots, and implementation
selection policy. Runtime Extension owns that Host machinery plus readiness,
reconciliation, and App Generation mechanics. Store, Receipt, Controller, and
Supervisor remain internal implementation concepts rather than additional
authoring models.

Install this catalog from its owning repository with:

```sh
npx skills add LioRael/lenso --list
```

Install every skill for the detected project agents:

```sh
npx skills add LioRael/lenso --all
```

For a user-level Codex installation:

```sh
npx skills add LioRael/lenso --skill '*' --agent codex --global --yes
```

Update installed skills later with `npx skills update`; same-named skills are
refreshed, while unrelated legacy skill names are not deleted automatically.

## Maintainer validation

Run structural/reference validation and installer discovery from the repository
root:

```sh
python3 skills/scripts/validate-pack.py
npx skills add . --list
```

Use `--installed-root` to detect a stale installed copy:

```sh
python3 skills/scripts/validate-pack.py \
  --installed-root "$HOME/.agents/skills"
```

Structural validation does not prove usefulness. Run the independent prompts
and evidence checks in [behavioral scenarios](validation/scenarios.md) after a
substantial workflow change. A skill passes only when the agent reaches the
expected artifacts and observable completion state without being told the
intended implementation.
