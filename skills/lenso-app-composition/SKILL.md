---
name: lenso-app-composition
description: Edit a source-derived Lenso App Definition to select locked Module packages, keyed Instances, configuration, lanes, and real binding decisions, then verify and materialize the immutable Resolved App Plan. Use for App choices rather than Module behavior.
---

# Lenso App Composition

Select the exact Modules that make one App without hand-authoring the derived
Composition. The App Definition owns intent; package managers own acquisition
and locks; generated Descriptors close ordinary bindings; Kernel receives only
canonical Resolved Plan bytes.

## Workflow

1. **Load the authoring authority.** Find repository instructions,
   `lenso.app.json`, package manifests and locks, generated Module Descriptors,
   optional configuration Schemas, the last approved Plan, and the installed
   `lenso --help`. Read the
   [project document recipe](references/project-document.md). Finish when every
   path is classified as authored, generated, package-manager-owned, or
   canonical output.
2. **Select locked packages.** Use `lenso app add` for supported Cargo Modules
   and the owning package manager for other inputs. Preview each edit. Finish
   when the package manifest, lock, package-owned Descriptor, and App Definition
   identify the same immutable release.
3. **Declare keyed Instances.** Give every Instance a stable App-local key,
   package, non-secret configuration or secret references, and optional lane.
   Do not restate provided Capabilities, Ports, Operations, execution classes,
   or lifecycle policy already derived from Module source. Finish when the App
   Definition contains intent only.
4. **Decide only real ambiguity.** Follow
   [bindings and resolution](references/bindings-and-resolution.md). Let the
   resolver close unambiguous requirements from generated Descriptors. Record
   an explicit provider decision only for a real `one` or `optional` ambiguity;
   preserve deterministic order for `many`. Finish when every remaining choice
   belongs to the App owner rather than the tool.
5. **Keep generated contracts generated.** Review package-owned Descriptors,
   Schemas, projections, and derived binding changes as locked artifacts. Do
   not copy their fields into the App Definition. Finish when regeneration is
   deterministic and every selected artifact agrees with its package lock.
6. **Apply optional profiles or placement.** Read
   [Web profiles and lanes](references/web-profiles-and-lanes.md) only for a
   target Web UI or replicated-lane App. Expand every recipe into ordinary
   Module Instances and bindings in the Plan. Finish when the profile/lane adds
   no hidden runtime graph.
7. **Check and resolve.** Run `lenso app check`, then use `lenso app resolve`
   only when a Host or review step needs canonical Plan bytes. Review the App
   Definition diff and generated Plan diff. Finish when
   package locks, execution classes, entrypoints, configuration Schemas,
   sensitive references, contract freshness, endpoints, bindings, and lane
   placement all pass before boot.
8. **Run through the owning Host and prove removal.** Give the reviewed Plan to
   the product-owned Runner or Host; the authoring CLI does not expose a generic
   `run --plan` command. Change intent and resolve again before restart. Use
   `lenso app remove` in a focused fixture and resolve the remainder. Finish
   when no running Kernel mutation or hidden install state is needed.

Return the project and Plan paths, packages selected, keyed Instances,
bindings and cardinalities, execution classes and placement, validation
results, package/contract freshness evidence, reviewed diffs, run evidence,
and whether removing each optional Module leaves a valid Composition.
