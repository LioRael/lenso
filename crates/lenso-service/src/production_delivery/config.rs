use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::extraction_input_digest;

use super::{DeliveryEffects, DeliveryIssue, DeliveryIssueCode, issue};

pub const CONFIG_CONTRACT_PROTOCOL: &str = "lenso.config-contract.v1";
pub const CONFIG_REVISION_PROTOCOL: &str = "lenso.config-revision.v1";
pub const CONFIG_ACTIVATION_PLAN_PROTOCOL: &str = "lenso.config-activation-plan.v1";
pub const CONFIG_ACTIVATION_RECEIPT_PROTOCOL: &str = "lenso.config-activation-receipt.v1";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ConfigValueType {
    String,
    Integer,
    Number,
    Boolean,
    Object,
    Array,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ConfigFieldSensitivity {
    Public,
    Sensitive,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ConfigFieldScope {
    Service,
    Workload,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ConfigFieldActivation {
    Hot,
    Restart,
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct ConfigField {
    pub path: String,
    pub value_type: ConfigValueType,
    pub required: bool,
    pub sensitivity: ConfigFieldSensitivity,
    pub scope: ConfigFieldScope,
    pub activation: ConfigFieldActivation,
    pub mutable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigContractDefinition {
    pub protocol: String,
    pub reference: String,
    pub digest: String,
    pub fields: Vec<ConfigField>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SecretReferenceStatus {
    Resolved,
    Unresolved,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SecretReference {
    pub reference_id: String,
    pub provider: String,
    pub purpose: String,
    pub scope: String,
    pub status: SecretReferenceStatus,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SecretReferenceObservation {
    pub status: SecretReferenceStatus,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

/// Replaceable boundary for observing an opaque reference without reading its value.
pub trait SecretProvider: std::fmt::Debug + Send + Sync {
    fn provider_name(&self) -> &str;

    fn observe_reference(&self, reference_id: &str) -> Option<SecretReferenceObservation>;
}

#[derive(Debug, Clone, Default)]
pub struct DeterministicSecretProvider {
    provider_name: String,
    observations: BTreeMap<String, SecretReferenceObservation>,
}

impl DeterministicSecretProvider {
    #[must_use]
    pub fn new(
        provider_name: impl Into<String>,
        observations: impl IntoIterator<Item = (String, SecretReferenceObservation)>,
    ) -> Self {
        Self {
            provider_name: provider_name.into(),
            observations: observations.into_iter().collect(),
        }
    }
}

impl SecretProvider for DeterministicSecretProvider {
    fn provider_name(&self) -> &str {
        &self.provider_name
    }

    fn observe_reference(&self, reference_id: &str) -> Option<SecretReferenceObservation> {
        self.observations.get(reference_id).cloned()
    }
}

#[must_use]
pub fn observe_secret_reference(
    provider: &dyn SecretProvider,
    reference_id: impl Into<String>,
    purpose: impl Into<String>,
    scope: impl Into<String>,
) -> SecretReference {
    let reference_id = reference_id.into();
    let observation = provider
        .observe_reference(&reference_id)
        .filter(|observation| secret_metadata_is_safe(&observation.metadata))
        .unwrap_or(SecretReferenceObservation {
            status: SecretReferenceStatus::Unresolved,
            metadata: BTreeMap::new(),
        });
    SecretReference {
        reference_id,
        provider: provider.provider_name().to_owned(),
        purpose: purpose.into(),
        scope: scope.into(),
        status: observation.status,
        metadata: observation.metadata,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigImpact {
    pub path: String,
    pub scope: ConfigFieldScope,
    pub activation: ConfigFieldActivation,
    pub mutable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigRevision {
    pub protocol: String,
    pub revision_id: String,
    pub revision_digest: String,
    pub service_id: String,
    pub contract_reference: String,
    pub contract_digest: String,
    pub values: BTreeMap<String, Value>,
    pub secret_references: Vec<SecretReference>,
    pub impacts: Vec<ConfigImpact>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ConfigOperation {
    Stage,
    Activate,
    Rollback,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ConfigRevisionActivation {
    Staged,
    Active,
    RolledBack,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigActivationPlan {
    pub protocol: String,
    pub plan_id: String,
    pub plan_digest: String,
    pub environment: String,
    pub expected_environment_revision: u64,
    pub operation: ConfigOperation,
    pub target_revision_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_revision_id: Option<String>,
    pub contract_digest: String,
    pub effects: DeliveryEffects,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigActivationReceipt {
    pub protocol: String,
    pub receipt_id: String,
    pub plan_id: String,
    pub environment: String,
    pub environment_revision_before: u64,
    pub environment_revision_after: u64,
    pub target_revision_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_revision_id: Option<String>,
    pub activation: ConfigRevisionActivation,
    pub effects: DeliveryEffects,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigState {
    pub environment: String,
    pub environment_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub staged_revision_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_revision_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_revision_id: Option<String>,
    #[serde(default)]
    pub history: Vec<ConfigActivationReceipt>,
}

impl ConfigState {
    #[must_use]
    pub fn new(environment: impl Into<String>, environment_revision: u64) -> Self {
        Self {
            environment: environment.into(),
            environment_revision,
            staged_revision_id: None,
            active_revision_id: None,
            previous_revision_id: None,
            history: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigApplyRejection {
    pub issues: Vec<DeliveryIssue>,
    pub effects: DeliveryEffects,
}

pub fn build_config_contract(
    reference: impl Into<String>,
    mut fields: Vec<ConfigField>,
) -> Result<ConfigContractDefinition, Vec<DeliveryIssue>> {
    let reference = reference.into();
    fields.sort();
    let mut issues = Vec::new();
    if reference.trim().is_empty() || fields.is_empty() {
        issues.push(issue(
            DeliveryIssueCode::ConfigContractMismatch,
            "A Config Contract requires a stable reference and at least one field.",
            "Declare the exact configuration surface consumed by the Service Release.",
            "Correct the Config Contract and build it again.",
        ));
    }
    let unique_paths = fields
        .iter()
        .map(|field| field.path.as_str())
        .collect::<BTreeSet<_>>();
    if unique_paths.len() != fields.len() || unique_paths.contains("") {
        issues.push(issue(
            DeliveryIssueCode::ConfigContractMismatch,
            "Config Contract field paths must be non-empty and unique.",
            "Assign one stable path to every configuration field.",
            "Correct duplicate or empty paths and build the contract again.",
        ));
    }
    if !issues.is_empty() {
        return Err(issues);
    }
    let digest = digest_json(&(
        CONFIG_CONTRACT_PROTOCOL,
        reference.as_str(),
        fields.as_slice(),
    ));
    Ok(ConfigContractDefinition {
        protocol: CONFIG_CONTRACT_PROTOCOL.to_owned(),
        reference,
        digest,
        fields,
    })
}

pub fn build_config_revision(
    service_id: impl Into<String>,
    contract: &ConfigContractDefinition,
    values: BTreeMap<String, Value>,
    mut secret_references: Vec<SecretReference>,
    secret_provider: &dyn SecretProvider,
) -> Result<ConfigRevision, Vec<DeliveryIssue>> {
    let service_id = service_id.into();
    let mut issues = Vec::new();
    if !config_contract_integrity_is_valid(contract) {
        issues.push(issue(
            DeliveryIssueCode::ConfigContractMismatch,
            "The Config Contract content does not match its digest.",
            "Use the exact Config Contract bound to the Service Release.",
            "Regenerate the Config Contract and Config Revision.",
        ));
    }

    let fields = contract
        .fields
        .iter()
        .map(|field| (field.path.as_str(), field))
        .collect::<BTreeMap<_, _>>();
    for (path, value) in &values {
        let Some(field) = fields.get(path.as_str()) else {
            issues.push(issue(
                DeliveryIssueCode::ConfigContractMismatch,
                format!("Configuration field `{path}` is not declared by the Config Contract."),
                "Remove unknown values or update the Config Contract before building a revision.",
                "Correct the values and build the Config Revision again.",
            ));
            continue;
        };
        if field.sensitivity == ConfigFieldSensitivity::Sensitive {
            issues.push(issue(
                DeliveryIssueCode::PlaintextSecretDetected,
                format!("Sensitive field `{path}` was supplied as configuration data."),
                "Replace the value with an opaque Secret Reference.",
                "Remove the plaintext value and bind a Secret Reference.",
            ));
        } else if !value_matches(field.value_type, value) {
            issues.push(issue(
                DeliveryIssueCode::ConfigContractMismatch,
                format!("Configuration field `{path}` has the wrong JSON value type."),
                "Supply a value matching the Config Contract field type.",
                "Correct the value and build the Config Revision again.",
            ));
        }
    }

    secret_references.sort_by(|left, right| {
        (&left.purpose, &left.reference_id).cmp(&(&right.purpose, &right.reference_id))
    });
    let unique_reference_ids = secret_references
        .iter()
        .map(|reference| reference.reference_id.as_str())
        .collect::<BTreeSet<_>>();
    let unique_purposes = secret_references
        .iter()
        .map(|reference| reference.purpose.as_str())
        .collect::<BTreeSet<_>>();
    if unique_reference_ids.len() != secret_references.len()
        || unique_purposes.len() != secret_references.len()
    {
        issues.push(issue(
            DeliveryIssueCode::ConfigContractMismatch,
            "Secret References must bind one unique opaque reference to each sensitive field.",
            "Keep exactly one resolved Secret Reference per declared sensitive purpose.",
            "Remove duplicate Secret References and build the Config Revision again.",
        ));
    }
    for reference in &secret_references {
        let observed = secret_provider.observe_reference(&reference.reference_id);
        if reference.reference_id.trim().is_empty()
            || reference.provider.trim().is_empty()
            || reference.purpose.trim().is_empty()
            || reference.scope.trim().is_empty()
            || !secret_metadata_is_safe(&reference.metadata)
        {
            issues.push(issue(
                DeliveryIssueCode::PlaintextSecretDetected,
                "A Secret Reference contains unsafe or value-shaped metadata.",
                "Keep only opaque provider identity, purpose, scope, and non-sensitive status metadata.",
                "Remove sensitive metadata and build the Config Revision again.",
            ));
        }
        if reference.provider != secret_provider.provider_name()
            || observed.as_ref().is_none_or(|observed| {
                observed.status != reference.status || observed.metadata != reference.metadata
            })
        {
            issues.push(issue(
                DeliveryIssueCode::SecretReferenceUnresolved,
                format!(
                    "Secret Reference `{}` is not attested by the selected Secret Provider.",
                    reference.reference_id
                ),
                "Observe the opaque reference through the configured provider boundary.",
                "Refresh the provider observation and build the Config Revision again.",
            ));
        }
        if reference.status != SecretReferenceStatus::Resolved {
            issues.push(issue(
                DeliveryIssueCode::SecretReferenceUnresolved,
                format!(
                    "Secret Reference `{}` is not resolved.",
                    reference.reference_id
                ),
                "Resolve the opaque reference through the selected Secret Provider.",
                "Refresh Secret Reference status before activation.",
            ));
        }
        if !fields
            .get(reference.purpose.as_str())
            .is_some_and(|field| field.sensitivity == ConfigFieldSensitivity::Sensitive)
        {
            issues.push(issue(
                DeliveryIssueCode::ConfigContractMismatch,
                format!(
                    "Secret Reference `{}` does not bind a declared sensitive Config field.",
                    reference.reference_id
                ),
                "Bind opaque references only to sensitive fields in the exact Config Contract.",
                "Correct the Secret Reference purpose and build the Config Revision again.",
            ));
        }
    }

    for field in &contract.fields {
        if !field.required {
            continue;
        }
        let present = match field.sensitivity {
            ConfigFieldSensitivity::Public => values.contains_key(&field.path),
            ConfigFieldSensitivity::Sensitive => secret_references
                .iter()
                .any(|reference| reference.purpose == field.path),
        };
        if !present {
            issues.push(issue(
                if field.sensitivity == ConfigFieldSensitivity::Sensitive {
                    DeliveryIssueCode::SecretReferenceUnresolved
                } else {
                    DeliveryIssueCode::ConfigContractMismatch
                },
                format!("Required configuration field `{}` is missing.", field.path),
                "Supply the required non-secret value or opaque Secret Reference.",
                "Complete the Config Revision and validate it again.",
            ));
        }
    }
    if !issues.is_empty() {
        return Err(issues);
    }

    let impacts = contract
        .fields
        .iter()
        .filter(|field| {
            values.contains_key(&field.path)
                || secret_references
                    .iter()
                    .any(|reference| reference.purpose == field.path)
        })
        .map(|field| ConfigImpact {
            path: field.path.clone(),
            scope: field.scope,
            activation: field.activation,
            mutable: field.mutable,
        })
        .collect::<Vec<_>>();
    let revision_digest = digest_json(&(
        CONFIG_REVISION_PROTOCOL,
        service_id.as_str(),
        contract.reference.as_str(),
        contract.digest.as_str(),
        &values,
        &secret_references,
        &impacts,
    ));
    Ok(ConfigRevision {
        protocol: CONFIG_REVISION_PROTOCOL.to_owned(),
        revision_id: format!("config-revision:{revision_digest}"),
        revision_digest,
        service_id,
        contract_reference: contract.reference.clone(),
        contract_digest: contract.digest.clone(),
        values,
        secret_references,
        impacts,
    })
}

fn secret_metadata_is_safe(metadata: &BTreeMap<String, String>) -> bool {
    metadata.iter().all(|(key, value)| match key.as_str() {
        "leaseExpiresAt" | "lastResolvedAt" => timestamp_metadata_is_safe(value),
        "rotationRevision" | "providerRevision" => {
            !value.is_empty()
                && value.len() <= 20
                && value.bytes().all(|byte| byte.is_ascii_digit())
        }
        "rotationStatus" => matches!(
            value.as_str(),
            "current" | "due" | "rotating" | "stale" | "revoked"
        ),
        _ => false,
    })
}

/// Return whether a Secret Reference contains only the bounded, non-value metadata
/// accepted by the production delivery contract.
#[must_use]
pub fn secret_reference_metadata_is_safe(reference: &SecretReference) -> bool {
    secret_metadata_is_safe(&reference.metadata)
}

fn timestamp_metadata_is_safe(value: &str) -> bool {
    value.len() >= 20
        && value.len() <= 40
        && value.as_bytes().get(4) == Some(&b'-')
        && value.as_bytes().get(7) == Some(&b'-')
        && value.as_bytes().get(10) == Some(&b'T')
        && (value.ends_with('Z')
            || value
                .as_bytes()
                .iter()
                .rev()
                .take(6)
                .any(|byte| *byte == b'+'))
        && value.bytes().all(|byte| {
            byte.is_ascii_digit() || matches!(byte, b'-' | b':' | b'T' | b'.' | b'Z' | b'+')
        })
}

pub fn plan_config_activation(
    state: &ConfigState,
    contract: &ConfigContractDefinition,
    revision: &ConfigRevision,
    secret_provider: &dyn SecretProvider,
    operation: ConfigOperation,
) -> Result<ConfigActivationPlan, Vec<DeliveryIssue>> {
    if state.environment.trim().is_empty()
        || !config_revision_matches_contract(revision, contract, secret_provider)
    {
        return Err(vec![issue(
            DeliveryIssueCode::ConfigContractMismatch,
            "Config activation requires an integrity-valid Config Revision and environment identity.",
            "Regenerate the Config Revision from its exact Config Contract.",
            "Correct the input and plan activation again.",
        )]);
    }
    if operation == ConfigOperation::Activate
        && state.staged_revision_id.as_deref() != Some(revision.revision_id.as_str())
        && state.active_revision_id.as_deref() != Some(revision.revision_id.as_str())
    {
        return Err(vec![issue(
            DeliveryIssueCode::ConfigContractMismatch,
            "The Config Revision must be staged before activation.",
            "Stage and validate the exact revision before making it active.",
            "Plan and apply the stage operation first.",
        )]);
    }
    if operation == ConfigOperation::Rollback
        && state.previous_revision_id.as_deref() != Some(revision.revision_id.as_str())
    {
        return Err(vec![issue(
            DeliveryIssueCode::ConfigContractMismatch,
            "Config rollback must target the environment's explicit previous Config Revision.",
            "Select the exact previous revision recorded by the active environment state.",
            "Refresh Config state and plan rollback to its previous revision.",
        )]);
    }
    let previous_revision_id = state.active_revision_id.clone();
    let effects = DeliveryEffects::default();
    let plan_digest = digest_json(&(
        CONFIG_ACTIVATION_PLAN_PROTOCOL,
        state.environment.as_str(),
        state.environment_revision,
        operation,
        revision.revision_id.as_str(),
        previous_revision_id.as_deref(),
        revision.contract_digest.as_str(),
        &effects,
    ));
    Ok(ConfigActivationPlan {
        protocol: CONFIG_ACTIVATION_PLAN_PROTOCOL.to_owned(),
        plan_id: format!("config-activation-plan:{plan_digest}"),
        plan_digest,
        environment: state.environment.clone(),
        expected_environment_revision: state.environment_revision,
        operation,
        target_revision_id: revision.revision_id.clone(),
        previous_revision_id,
        contract_digest: revision.contract_digest.clone(),
        effects,
    })
}

pub fn apply_config_activation(
    state: &mut ConfigState,
    plan: &ConfigActivationPlan,
) -> Result<ConfigActivationReceipt, ConfigApplyRejection> {
    if !config_activation_plan_integrity_is_valid(plan) {
        return Err(ConfigApplyRejection {
            issues: vec![issue(
                DeliveryIssueCode::StaleInput,
                "Config activation inputs changed after the plan was generated.",
                "Generate a new plan from the current environment and Config Revision state.",
                "Refresh state and plan the configuration operation again.",
            )],
            effects: DeliveryEffects::default(),
        });
    }
    if let Some(existing) = state
        .history
        .iter()
        .find(|item| item.plan_id == plan.plan_id)
    {
        return config_activation_receipt_integrity_is_valid(existing, plan)
            .then(|| existing.clone())
            .ok_or_else(|| ConfigApplyRejection {
                issues: vec![issue(
                    DeliveryIssueCode::StaleInput,
                    "The completed Config activation receipt no longer matches the exact plan.",
                    "Preserve the immutable plan and append-only receipt together.",
                    "Restore the original receipt or create a new Config activation plan.",
                )],
                effects: DeliveryEffects::default(),
            });
    }
    if state.environment != plan.environment
        || state.environment_revision != plan.expected_environment_revision
    {
        return Err(ConfigApplyRejection {
            issues: vec![issue(
                DeliveryIssueCode::StaleInput,
                "Config activation inputs changed after the plan was generated.",
                "Generate a new plan from the current environment and Config Revision state.",
                "Refresh state and plan the configuration operation again.",
            )],
            effects: DeliveryEffects::default(),
        });
    }

    let activation = match plan.operation {
        ConfigOperation::Stage => {
            state.staged_revision_id = Some(plan.target_revision_id.clone());
            ConfigRevisionActivation::Staged
        }
        ConfigOperation::Activate => {
            if state.staged_revision_id.as_deref() != Some(plan.target_revision_id.as_str())
                && state.active_revision_id.as_deref() != Some(plan.target_revision_id.as_str())
            {
                return Err(ConfigApplyRejection {
                    issues: vec![issue(
                        DeliveryIssueCode::StaleInput,
                        "The staged Config Revision changed before activation.",
                        "Regenerate the plan from the current staged revision.",
                        "Restage or replan activation.",
                    )],
                    effects: DeliveryEffects::default(),
                });
            }
            state.previous_revision_id = state.active_revision_id.clone();
            state.active_revision_id = Some(plan.target_revision_id.clone());
            state.staged_revision_id = None;
            ConfigRevisionActivation::Active
        }
        ConfigOperation::Rollback => {
            if state.previous_revision_id.as_deref() != Some(plan.target_revision_id.as_str())
                || state.active_revision_id != plan.previous_revision_id
            {
                return Err(ConfigApplyRejection {
                    issues: vec![issue(
                        DeliveryIssueCode::StaleInput,
                        "The previous Config Revision changed before rollback.",
                        "Regenerate rollback from the current active and previous revision pair.",
                        "Refresh state and replan the rollback.",
                    )],
                    effects: DeliveryEffects::default(),
                });
            }
            state.previous_revision_id = state.active_revision_id.clone();
            state.active_revision_id = Some(plan.target_revision_id.clone());
            state.staged_revision_id = None;
            ConfigRevisionActivation::RolledBack
        }
    };
    let revision_before = state.environment_revision;
    state.environment_revision += 1;
    let effects = DeliveryEffects {
        mutates_configuration: true,
        appends_ledger: true,
        ..DeliveryEffects::default()
    };
    let receipt_id = format!(
        "config-activation-receipt:{}",
        digest_json(&(
            plan.plan_id.as_str(),
            revision_before,
            state.environment_revision,
            activation,
        ))
    );
    let receipt = ConfigActivationReceipt {
        protocol: CONFIG_ACTIVATION_RECEIPT_PROTOCOL.to_owned(),
        receipt_id,
        plan_id: plan.plan_id.clone(),
        environment: state.environment.clone(),
        environment_revision_before: revision_before,
        environment_revision_after: state.environment_revision,
        target_revision_id: plan.target_revision_id.clone(),
        previous_revision_id: plan.previous_revision_id.clone(),
        activation,
        effects,
    };
    state.history.push(receipt.clone());
    Ok(receipt)
}

#[must_use]
pub fn config_contract_integrity_is_valid(contract: &ConfigContractDefinition) -> bool {
    let mut canonical_fields = contract.fields.clone();
    canonical_fields.sort();
    let unique_paths = contract
        .fields
        .iter()
        .map(|field| field.path.as_str())
        .collect::<BTreeSet<_>>();
    contract.protocol == CONFIG_CONTRACT_PROTOCOL
        && !contract.reference.trim().is_empty()
        && !contract.fields.is_empty()
        && contract.fields == canonical_fields
        && unique_paths.len() == contract.fields.len()
        && !unique_paths.contains("")
        && digest_json(&(
            contract.protocol.as_str(),
            contract.reference.as_str(),
            contract.fields.as_slice(),
        )) == contract.digest
}

#[must_use]
pub fn config_revision_integrity_is_valid(revision: &ConfigRevision) -> bool {
    revision.protocol == CONFIG_REVISION_PROTOCOL
        && revision.revision_id == format!("config-revision:{}", revision.revision_digest)
        && digest_json(&(
            revision.protocol.as_str(),
            revision.service_id.as_str(),
            revision.contract_reference.as_str(),
            revision.contract_digest.as_str(),
            &revision.values,
            &revision.secret_references,
            &revision.impacts,
        )) == revision.revision_digest
}

#[must_use]
pub fn config_revision_matches_contract(
    revision: &ConfigRevision,
    contract: &ConfigContractDefinition,
    secret_provider: &dyn SecretProvider,
) -> bool {
    if !config_contract_integrity_is_valid(contract)
        || !config_revision_integrity_is_valid(revision)
        || revision.contract_reference != contract.reference
        || revision.contract_digest != contract.digest
    {
        return false;
    }
    build_config_revision(
        revision.service_id.clone(),
        contract,
        revision.values.clone(),
        revision.secret_references.clone(),
        secret_provider,
    )
    .is_ok_and(|expected| expected == *revision)
}

#[must_use]
pub fn config_activation_plan_integrity_is_valid(plan: &ConfigActivationPlan) -> bool {
    plan.protocol == CONFIG_ACTIVATION_PLAN_PROTOCOL
        && plan.plan_id == format!("config-activation-plan:{}", plan.plan_digest)
        && digest_json(&(
            plan.protocol.as_str(),
            plan.environment.as_str(),
            plan.expected_environment_revision,
            plan.operation,
            plan.target_revision_id.as_str(),
            plan.previous_revision_id.as_deref(),
            plan.contract_digest.as_str(),
            &plan.effects,
        )) == plan.plan_digest
}

#[must_use]
pub fn config_activation_receipt_integrity_is_valid(
    receipt: &ConfigActivationReceipt,
    plan: &ConfigActivationPlan,
) -> bool {
    let activation = match plan.operation {
        ConfigOperation::Stage => ConfigRevisionActivation::Staged,
        ConfigOperation::Activate => ConfigRevisionActivation::Active,
        ConfigOperation::Rollback => ConfigRevisionActivation::RolledBack,
    };
    let effects = DeliveryEffects {
        mutates_configuration: true,
        appends_ledger: true,
        ..DeliveryEffects::default()
    };
    receipt.protocol == CONFIG_ACTIVATION_RECEIPT_PROTOCOL
        && config_activation_plan_integrity_is_valid(plan)
        && receipt.plan_id == plan.plan_id
        && receipt.environment == plan.environment
        && receipt.environment_revision_before == plan.expected_environment_revision
        && receipt.environment_revision_after == receipt.environment_revision_before + 1
        && receipt.target_revision_id == plan.target_revision_id
        && receipt.previous_revision_id == plan.previous_revision_id
        && receipt.activation == activation
        && receipt.effects == effects
        && receipt.receipt_id
            == format!(
                "config-activation-receipt:{}",
                digest_json(&(
                    plan.plan_id.as_str(),
                    receipt.environment_revision_before,
                    receipt.environment_revision_after,
                    activation,
                ))
            )
}

fn value_matches(expected: ConfigValueType, value: &Value) -> bool {
    match expected {
        ConfigValueType::String => value.is_string(),
        ConfigValueType::Integer => value.as_i64().is_some() || value.as_u64().is_some(),
        ConfigValueType::Number => value.is_number(),
        ConfigValueType::Boolean => value.is_boolean(),
        ConfigValueType::Object => value.is_object(),
        ConfigValueType::Array => value.is_array(),
    }
}

fn digest_json(value: &impl Serialize) -> String {
    extraction_input_digest(serde_json::to_vec(value).expect("delivery values must serialize"))
}
