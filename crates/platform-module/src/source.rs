use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Runtime provenance for an already loaded Module.
///
/// Delivery belongs to `ModuleRelease.delivery`; a Module is either linked into
/// the Host or provided by an independently deployed Service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModuleSource {
    Linked,
    Service,
}
