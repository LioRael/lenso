# Module-first Lenso skills

These public skills follow the vNext product model on `main`: selected product
behavior is built from ordinary Modules, while Capabilities, App Composition,
and the runtime seams each keep one distinct responsibility.

This catalog is project-level agent documentation. It routes work to the
repository that owns each implementation; its location does not move CLI,
protocol, Driver, Adapter, Module, or example ownership into the portable core.

```text
product outcome
      |
      v
Module map -> Capability contracts -> Module implementations
                                      |
                                      v
                         App Composition -> Resolved App Plan
                                      |
                                      v
                         Driver + Execution Adapters
```

“Everything is a Module” is the product rule, not a reason to turn every
technical object into one. A Module owns removable product behavior. A
Capability is its collaboration Interface. App Composition selects Instances
and bindings. Runtime Drivers, Execution Adapters, Runners, and the portable
Kernel make those Modules run.

| Skill | Use it for |
| --- | --- |
| `lenso-start` | Explicitly route a request to one primary vNext workflow. |
| `lenso-business-planning` | Turn a product outcome into a vertical Module map. |
| `lenso-capability-authoring` | Design or evolve a versioned Capability contract and bindings. |
| `lenso-module-authoring` | Implement any Module shape, including Rust, Bun, Web, stateful, and cross-cutting Modules. |
| `lenso-app-composition` | Select packages and keyed Module Instances, bind Capabilities, and resolve the Plan. |
| `lenso-runtime-extension` | Extend the Driver, Execution Adapter, Runner, or host mechanism that executes Modules. |

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
        +--> Module behavior + factory/entrypoint + lifecycle
        +--> App package/Instance/binding choices + Resolved Plan
        `--> Driver/Adapter/Runner host mechanics
```

The entrypoint skills stay short enough to preserve their ordered workflow.
Detailed branch references contain the package layouts, code/config examples,
failure cases, verification matrix, and completion evidence needed to perform
the work. Exact current APIs and commands still come from the selected package
versions, repository instructions, manifests, `--help`, and CI.

The support-ticket planning example shows how the workflows hand off to each
other. The Module references contain complete native Rust, Bun, Web/UI,
stateful, and cross-cutting recipes. Capability and App Composition contain a
request contract and a source-derived `lenso.app.json` example.

The old Service, Provider, Host, Console Surface, and API-client workflows are
not peer vNext authoring models. Out-of-process behavior, UI Contributions,
Auth, State, Story, Audit, OpenTelemetry, Web ingress, and similar product
concerns route through Module authoring. Generated consumers and providers
route through Capability authoring. Process, transport, and endpoint mechanics
route through runtime extension.

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
