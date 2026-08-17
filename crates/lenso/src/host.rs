//! Public authoring helpers for Lenso host applications.

pub mod outbox;
pub mod transaction;

#[cfg(feature = "linked-module")]
pub use crate::ModuleManifest;
#[cfg(feature = "linked-module")]
pub use platform_core::Migration;
#[cfg(feature = "linked-module")]
pub use platform_module::HostLinkedModule;

/// HTTP authoring helpers for host-owned linked modules.
#[cfg(feature = "linked-module")]
pub mod http {
    pub use axum::Json;
    pub use axum::extract::{Path, State};
    pub use axum::routing::{delete, get, patch, post, put};
    pub use platform_core::{AppContext, AppError, ErrorCode, RequestContext};
    pub use platform_http::responses::json;
    pub use platform_http::{
        ApiErrorResponse, ApiOpenApiRouter, ErrorResponse, HttpRequestContext, JsonBody,
        OpenApiRouter, UserActor, routes,
    };
    pub use platform_module::{
        LinkedBinding, LinkedHttpContribution, ModuleHttpMethod, ModuleHttpRoute,
    };
}

/// Durable Event consumption for host-owned linked modules.
///
/// Delivery is at least once. Business effects must be idempotent with the
/// stable [`ClaimedOutboxEvent::id`] across retries and process restarts.
#[cfg(feature = "linked-module")]
pub mod events {
    pub use crate::host::outbox::{
        AppError, AppResult, ClaimedOutboxEvent, ErrorCode, EventHandler,
    };
}

/// Runtime behavior authoring for host-owned linked modules.
#[cfg(feature = "linked-module")]
pub mod runtime {
    pub use platform_core::{
        ActorContext, AppContext, AppError, AppResult, CorrelationId, ErrorCode, ExecutionContext,
        ExecutionId, TenantId, TraceContext,
    };
    pub use platform_module::{LinkedBinding, Module};
    pub use platform_runtime::{
        FunctionDefinition, FunctionHandler, RetryPolicy, RuntimeDescriptor,
    };
}

#[cfg(feature = "host")]
mod boot;
#[cfg(feature = "host")]
pub use boot::*;
