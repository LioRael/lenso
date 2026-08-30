# Lenso agent skills

The canonical Skill pack mirrors the public documentation without copying its
page tree. A user enters through Core, Web, or Agent; the pack then routes the
first concrete artifact to one stable owner Skill.

## The two layers

```text
task-oriented entrance                    owner workflow

Core ─┐
Web  ─┼─> lenso-start ──────────────────> Planning | Capability | Plugin
Agent ┘                                      | App configuration | Runtime
```

The entrance explains the journey. The owner Skill controls implementation.
This avoids two failure modes: forcing a new user to understand framework
ownership before stating a result, and creating separate Web/Agent Skills that
duplicate Plugin, App, and runtime rules.

## Choose the task map

Invoke `$lenso-start` when the owner or sequence is unclear:

| Path | Use it for | Observable completion |
| --- | --- | --- |
| Core | Plugins, Capabilities, App changes, composition, runtime, framework semantics | One owned artifact plus success, honest failure, inspection, and removal/replacement proof |
| Web | Endpoints, Auth, upstream calls, Ingress, socket tests, deployment | One real HTTP path with readiness and removal proof |
| Agent | Profiles, Tools, Sessions, Memory, child Agents, MCP, surfaces | One real Turn with Session and Tool/Context evidence |

The selected map names one primary owner and any later handoffs. It does not
run several owner workflows in parallel.

## Choose the owner

| Result | Primary Skill | Completion artifact |
| --- | --- | --- |
| Ownership or product behavior is unclear | `lenso-business-planning` | Plugin cards, Capability edges, and one tracer slice |
| Define or evolve a cross-Plugin role | `lenso-capability-authoring` | Descriptor, Schemas, generated projections, compatibility and behavior proof |
| Implement removable behavior | `lenso-plugin-authoring` | Plugin Contract, selected implementation, real consumer path, and deletion proof |
| Change one App's visible selection or configuration | `lenso-app-configuration` | Minimal `plugins/` difference plus checked and explained derived App |
| Add Host execution or Generation mechanics | `lenso-runtime-extension` | Narrow Driver, Adapter, Runner, or Host-policy change with conformance and real-host proof |

Portable Kernel semantics are handled through [`CONTEXT.md`](../../CONTEXT.md),
the relevant ADR, and product-neutral conformance. Kernel is not a routine
sixth owner workflow.

## Invoke the pack

`lenso-start` is an explicit human-invoked router:

```text
Use $lenso-start to route and complete this Lenso task.

Outcome: [one observable user or Plugin result]
Authority: [repository, Host, Capability, or unknown]
Constraints: [language, target, compatibility, security, delivery scope]
Completion:
- [success to exercise]
- [honest failure to preserve]
- [inspection and removal/replacement proof]
- [requested commit, PR, or merge boundary]
```

The five owner Skills are model-discoverable and may also be named directly.
Use one primary Skill. Add a secondary Skill only after an artifact crosses a
real ownership seam.

## How the directory works

Canonical sources live under [`skills/`](../../skills/):

- `SKILL.md` is the small owner Interface and ordered workflow;
- `references/` contains one conditional task or implementation branch;
- `agents/openai.yaml` contains discovery metadata and invocation policy;
- `scripts/` contains deterministic pack checks; and
- `validation/` contains independent forward scenarios.

Installers discover `skills/<name>/SKILL.md`, so canonical Skill directories
remain flat. Conditional detail may nest inside `references/`. For example,
Plugin authoring loads only one of portable Rust, linked Rust, or Bun before it
loads shared Contract/lifecycle and verification rules.

Keep each rule in one source of truth. Use current owner source, manifests,
locks, installed `--help`, and CI for cheap exact lookups; references should
carry ownership decisions, failure boundaries, implementation shapes, and
completion criteria that cannot be inferred safely from one command.

## Install and update

```sh
npx skills add LioRael/lenso --list
npx skills add LioRael/lenso --all
```

For user-level Codex discovery:

```sh
npx skills add LioRael/lenso \
  --skill '*' \
  --agent codex \
  --global \
  --yes
```

Refresh same-named Skills with `npx skills update`. Installed copies are
derived artifacts; edit only the canonical repository source.

## Validate a change

```sh
python3 skills/scripts/validate-pack.py
python3 -m unittest skills/scripts/test_validate_pack.py
npx skills add . --list
```

Run the active Agent runtime's official validator for every changed Skill.
After a substantial routing or workflow change, run the independent prompts in
[`skills/validation/scenarios.md`](../../skills/validation/scenarios.md) with
complete fixtures and isolated writable directories.

A pack change is complete when:

- installer discovery returns exactly the canonical six Skills;
- Core, Web, and Agent routes each choose one primary owner;
- every reference is reachable only through the branch that needs it;
- current-source inspection precedes implementation;
- changed scenarios produce artifacts and observable evidence rather than only
  correct vocabulary; and
- user approval and delivery boundaries remain explicit.
