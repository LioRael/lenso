use anyhow::Context as _;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

pub fn generate_contracts() -> anyhow::Result<()> {
    write_yaml(
        "contracts/openapi/app-api.v1.yaml",
        &lenso_api::openapi_document(),
    )?;
    write_json(
        "contracts/errors/error-response.v1.schema.json",
        &error_response_schema(),
    )?;

    Ok(())
}

pub fn generated_error_response_schema() -> Value {
    error_response_schema()
}

fn write_yaml(path: impl AsRef<Path>, value: &impl serde::Serialize) -> anyhow::Result<()> {
    let path = path.as_ref();
    ensure_parent(path)?;
    let rendered = serde_yaml::to_string(value).context("contract should serialize as yaml")?;
    fs::write(path, rendered).with_context(|| format!("failed to write {}", path.display()))
}

fn write_json(path: impl AsRef<Path>, value: &Value) -> anyhow::Result<()> {
    let path = path.as_ref();
    ensure_parent(path)?;
    let rendered = format!(
        "{}\n",
        serde_json::to_string_pretty(value).context("contract should serialize as json")?
    );
    fs::write(path, rendered).with_context(|| format!("failed to write {}", path.display()))
}

fn ensure_parent(path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    Ok(())
}

fn error_response_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://contracts.lenso.local/errors/error-response.v1.schema.json",
        "title": "ErrorResponse",
        "type": "object",
        "required": [
            "type",
            "title",
            "status",
            "detail",
            "code",
            "request_id",
            "correlation_id",
            "errors"
        ],
        "properties": {
            "type": {
                "type": "string",
                "format": "uri-reference"
            },
            "title": { "type": "string" },
            "status": {
                "type": "integer",
                "minimum": 100,
                "maximum": 599
            },
            "detail": { "type": "string" },
            "code": { "type": "string" },
            "request_id": { "type": ["string", "null"] },
            "correlation_id": { "type": ["string", "null"] },
            "errors": {
                "type": "array",
                "items": { "$ref": "#/$defs/ProblemErrorDetail" }
            }
        },
        "$defs": {
            "ProblemErrorDetail": {
                "type": "object",
                "required": ["field", "reason"],
                "properties": {
                    "field": { "type": ["string", "null"] },
                    "reason": { "type": "string" }
                },
                "additionalProperties": false
            }
        },
        "additionalProperties": false
    })
}
