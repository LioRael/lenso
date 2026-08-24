use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{Binding, Module, PackageInput};

/// One App-local Execution Lane declared by App Composition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExecutionLane {
    id: String,
}

impl ExecutionLane {
    /// Declares one single-owner Kernel lane.
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }

    /// Returns the App-local lane identity.
    pub fn id(&self) -> &str {
        &self.id
    }
}

fn default_execution_lanes() -> Vec<ExecutionLane> {
    vec![ExecutionLane::new("main")]
}

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

/// One Capability Descriptor and any language projections owned by this project.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ContractInput {
    capability_id: String,
    descriptor_version: String,
    descriptor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rust: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    typescript: Option<String>,
}

impl ContractInput {
    /// Declares the descriptor and both generated language projections.
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
            rust: Some(rust.into()),
            typescript: Some(typescript.into()),
        }
    }
    /// Declares a Descriptor without claiming ownership of a language projection.
    pub fn descriptor_only(
        capability_id: impl Into<String>,
        descriptor_version: impl Into<String>,
        descriptor: impl Into<String>,
    ) -> Self {
        Self {
            capability_id: capability_id.into(),
            descriptor_version: descriptor_version.into(),
            descriptor: descriptor.into(),
            rust: None,
            typescript: None,
        }
    }
    /// Adds the generated Rust projection owned by this project.
    #[must_use]
    pub fn with_rust_projection(mut self, path: impl Into<String>) -> Self {
        self.rust = Some(path.into());
        self
    }
    /// Adds the generated TypeScript projection owned by this project.
    #[must_use]
    pub fn with_typescript_projection(mut self, path: impl Into<String>) -> Self {
        self.typescript = Some(path.into());
        self
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
    /// Returns the generated Rust path, or an empty string when it is not owned here.
    ///
    /// New callers should prefer [`Self::rust_projection`] so absence remains typed.
    pub fn rust(&self) -> &str {
        self.rust.as_deref().unwrap_or_default()
    }
    /// Returns the generated Rust path when this project owns that projection.
    pub fn rust_projection(&self) -> Option<&str> {
        self.rust.as_deref()
    }
    /// Returns the generated TypeScript path, or an empty string when it is not owned here.
    ///
    /// New callers should prefer [`Self::typescript_projection`] so absence remains typed.
    pub fn typescript(&self) -> &str {
        self.typescript.as_deref().unwrap_or_default()
    }
    /// Returns the generated TypeScript path when this project owns that projection.
    pub fn typescript_projection(&self) -> Option<&str> {
        self.typescript.as_deref()
    }
}

/// Declarative App Composition stored in a project document.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CompositionFile {
    #[serde(default)]
    modules: Vec<Module>,
    #[serde(default)]
    bindings: Vec<Binding>,
    #[serde(default = "default_execution_lanes")]
    execution_lanes: Vec<ExecutionLane>,
}

impl Default for CompositionFile {
    fn default() -> Self {
        Self {
            modules: Vec::new(),
            bindings: Vec::new(),
            execution_lanes: default_execution_lanes(),
        }
    }
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
    /// Adds one App-local Execution Lane.
    pub fn add_execution_lane(&mut self, lane: ExecutionLane) {
        if self.execution_lanes.len() == 1 && self.execution_lanes[0].id() == "main" {
            self.execution_lanes.clear();
        }
        self.execution_lanes.push(lane);
    }
    /// Returns authored Execution Lanes.
    pub fn execution_lanes(&self) -> &[ExecutionLane] {
        &self.execution_lanes
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

#[cfg(test)]
mod tests {
    use super::ContractInput;

    #[test]
    fn contract_inputs_can_own_only_one_language_projection() {
        let contract = ContractInput::descriptor_only(
            "example.greeting@1",
            "1.0.0",
            "contract/capability.json",
        )
        .with_rust_projection("contract/src/generated.rs");

        assert_eq!(
            contract.rust_projection(),
            Some("contract/src/generated.rs")
        );
        assert_eq!(contract.typescript_projection(), None);

        let value = serde_json::to_value(&contract).expect("contract should serialize");
        assert_eq!(value["rust"], "contract/src/generated.rs");
        assert!(value.get("typescript").is_none());
    }

    #[test]
    fn descriptor_only_documents_remain_valid_contract_inputs() {
        let contract: ContractInput = serde_json::from_value(serde_json::json!({
            "capability_id": "example.greeting@1",
            "descriptor_version": "1.0.0",
            "descriptor": "contract/capability.json"
        }))
        .expect("language projections should be optional");

        assert_eq!(contract.rust_projection(), None);
        assert_eq!(contract.typescript_projection(), None);
    }
}
