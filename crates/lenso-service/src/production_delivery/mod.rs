mod config;
mod deployment;
mod edge;
mod eligibility;
mod policy;
mod promotion;
mod release;
mod resilience;
mod rollout;
mod trust;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub use config::*;
pub use deployment::*;
pub use edge::*;
pub use eligibility::*;
pub use policy::*;
pub use promotion::*;
pub use release::*;
pub use resilience::*;
pub use rollout::*;
pub use trust::*;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryDecision {
    Passed,
    Advisory,
    Blocked,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryIssueCode {
    ReleaseInputInvalid,
    MutableArtifactReference,
    MissingSbom,
    MissingProvenance,
    ProvenanceSubjectMismatch,
    SignatureMissing,
    SignatureInvalid,
    SignerUntrusted,
    SignerRevoked,
    ReleaseTampered,
    ConfigContractMismatch,
    PlaintextSecretDetected,
    SecretReferenceUnresolved,
    StaleInput,
    ConcurrentMutation,
    PolicyEvidenceMissing,
    PolicyRuleBlocked,
    ContractIncompatible,
    MigrationUnsafe,
    WorkflowIncompatible,
    RollbackUnsafe,
    EdgeOperationUnknown,
    EdgePathConflict,
    EdgeExposureUnsafe,
    DeploymentInputInvalid,
    MigrationIncomplete,
    ObservationStale,
    ApprovalRequired,
    ApprovalInvalid,
    ReliabilityEvidenceMissing,
    CanaryBreach,
    RollbackIncomplete,
    CoordinationUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryIssue {
    pub code: DeliveryIssueCode,
    pub message: String,
    #[serde(default)]
    pub evidence_references: Vec<String>,
    pub remediation: String,
    #[serde(default)]
    pub next_actions: Vec<String>,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryEffects {
    pub mutates_environment: bool,
    pub mutates_configuration: bool,
    pub mutates_gateway: bool,
    pub mutates_deployment: bool,
    pub appends_ledger: bool,
}

pub(crate) fn issue(
    code: DeliveryIssueCode,
    message: impl Into<String>,
    remediation: impl Into<String>,
    next_action: impl Into<String>,
) -> DeliveryIssue {
    DeliveryIssue {
        code,
        message: message.into(),
        evidence_references: Vec::new(),
        remediation: remediation.into(),
        next_actions: vec![next_action.into()],
    }
}

pub(crate) fn valid_sha256_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}
