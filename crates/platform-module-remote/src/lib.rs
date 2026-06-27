//! HTTP-backed module source for out-of-process modules.
//!
//! This crate owns transport only. Core contracts stay in `platform-module`,
//! and host integration stays in `lenso-bootstrap`.

mod admin_action;
mod admin_data;
mod binding;
mod config;
mod event;
mod grpc;
mod protocol;
mod proxy;
mod request;
mod response;
mod router;
mod runtime;
mod source;
mod validation;

pub use admin_action::RemoteAdminActionSource;
pub use admin_data::RemoteAdminDataSource;
pub use binding::RemoteBinding;
pub use config::{RemoteModuleConfig, RemoteModuleTransport};
pub use event::{RemoteEventHandler, RemoteEventHostActionRunner};
pub use protocol::{
    RemoteErrorBody, RemoteErrorDetail, RemoteErrorEnvelope, RemoteManifestResponse,
};
pub use proxy::{
    RemoteHttpProxyMatch, RemoteHttpProxyModule, RemoteHttpProxyRegistry, RemoteHttpProxyRoute,
};
pub use router::{
    RemoteHttpProxyResponse, RemoteHttpProxyStatus, install_remote_http_proxy_registry, router,
};
pub use runtime::RemoteRuntimeFunction;
pub use source::{LoadedRemoteModule, RemoteModuleSource};
