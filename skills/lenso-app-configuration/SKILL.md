---
name: lenso-app-configuration
description: Create or change a Lenso source App, local Plugin discovery, optional support or precompiled development Host; configure and inspect an existing App through its plugins/ Plugin Root. The App owner never authors bindings, implementation selection, or a Plan.
---

# Lenso App Configuration

Express only how this App differs from its Host defaults. The Host Catalog owns
available linked Plugins, default Instances, root Slots, and private attachments.
The App owner owns one strict `plugins/` directory. Resolution derives all
bindings and the immutable Plan from those two inputs.

## Workflow

For a source App, local discovery, optional support or a precompiled development
Host, first follow [local development](references/local-development.md).
That branch builds the Host authority before inspecting its distribution.
Use the workflow below for an existing Host-derived Plugin Root.

1. **Locate both authorities.** Read repository instructions, the generated
   `.lenso/host-catalog.json`, `plugins/`, package manifests and locks, Plugin
   schemas, and installed `lenso plugins --help` plus `lenso app --help`. Read
   [the Plugin Root shape](references/plugin-root.md).
   Finish when each fact belongs to Host, Plugin package, or App owner.
2. **Inspect the relevant App state.** Use the existing check/show evidence or
   run `lenso app check` and `lenso app show` when that state is unknown or stale.
   A missing or empty Plugin Root must resolve to the exact Host
   defaults. The Plugin Root remains the only App-owner composition surface.
3. **Change one Plugin entry.** Use `lenso plugins add <bundle>` for an external
   package, `configure <plugin-id> <instance> --file <toml>` for an Instance,
   and `disable|enable|remove` for selection changes. Keep one Plugin directory
   and one TOML file per Instance. Optional structured files live only under
   `plugins/<plugin-id>/<instance>/`. Finish when `git diff` contains only the
   intended Plugin Root difference.
4. **Keep configuration typed and non-secret.** Edit only fields declared by
   the Plugin schema. Package defaults and Host configuration merge before the
   Instance patch. Secret values remain environment-backed.
5. **Escalate derivation gaps to the Host.** If a requirement or executable
   implementation cannot be selected deterministically, route root Slot,
   private attachment, or implementation-policy work to the product Host. Read
   [resolution and generations](references/resolution.md).
6. **Check and observe.** Run `lenso app check` and use `lenso app show` to
   review selection, bindings, and provenance. Exact Plan bytes are a Host
   diagnostic/replay seam, not an App-owner file workflow.
7. **Prove affected behavior.** Exercise the smallest real consumer path.
   Disable/remove and check again when selection/removal behavior changes or
   is claimed, rather than after every configuration edit. For a live
   Generation switch, prove readiness before routing changes and preserve
   existing leases. Money, Auth or migration changes retain explicit authority,
   allow/deny and affected concurrency/idempotency/recovery proof against real
   storage. Final repository review and candidate gates remain delivery checks.

Return the changed Plugin/Instance and paths, actual commands and affected
behavior, remaining blockers and delivery state. No separate handoff, frozen
release or full report is required for a local edit.
