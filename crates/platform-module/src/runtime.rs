//! Pure-data runtime declarations for module manifests.
//!
//! These declarations describe runtime behavior a module can provide without
//! carrying executable handlers. Loading sources decide how to bind them later.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RuntimeSurface {
    #[serde(default)]
    pub functions: Vec<RuntimeFunctionDeclaration>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RuntimeFunctionDeclaration {
    /// Stable function name used by `runtime.function_runs`.
    pub name: String,
    /// Version for the declared function contract.
    pub version: u16,
    /// Queue name the host should use when registering the function.
    pub queue: String,
    /// Optional schema identifier for the function input payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<String>,
    /// Optional retry policy requested by the module. The host still owns
    /// enforcement and may clamp values when behavior registration is added.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_policy: Option<RuntimeRetryPolicyDeclaration>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RuntimeRetryPolicyDeclaration {
    pub max_attempts: u32,
    pub initial_delay_ms: u64,
}
