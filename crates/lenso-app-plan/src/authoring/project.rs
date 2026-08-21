use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{Binding, Module, PackageInput};

/// Current version of the authoring project document.
pub const PROJECT_SCHEMA_VERSION: u32 = 1;

fn default_project_schema() -> u32 {
    PROJECT_SCHEMA_VERSION
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WebProfile {
    shell: String,
    browser_adapter: String,
    #[serde(default)]
    ui_contributions: Vec<String>,
    #[serde(default)]
    additional_modules: Vec<String>,
}

impl WebProfile {
    /// Creates a Web recipe with explicit Shell and Browser Adapter roles.
    pub fn new(shell: impl Into<String>, browser_adapter: impl Into<String>) -> Self {
        Self {
            shell: shell.into(),
            browser_adapter: browser_adapter.into(),
            ui_contributions: Vec::new(),
            additional_modules: Vec::new(),
        }
    }
    /// Adds an explicitly selected UI Contribution provider.
    #[must_use]
    pub fn with_ui_contribution(mut self, instance: impl Into<String>) -> Self {
        self.ui_contributions.push(instance.into());
        self
    }
    /// Adds an ordinary business or support Module to the recipe.
    #[must_use]
    pub fn with_module(mut self, instance: impl Into<String>) -> Self {
        self.additional_modules.push(instance.into());
        self
    }
    pub fn shell(&self) -> &str {
        &self.shell
    }
    pub fn browser_adapter(&self) -> &str {
        &self.browser_adapter
    }
    pub fn ui_contributions(&self) -> &[String] {
        &self.ui_contributions
    }
    pub fn selected_modules(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.shell())
            .chain(std::iter::once(self.browser_adapter()))
            .chain(self.ui_contributions.iter().map(String::as_str))
            .chain(self.additional_modules.iter().map(String::as_str))
    }
}

/// One generated Capability contract whose checked-in artifacts must be fresh.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ContractInput {
    capability_id: String,
    descriptor_version: String,
    descriptor: String,
    rust: String,
    typescript: String,
}

impl ContractInput {
    /// Declares the descriptor and its generated Rust/TypeScript artifacts.
    pub fn new(
        capability_id: impl Into<String>,
        descriptor_version: impl Into<String>,
        descriptor: impl Into<String>,
        rust: impl Into<String>,
        typescript: impl Into<String>,
    ) -> Self {
        Self {
            capability_id: capability_id.into(),
            descriptor_version: descriptor_version.into(),
            descriptor: descriptor.into(),
            rust: rust.into(),
            typescript: typescript.into(),
        }
    }
    pub fn capability_id(&self) -> &str {
        &self.capability_id
    }
    pub fn descriptor_version(&self) -> &str {
        &self.descriptor_version
    }
    /// Returns the descriptor path.
    pub fn descriptor(&self) -> &str {
        &self.descriptor
    }
    /// Returns the generated Rust path.
    pub fn rust(&self) -> &str {
        &self.rust
    }
    /// Returns the generated TypeScript path.
    pub fn typescript(&self) -> &str {
        &self.typescript
    }
}

/// Declarative App Composition stored in a project document.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CompositionFile {
    #[serde(default)]
    modules: Vec<Module>,
    #[serde(default)]
    bindings: Vec<Binding>,
}

impl CompositionFile {
    /// Adds one Module Instance to the authoring document.
    pub fn add_module(&mut self, module: Module) {
        self.modules.push(module);
    }
    /// Returns Module Instances in authored order.
    pub fn modules(&self) -> &[Module] {
        &self.modules
    }
    /// Returns mutable Module Instances for editing tooling/tests.
    pub fn modules_mut(&mut self) -> &mut Vec<Module> {
        &mut self.modules
    }
    /// Adds one explicit Capability binding.
    pub fn add_binding(&mut self, binding: Binding) {
        self.bindings.push(binding);
    }
    /// Returns authored bindings.
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }
    /// Returns mutable bindings for editing tooling/tests.
    pub fn bindings_mut(&mut self) -> &mut Vec<Binding> {
        &mut self.bindings
    }
}

/// Language-independent authoring project document.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ProjectFile {
    #[serde(default = "default_project_schema")]
    schema_version: u32,
    #[serde(default)]
    composition: CompositionFile,
    #[serde(default)]
    packages: BTreeMap<String, PackageInput>,
    #[serde(default)]
    contracts: Vec<ContractInput>,
    #[serde(default)]
    profiles: BTreeMap<String, WebProfile>,
}

impl Default for ProjectFile {
    fn default() -> Self {
        Self {
            schema_version: PROJECT_SCHEMA_VERSION,
            composition: CompositionFile::default(),
            packages: BTreeMap::new(),
            contracts: Vec::new(),
            profiles: BTreeMap::new(),
        }
    }
}

impl ProjectFile {
    /// Returns the project schema version.
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }
    /// Returns App Composition.
    pub fn composition(&self) -> &CompositionFile {
        &self.composition
    }
    /// Returns mutable App Composition.
    pub fn composition_mut(&mut self) -> &mut CompositionFile {
        &mut self.composition
    }
    /// Returns package inputs keyed by package identity.
    pub fn packages(&self) -> &BTreeMap<String, PackageInput> {
        &self.packages
    }
    /// Returns mutable package inputs.
    pub fn packages_mut(&mut self) -> &mut BTreeMap<String, PackageInput> {
        &mut self.packages
    }
    /// Returns checked-in generated contract inputs.
    pub fn contracts(&self) -> &[ContractInput] {
        &self.contracts
    }
    /// Returns mutable contract inputs.
    pub fn contracts_mut(&mut self) -> &mut Vec<ContractInput> {
        &mut self.contracts
    }
    /// Returns a named authoring profile.
    pub fn profile(&self, name: &str) -> Option<&WebProfile> {
        self.profiles.get(name)
    }
    /// Returns mutable Web profiles.
    pub fn profiles_mut(&mut self) -> &mut BTreeMap<String, WebProfile> {
        &mut self.profiles
    }
}
