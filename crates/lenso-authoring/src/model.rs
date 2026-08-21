use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::PathBuf,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Current version of the authoring project document.
pub const PROJECT_SCHEMA_VERSION: u32 = 1;
/// Current version of the authoring lock document embedded in a project.
pub const LOCK_SCHEMA_VERSION: u32 = 1;
/// Current version of the serialized resolved Plan document.
pub const RESOLVED_PLAN_SCHEMA_VERSION: u32 = 1;

fn default_project_schema() -> u32 {
    PROJECT_SCHEMA_VERSION
}

fn default_lock_schema() -> u32 {
    LOCK_SCHEMA_VERSION
}

fn default_configuration() -> Value {
    Value::Object(serde_json::Map::new())
}

/// Package-manager or artifact source selected by App authoring.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageSource {
    /// A statically linked Cargo/Rust Module package.
    #[default]
    Cargo,
    /// A Bun child-process Module package.
    Bun,
    /// An npm package executed by a Bun Adapter.
    Npm,
    /// An OCI-hosted artifact whose execution class is selected explicitly.
    Oci,
}

impl PackageSource {
    /// Returns the default official execution class for this package source.
    pub const fn default_execution_class(self) -> Option<&'static str> {
        match self {
            Self::Cargo => Some("lenso.native-rust@1"),
            Self::Bun | Self::Npm => Some("lenso.bun-process@1"),
            Self::Oci => None,
        }
    }

    /// Returns the stable source label embedded in a Resolved App Plan.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cargo => "cargo",
            Self::Bun => "bun",
            Self::Npm => "npm",
            Self::Oci => "oci",
        }
    }
}

impl fmt::Display for PackageSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One package-manager input owned by an App project.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PackageInput {
    name: String,
    source: PackageSource,
    version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    manifest: Option<String>,
}

impl PackageInput {
    /// Declares a package dependency without mutating a running App.
    pub fn new(name: impl Into<String>, source: PackageSource, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            source,
            version: version.into(),
            manifest: None,
        }
    }

    /// Associates the package input with a reviewable package-manager file.
    #[must_use]
    pub fn with_manifest(mut self, manifest: impl Into<String>) -> Self {
        self.manifest = Some(manifest.into());
        self
    }

    /// Returns the package identity.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Returns the selected package source.
    pub const fn source(&self) -> PackageSource {
        self.source
    }
    /// Returns the requested package version.
    pub fn version(&self) -> &str {
        &self.version
    }
    /// Returns the package-manager manifest path, when configured.
    pub fn manifest(&self) -> Option<&str> {
        self.manifest.as_deref()
    }
}

/// One exact package artifact recorded by package-manager lock state.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LockedPackage {
    package: String,
    source: PackageSource,
    version: String,
    artifact: String,
    digest: String,
}

impl LockedPackage {
    /// Creates one exact lock entry. The digest is normally `sha256:<hex>`.
    pub fn new(
        package: impl Into<String>,
        source: PackageSource,
        version: impl Into<String>,
        artifact: impl Into<String>,
        digest: impl Into<String>,
    ) -> Self {
        Self {
            package: package.into(),
            source,
            version: version.into(),
            artifact: artifact.into(),
            digest: digest.into(),
        }
    }

    /// Returns the package identity.
    pub fn package(&self) -> &str {
        &self.package
    }
    /// Returns the locked source.
    pub const fn source(&self) -> PackageSource {
        self.source
    }
    /// Returns the locked version.
    pub fn version(&self) -> &str {
        &self.version
    }
    /// Returns the exact artifact locator.
    pub fn artifact(&self) -> &str {
        &self.artifact
    }
    /// Returns the integrity digest.
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

/// Deterministic package-manager lock snapshot consumed by Plan resolution.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LockFile {
    #[serde(default = "default_lock_schema")]
    schema_version: u32,
    #[serde(default)]
    packages: BTreeMap<String, LockedPackage>,
}

impl Default for LockFile {
    fn default() -> Self {
        Self {
            schema_version: LOCK_SCHEMA_VERSION,
            packages: BTreeMap::new(),
        }
    }
}

impl LockFile {
    /// Adds or replaces one exact lock entry during an authoring operation.
    pub fn insert(&mut self, package: LockedPackage) {
        self.packages.insert(package.package().to_owned(), package);
    }

