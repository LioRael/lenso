//! Provider V1 host adapter for Service-delivered Modules.
//!
//! This crate owns transport only. Core contracts stay in `platform-module`,
//! and host integration stays in `lenso-bootstrap`.

mod admin_action;
mod admin_data;
mod binding;
mod config;
mod event;
mod grpc;
mod invocation;
mod protocol;
mod provider_runtime;
mod proxy;
mod request;
mod response;
mod router;
mod runtime;
mod source;
mod validation;

pub use admin_action::ProviderAdminActionSource;
pub use admin_data::ProviderAdminDataSource;
pub use binding::ProviderBinding;
pub use config::{ProviderConfig, ProviderTransport};
pub use event::{ProviderEventHandler, ProviderEventHostActionRunner};
pub use protocol::{
    PROVIDER_PROTOCOL, ProviderDescriptor, ProviderErrorBody, ProviderErrorDetail,
    ProviderErrorEnvelope, ProviderExport, ProviderExportHealth, ProviderHealth,
    ProviderHostEffectBatch, ProviderInvocation, ProviderInvocationAcknowledgement,
    ProviderInvocationMode, ProviderInvocationReference, ProviderManifestResponse,
    ProviderOperationKind, ProviderOutcome, ProviderOutcomeStatus, ProviderTransportBinding,
};
pub use provider_runtime::{
    BEARER_ENV_TRUST_PROFILE, EnvironmentBearerCredentialResolver, FixedBearerCredentialResolver,
    FixedProviderEndpointResolver, LoadedProviderRuntime, ProviderCredentialResolver,
    ProviderEndpointResolutionRequest, ProviderEndpointResolver, ProviderRuntimeAdapter,
    ProviderRuntimeAdapters,
};
pub use proxy::{
    ProviderHttpProxyMatch, ProviderHttpProxyModule, ProviderHttpProxyRegistry,
    ProviderHttpProxyRoute,
};
pub use router::{
    ProviderHttpProxyResponse, ProviderHttpProxyStatus, install_provider_http_proxy_registry,
    router,
};
pub use runtime::ProviderRuntimeFunction;
pub use source::{LoadedProvider, ProviderSource};
