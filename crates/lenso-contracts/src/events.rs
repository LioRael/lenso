//! Pure-data event handler declarations for module manifests.
//!
//! These declarations describe event subscriptions without carrying executable
//! handlers. Loading sources decide how to bind them to host-owned dispatch.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct EventSurface {
    #[serde(default)]
    pub handlers: Vec<EventHandlerDeclaration>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct EventHandlerDeclaration {
    /// Stable handler name used by remote loading sources as the invoke path.
    pub name: String,
    /// Stable event name consumed from `platform.outbox`, e.g.
    /// `identity.user_registered.v1`.
    pub event_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<crate::ServiceOperationMetadata>,
}