    /// Returns one exact lock entry.
    pub fn get(&self, package: &str) -> Option<&LockedPackage> {
        self.packages.get(package)
    }
    /// Returns all locked package entries in deterministic key order.
    pub fn packages(&self) -> &BTreeMap<String, LockedPackage> {
        &self.packages
    }
    /// Returns the lock schema version.
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }
}

/// A request, stream, or Event endpoint declared by a Module Instance.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CapabilityEndpoint {
    capability_id: String,
    descriptor_version: String,
    operations: Vec<String>,
    #[serde(default)]
    operation_kinds: BTreeMap<String, InteractionKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    admission: Option<RequestAdmission>,
    #[serde(default)]
    operation_admissions: BTreeMap<String, RequestAdmission>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    event_capacity: Option<usize>,
}

impl CapabilityEndpoint {
    /// Declares request Operations for one Capability endpoint.
    pub fn request(
        capability_id: impl Into<String>,
        descriptor_version: impl Into<String>,
        operations: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            capability_id: capability_id.into(),
            descriptor_version: descriptor_version.into(),
            operations: operations.into_iter().map(Into::into).collect(),
            ..Self::default()
        }
    }

    /// Declares one endpoint and marks all supplied Operations as streams.
    pub fn stream(
        capability_id: impl Into<String>,
        descriptor_version: impl Into<String>,
        operations: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        let mut endpoint = Self::request(capability_id, descriptor_version, operations);
        for operation in endpoint.operations.clone() {
            endpoint
                .operation_kinds
                .insert(operation, InteractionKind::Stream);
        }
        endpoint
    }

    /// Declares one endpoint and marks all supplied Operations as Events.
    pub fn event(
        capability_id: impl Into<String>,
        descriptor_version: impl Into<String>,
        operations: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        let mut endpoint = Self::request(capability_id, descriptor_version, operations);
        for operation in endpoint.operations.clone() {
            endpoint
                .operation_kinds
                .insert(operation, InteractionKind::Event);
        }
        endpoint
    }

    /// Sets one Operation's interaction kind.
    #[must_use]
    pub fn with_operation_kind(
        mut self,
        operation: impl Into<String>,
        kind: InteractionKind,
    ) -> Self {
        self.operation_kinds.insert(operation.into(), kind);
        self
    }

    /// Applies one bounded request policy to every request Operation.
    #[must_use]
    pub fn with_admission(mut self, admission: RequestAdmission) -> Self {
        self.admission = Some(admission);
        self
    }

    /// Applies one bounded Event mailbox capacity.
    #[must_use]
    pub fn with_event_capacity(mut self, capacity: usize) -> Self {
        self.event_capacity = Some(capacity);
        self
    }

    /// Returns the Capability identity.
    pub fn capability_id(&self) -> &str {
        &self.capability_id
    }
    /// Returns the exact Descriptor version.
    pub fn descriptor_version(&self) -> &str {
        &self.descriptor_version
    }
    /// Returns the declared Operations.
    pub fn operations(&self) -> &[String] {
        &self.operations
    }
    /// Returns the authored interaction kinds.
    pub fn operation_kinds(&self) -> &BTreeMap<String, InteractionKind> {
        &self.operation_kinds
    }
    /// Returns the endpoint-wide request policy.
    pub const fn admission(&self) -> Option<RequestAdmission> {
        self.admission
    }
    /// Returns Operation-specific request policies.
    pub fn operation_admissions(&self) -> &BTreeMap<String, RequestAdmission> {
        &self.operation_admissions
    }
    /// Returns the Event mailbox capacity.
    pub const fn event_capacity(&self) -> Option<usize> {
        self.event_capacity
    }
}

/// Transport-independent interaction kind in authoring data.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InteractionKind {
    /// One request produces one response.
    #[default]
    Request,
    /// One open establishes a bidirectional stream.
    Stream,
    /// One publication is delivered to subscribers.
    Event,
}

/// Bounded request queue and concurrency policy.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RequestAdmission {
    queue_capacity: usize,
    max_concurrency: usize,
}

impl RequestAdmission {
    /// Creates one request admission policy.
    pub const fn new(queue_capacity: usize, max_concurrency: usize) -> Self {
        Self {
            queue_capacity,
            max_concurrency,
        }
    }
    /// Returns the queue capacity.
    pub const fn queue_capacity(self) -> usize {
        self.queue_capacity
    }
    /// Returns the concurrency limit.
    pub const fn max_concurrency(self) -> usize {
        self.max_concurrency
    }
}

