use crate::{ActionError, Catalog, ErrorCode, Module};

#[cfg(feature = "json")]
type Invoke<R> = Box<
    dyn for<'a> Fn(
            &'a dyn crate::Observer,
            &'a crate::Context<R>,
            serde_json::Value,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<serde_json::Value, ActionError>>
                    + Send
                    + 'a,
            >,
        > + Send
        + Sync,
>;

/// Installed page bindings whose metadata and authorized operations share one definition.
#[cfg(feature = "json")]
pub struct ConsoleExtension<R> {
    pages: Vec<ConsolePage>,
    operations: Vec<(String, Invoke<R>)>,
}

#[cfg(feature = "json")]
impl<R> std::fmt::Debug for ConsoleExtension<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConsoleExtension")
            .field("pages", &self.pages)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "json")]
impl<R> Default for ConsoleExtension<R> {
    fn default() -> Self {
        Self {
            pages: Vec::new(),
            operations: Vec::new(),
        }
    }
}

#[cfg(feature = "json")]
impl<R: Sync + 'static> ConsoleExtension<R> {
    /// Creates an empty extension; registration does not execute any operation.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers an authorized operation without offering it as a page control.
    ///
    /// # Errors
    /// Returns `conflict` for an already registered operation id.
    pub fn register<A>(&mut self, action: A) -> Result<&mut Self, ActionError>
    where
        A: crate::Action<R> + Send + 'static,
        A::Input: serde::de::DeserializeOwned,
        A::Output: serde::Serialize,
    {
        self.bind(action)?;
        Ok(self)
    }

    /// Installs an independently authored extension, preserving its declaration order.
    ///
    /// # Errors
    /// Returns `conflict` without changing this extension if a page or operation already exists.
    pub fn merge(&mut self, other: Self) -> Result<&mut Self, ActionError> {
        if other
            .pages
            .iter()
            .any(|page| self.pages.iter().any(|installed| installed.id == page.id))
            || other
                .operations
                .iter()
                .any(|(id, _)| self.operations.iter().any(|(installed, _)| installed == id))
        {
            return Err(ActionError::new(
                ErrorCode::Conflict,
                "Console extension conflict",
            ));
        }
        self.pages.extend(other.pages);
        self.operations.extend(other.operations);
        Ok(self)
    }

    fn bind<A>(&mut self, action: A) -> Result<String, ActionError>
    where
        A: crate::Action<R> + Send + 'static,
        A::Input: serde::de::DeserializeOwned,
        A::Output: serde::Serialize,
    {
        let id = action.id().to_owned();
        if self.operations.iter().any(|(key, _)| key == &id) {
            return Err(ActionError::new(
                ErrorCode::Conflict,
                "Duplicate Console action",
            ));
        }
        let action = std::sync::Arc::new(action);
        self.operations.push((
            id.clone(),
            Box::new(move |observer, context, input| {
                let action = std::sync::Arc::clone(&action);
                Box::pin(async move {
                    let executor = crate::Executor::new(move |record| observer.record(record));
                    executor.call_json(action.as_ref(), context, input).await
                })
            }),
        ));
        Ok(id)
    }

    /// Defines a page and its list binding from the actual application action.
    ///
    /// # Errors
    /// Returns `conflict` for duplicate page or operation identifiers.
    pub fn page<A>(
        &mut self,
        id: impl Into<String>,
        title: impl Into<String>,
        list: A,
    ) -> Result<&mut Self, ActionError>
    where
        A: crate::Action<R> + Send + 'static,
        A::Input: serde::de::DeserializeOwned,
        A::Output: serde::Serialize,
    {
        let id = id.into();
        if self.pages.iter().any(|page| page.id == id) {
            return Err(ActionError::new(
                ErrorCode::Conflict,
                "Duplicate Console page",
            ));
        }
        let list = self.bind(list)?;
        self.pages.push(ConsolePage {
            id,
            title: title.into(),
            list,
            detail: None,
            actions: Vec::new(),
        });
        Ok(self)
    }

    /// Adds a detail binding to an installed page.
    ///
    /// # Errors
    /// Returns `not_found` for a missing page or `conflict` for an existing detail or operation.
    pub fn detail<A>(&mut self, page: &str, action: A) -> Result<&mut Self, ActionError>
    where
        A: crate::Action<R> + Send + 'static,
        A::Input: serde::de::DeserializeOwned,
        A::Output: serde::Serialize,
    {
        let index = self
            .pages
            .iter()
            .position(|item| item.id == page)
            .ok_or_else(|| ActionError::new(ErrorCode::NotFound, "Console page not found"))?;
        if self.pages[index].detail.is_some() {
            return Err(ActionError::new(
                ErrorCode::Conflict,
                "Console detail already installed",
            ));
        }
        let id = self.bind(action)?;
        self.pages[index].detail = Some(id);
        Ok(self)
    }

    /// Adds an existing management action, retaining its own validation and Access checks.
    ///
    /// # Errors
    /// Returns `not_found` for a missing page or `conflict` for duplicate operations.
    pub fn operation<A>(&mut self, page: &str, action: A) -> Result<&mut Self, ActionError>
    where
        A: crate::Action<R> + Send + 'static,
        A::Input: serde::de::DeserializeOwned,
        A::Output: serde::Serialize,
    {
        let index = self
            .pages
            .iter()
            .position(|item| item.id == page)
            .ok_or_else(|| ActionError::new(ErrorCode::NotFound, "Console page not found"))?;
        let id = self.bind(action)?;
        self.pages[index].actions.push(id);
        Ok(self)
    }

    /// Derives module operations from installed bindings instead of another handler directory.
    pub fn module(&self, id: impl Into<String>) -> Module {
        let mut module = Module::new(id);
        for (id, _) in &self.operations {
            module.action_id(id.clone());
        }
        module
    }

    /// Validates installed page references against the application's catalog.
    ///
    /// # Errors
    /// Returns manifest conflicts or references to actions absent from the catalog.
    pub fn manifest(&self, catalog: &Catalog) -> Result<ConsoleManifest, ActionError> {
        ConsoleManifest::with_operations(
            catalog,
            self.pages.clone(),
            self.operations.iter().map(|(id, _)| id.as_str()),
        )
    }

    /// Calls an installed operation through the caller's ordinary executor and record sink.
    ///
    /// # Errors
    /// Returns `not_found` for unknown bindings or the action's decoding, authorization and execution errors.
    pub async fn call<O: crate::Observer>(
        &self,
        executor: &crate::Executor<O>,
        context: &crate::Context<R>,
        id: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ActionError> {
        let (_, operation) = self
            .operations
            .iter()
            .find(|(key, _)| key == id)
            .ok_or_else(|| ActionError::new(ErrorCode::NotFound, "Management action not found"))?;
        operation(executor.observer(), context, input).await
    }
}

