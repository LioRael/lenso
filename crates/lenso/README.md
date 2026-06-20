# Lenso

`lenso` is the public Rust facade for Lenso module-authoring and host boot
contracts.

Install it from crates.io:

```sh
cargo add lenso@0.3.7
```

The default facade exposes serializable module manifest declarations:

- module manifests and manifest lints;
- schema-admin and declarative admin action declarations;
- HTTP route metadata;
- runtime function declarations;
- event handler declarations;
- lifecycle declarations;
- Runtime Console surface declarations;
- story display metadata.

Enable the `host` feature for the narrow host boot facade:

```toml
lenso = { version = "0.3.7", features = ["host"] }
```

Application SQL, repositories, auth/session policy, CRUD shape, and Runtime
Console UI stay in the host application or module code.

## Example

```rust
use lenso::{
    AdminSchema, EntitySchema, FieldSchema, FieldType, ModuleManifest, ModuleSource,
    RuntimeFunctionDeclaration, RuntimeSurface, lint_module_manifest,
};

let manifest = ModuleManifest::builder("example")
    .capabilities(vec!["example.records.read".to_owned()])
    .admin(AdminSchema {
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
    })
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