/// One Capability requirement declared by a Module Instance.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CapabilityRequirement {
    capability_id: String,
    descriptor_version: String,
    cardinality: Cardinality,
}

impl CapabilityRequirement {
    /// Declares an exactly-one requirement.
    pub fn one(capability_id: impl Into<String>, descriptor_version: impl Into<String>) -> Self {
        Self::new(capability_id, descriptor_version, Cardinality::One)
    }
    /// Declares an optional requirement.
    pub fn optional(
        capability_id: impl Into<String>,
        descriptor_version: impl Into<String>,
    ) -> Self {
        Self::new(capability_id, descriptor_version, Cardinality::Optional)
    }
    /// Declares a deterministic many-provider requirement.
    pub fn many(capability_id: impl Into<String>, descriptor_version: impl Into<String>) -> Self {
        Self::new(capability_id, descriptor_version, Cardinality::Many)
    }
    /// Declares a requirement with an explicit cardinality.
    pub fn new(
        capability_id: impl Into<String>,
        descriptor_version: impl Into<String>,
        cardinality: Cardinality,
    ) -> Self {
        Self {
            capability_id: capability_id.into(),
            descriptor_version: descriptor_version.into(),
            cardinality,
        }
    }
    /// Returns the Capability identity.
    pub fn capability_id(&self) -> &str {
        &self.capability_id
    }
    /// Returns the exact Descriptor version.
    pub fn descriptor_version(&self) -> &str {
        &self.descriptor_version
    }
    /// Returns the requirement cardinality.
    pub const fn cardinality(&self) -> Cardinality {
        self.cardinality
    }
}

/// Binding cardinality in App Composition.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Cardinality {
    One,
    Optional,
    Many,
}

/// One consumer-to-provider binding selected before Kernel boot.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Binding {
    consumer: String,
    capability_id: String,
    descriptor_version: String,
    provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    admission: Option<RequestAdmission>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    event_capacity: Option<usize>,
}

impl Binding {
    /// Binds a consumer requirement to one explicit provider Instance.
    pub fn new(
        consumer: impl Into<String>,
        capability_id: impl Into<String>,
        descriptor_version: impl Into<String>,
        provider: impl Into<String>,
    ) -> Self {
        Self {
            consumer: consumer.into(),
            capability_id: capability_id.into(),
            descriptor_version: descriptor_version.into(),
            provider: provider.into(),
            admission: None,
            event_capacity: None,
        }
    }
    /// Overrides request admission for this binding.
    #[must_use]
    pub fn with_admission(mut self, admission: RequestAdmission) -> Self {
        self.admission = Some(admission);
        self
    }
    /// Overrides Event mailbox capacity for this binding.
    #[must_use]
    pub fn with_event_capacity(mut self, capacity: usize) -> Self {
        self.event_capacity = Some(capacity);
        self
    }
    /// Returns the consumer Instance key.
    pub fn consumer(&self) -> &str {
        &self.consumer
    }
    /// Returns the Capability identity.
    pub fn capability_id(&self) -> &str {
        &self.capability_id
    }
    /// Returns the Descriptor version.
    pub fn descriptor_version(&self) -> &str {
        &self.descriptor_version
    }
    /// Returns the provider Instance key.
    pub fn provider(&self) -> &str {
        &self.provider
    }
    /// Returns an explicit request policy.
    pub const fn admission(&self) -> Option<RequestAdmission> {
        self.admission
    }
    /// Returns an explicit Event capacity.
    pub const fn event_capacity(&self) -> Option<usize> {
        self.event_capacity
    }
}

/// One Module Instance in App Composition.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Module {
    key: String,
    package: String,
    #[serde(default = "default_entrypoint")]
    entrypoint: String,
    #[serde(default = "default_configuration")]
    configuration: Value,
    #[serde(default)]
    provides: Vec<CapabilityEndpoint>,
    #[serde(default)]
    requires: Vec<CapabilityRequirement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    execution_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    configuration_schema: Option<String>,
}

fn default_entrypoint() -> String {
    "default".to_owned()
}

