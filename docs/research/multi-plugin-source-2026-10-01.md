# Independent source Plugins in one Cargo package

Implementation worktree: `feat/multi-plugin-source`, based on
`119b9af70b82816588c00c8bf812f7c62d1a18b5`. No landing or publication yet.

`#[lenso::plugin(id = "example.health", root_slot = "web")]` declares a
module-owned identity. Cargo supplies its release version. Omitted identity
attributes retain existing single-Plugin package metadata behavior. Each
module owns generated descriptors, factories and linkage; structs may share
the name `Plugin` across modules. Both attributes are required together.

Default discovery walks the public, unconditional library module graph,
including inline modules and contained `#[path]` modules. It excludes
unreachable files, private/conditional modules, binaries, tests and examples.
It does not execute build scripts or Cargo. Duplicate identities fail with
source evidence. One declaration per module is supported; conditional Plugins
use a custom Host.

Discovery records module-specific linker anchors. Assembly deduplicates Cargo
dependencies while linking the selected modules. Source candidates require
explicit Plugin Root selection. Siblings remain available in inventory but
do not become active Instances just because their crate was selected. Custom
Host selection/configuration uses the existing `plugin`, `plugin_with` and
`configured_plugin` APIs.

Local contracts can use the module-aware Rust projection CLI/API to scope
authoring macros and type paths without changing contract identities or wire
schemas. Multiple raw Provider traits can share one `#[provides]` annotation.
These close concrete relay authoring gaps; they do not add registry fetching
or automatic external contract generation.

## Validation

- Native adapter full existing suite passed, including package metadata fallback.
- New native regression: two independent identities, explicit Instance selection,
  idempotent linker registration, duplicate factory rejection. Named dependency
  binding fixture passes with source identity.
- Authoring library: 149 passed, 3 ignored; focused discovery: 34 passed.
- Engine App focused local Host suite: 52 passed.
- Codegen: CLI 8, codegen 44, integration 2 + 4, doctest 1 passed; macro unit
  suite 14 passed. Module projection drift/path regression rerun passed.
- Affected framework packages and example passed Clippy with warnings denied.
- Real default CLI built and checked Health-only and explicitly configured
  Health+Greeting Apps. HTTP: Health 200, excluded Greeting 404, configured
  Greeting 200 with `Configured greeting`; clean shutdown.

Runtime evidence was captured at `/tmp/lenso-multi-http-evidence.json`.
Runnable source and configuration are in `examples/multiple-plugins`.
Relay has a separate consumer migration and acceptance record. Final combined
candidate review, CI, exact-SHA landing and publication remain separate steps
owned by the coordinating delivery task.