/// References existing authorized operations rather than declaring new execution handlers.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
pub struct ConsolePage {
    /// Stable identifier used by the Console frontend.
    pub id: String,
    /// Human-readable navigation label.
    pub title: String,
    /// Action returning list data.
    pub list: String,
    /// Optional action returning detail data.
    pub detail: Option<String>,
    /// Existing operations offered as management controls.
    pub actions: Vec<String>,
}

/// Versioned description exposed only through an authorized management endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
pub struct ConsoleManifest {
    /// External description format version, independent of SDK package versions.
    pub version: u32,
    /// Installed modules and operations.
    pub modules: Vec<Module>,
    /// Management pages referencing the installed operations.
    pub pages: Vec<ConsolePage>,
}

impl ConsoleManifest {
    /// Validates that pages reference actual installed operations.
    ///
    /// # Errors
    /// Returns `conflict` for duplicate pages or `not_found` for unknown actions.
    pub fn new(catalog: &Catalog, pages: Vec<ConsolePage>) -> Result<Self, ActionError> {
        Self::with_operations(catalog, pages, std::iter::empty())
    }

    fn with_operations<'a>(
        catalog: &Catalog,
        pages: Vec<ConsolePage>,
        operations: impl IntoIterator<Item = &'a str>,
    ) -> Result<Self, ActionError> {
        use std::collections::HashSet;
        let actions: HashSet<_> = catalog
            .describe()
            .iter()
            .flat_map(Module::actions)
            .map(String::as_str)
            .collect();
        let mut ids = HashSet::new();
        for page in &pages {
            if !ids.insert(&page.id) {
                return Err(ActionError::new(
                    ErrorCode::Conflict,
                    "Duplicate Console page",
                ));
            }
            for action in std::iter::once(&page.list)
                .chain(page.detail.iter())
                .chain(page.actions.iter())
            {
                if !actions.contains(action.as_str()) {
                    return Err(ActionError::new(
                        ErrorCode::NotFound,
                        format!("Unknown Console action: {action}"),
                    ));
                }
            }
        }
        for action in operations {
            if !actions.contains(action) {
                return Err(ActionError::new(
                    ErrorCode::NotFound,
                    format!("Unknown Console action: {action}"),
                ));
            }
        }
        Ok(Self {
            version: 1,
            modules: catalog.describe().to_owned(),
            pages,
        })
    }
}