impl Module {
    /// Selects one package under an App-local Instance key.
    pub fn new(key: impl Into<String>, package: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            package: package.into(),
            entrypoint: default_entrypoint(),
            configuration: default_configuration(),
            provides: Vec::new(),
            requires: Vec::new(),
            execution_class: None,
            configuration_schema: None,
        }
    }
    /// Selects an explicit package entrypoint.
    #[must_use]
    pub fn with_entrypoint(mut self, entrypoint: impl Into<String>) -> Self {
        self.entrypoint = entrypoint.into();
        self
    }
    /// Supplies opaque non-secret Module configuration.
    #[must_use]
    pub fn with_configuration(mut self, configuration: Value) -> Self {
        self.configuration = configuration;
        self
    }
    /// Replaces Module configuration in an existing authoring document.
    pub fn set_configuration(&mut self, configuration: Value) {
        self.configuration = configuration;
    }
    /// Associates a JSON Schema used by `check` for configuration shape.
    #[must_use]
    pub fn with_configuration_schema(mut self, path: impl Into<String>) -> Self {
        self.configuration_schema = Some(path.into());
        self
    }
    /// Declares one provided Capability endpoint.
    #[must_use]
    pub fn with_capability(mut self, capability: CapabilityEndpoint) -> Self {
        self.provides.push(capability);
        self
    }
    /// Declares one required Capability.
    #[must_use]
    pub fn with_requirement(mut self, requirement: CapabilityRequirement) -> Self {
        self.requires.push(requirement);
        self
    }
    /// Selects an explicit Execution Adapter class.
    pub fn set_execution_class(&mut self, execution_class: impl Into<String>) {
        self.execution_class = Some(execution_class.into());
    }
    /// Returns the Instance key.
    pub fn key(&self) -> &str {
        &self.key
    }
    /// Returns the package identity.
    pub fn package(&self) -> &str {
        &self.package
    }
    /// Returns the entrypoint.
    pub fn entrypoint(&self) -> &str {
        &self.entrypoint
    }
    /// Returns Module configuration.
    pub fn configuration(&self) -> &Value {
        &self.configuration
    }
    /// Returns provided Capability endpoints.
    pub fn provides(&self) -> &[CapabilityEndpoint] {
        &self.provides
    }
    /// Returns required Capabilities.
    pub fn requires(&self) -> &[CapabilityRequirement] {
        &self.requires
    }
    /// Returns the selected Execution Adapter class, when explicit.
    pub fn execution_class(&self) -> Option<&str> {
        self.execution_class.as_deref()
    }
    /// Returns the optional configuration schema path.
    pub fn configuration_schema(&self) -> Option<&str> {
        self.configuration_schema.as_deref()
    }
}

/// Authoring recipe for selecting target-owned Web UI Modules before resolution.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct WebProfile {
    modules: Vec<String>,
}

impl WebProfile {
    /// Creates a named recipe containing explicit Module Instance keys.
    pub fn new(modules: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            modules: modules.into_iter().map(Into::into).collect(),
        }
    }
    /// Returns the selected Module Instance keys.
    pub fn modules(&self) -> &[String] {
        &self.modules
    }
}

/// One generated Capability contract whose checked-in artifacts must be fresh.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ContractInput {
    descriptor: String,
    rust: String,
    typescript: String,
}

