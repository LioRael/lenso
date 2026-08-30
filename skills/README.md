# Lenso skills

The canonical pack has two layers:

1. `lenso-start` is the task-oriented entrance. It maps a request through the
   Core, Web, or Agent journey.
2. Five owner Skills perform the work at one stable Lenso seam.

Product journeys may cross several owners, but they do not create another
composition model or duplicate the owner rules.

```text
observable result
      |
      v
  lenso-start
  ├── Core task map
  ├── Web task map
  `── Agent task map
      |
      v
one primary owner
  ├── business planning
  ├── Capability authoring
  ├── Plugin authoring
  ├── App configuration
  `── runtime extension
```

## Start from the task

| Path | Typical request | First owner |
| --- | --- | --- |
| Core | Create a Plugin, evolve a Capability, change an App, or extend execution | The matching owner Skill |
| Web | Add an endpoint, Auth, upstream call, Ingress behavior, test, or deployment boundary | Usually Plugin authoring or App configuration |
| Agent | Change a Profile, Tool, Session, child Agent, MCP server, or Agent surface | Usually Plugin authoring or App configuration |

Invoke `$lenso-start` when the owner or sequence is unclear. Its three task maps
define the observable journey and then route each artifact to one owner.

## Owner Skills

| Skill | Interface |
| --- | --- |
| `lenso-business-planning` | Turn an unclear outcome into vertical Plugin cards, Capability edges, and one tracer slice. |
| `lenso-capability-authoring` | Create or evolve one versioned cross-Plugin role and its generated projections. |
| `lenso-plugin-authoring` | Implement one removable Plugin Contract through one shipped authoring path. |
| `lenso-app-configuration` | Change only one App's visible `plugins/` differences and inspect the derived App. |
| `lenso-runtime-extension` | Extend a Driver, Execution Adapter, Host/Runner, selection policy, or App Generation mechanism. |

Kernel semantics are not a sixth owner Skill. Portable graph, lifecycle,
invocation, admission, readiness, supervision, and diagnostics begin with the
core repository's `CONTEXT.md`, relevant ADR, and product-neutral conformance.

## Directory architecture

Discoverable Skill directories stay flat because installers find
`skills/<name>/SKILL.md`. Information inside each Skill is hierarchical:

```text
skills/
├── lenso-start/
│   ├── SKILL.md                 # choose Core, Web, or Agent
│   └── references/              # task maps loaded one at a time
├── lenso-plugin-authoring/
│   ├── SKILL.md                 # shared owner workflow
│   └── references/
│       ├── paths/               # portable Rust, linked Rust, or Bun
│       ├── contract-and-lifecycle.md
│       └── verification.md
├── lenso-<other-owner>/          # same entrypoint/reference pattern
├── scripts/                      # deterministic pack validation
└── validation/                   # independent behavioral scenarios
```

`SKILL.md` carries the small Interface: purpose, shared ordered work, ownership
guardrails, completion criteria, and branch pointers. A reference carries only
one conditional path. Exact APIs remain in current repository source,
manifests, locks, installed `--help`, and CI rather than being cached at the
entrypoint.

This structure keeps the owner Skills deep: callers learn five stable
Interfaces while path-specific implementation detail stays local and
disclosed only when needed.

## Install

Inspect the catalog:

```sh
npx skills add LioRael/lenso --list
```

Install all six Skills:

```sh
npx skills add LioRael/lenso --all
```

For a user-level Codex installation:

```sh
npx skills add LioRael/lenso --skill '*' --agent codex --global --yes
```

Refresh same-named installed copies with `npx skills update`. Installation does
not remove unrelated legacy names.

## Validate

From the repository root:

```sh
python3 skills/scripts/validate-pack.py
python3 -m unittest skills/scripts/test_validate_pack.py
npx skills add . --list
```

For each changed Skill, also run the active runtime's official validator. Use
`--installed-root` to compare every canonical payload file with an installed
copy:

```sh
python3 skills/scripts/validate-pack.py \
  --installed-root "$HOME/.agents/skills"
```

Structural validation proves packaging and routing invariants, not usefulness.
Run the independent prompts in [behavioral scenarios](validation/scenarios.md)
after a substantial change. A scenario passes only when the Agent selects the
right owner, produces the requested artifacts, and records observable success,
honest failure, and removal or replacement evidence.
