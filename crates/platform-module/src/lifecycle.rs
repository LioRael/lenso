//! Host-owned lifecycle declarations for module manifests.
//!
//! Lifecycle entries are data, not startup callbacks. The host validates these
//! declarations and schedules runtime-owned work during worker startup.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LifecycleSurface {
    #[serde(default)]
    pub startup_checks: Vec<LifecycleStartupCheckDeclaration>,
    #[serde(default)]
    pub activation_jobs: Vec<LifecycleActivationJobDeclaration>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LifecycleStartupCheckDeclaration {
    pub name: String,
    #[serde(default)]
    pub required: bool,
    #[serde(flatten)]
    pub check: LifecycleStartupCheckKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum LifecycleStartupCheckKind {
    FunctionRegistered { function_name: String },
    CapabilityDeclared { capability: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LifecycleActivationJobDeclaration {
    pub name: String,
    pub function_name: String,
    #[serde(default = "default_every_startup")]
    pub run_policy: LifecycleActivationRunPolicy,
    #[serde(default)]
    pub input: Value,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum LifecycleActivationRunPolicy {
    EveryStartup,
}

fn default_every_startup() -> LifecycleActivationRunPolicy {
    LifecycleActivationRunPolicy::EveryStartup
}
