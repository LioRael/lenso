use anyhow::Context as _;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

pub fn generate_contracts() -> anyhow::Result<()> {
    write_yaml(
        "contracts/openapi/app-api.v1.yaml",
        &lenso_api::openapi_document(),
    )?;
    write_yaml(
        "contracts/openapi/autonomous-service-runtime.v1.yaml",
        &generated_autonomous_service_runtime_openapi(),
    )?;
    write_json(
        "contracts/errors/error-response.v1.schema.json",
        &error_response_schema(),
    )?;
    write_json(
        "contracts/services/lenso-service.v2.schema.json",
        &generated_autonomous_service_schema(),
    )?;
    write_json(
        "contracts/services/support-http.v1.bindings.json",
        &generated_direct_http_bindings(),
    )?;
    write_text(
        "contracts/services/support-grpc.v1.proto",
        lenso_service::DIRECT_GRPC_PROTO_V1_FIXTURE,
    )?;
    write_json(
        "contracts/services/support-grpc.v1.bindings.json",
        &generated_direct_grpc_bindings(),
    )?;
    write_json(
        "contracts/services/lenso-system.v2.schema.json",
        &generated_system_v2_schema(),
    )?;
    write_json(
        "contracts/services/lenso-system.v2.fixture.json",
        &generated_system_v2_fixture(),
    )?;
    write_json(
        "contracts/context/lenso-context.v1.schema.json",
        &generated_common_context_schema(),
    )?;
    write_json(
        "contracts/context/lenso-context.v1.fixture.json",
        &generated_common_context_fixture(),
    )?;
    write_text(
        "docs/architecture/common-context-contracts.md",
        generated_common_context_glossary(),
    )?;
    write_text(
        "docs/architecture/contract-compatibility.md",
        generated_contract_compatibility(),
    )?;
    write_json(
        "contracts/compatibility/contract-compatibility.v1.json",
        &generated_contract_compatibility_matrix(),
    )?;

    Ok(())
}

pub fn generated_autonomous_service_runtime_openapi() -> utoipa::openapi::OpenApi {
    lenso_autonomous_service::openapi_document()
}

pub fn generated_error_response_schema() -> Value {
    error_response_schema()
}

pub fn generated_autonomous_service_schema() -> Value {
    serde_json::from_str(lenso_service::SERVICE_V2_CONTRACT_SCHEMA_JSON)
        .expect("packaged Autonomous Service schema must be valid JSON")
}

pub fn generated_direct_http_bindings() -> Value {
    let openapi: Value = serde_yaml::from_str(lenso_service::DIRECT_HTTP_OPENAPI_V1_FIXTURE_YAML)
        .expect("packaged direct HTTP OpenAPI fixture must be valid YAML");
    serde_json::to_value(
        lenso_service::generate_direct_http_bindings("support-http", "v1", &openapi)
            .expect("packaged direct HTTP OpenAPI fixture must generate bindings"),
    )
    .expect("direct HTTP bindings must serialize")
}

pub fn generated_direct_grpc_bindings() -> Value {
    serde_json::to_value(
        lenso_service::generate_direct_grpc_bindings(
            "support-grpc",
            "v1",
            lenso_service::DIRECT_GRPC_PROTO_V1_FIXTURE,
            lenso_service::DIRECT_GRPC_DESCRIPTOR_V1,
        )
        .expect("packaged direct gRPC Protobuf fixture must generate bindings"),
    )
    .expect("direct gRPC bindings must serialize")
}

pub fn generated_direct_grpc_proto() -> &'static str {
    lenso_service::DIRECT_GRPC_PROTO_V1_FIXTURE
}

pub fn generated_system_v2_schema() -> Value {
    serde_json::from_str(lenso_service::SYSTEM_V2_CONTRACT_SCHEMA_JSON)
        .expect("packaged System v2 schema must be valid JSON")
}

pub fn generated_system_v2_fixture() -> Value {
    serde_json::from_str(lenso_service::MIXED_SYSTEM_V2_FIXTURE_JSON)
        .expect("packaged mixed System v2 fixture must be valid JSON")
}

pub fn generated_common_context_schema() -> Value {
    serde_json::from_str(lenso_service::COMMON_CONTEXT_V1_SCHEMA_JSON)
        .expect("packaged common context schema must be valid JSON")
}

pub fn generated_common_context_fixture() -> Value {
    serde_json::from_str(lenso_service::COMMON_CONTEXT_V1_FIXTURE_JSON)
        .expect("packaged common context fixture must be valid JSON")
}

pub fn generated_common_context_glossary() -> &'static str {
    lenso_service::COMMON_CONTEXT_GLOSSARY_MARKDOWN
}

pub fn generated_contract_compatibility() -> &'static str {
    lenso_service::CONTRACT_COMPATIBILITY_MARKDOWN
}

pub fn generated_contract_compatibility_matrix() -> Value {
    let mut rows = Vec::new();
    for (kind, fixtures, evaluate) in [
        (
            "event_contract",
            lenso_service::EVENT_COMPATIBILITY_FIXTURES,
            lenso_service::evaluate_event_compatibility
                as fn(&Value) -> lenso_service::ContractCompatibilityResult,
        ),
        (
            "config_contract",
            lenso_service::CONFIG_COMPATIBILITY_FIXTURES,
            lenso_service::evaluate_config_compatibility
                as fn(&Value) -> lenso_service::ContractCompatibilityResult,
        ),
        (
            "reliability_contract",
            lenso_service::RELIABILITY_COMPATIBILITY_FIXTURES,
            lenso_service::evaluate_reliability_compatibility
                as fn(&Value) -> lenso_service::ContractCompatibilityResult,
        ),
    ] {
        for fixture in fixtures {
            let input: Value = serde_json::from_str(fixture.json)
                .expect("compatibility fixture must be valid JSON");
            rows.push(
                json!({"contractKind": kind, "name": fixture.name, "result": evaluate(&input)}),
            );
        }
    }
    Value::Array(rows)
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

fn write_text(path: impl AsRef<Path>, value: &str) -> anyhow::Result<()> {
    let path = path.as_ref();
    ensure_parent(path)?;
    fs::write(path, value).with_context(|| format!("failed to write {}", path.display()))
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
