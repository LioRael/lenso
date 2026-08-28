---
name: lenso-app-configuration
description: Change one Lenso App through its visible plugins/ Plugin Root. Use for adding, configuring, disabling, enabling, or removing Plugin Instances and inspecting the Host-derived App; the App owner never authors bindings, implementation selection, or a Plan.
---

# Lenso App Configuration

Express only how this App differs from its Host defaults. The Host Catalog owns
available linked Plugins, default Instances, root Slots, and private attachments.
The App owner owns one strict `plugins/` directory. Resolution derives all
bindings and the immutable Plan from those two inputs.

## Workflow

1. **Locate both authorities.** Read repository instructions, the generated
   `.lenso/host-catalog.json`, `plugins/`, package manifests and locks, Plugin
   schemas, and installed `lenso plugins --help` plus `lenso app --help`. Read
   [the Plugin Root shape](references/plugin-root.md).
   Finish when each fact belongs to Host, Plugin package, or App owner.
2. **Inspect the default App.** Run `lenso app check` and `lenso app show`
   before editing. A missing or empty Plugin Root must resolve to the exact Host
   defaults. Do not create an App Definition or enabled-list file.
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
7. **Prove behavior and removal.** Exercise the smallest real consumer path,
   then disable or remove the Plugin and check the derived App again. For a
   live Host, prove the candidate passes readiness before routing switches and
   that existing Turns retain their Generation lease. Finish when both the
   selected state and the removed state have checked, observable evidence.

Return the Host and Plugin Root paths, changed Plugin/Instance, configuration
ownership, exact commands, derived App evidence, behavior proof, removal proof,
and delivery state.
