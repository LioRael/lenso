# Lenso

`lenso` is the public Rust facade for Lenso module-authoring contracts.

This first facade slice exposes serializable module manifest declarations:

- module manifests and manifest lints;
- schema-admin and declarative admin action declarations;
- HTTP route metadata;
- runtime function declarations;
- event handler declarations;
- lifecycle declarations;
- Runtime Console surface declarations;
- story display metadata.

Host application internals, storage, HTTP server wiring, worker execution, and
linked-module behavior bindings remain internal to the Lenso backend workspace
until their public API is intentionally designed.

## Example

```rust
use lenso::{
    AdminSchema, AdminSurface, EntitySchema, FieldSchema, FieldType, ModuleManifest,
    ModuleSource, RuntimeFunctionDeclaration, RuntimeSurface, lint_module_manifest,
};

let manifest = ModuleManifest::builder("example")
    .capabilities(vec!["example.records.read".to_owned()])
    .admin(AdminSurface::Schema(AdminSchema {
        entities: vec![EntitySchema {
            name: "records".to_owned(),
            label: "Records".to_owned(),
            fields: vec![FieldSchema {
                name: "id".to_owned(),
                label: "ID".to_owned(),
                field_type: FieldType::String,
                nullable: false,
            }],
            read_capability: "example.records.read".to_owned(),
        }],
    }))
    .runtime(RuntimeSurface {
        functions: vec![RuntimeFunctionDeclaration {
            name: "example.refresh.v1".to_owned(),
            version: 1,
            queue: "example".to_owned(),
            input_schema: Some("example.refresh.v1".to_owned()),
            retry_policy: None,
        }],
    })
    .build();

let lints = lint_module_manifest(ModuleSource::Remote, &manifest);
assert!(
    lints
        .iter()
        .all(|lint| !matches!(lint.severity, lenso::ModuleManifestLintSeverity::Error))
);
```
