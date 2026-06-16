use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};

/// A registered setting's static metadata.
#[derive(Debug, Serialize, ToSchema)]
pub struct ConfigDescriptorDto {
    pub key: String,
    pub service: String,
    pub group: Option<String>,
    pub section: Option<String>,
    pub order: i32,
    pub visible_when: Option<ConfigVisibilityConditionDto>,
    pub value_type: Value,
    pub default: Value,
    pub editable: bool,
    pub restart_only: bool,
    pub description: String,
}

/// Presentation metadata for a set of related settings.
#[derive(Debug, Serialize, ToSchema)]
pub struct ConfigGroupDto {
    pub id: String,
    pub label: String,
    pub description: String,
    pub order: i32,
}

/// A declarative condition that controls whether a setting is shown.
#[derive(Debug, Serialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConfigVisibilityConditionDto {
    Equals {
        service: String,
        key: String,
        value: Value,
    },
}

/// The list of all registered descriptors.
#[derive(Debug, Serialize, ToSchema)]
pub struct ConfigDescriptorListResponse {
    pub groups: Vec<ConfigGroupDto>,
    pub data: Vec<ConfigDescriptorDto>,
}

/// A resolved config value for the running service plus any persisted desired value.
#[derive(Debug, Serialize, ToSchema)]
pub struct ConfigValueDto {
    pub key: String,
    /// Backward-compatible alias for `effective_value`.
    pub value: Value,
    pub effective_value: Value,
    pub desired_value: Value,
    pub pending_restart: bool,
    pub source: String,
}

/// The list of effective and desired values for the running service.
#[derive(Debug, Serialize, ToSchema)]
pub struct ConfigValueListResponse {
    pub data: Vec<ConfigValueDto>,
}

/// Request body for writing a value.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ConfigWriteRequest {
    pub value: Value,
}

/// Response after a successful write.
#[derive(Debug, Serialize, ToSchema)]
pub struct ConfigWriteResponse {
    pub key: String,
    pub service: String,
    pub value: Value,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<String>,
    /// True when the key is restart-only: the value is persisted but not applied
    /// to running instances until restart.
    pub applies_on_restart: bool,
}

/// One audit entry.
#[derive(Debug, Serialize, ToSchema)]
pub struct ConfigAuditDto {
    pub service: String,
    pub key: String,
    pub old_value: Option<Value>,
    pub new_value: Value,
    pub actor: Option<String>,
    pub changed_at: DateTime<Utc>,
}

/// Audit history response.
#[derive(Debug, Serialize, ToSchema)]
pub struct ConfigAuditListResponse {
    pub data: Vec<ConfigAuditDto>,
}

/// Query params for the audit endpoint.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ConfigAuditQuery {
    pub limit: Option<i64>,
}