impl ContractInput {
    /// Declares the descriptor and its generated Rust/TypeScript artifacts.
    pub fn new(
        descriptor: impl Into<String>,
        rust: impl Into<String>,
        typescript: impl Into<String>,
    ) -> Self {
        Self {
            descriptor: descriptor.into(),
            rust: rust.into(),
            typescript: typescript.into(),
        }
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
    lock: LockFile,
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
            lock: LockFile::default(),
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
    /// Returns package-manager lock state.
    pub fn lock(&self) -> &LockFile {
        &self.lock
    }
    /// Returns mutable lock state.
    pub fn lock_mut(&mut self) -> &mut LockFile {
        &mut self.lock
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

/// One requested `add` operation and its package-manager input.
#[derive(Clone, Debug)]
pub struct AddModule {
    module: Module,
    package: PackageInput,
}

impl AddModule {
    /// Creates one add request.
    pub fn new(module: Module, package: PackageInput) -> Self {
        Self { module, package }
    }
    /// Returns the Module to add.
    pub fn module(&self) -> &Module {
        &self.module
    }
    /// Returns the package input to add.
    pub fn package(&self) -> &PackageInput {
        &self.package
    }
}

/// Authoring checks that depend on the host's installed Execution Adapters.
#[derive(Clone, Debug)]
pub struct CheckOptions {
    available_execution_classes: BTreeSet<String>,
    verify_artifact_digests: bool,
}

impl Default for CheckOptions {
    fn default() -> Self {
        Self {
            available_execution_classes: ["lenso.native-rust@1".to_owned()].into_iter().collect(),
            verify_artifact_digests: true,
        }
    }
}

impl CheckOptions {
    /// Creates checks for an explicit host Adapter set.
    pub fn new(classes: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            available_execution_classes: classes.into_iter().map(Into::into).collect(),
            verify_artifact_digests: true,
        }
    }
    /// Adds one available Execution Adapter class.
    #[must_use]
    pub fn with_execution_class(mut self, class: impl Into<String>) -> Self {
        self.available_execution_classes.insert(class.into());
        self
    }
    /// Replaces the available Execution Adapter classes while preserving other checks.
    #[must_use]
    pub fn with_available_execution_classes(
        mut self,
        classes: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.available_execution_classes = classes.into_iter().map(Into::into).collect();
        self
    }
    /// Disables local artifact hashing for remote package locators.
    #[must_use]
    pub const fn without_artifact_digest_check(mut self) -> Self {
        self.verify_artifact_digests = false;
        self
    }
    /// Returns available Execution Adapter classes.
    pub fn available_execution_classes(&self) -> &BTreeSet<String> {
        &self.available_execution_classes
    }
    /// Returns whether local artifact digests are checked.
    pub const fn verify_artifact_digests(&self) -> bool {
        self.verify_artifact_digests
    }
}

/// Resolution options, including a selected authoring profile.
#[derive(Clone, Debug, Default)]
pub struct ResolutionOptions {
    profile: Option<String>,
    check: CheckOptions,
}

impl ResolutionOptions {
    /// Selects one profile to materialize before Plan resolution.
    #[must_use]
    pub fn with_profile(mut self, profile: impl Into<String>) -> Self {
        self.profile = Some(profile.into());
        self
    }
    /// Supplies host Execution Adapter availability checks.
    #[must_use]
    pub fn with_check_options(mut self, check: CheckOptions) -> Self {
        self.check = check;
        self
    }
    /// Returns the selected profile, if any.
    pub fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }
    /// Returns the host check options.
    pub fn check(&self) -> &CheckOptions {
        &self.check
    }
}

/// A serialized Plan plus the exact artifact and lock identity used to create it.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ResolvedPlanFile {
    /// Serialized Plan schema version.
    pub schema_version: u32,
    /// Digest of the canonical lock state.
    pub lock_digest: String,
    /// Deterministically ordered Module Instances and their artifacts.
    pub modules: Vec<ResolvedModule>,
    /// Deterministically ordered explicit bindings.
    pub bindings: Vec<Binding>,
}

/// One resolved Module Instance with its exact locked artifact.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ResolvedModule {
    /// App Composition data for the Module Instance.
    pub module: Module,
    /// Exact lock entry selected for the Module Instance.
    pub artifact: LockedPackage,
}

/// Result of materializing an immutable Plan.
#[derive(Clone, Debug)]
pub struct ResolvedProject {
    pub(crate) plan: lenso_app_plan::ResolvedAppPlan,
    pub(crate) document: ResolvedPlanFile,
    pub(crate) canonical_bytes: Vec<u8>,
}

impl ResolvedProject {
    /// Returns the typed immutable Plan passed to Kernel.
    pub fn plan(&self) -> &lenso_app_plan::ResolvedAppPlan {
        &self.plan
    }
    /// Returns the canonical serialized Plan document.
    pub fn document(&self) -> &ResolvedPlanFile {
        &self.document
    }
    /// Returns byte-stable canonical Plan bytes.
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }
    /// Returns the SHA-256 fingerprint of the canonical Plan document.
    pub fn fingerprint(&self) -> String {
        crate::sha256_bytes(&self.canonical_bytes)
    }
}

/// A path relative to a project document, used by the CLI and add workflow.
#[derive(Clone, Debug)]
pub struct ProjectPath {
    path: PathBuf,
}

impl ProjectPath {
    /// Creates a project handle for one JSON project document.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
    /// Returns the project document path.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}
