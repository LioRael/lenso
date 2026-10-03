# Exact support before compilation

`lenso app check --target native|workers` uses existing generated Host authority
and the existing Plugin Root resolver when contracts are available. It checks the selected Instances,
Capability closure, execution profile, complete named-resource combination,
source entry and selected tools without running a compiler, package hook,
factory, secret provider or database query.

```sh
lenso app check --root ./app --target native --from ./previous-dist \
  --host-facilities ./native-references.json --json
lenso app check --root ./app --target workers --from ./previous-dist \
  --host-facilities ./workers-references.json --json
```

`--from` reuses an existing generated Host's contracts and source provenance;
it does not select a new implementation. Source identity must still match.
Root configuration and named dependency choices are resolved afresh through
the same resolver. A built/initialized App with its own Host authority omits
`--from`. `app check` without `--target` retains its existing behavior.

For a first source project, omit `--from`. With no Host authority, the same
command performs static discovery, validates authored Instance selection,
resource references and exact support candidates, checks entry files and local
tool/dependency inputs, and returns `static_passed_contract_pending`. It never
imports a TS module, fabricates a descriptor, or invokes Cargo/Bun. The report
lists deferred execution selection, Capability closure, semantic exports,
resource factories, owner schema and whole-Plan restrictions. Exit success
means these static checks passed, not that a complete target Plan was admitted.
Stale explicit generated authority still fails instead of invoking extraction.
Local source does not require a Bundle, packing, freezing or installation.

The build gate accepts this pending result so a first project can reach the
existing owning-contract pipeline. Rust/TS descriptor export and the existing
semantic resolver remain responsible for the deferred checks. Static preflight
does not establish registry dependency availability or live resource readiness.

Assembly has two crate-local entry points in `app::target_check`:

```rust,ignore
inspect_sources(source_root, target, facilities)?; // static, no execution
inspect_resolved(source_root, authority_root, target, &resolved_app, facilities)?;
```

The second receives the existing `ResolvedApp` after the owning Rust/TS
descriptor export and shared semantic resolution, plus the stage that owns the
selected facility inventory. It uses the same complete tuple/Capability and
Workers admission code as generated-Host checking. There is no second source
descriptor format; the existing TS `describePortablePlugin` export remains
owned by the TS/assembly path. The bridge does not invoke that export itself.

## Declare whole combinations

Cargo sources use the existing Engine-owned `package.metadata.lenso-cli`
metadata. The portable SDK's strict `package.metadata.lenso` schema is unchanged.

```toml
[package.metadata.lenso-cli.support]
# These are references, not proof checked by this command.
evidence = ["owner-maintained-exact-revision-receipt"]

[[package.metadata.lenso-cli.support.combinations]]
environment = "native"
execution = "lenso.native-rust@1"
resources = { db = "postgresql" }

[[package.metadata.lenso-cli.support.combinations]]
environment = "workers"
execution = "lenso.native-rust@1"
resources = { db = "d1" }
```

These declarations permit Native/PostgreSQL and Workers/D1. They permit
neither Native/D1 nor Workers/PostgreSQL. Matching includes every resource
name and implementation: missing, renamed or extra resources do not match.
Separate `environments`/`storage` lists are rejected rather than expanded.

Several independently identified source Plugins may share one Cargo package.
Put an override under
`package.metadata.lenso-cli.plugins."example.plugin".support`; otherwise
package-wide support is the default. Ordinary internal Rust modules remain
internal modules. Bun sources use the same `support` object under their existing
`package.json` `lenso` metadata. No new Plugin/Instance/Capability/Plan type is
introduced.

Without a declaration, a Plugin has a simple Native combination for its
selected execution and no named resources. It is not forced to author a whole
target matrix. The built-in linked HTTP Ingress uses the selected Host target.
An explicit Workers declaration is necessary for application Plugins.

## Bind references per Instance

Use the existing `lenso.host-facilities.v1` profile and source-owned facility
inventory. The owner configuration identifies the resource implementation.
For Native resources, `reference` is an opaque reference name, and `binding`
is null. For Workers, `binding` is an exact Workers binding name.

```json
{
  "schema": "lenso.host-facilities.v1",
  "instances": {
    "example.store/default": {
      "db": {
        "configuration": {
          "implementation": "postgresql",
          "reference": "DATABASE_URL"
        },
        "binding": null
      }
    }
  }
}
```

For Workers/D1, use `"implementation": "d1"` and `"binding": "DB"`.
Every grant must name a selected Instance and an owner-declared slot with a
factory for the selected target. Configurations, bindings and resources are
checked independently for each Instance of one implementation. The check
never dereferences `DATABASE_URL` or reads a secret value. Owner schema
validation and actual resource construction remain at the existing Ready Gate.

The Workers target here is the existing static linked-Rust Workers profile.
Its existing admission check still requires HTTP Ingress, one main lane,
Request/Stream semantics and no restart supervision or WebSocket Endpoint.
It does not silently admit Component/Bun/Process execution in Workers.

## Build gate and evidence

The local App build calls this gate before compilation when support declarations,
`--check-from` or `--host-facilities` are selected. Workers build uses its
existing `--workers-facilities` references. Apps without those inputs retain
their existing simple build path. `--source HOST.ts` remains its separate,
existing explicit Host-authoring path.

```sh
lenso app build --root ./app --check-from ./previous-dist \
  --host-facilities ./native-references.json
```

With resolved contracts this is offline declaration/identity admission. The JSON reports
`support: "declared"`, `qualification: "not_assessed"`,
`resource_readiness: "not_run"`, `build_started: false` and
`secret_values_read: false`. Evidence references do not become verified
receipts. Passing the check establishes neither database readiness nor a
Native/Workers deployment qualification.

Source-first JSON also reports `contract_resolution: "pending"`,
`requires_follow_up: true`, and explicit `verified`/`deferred` lists. Resolved
checking reports `contract_resolution: "resolved"` and
`requires_follow_up: false`; qualification remains `not_assessed`.

The focused CLI corpus uses syntactically valid source containing
`compile_error!` plus poisoned Cargo/rustc/Bun/wasm-bindgen/Jco executables.
Native/PostgreSQL and Workers/D1 pass without starting them; cross combinations,
missing bindings and missing entries fail without creating build output.
