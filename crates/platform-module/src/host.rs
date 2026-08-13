use crate::{LinkedBinding, Module, ModuleManifest};
use platform_core::{AppContext, AppResult, Migration};
use std::any::{Any, TypeId};
use std::sync::Arc;

#[derive(Clone)]
pub struct HostContribution {
    type_id: TypeId,
    value: Arc<dyn Any + Send + Sync>,
}

impl std::fmt::Debug for HostContribution {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HostContribution")
            .field("type_id", &self.type_id)
            .finish_non_exhaustive()
    }
}

impl HostContribution {
    pub fn typed<T>(value: T) -> Self
    where
        T: Send + Sync + 'static,
    {
        Self {
            type_id: TypeId::of::<T>(),
            value: Arc::new(value),
        }
    }

    pub fn get<T>(&self) -> Option<&T>
    where
        T: Send + Sync + 'static,
    {
        (self.type_id == TypeId::of::<T>())
            .then(|| self.value.downcast_ref::<T>())
            .flatten()
    }
}

#[derive(Debug, Clone)]
pub struct HostLinkedModule {
    pub module_name: &'static str,
    pub manifest: fn() -> ModuleManifest,
    pub load: Option<fn(&AppContext) -> Module>,
    try_load: Option<fn(&AppContext) -> AppResult<Module>>,
    pub http_binding: Option<fn() -> LinkedBinding>,
    pub migrations: &'static [Migration],
    contributions: Vec<HostContribution>,
}

impl HostLinkedModule {
    #[must_use]
    pub fn manifest_only(
        module_name: &'static str,
        manifest: fn() -> ModuleManifest,
        migrations: &'static [Migration],
    ) -> Self {
        Self {
            module_name,
            manifest,
            load: None,
            try_load: None,
            http_binding: None,
            migrations,
            contributions: Vec::new(),
        }
    }

    #[must_use]
    pub fn linked(
        module_name: &'static str,
        manifest: fn() -> ModuleManifest,
        load: fn(&AppContext) -> Module,
        migrations: &'static [Migration],
    ) -> Self {
        Self {
            module_name,
            manifest,
            load: Some(load),
            try_load: None,
            http_binding: None,
            migrations,
            contributions: Vec::new(),
        }
    }

    /// Compose a linked Module whose context-bound setup can reject invalid
    /// deployment configuration without panicking.
    #[must_use]
    pub fn try_linked(
        module_name: &'static str,
        manifest: fn() -> ModuleManifest,
        load: fn(&AppContext) -> AppResult<Module>,
        migrations: &'static [Migration],
    ) -> Self {
        Self {
            module_name,
            manifest,
            load: None,
            try_load: Some(load),
            http_binding: None,
            migrations,
            contributions: Vec::new(),
        }
    }

    #[doc(hidden)]
    pub fn try_load_module(&self, context: &AppContext) -> AppResult<Module> {
        if let Some(load) = self.try_load {
            return load(context);
        }
        Ok(match self.load {
            Some(load) => load(context),
            None => Module::linked((self.manifest)(), LinkedBinding::builder().build()),
        })
    }

    #[must_use]
    pub fn with_http_binding(mut self, http_binding: fn() -> LinkedBinding) -> Self {
        self.http_binding = Some(http_binding);
        self
    }

    #[must_use]
    pub fn with_contribution<T>(mut self, contribution: T) -> Self
    where
        T: Send + Sync + 'static,
    {
        self.contributions
            .push(HostContribution::typed(contribution));
        self
    }

    pub fn contributions<T>(&self) -> impl Iterator<Item = &T>
    where
        T: Send + Sync + 'static,
    {
        self.contributions
            .iter()
            .filter_map(HostContribution::get::<T>)
    }
}
