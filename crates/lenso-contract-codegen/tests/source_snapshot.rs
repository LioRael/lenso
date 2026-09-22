use std::path::Path;

use lenso_contract_authoring::{CapabilitySnapshot, OperationSnapshot};
use lenso_contract_codegen::{
    CodegenError, check_source_snapshot, generate, write_source_snapshot,
};
use serde_json::json;

fn snapshot() -> CapabilitySnapshot {
    CapabilitySnapshot {
        capability_id: "example.derived@1".to_owned(),
        version: "1.0.0".to_owned(),
        portable: true,
        cross_lane_transfer: false,
        operations: vec![OperationSnapshot {
            name: "run".to_owned(),
            interaction: "request".to_owned(),
            request_schema: json!({
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            response_schema: json!({
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            domain_error_schema: json!({
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "oneOf": [{"const": "rejected"}]
            }),
        }],
    }
}

#[test]
fn source_snapshots_are_deterministic_and_drift_checked() {
    let root = tempfile::tempdir().unwrap();
    let descriptor = root.path().join("capability.json");
    write_source_snapshot(&snapshot(), &descriptor).unwrap();
    check_source_snapshot(&snapshot(), &descriptor).unwrap();
    let generated = generate(&descriptor).unwrap();
    assert_eq!(generated.metadata.capability_id, "example.derived@1");
    assert!(generated.rust.contains("pub trait DerivedProvider"));
    assert!(
        generated
            .typescript
            .contains("export interface DerivedProvider")
    );

    std::fs::write(root.path().join("schemas/run-request.schema.json"), "{}\n").unwrap();
    assert!(matches!(
        check_source_snapshot(&snapshot(), &descriptor),
        Err(CodegenError::GeneratedArtifactDrift { .. })
    ));
    assert!(Path::new(&descriptor).exists());
}

#[test]
fn stream_source_snapshots_use_open_and_message_artifacts() {
    let root = tempfile::tempdir().unwrap();
    let descriptor = root.path().join("capability.json");
    let mut snapshot = snapshot();
    snapshot.operations[0].interaction = "stream".to_owned();

    write_source_snapshot(&snapshot, &descriptor).unwrap();
    assert!(root.path().join("schemas/run-open.schema.json").is_file());
    assert!(
        root.path()
            .join("schemas/run-message.schema.json")
            .is_file()
    );
    assert!(!root.path().join("schemas/run-request.schema.json").exists());
    let generated = generate(&descriptor).unwrap();
    assert!(generated.rust.contains("NativeStreamSession"));
    assert!(generated.typescript.contains("StreamSession"));
}

#[test]
fn schema_titles_preserve_authored_type_names_across_projections() {
    let root = tempfile::tempdir().unwrap();
    let descriptor = root.path().join("capability.json");
    let mut snapshot = snapshot();
    snapshot.operations[0].request_schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "CatalogRequest",
        "type": "object",
        "required": ["selected", "tools"],
        "properties": {
            "selected": {
                "anyOf": [
                    { "$ref": "#/$defs/ToolDefinition" },
                    { "type": "null" }
                ]
            },
            "tools": {
                "type": "array",
                "items": { "$ref": "#/$defs/ToolDefinition" }
            }
        },
        "additionalProperties": false,
        "$defs": {
            "ToolDefinition": {
                "type": "object",
                "required": ["name"],
                "properties": { "name": { "type": "string" } },
                "additionalProperties": false
            }
        }
    });

    write_source_snapshot(&snapshot, &descriptor).unwrap();
    let generated = generate(&descriptor).unwrap();
    assert!(generated.rust.contains("pub struct CatalogRequest"));
    assert!(generated.rust.contains("pub tools: Vec<ToolDefinition>"));
    assert!(
        generated
            .rust
            .contains("pub selected: Option<ToolDefinition>")
    );
    assert!(generated.rust.contains("pub struct ToolDefinition"));
    assert!(
        generated
            .typescript
            .contains("export interface ToolDefinition")
    );
}
