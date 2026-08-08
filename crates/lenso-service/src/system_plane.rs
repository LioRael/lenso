use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashSet};
use utoipa::ToSchema;

pub mod enrollment;
pub mod module_operations;
pub mod runtime_observability;
pub mod runtime_operations;

pub use enrollment::*;
pub use module_operations::*;
pub use runtime_observability::*;
pub use runtime_operations::*;

pub const CORE_PROTOCOL: &str = "lenso.system-plane.v1";
pub const CORE_PATH: &str = "/system-plane/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoreDocument {
    pub protocol: String,
    #[schema(min_length = 1)]
    pub service_id: String,
    #[schema(min_length = 1)]
    pub service_principal: String,
    #[schema(min_length = 1)]
    pub service_revision: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<CapabilityAdvertisement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityAdvertisement {
    #[schema(pattern = r"^lenso\.system-plane\.[a-z0-9]+(?:[.-][a-z0-9]+)*\.v[1-9][0-9]*$")]
    pub contract_id: String,
    #[schema(minimum = 1)]
    pub major_version: u32,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub feature_ids: BTreeSet<String>,
    #[schema(pattern = r"^sha256:[0-9a-f]{64}$")]
    pub schema_digest: String,
    #[schema(pattern = r"^/system-plane/v1/[a-z0-9]+(?:[/-][a-z0-9]+)*$")]
    pub endpoint: String,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CoreIssueCode {
    InvalidProtocol,
    MissingServiceIdentity,
    MissingServicePrincipal,
    MissingServiceRevision,
    InvalidCapabilityContractId,
    InvalidCapabilityMajorVersion,
    CapabilityMajorVersionMismatch,
    InvalidFeatureId,
    InvalidSchemaDigest,
    InvalidEndpointReference,
    DuplicateCapability,
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoreIssue {
    pub code: CoreIssueCode,
    pub path: String,
    pub message: String,
    pub next_action: String,
}

#[must_use]
pub fn validate_core_document(document: &CoreDocument) -> Vec<CoreIssue> {
    let mut issues = Vec::new();

    if document.protocol != CORE_PROTOCOL {
        push_issue(
            &mut issues,
            CoreIssueCode::InvalidProtocol,
            "$.protocol",
            format!("protocol must be `{CORE_PROTOCOL}`"),
            "Use the supported System Plane Core Protocol identifier.",
        );
    }
    validate_present(
        &document.service_id,
        CoreIssueCode::MissingServiceIdentity,
        "$.serviceId",
        "serviceId must identify the managed Service",
        "Publish the stable logical Service identity.",
        &mut issues,
    );
    validate_present(
        &document.service_principal,
        CoreIssueCode::MissingServicePrincipal,
        "$.servicePrincipal",
        "servicePrincipal must identify the managed Service authority",
        "Publish the stable Service Principal independently from endpoint and Workload identity.",
        &mut issues,
    );
    validate_present(
        &document.service_revision,
        CoreIssueCode::MissingServiceRevision,
        "$.serviceRevision",
        "serviceRevision must identify the advertised Service state",
        "Publish a stable revision that changes when the Core advertisement changes.",
        &mut issues,
    );

    let mut advertised_contracts = HashSet::new();
    for (index, capability) in document.capabilities.iter().enumerate() {
        let base = format!("$.capabilities[{index}]");
        let contract_major = capability_major_version(&capability.contract_id);

        if contract_major.is_none() {
            push_issue(
                &mut issues,
                CoreIssueCode::InvalidCapabilityContractId,
                format!("{base}.contractId"),
                "contractId must match `lenso.system-plane.<capability>.v<major>`",
                "Publish a stable capability-specific System Plane Contract identifier.",
            );
        }
        if capability.major_version == 0 {
            push_issue(
                &mut issues,
                CoreIssueCode::InvalidCapabilityMajorVersion,
                format!("{base}.majorVersion"),
                "majorVersion must be greater than zero",
                "Publish the supported major version for this Capability Contract.",
            );
        } else if contract_major.is_some_and(|major| major != capability.major_version) {
            push_issue(
                &mut issues,
                CoreIssueCode::CapabilityMajorVersionMismatch,
                format!("{base}.majorVersion"),
                "majorVersion must match the version suffix in contractId",
                "Make the advertised major version and Contract identifier agree.",
            );
        }

        for feature_id in &capability.feature_ids {
            if !valid_dotted_id(feature_id) {
                push_issue(
                    &mut issues,
                    CoreIssueCode::InvalidFeatureId,
                    format!("{base}.featureIds"),
                    format!("feature identifier `{feature_id}` is not canonical"),
                    "Use lowercase dot-separated identifiers with alphanumeric or hyphenated segments.",
                );
            }
        }
        if !valid_sha256_digest(&capability.schema_digest) {
            push_issue(
                &mut issues,
                CoreIssueCode::InvalidSchemaDigest,
                format!("{base}.schemaDigest"),
                "schemaDigest must be a lowercase `sha256:<64 hex>` digest",
                "Publish the digest of the exact Capability Contract schema.",
            );
        }
        if !valid_capability_endpoint(&capability.endpoint) {
            push_issue(
                &mut issues,
                CoreIssueCode::InvalidEndpointReference,
                format!("{base}.endpoint"),
                "endpoint must be a relative subpath below `/system-plane/v1/`",
                "Publish a capability endpoint inside the managed Service System Plane namespace.",
            );
        }
        if !advertised_contracts.insert(capability.contract_id.as_str()) {
            push_issue(
                &mut issues,
                CoreIssueCode::DuplicateCapability,
                format!("{base}.contractId"),
                format!(
                    "Capability Contract `{}` is advertised more than once",
                    capability.contract_id
                ),
                "Publish each exact Capability Contract identity once.",
            );
        }
    }

    issues
}

#[must_use]
pub fn core_document_schema() -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(CoreDocument))
        .expect("System Plane Core schema serializes");
    schema["$id"] = Value::String(
        "https://contracts.lenso.local/system-plane/lenso.system-plane.v1.schema.json".to_owned(),
    );
    schema["title"] = Value::String("Lenso System Plane Core Document".to_owned());
    schema["properties"]["protocol"] = json!({ "const": CORE_PROTOCOL });
    for field in ["serviceId", "servicePrincipal", "serviceRevision"] {
        schema["properties"][field]["minLength"] = json!(1);
    }
    schema["$defs"]["CapabilityAdvertisement"]["properties"]["contractId"]["pattern"] =
        json!(r"^lenso\.system-plane\.[a-z0-9]+(?:[.-][a-z0-9]+)*\.v[1-9][0-9]*$");
    schema["$defs"]["CapabilityAdvertisement"]["properties"]["majorVersion"]["minimum"] = json!(1);
    schema["$defs"]["CapabilityAdvertisement"]["properties"]["featureIds"]["items"] = json!({
        "type": "string",
        "pattern": r"^[a-z0-9]+(?:[.-][a-z0-9]+)*$"
    });
    schema["$defs"]["CapabilityAdvertisement"]["properties"]["schemaDigest"]["pattern"] =
        json!(r"^sha256:[0-9a-f]{64}$");
    schema["$defs"]["CapabilityAdvertisement"]["properties"]["endpoint"]["pattern"] =
        json!(r"^/system-plane/v1/[a-z0-9]+(?:[/-][a-z0-9]+)*$");
    schema
}

