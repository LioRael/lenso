//! A loaded module: serializable manifest + behavior binding + internal config.

use crate::binding::ModuleBinding;
use crate::linked::LinkedBinding;
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
    pub runtime_config: &'static [RuntimeConfigDescriptor],
}

impl Module {
    /// Build a compile-time (Linked) module from a manifest + linked behavior.
    /// Config defaults to empty; attach it with [`Module::with_runtime_config`].
    #[must_use]
    pub fn linked(manifest: ModuleManifest, binding: LinkedBinding) -> Self {
        Self {
            manifest,
            binding: Arc::new(binding),
            runtime_config: &[],
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
}
