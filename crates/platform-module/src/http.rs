//! Pure-data HTTP route declarations for module manifests.
//!
//! These declarations are metadata only. Linked modules still contribute real
//! Axum/OpenAPI routes through `app-bootstrap`; remote route proxying requires a
//! separate host protocol before these entries can be mounted.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "UPPERCASE")]
#[non_exhaustive]
pub enum ModuleHttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ModuleHttpRoute {
    pub method: ModuleHttpMethod,
    /// Module-local path, e.g. `/contacts` or `/contacts/{id}`.
    pub path: String,
    /// Optional capability required before a future host proxy exposes this
    /// route. No enforcement exists until the proxy protocol is implemented.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
}
