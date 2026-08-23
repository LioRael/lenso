# Lenso agent skills

The canonical skill pack turns Lenso's vNext architecture into repeatable work
for coding agents. It answers two questions before implementation begins:

1. Which seam owns the requested outcome?
2. What artifacts and observable evidence complete that work?

The source lives in [`skills/`](../../skills/). Installed copies are derived
artifacts. Runtime, authoring, protocol, Driver, Adapter, Capability, and Module
implementation ownership remains in the repositories assigned by ADR 0064.

## Choose one primary workflow

| Request | Primary skill | Completion artifact |
| --- | --- | --- |
| Ownership or product boundary is unclear | `lenso-business-planning` | Module cards, Capability edges, and one executable slice |
| Capability identity, Operations, Schemas, compatibility, or generated bindings | `lenso-capability-authoring` | Descriptor, Schemas, generated targets, compatibility and behavior proof |
| Removable Rust, Bun, Web, stateful, or cross-cutting product behavior | `lenso-module-authoring` | Module package, factory/entrypoint, lifecycle, Composition and removal proof |
| Package selection, keyed Instances, configuration, bindings, profiles, or lanes | `lenso-app-composition` | Checked project document and immutable Resolved App Plan |
| Driver, Execution Adapter, Runner, process/wire, or host execution mechanics | `lenso-runtime-extension` | Narrow host implementation plus conformance and real-host evidence |
| The owner is not yet clear | `lenso-start` | One primary workflow and one observable completion state |

Kernel semantics are not a seventh product workflow. Changes to portable graph,
lifecycle, invocation, admission, supervision, readiness, or diagnostics begin
with [`CONTEXT.md`](../../CONTEXT.md) and the relevant ADR.

## Invoke the pack

`lenso-start` is an explicit router. Invoke it when a request plausibly fits
more than one owner:

```text
Use $lenso-start to route a request for a durable background notification flow.
```

The other five skills are discoverable by their descriptions and may also be
named directly:

```text
Use $lenso-module-authoring to implement the generated Ticket provider in Bun.
```

```text
Use $lenso-app-composition to bind two HTTP endpoint providers to Web Ingress.
```

Select one primary workflow. Add a secondary workflow only when the request
crosses a real ownership boundary; for example, change the Capability source
before implementing its provider, or implement the Module before selecting its
Instance in App Composition.

## Install and update

Inspect the public catalog before installing:

```sh
npx skills add LioRael/lenso --list
```

Install all six skills for user-level Codex discovery:

```sh
npx skills add LioRael/lenso \
  --skill '*' \
  --agent codex \
  --global \
  --yes
```

Update same-named installed skills later:

```sh
npx skills update
```

Installation does not remove unrelated legacy skill names automatically. Use
the canonical six-workflow table above for vNext work.

For a local checkout, inspect what the installer will discover without
changing an installed copy:

```sh
npx skills add . --list
```

## How an agent executes a skill

1. Read the selected `SKILL.md` completely and finish its shared workflow in
   order.
2. Follow only the branch references named by the selected Module shape,
   interaction kind, authoring profile, or runtime seam.
3. Resolve exact APIs from the target repository's instructions, selected
   dependency source, manifests, lockfiles, `--help`, and CI. Examples in the
   skill teach the implementation shape; they do not pin a future package API.
4. Hand work to another skill only at an explicit ownership boundary.
5. Return concrete artifact paths, exact checks, behavior evidence, honest
   failures or blockers, and the delivery state.

A skill has not completed merely because the architecture nouns are correct.
The requested artifact and its observable proof must exist. A missing artifact
that the request claims already exists is a fixture or input blocker; the agent
should name it rather than inventing package state.

## Canonical source and installed copies

Edit [`skills/`](../../skills/) in this repository. Do not maintain a second
`SKILL.md` source in `lenso-cli` or an implementation repository. A skill may
link to those owners and inspect their current source without copying their
implementation ownership here.

The pack uses progressive disclosure:

- `SKILL.md` contains the shared ordered workflow and branch pointers.
- `references/` contains branch-specific recipes, examples, and verification.
- `agents/openai.yaml` contains the discovery metadata and invocation policy.
- `scripts/` contains deterministic maintenance checks.
- `validation/` contains independent forward-test scenarios.

Keep exact commands and type names next to the branch that needs them. Keep one
source of truth for each rule, and prefer live source inspection when a command
or API is cheap to discover.

## Validate a skill change

From the repository root, validate pack structure, metadata, local links,
reference reachability, fenced JSON, and router policy:

```sh
python3 skills/scripts/validate-pack.py
```

Confirm installer discovery:

```sh
npx skills add . --list
```

When validating a copied user installation, compare every canonical payload
file, not only `SKILL.md`:

```sh
python3 skills/scripts/validate-pack.py \
  --installed-root "$HOME/.agents/skills"
```

Also run the official skill validator supplied by the active agent runtime for
each of the six skill directories. Its installation path belongs to that
runtime and is intentionally not hard-coded in this public repository.

Structural checks do not prove that the instructions change agent behavior.
After a substantial edit, run the independent prompts in
[`skills/validation/scenarios.md`](../../skills/validation/scenarios.md):

1. give the evaluator the named skill, realistic prompt, required raw fixture,
   and an isolated writable directory;
2. keep the expected observations out of its context;
3. review actual artifacts and executed evidence;
4. record a missing claimed fixture as inconclusive; and
5. patch only failures demonstrated by the run.

## Completion checklist

A skill-pack change is ready when:

- every skill has a discriminating description and reachable branch references;
- examples agree with current authoritative source or explicitly require live
  source inspection;
- all six directories pass structural and official validation;
- local installer discovery returns exactly the canonical six workflows;
- any installed copy matches the complete canonical payload;
- changed workflows pass independent forward testing with adequate fixtures;
- public documentation contains portable commands rather than contributor-local
  absolute paths; and
- the change preserves user approval boundaries and unrelated repository work.
