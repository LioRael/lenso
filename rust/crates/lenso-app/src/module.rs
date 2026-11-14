use crate::{
    Action, ActionDescription, ActionError, ActionMetadata, ErrorCode, ModuleMetadata,
    describe_action,
};

/// Installed operations derived from their actual native handles.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
pub struct Module {
    id: String,
    actions: Vec<String>,
    tasks: Vec<String>,
    events: Vec<String>,
    resources: Vec<String>,
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    metadata: Option<ModuleMetadata>,
    #[cfg_attr(
        feature = "json",
        serde(
            rename = "actionMetadata",
            skip_serializing_if = "std::collections::BTreeMap::is_empty"
        )
    )]
    action_metadata: std::collections::BTreeMap<String, ActionMetadata>,
}

impl Module {
    pub(crate) fn action_description(&mut self, description: ActionDescription) {
        if let Some(metadata) = description.metadata {
            self.action_metadata
                .insert(description.id.clone(), metadata);
        }
        self.actions.push(description.id);
    }
    /// Starts an explicitly assembled module; native handlers stay application-owned.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            actions: Vec::new(),
            tasks: Vec::new(),
            events: Vec::new(),
            resources: Vec::new(),
            metadata: None,
            action_metadata: std::collections::BTreeMap::new(),
        }
    }

    /// Describes an operation using its native action identifier.
    pub fn action<R, A: Action<R>>(&mut self, action: &A) -> &mut Self {
        self.action_description(describe_action(action));
        self
    }

    /// Adds optional public module annotations without changing native exports.
    pub fn with_metadata(mut self, metadata: ModuleMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Returns public metadata, or None for a legacy module.
    pub fn metadata(&self) -> Option<&ModuleMetadata> {
        self.metadata.as_ref()
    }

    /// Describes all installed Actions, marking undeclared metadata as unknown.
    pub fn describe_actions(&self) -> Vec<ActionDescription> {
        self.actions
            .iter()
            .map(|id| ActionDescription {
                id: id.clone(),
                metadata: self.action_metadata.get(id).cloned(),
            })
            .collect()
    }

    /// Declares a named resource requirement while retaining its native interface.
    pub fn resource(&mut self, id: impl Into<String>) -> &mut Self {
        self.resources.push(id.into());
        self
    }

    /// Declares a resource from its configured definition rather than repeating an identifier.
    pub fn requires<D: crate::Resource>(&mut self, definition: &D) -> &mut Self {
        self.resources.push(definition.id().to_owned());
        self
    }

    /// Declares a durable event supplied by the module.
    #[cfg(feature = "json")]
    pub fn durable_event<T>(&mut self, event: &crate::DurableEvent<T>) -> &mut Self {
        self.events.push(event.id().to_owned());
        self
    }

    /// Describes a durable operation using its native task identifier.
    #[cfg(feature = "json")]
    pub fn task<R, T: crate::Task<R>>(&mut self, task: &T) -> &mut Self {
        self.tasks.push(task.id().to_owned());
        self
    }

    /// Declares a process-local or persistent event supplied by the module.
    pub fn event<T>(&mut self, event: &crate::Event<T>) -> &mut Self {
        self.events.push(event.id().to_owned());
        self
    }

    /// Returns the stable module identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns installed action identifiers in declaration order.
    pub fn actions(&self) -> &[String] {
        &self.actions
    }
}

/// An introspection catalog that rejects conflicting module and operation identifiers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalog {
    modules: Vec<Module>,
}

impl Catalog {
    /// Creates an empty catalog without runtime discovery or global registration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Atomically installs a module description.
    ///
    /// # Errors
    /// Returns `conflict` for duplicate module, action, task or event identifiers.
    pub fn register(&mut self, module: Module) -> Result<(), ActionError> {
        use std::collections::HashSet;
        let all: Vec<_> = self
            .modules
            .iter()
            .chain(std::iter::once(&module))
            .collect();
        for ids in [
            all.iter()
                .map(|module| module.id.as_str())
                .collect::<Vec<_>>(),
            all.iter()
                .flat_map(|module| module.actions.iter().map(String::as_str))
                .collect(),
            all.iter()
                .flat_map(|module| module.tasks.iter().map(String::as_str))
                .collect(),
            all.iter()
                .flat_map(|module| module.events.iter().map(String::as_str))
                .collect(),
        ] {
            let mut seen = HashSet::new();
            if ids.into_iter().any(|id| !seen.insert(id)) {
                return Err(ActionError::new(
                    ErrorCode::Conflict,
                    "Duplicate module identifier",
                ));
            }
        }
        self.modules.push(module);
        Ok(())
    }

    /// Returns descriptions without constructing another execution registry.
    pub fn describe(&self) -> &[Module] {
        &self.modules
    }

    /// Shared Action descriptions for external consumers; this is not an execution registry.
    pub fn describe_actions(&self) -> Vec<ActionDescription> {
        self.modules
            .iter()
            .flat_map(Module::describe_actions)
            .collect()
    }

    /// Checks explicit resource requirements before the host starts serving requests.
    ///
    /// # Errors
    /// Returns `not_found` when an installed module requires an unavailable resource.
    pub fn require_resources<'a>(
        &self,
        available: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), ActionError> {
        use std::collections::HashSet;
        let provided: HashSet<_> = available.into_iter().collect();
        for resource in self.modules.iter().flat_map(|module| &module.resources) {
            if !provided.contains(resource.as_str()) {
                return Err(ActionError::new(
                    ErrorCode::NotFound,
                    format!("Missing resource: {resource}"),
                ));
            }
        }
        Ok(())
    }
}
