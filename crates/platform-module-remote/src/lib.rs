//! HTTP-backed module source for out-of-process modules.
//!
//! This crate owns transport only. Core contracts stay in `platform-module`,
//! and host integration stays in `app-bootstrap`.

mod admin_data;
mod binding;
mod config;
mod protocol;
mod source;

pub use admin_data::RemoteAdminDataSource;
pub use binding::RemoteBinding;
pub use config::RemoteModuleConfig;
pub use protocol::RemoteManifestResponse;
pub use source::RemoteModuleSource;
