---
name: lenso-app-configuration
description: Configure a Lenso App through its visible plugins/ Plugin Root, inspect the Host-derived App, and prove changes without authoring a graph, bindings, or Plan. Use for adding, configuring, disabling, enabling, or removing Plugins; route Plugin implementation and Host mechanics elsewhere.
---

# Lenso App Configuration

Express only how this App differs from its Host defaults. The Host Catalog owns
available linked Plugins, default Instances, root Slots, and private attachments.
The App owner owns one strict `plugins/` directory. Resolution derives all
bindings and the immutable Plan from those two inputs.

## Workflow

1. **Locate both authorities.** Read repository instructions, the Host Catalog
   construction, `plugins/`, package manifests and locks, Plugin schemas, and
   `lenso --help`. Read [the Plugin Root shape](references/plugin-root.md).
   Finish when each fact belongs to Host, Plugin package, or App owner.
2. **Inspect the default App.** Run `lenso app check` and `lenso app show`
   before editing. A missing or empty Plugin Root must resolve to the exact Host
   defaults. Do not create an App Definition or enabled-list file.
3. **Change one Plugin entry.** Use `lenso plugins add` for an external package,
   `configure` for an Instance TOML, and `disable|enable|remove` for lifecycle
   changes. Keep one Plugin directory and one TOML file per Instance. Never
   hand-author endpoints, requirements, bindings, execution classes, or Slots.
4. **Keep configuration typed and non-secret.** Edit only fields declared by
   the Plugin schema. Package defaults and Host configuration merge before the
   Instance patch. Secret values remain environment-backed.
5. **Escalate real ambiguity to the Host.** If a requirement cannot be derived
   deterministically from selected Instances, route the root Slot or private
   attachment decision to the product Host; do not add a binding sidecar to
   `plugins/`. Read
   [resolution and generations](references/resolution.md).
6. **Check and observe.** Run `lenso app check`, use `lenso app show` to review
   selection and bindings, and use `lenso app resolve` only when exact Plan
   bytes are needed for diagnostics or replay. A Plan is output, never source.
7. **Prove behavior and removal.** Exercise the smallest real consumer path,
   then disable or remove the Plugin and resolve again. For a live Host, prove
   the candidate passes readiness before routing switches and that existing
   Turns retain their Generation lease.

Return the Host and Plugin Root paths, changed Plugin/Instance, configuration
ownership, exact commands, derived App evidence, behavior proof, removal proof,
and delivery state.