fn validate_present(
    value: &str,
    code: CoreIssueCode,
    path: &str,
    message: &str,
    next_action: &str,
    issues: &mut Vec<CoreIssue>,
) {
    if value.trim().is_empty() {
        push_issue(issues, code, path, message, next_action);
    }
}

fn capability_major_version(contract_id: &str) -> Option<u32> {
    let remainder = contract_id.strip_prefix("lenso.system-plane.")?;
    let (capability, version) = remainder.rsplit_once(".v")?;
    if !valid_dotted_id(capability) {
        return None;
    }
    let major = version.parse::<u32>().ok()?;
    (major > 0 && version == major.to_string()).then_some(major)
}

fn valid_dotted_id(value: &str) -> bool {
    !value.is_empty() && value.split('.').all(valid_id_segment)
}

fn valid_id_segment(segment: &str) -> bool {
    !segment.is_empty()
        && !segment.starts_with('-')
        && !segment.ends_with('-')
        && !segment.contains("--")
        && segment
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_sha256_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn valid_capability_endpoint(value: &str) -> bool {
    value
        .strip_prefix("/system-plane/v1/")
        .is_some_and(|suffix| {
            !suffix.is_empty()
                && !suffix.starts_with('/')
                && !suffix.contains("//")
                && !suffix.contains(['?', '#'])
                && suffix.split('/').all(valid_id_segment)
        })
}

fn push_issue(
    issues: &mut Vec<CoreIssue>,
    code: CoreIssueCode,
    path: impl Into<String>,
    message: impl Into<String>,
    next_action: impl Into<String>,
) {
    issues.push(CoreIssue {
        code,
        path: path.into(),
        message: message.into(),
        next_action: next_action.into(),
    });
}
