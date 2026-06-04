//! HTTP-backed module source for out-of-process modules.
//!
//! This crate owns transport only. Core contracts stay in `platform-module`,
//! and host integration stays in `app-bootstrap`.

mod admin_data;
mod binding;
mod config;
mod protocol;
mod proxy;
mod request;
mod response;
mod router;
mod source;

pub use admin_data::RemoteAdminDataSource;
pub use binding::RemoteBinding;
pub use config::RemoteModuleConfig;
pub use protocol::{
    RemoteErrorBody, RemoteErrorDetail, RemoteErrorEnvelope, RemoteManifestResponse,
};
pub use proxy::{
    RemoteHttpProxyMatch, RemoteHttpProxyModule, RemoteHttpProxyRegistry, RemoteHttpProxyRoute,
};
pub use router::{
    RemoteHttpProxyResponse, RemoteHttpProxyStatus, install_remote_http_proxy_registry, router,
};
pub use source::RemoteModuleSource;
