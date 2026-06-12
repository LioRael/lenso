//! A loaded module: serializable manifest + behavior binding + internal config.

use crate::admin_data::{AdminActionSource, AdminDataSource};
use crate::binding::ModuleBinding;
use crate::linked::{LinkedBinding, LinkedHttpContribution};
use crate::manifest::ModuleManifest;
use platform_core::RuntimeConfigDescriptor;
use std::sync::Arc;

/// One loaded module. `manifest` is serializable data; `binding` is behavior;
/// `runtime_config` is internal `&'static` config NOT in the manifest (the
/// registry needs the real `RuntimeConfigType` enum to validate). Cross-source
/// config wire form is deferred to a later spec.
#[derive(Debug)]
pub struct Module {
    pub manifest: ModuleManifest,
    pub binding: Arc<dyn ModuleBinding>,
    pub source: ModuleSource,
    pub load_status: ModuleLoadStatus,
    pub linked_http: Option<LinkedHttpContribution>,
    pub runtime_config: &'static [RuntimeConfigDescriptor],
    /// Optional schema-admin data source. `None` for modules without an admin
    /// surface (e.g. notifications). Set via [`Module::with_admin_data`].
    pub admin_data: Option<Arc<dyn AdminDataSource>>,
    /// Optional executable admin actions. Set via [`Module::with_admin_actions`].
    pub admin_actions: Option<Arc<dyn AdminActionSource>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModuleSource {
    Linked,
    Remote,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleLoadStatus {
    Loaded,
    Error { message: String },
}

impl Module {
    /// Build a compile-time (Linked) module from a manifest + linked behavior.
    /// Config defaults to empty; attach it with [`Module::with_runtime_config`].
    #[must_use]
    pub fn linked(manifest: ModuleManifest, binding: LinkedBinding) -> Self {
        let linked_http = binding.http;
        Self {
            manifest,
            binding: Arc::new(binding),
            source: ModuleSource::Linked,
            load_status: ModuleLoadStatus::Loaded,
            linked_http,
            runtime_config: &[],
            admin_data: None,
            admin_actions: None,
        }
    }

    /// Build a remote module from a manifest + transport-backed behavior.
    /// Remote behavior is intentionally narrow in the first slice.
    #[must_use]
    pub fn remote(manifest: ModuleManifest, binding: Arc<dyn ModuleBinding>) -> Self {
        Self {
            manifest,
            binding,
            source: ModuleSource::Remote,
            load_status: ModuleLoadStatus::Loaded,
            linked_http: None,
            runtime_config: &[],
            admin_data: None,
            admin_actions: None,
        }
    }

    /// Attach the module's editable configuration descriptors.
    #[must_use]
    pub fn with_runtime_config(
        mut self,
        runtime_config: &'static [RuntimeConfigDescriptor],
    ) -> Self {
        self.runtime_config = runtime_config;
        self
    }

    /// Attach a schema-admin data source (read access to admin entities).
    #[must_use]
    pub fn with_admin_data(mut self, data: Arc<dyn AdminDataSource>) -> Self {
        self.admin_data = Some(data);
        self
    }

    /// Attach admin action behavior for manifest-declared actions.
    #[must_use]
    pub fn with_admin_actions(mut self, actions: Arc<dyn AdminActionSource>) -> Self {
        self.admin_actions = Some(actions);
        self
    }
}
