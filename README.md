# Lenso

**Build products from replaceable Plugins.**

Lenso is a local-first, language-independent runtime for applications whose
behavior must be added, replaced, and removed without turning composition into
hidden framework magic.

Define product roles as typed Capabilities, implement them as Plugins, and let
each Host resolve one exact application before it boots. Humans and coding
agents work against the same inspectable Plugin Root and the runtime executes
the resulting immutable Plan with explicit lifecycle and failure semantics.

[Get started](https://lenso.dev/docs/quickstart/) ·
[Read the documentation](https://lenso.dev/docs/) ·
[Contributing](CONTRIBUTING.md) ·
[Explore executable examples](https://github.com/LioRael/lenso-examples) ·
[Install the Agent skills](skills/README.md)

## What Lenso gives you

- **Replaceable product behavior.** Plugins own their configuration, state,
  lifecycle, failure policy, and provided or required Capabilities.
- **Typed collaboration.** Capability Interfaces make Plugin relationships
  explicit across request, stream, and event Operations.
- **Reviewable composition.** An App owner changes only the visible `plugins/`
  directory; the Host supplies defaults and rejects ambiguous or incompatible
  selections.
- **Deterministic execution.** Every accepted App becomes an immutable Resolved
  App Plan before the Kernel starts it.
- **Agent-ready workflows.** Public skills route planning, Capability,
  Plugin, App configuration, and runtime work to checkable artifacts and
  evidence.

Lenso is designed for long-lived products with evolving boundaries: business
systems, developer tools, automation products, and Agent applications. It is
not a Web framework, a distributed control plane, or a promise that every
Plugin can run unchanged in every environment.

## Try one Plugin

Install the CLI and exercise a typed Plugin through a real Execution Adapter:

```sh
npm install -g @lenso/cli

lenso plugin new example.echo
cd example.echo
lenso plugin check
lenso plugin dev \
  --operation execute \
  --request-json '{"name":"example.echo","arguments_json":"{\"text\":\"hello\"}"}'
lenso plugin pack
```

The generated Rust project produces portable Wasm and trusted Process
implementations from the same source. `plugin dev` selects the fastest local
implementation; `plugin pack` builds and verifies the distributable
`.lenso-plugin` Release. A Bun / TypeScript path is also available with
`lenso plugin new example.echo --runtime bun`.

Read the [complete quickstart](https://lenso.dev/docs/quickstart/) to understand
how the verified Bundle connects to a compatible product Host.

## How it works

```text
Plugin source -> generated Descriptor -> Host + Plugin Root -> Resolved App Plan
                                                             |
                                                             v
Runtime Driver -> portable Kernel -> Execution Adapters -> Plugin Instances
```

The Plan records exact Plugin identities, Capability bindings, execution
choices, and policy inputs. The Kernel validates and runs that Plan; it does
not discover packages, choose versions, or rewrite the application graph while
booting.

The `main` branch contains the vNext runtime and its design evidence. The final
v0.3.x source remains available from the `lenso@0.3.47` tag and Git history.

## Workspace

The Rust workspace is the shared home for the framework's frequently co-evolving
main chain:

- Plan, Kernel, Runtime Driver interfaces, and deterministic conformance;
- native, Process, Wasm, QuickJS, Bun, and remote Execution Adapters;
- Engine authoring, configuration, resolution, and embedding APIs;
- the `lenso` CLI and Rust Plugin authoring SDKs;
- portable contract tooling plus language-neutral fixtures under `spec/`; and
- optional Rust Web packages and focused executable examples.

Internal crates use workspace or path dependencies so a framework change can be
validated atomically without publishing temporary versions. Public crate names
and versions remain stable, and packaged-consumer validation remains a separate
release gate.

The repository boundary follows language and product ownership rather than
runtime mechanics. JavaScript and TypeScript SDKs, Bun fixtures, and browser
integration live in `lenso-js`. The Site, Lenso UI, Marketplace backend, and
downstream products remain independent. Optional product Plugins such as Auth
stay with their product owner unless frequent shared Rust evolution provides a
concrete reason to move them here. See ADR 0077.

The Kernel has no Service, Provider, System Plane, Console, Story, Auth,
PostgreSQL, Outbox, Workflow, migration, release, or discovery implementation.
Those concerns can return only as ordinary Plugins, Execution Adapters,
authoring tools, or separate repositories when a vNext decision assigns them an
owner.

## Agent skills

The [project skill pack](skills/README.md) turns the vNext architecture into
cross-repository planning, Capability, Plugin, App configuration, and runtime
workflows without relocating implementation ownership. List the six workflows
with:

```sh
npx skills add LioRael/lenso --list
```

Start with `lenso-start` when the owning seam is not yet clear.
The [Agents and skills guide](docs/agents/skills.md) documents invocation,
installation, progressive disclosure, contributor validation, and behavioral
forward testing.

## Contributing

[`CONTRIBUTING.md`](CONTRIBUTING.md) is the human contribution entry point.
Editors and AI tools are optional: contributors can use any development setup
that produces a reviewable immutable commit. Maintainers run the one necessary
upstream candidate gate before fast-forward integration.

Choose focused checks for prose, Rust code, or workflow/build changes rather
than running every workspace and platform command for every edit. The portable
Plan, Kernel, and conformance Interface are compile-checked for
`wasm32-unknown-unknown` and `wasm32-wasip2` when the final candidate requires
that proof. Host Driver and Adapter repositories own their target-specific
checks against released core packages.

## Architecture

- [`CONTEXT.md`](CONTEXT.md) is the canonical vocabulary and invariant set.
- [`docs/architecture/lenso-vnext.md`](docs/architecture/lenso-vnext.md) is the
  runtime overview.
- [`docs/architecture/lenso-authoring.md`](docs/architecture/lenso-authoring.md)
  documents project authoring and Plan resolution.
- [`docs/adr/README.md`](docs/adr/README.md) routes the normative ADRs 0030–0077.
- [`docs/architecture/execution-target-capability-matrix.md`](docs/architecture/execution-target-capability-matrix.md)
  defines target-admission facts separately from qualification.

## Branches

`main` is the vNext integration and release line. Work starts from
`origin/main`; maintainers validate an immutable candidate and fast-forward that
exact revision. Landing, CI, package publication, and deployment remain
separate operations.
