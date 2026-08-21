use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::{self, Write as _},
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use lenso_app_plan::{
    AppComposition, CapabilityBinding as PlanBinding, CapabilityCardinality,
    CapabilityEndpointPlan, CapabilityOperationKind, CapabilityRequirementPlan, EventAdmissionPlan,
    ExecutionClassId, ModuleArtifact, ModuleInstancePlan, RequestAdmissionPlan, ResolvedAppPlan,
};
use lenso_kernel::{ExecutionAdapterCatalog, RuntimeDriver, TerminalOutcome};
use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::validation::validate_configuration;
use crate::{
    AddModule, Binding, CapabilityEndpoint, Cardinality, CheckOptions, InteractionKind,
    LOCK_SCHEMA_VERSION, LockedPackage, Module, PROJECT_SCHEMA_VERSION, PackageInput,
    PackageSource, ProjectFile, ProjectPath, RESOLVED_PLAN_SCHEMA_VERSION, ResolutionOptions,
    ResolvedModule, ResolvedPlanFile, ResolvedProject,
};

#[derive(Debug)]
pub enum AuthoringError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    UnsupportedProjectSchema {
        actual: u32,
    },
    UnsupportedLockSchema {
        actual: u32,
    },
    DuplicateModule {
        key: String,
    },
    MissingPackageInput {
        package: String,
    },
    MissingLockedPackage {
        package: String,
    },
    LockMismatch {
        package: String,
        detail: String,
    },
    InvalidDigest {
        package: String,
        digest: String,
    },
    ArtifactMismatch {
        package: String,
        detail: String,
    },
    SecretArtifactLocator {
        package: String,
    },
    UnavailableExecutionClass {
        instance: String,
        execution_class: String,
    },
    MissingEntrypoint {
        instance: String,
    },
    SecretValue {
        path: String,
    },
    InvalidConfiguration {
        path: String,
        detail: String,
    },
    Contract {
        path: PathBuf,
        detail: String,
    },
    Plan {
        detail: String,
    },
    InvalidProfile {
        profile: String,
        detail: String,
    },
    Runner {
        detail: String,
    },
}

impl fmt::Display for AuthoringError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::Json { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::UnsupportedProjectSchema { actual } => write!(
                formatter,
                "unsupported authoring project schema {actual}; expected {PROJECT_SCHEMA_VERSION}"
            ),
            Self::UnsupportedLockSchema { actual } => write!(
                formatter,
                "unsupported authoring lock schema {actual}; expected {LOCK_SCHEMA_VERSION}"
            ),
            Self::DuplicateModule { key } => write!(formatter, "duplicate Module Instance {key}"),
            Self::MissingPackageInput { package } => write!(
                formatter,
                "Module package {package} has no package-manager input"
            ),
            Self::MissingLockedPackage { package } => write!(
                formatter,
                "package {package} has no exact lock entry; refresh package-manager lock state"
            ),
            Self::LockMismatch { package, detail } => {
                write!(formatter, "package {package} lock mismatch: {detail}")
            }
            Self::InvalidDigest { package, digest } => write!(
                formatter,
                "package {package} has invalid integrity digest {digest}"
            ),
            Self::ArtifactMismatch { package, detail } => {
                write!(formatter, "package {package} artifact mismatch: {detail}")
            }
            Self::SecretArtifactLocator { package } => write!(
                formatter,
                "package {package} artifact locator contains credentials; use a credential-free locator"
            ),
            Self::UnavailableExecutionClass {
                instance,
                execution_class,
            } => write!(
                formatter,
                "Module Instance {instance} requires unavailable Execution Adapter {execution_class}"
            ),
            Self::MissingEntrypoint { instance } => write!(
                formatter,
                "Bun Module Instance {instance} needs a script entrypoint"
            ),
            Self::SecretValue { path } => write!(
                formatter,
                "configuration {path} contains a secret value; use a secret reference"
            ),
            Self::InvalidConfiguration { path, detail } => {
                write!(formatter, "invalid configuration {path}: {detail}")
            }
            Self::Contract { path, detail } => {
                write!(formatter, "contract {}: {detail}", path.display())
            }
            Self::Plan { detail } => {
                write!(formatter, "App Composition could not resolve: {detail}")
            }
            Self::InvalidProfile { profile, detail } => {
                write!(formatter, "invalid authoring profile {profile}: {detail}")
            }
            Self::Runner { detail } => {
                write!(formatter, "Runner rejected the resolved Plan: {detail}")
            }
        }
    }
}

impl std::error::Error for AuthoringError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Json { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct AddResult {
    changed_files: Vec<PathBuf>,
}

impl AddResult {
    pub fn changed_files(&self) -> &[PathBuf] {
        &self.changed_files
    }
}

pub fn sha256_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut value = String::with_capacity(7 + digest.len() * 2);
    value.push_str("sha256:");
    for byte in digest {
        let _ = write!(value, "{byte:02x}");
    }
    value
}

pub fn sha256_file(path: &Path) -> Result<String, AuthoringError> {
    Ok(sha256_bytes(&read_file(path)?))
}

fn read_file(path: &Path) -> Result<Vec<u8>, AuthoringError> {
    fs::read(path).map_err(|source| AuthoringError::Io {
        path: path.to_owned(),
        source,
    })
}

fn write_file(path: &Path, contents: &[u8]) -> Result<(), AuthoringError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| AuthoringError::Io {
            path: parent.to_owned(),
            source,
        })?;
    }
    fs::write(path, contents).map_err(|source| AuthoringError::Io {
        path: path.to_owned(),
        source,
    })
}

impl ProjectPath {
    pub fn load(path: &Path) -> Result<ProjectFile, AuthoringError> {
        serde_json::from_slice(&read_file(path)?).map_err(|source| AuthoringError::Json {
            path: path.to_owned(),
            source,
        })
    }

    pub fn add(&self, request: &AddModule) -> Result<AddResult, AuthoringError> {
        let original_project = read_file(self.path())?;
        let mut project = Self::load(self.path())?;
        add_module(&mut project, request)?;
        let root = self.path().parent().unwrap_or_else(|| Path::new("."));
        let mut changed_files = vec![self.path().to_owned()];
        let manifest_path = project
            .packages()
            .get(request.package().name())
            .and_then(PackageInput::manifest)
            .map(|manifest| root.join(manifest));
        if let Some(path) = &manifest_path
            && !path.is_file()
        {
            return Err(AuthoringError::Io {
                path: path.clone(),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "selected package manifest does not exist",
                ),
            });
        }
        let manifest_contents = manifest_path
            .as_deref()
            .map(|path| package_manifest_contents(path, request.package()))
            .transpose()?
            .flatten();
        write_file(self.path(), &canonical_pretty_json(&project))?;
        if let (Some(path), Some(contents)) = (manifest_path, manifest_contents) {
            if let Err(error) = write_file(&path, &contents) {
                let _ = write_file(self.path(), &original_project);
                return Err(error);
            }
            changed_files.push(path);
        }
        Ok(AddResult { changed_files })
    }
}

fn add_module(project: &mut ProjectFile, request: &AddModule) -> Result<(), AuthoringError> {
    if project
        .composition()
        .modules()
        .iter()
        .any(|module| module.key() == request.module().key())
    {
        return Err(AuthoringError::DuplicateModule {
            key: request.module().key().to_owned(),
        });
    }
    if request.module().package() != request.package().name() {
        return Err(AuthoringError::LockMismatch {
            package: request.package().name().to_owned(),
            detail: format!("Module selects {}", request.module().package()),
        });
    }
    if let Some(existing) = project.packages().get(request.package().name()) {
        if existing != request.package() {
            return Err(AuthoringError::LockMismatch {
                package: request.package().name().to_owned(),
                detail: "package input already has different authoring data".to_owned(),
            });
        }
    } else {
        project.packages_mut().insert(
            request.package().name().to_owned(),
            request.package().clone(),
        );
    }
    project
        .composition_mut()
        .add_module(request.module().clone());
    Ok(())
}

fn package_manifest_contents(
    path: &Path,
    package: &PackageInput,
) -> Result<Option<Vec<u8>>, AuthoringError> {
    match package.source() {
        PackageSource::Cargo => cargo_manifest_contents(path, package),
        PackageSource::Bun | PackageSource::Npm | PackageSource::Oci => {
            json_manifest_contents(path, package)
        }
    }
}

fn cargo_manifest_contents(
    path: &Path,
    package: &PackageInput,
) -> Result<Option<Vec<u8>>, AuthoringError> {
    let original =
        String::from_utf8(read_file(path)?).map_err(|error| AuthoringError::LockMismatch {
            package: package.name().to_owned(),
            detail: format!("Cargo manifest is not UTF-8: {error}"),
        })?;
    let package_name = package.name().to_owned();
    let dependency = format!("{package_name} = \"{}\"", package.version());
    let mut lines: Vec<String> = original.lines().map(ToOwned::to_owned).collect();
    let mut in_dependencies = false;
    let mut changed = false;
    let mut insert_at = None;
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_dependencies = trimmed == "[dependencies]";
        }
        if in_dependencies {
            if trimmed.starts_with(&format!("{package_name} =")) {
                if trimmed == dependency {
                    return Ok(None);
                }
                let right_hand_side = trimmed
                    .split_once('=')
                    .map_or("", |(_, value)| value.trim_start());
                if !is_simple_cargo_version(right_hand_side) {
                    return Err(AuthoringError::LockMismatch {
                        package: package_name.clone(),
                        detail: "existing Cargo dependency declaration is not a simple version"
                            .to_owned(),
                    });
                }
                lines[index].clone_from(&dependency);
                changed = true;
                break;
            }
            insert_at = Some(index + 1);
        }
    }
    if !changed {
        if let Some(index) = insert_at {
            lines.insert(index, dependency);
        } else {
            if !lines.is_empty() && lines.last().is_some_and(|line| !line.is_empty()) {
                lines.push(String::new());
            }
            lines.push("[dependencies]".to_owned());
            lines.push(dependency);
        }
        changed = true;
    }
    debug_assert!(changed);
    let mut rendered = lines.join("\n");
    rendered.push('\n');
    Ok(Some(rendered.into_bytes()))
}

fn is_simple_cargo_version(value: &str) -> bool {
    let Some(value) = value.strip_prefix('"') else {
        return false;
    };
    let Some(end) = value.find('"') else {
        return false;
    };
    value[end + 1..].trim().is_empty()
}

fn json_manifest_contents(
    path: &Path,
    package: &PackageInput,
) -> Result<Option<Vec<u8>>, AuthoringError> {
    let mut document: Value =
        serde_json::from_slice(&read_file(path)?).map_err(|source| AuthoringError::Json {
            path: path.to_owned(),
            source,
        })?;
    let object = document
        .as_object_mut()
        .ok_or_else(|| AuthoringError::LockMismatch {
            package: package.name().to_owned(),
            detail: "package manifest must be a JSON object".to_owned(),
        })?;
    let section = if package.source() == PackageSource::Oci {
        "lenso_oci"
    } else {
        "dependencies"
    };
    let dependencies = object
        .entry(section.to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    let dependencies =
        dependencies
            .as_object_mut()
            .ok_or_else(|| AuthoringError::LockMismatch {
                package: package.name().to_owned(),
                detail: format!("manifest field {section} must be an object"),
            })?;
    let old = dependencies.insert(
        package.name().to_owned(),
        Value::String(package.version().to_owned()),
    );
    if old.is_some_and(|value| value == Value::String(package.version().to_owned())) {
        return Ok(None);
    }
    sort_json_value(&mut document);
    Ok(Some(
        serde_json::to_vec_pretty(&document).expect("JSON values are serializable"),
    ))
}

impl ProjectFile {
    /// Checks project data, package locks, generated artifacts, configuration,
    /// execution classes, and explicit Capability bindings.
    pub fn check(
        &self,
        root: &Path,
        options: &CheckOptions,
    ) -> Result<CheckReport, AuthoringError> {
        self.validate_schema()?;
        self.check_contracts(root)?;
        let modules = self.selected_modules(None)?;
        self.check_packages(root, &modules, options)?;
        let composition = build_composition(self, &modules)?;
        composition
            .resolve()
            .map_err(|error| AuthoringError::Plan {
                detail: error.to_string(),
            })?;
        Ok(CheckReport {
            modules: modules.len(),
            bindings: self.composition().bindings().len(),
            contracts: self.contracts().len(),
            execution_classes: options.available_execution_classes().clone(),
        })
    }

    /// Resolves one deterministic immutable Plan from Composition and lock state.
    pub fn resolve(
        &self,
        root: &Path,
        options: &ResolutionOptions,
    ) -> Result<ResolvedProject, AuthoringError> {
        self.validate_schema()?;
        self.check_contracts(root)?;
        let modules = self.selected_modules(options.profile())?;
        self.check_packages(root, &modules, options.check())?;
        let composition = build_composition(self, &modules)?;
        let plan = composition
            .resolve()
            .map_err(|error| AuthoringError::Plan {
                detail: error.to_string(),
            })?;
        let document = resolved_document(self, &modules, &plan)?;
        let canonical_bytes = canonical_json_bytes(&document);
        Ok(ResolvedProject {
            plan,
            document,
            canonical_bytes,
        })
    }

    fn validate_schema(&self) -> Result<(), AuthoringError> {
        if self.schema_version() != PROJECT_SCHEMA_VERSION {
            return Err(AuthoringError::UnsupportedProjectSchema {
                actual: self.schema_version(),
            });
        }
        if self.lock().schema_version() != LOCK_SCHEMA_VERSION {
            return Err(AuthoringError::UnsupportedLockSchema {
                actual: self.lock().schema_version(),
            });
        }
        Ok(())
    }

    fn check_contracts(&self, root: &Path) -> Result<(), AuthoringError> {
        for contract in self.contracts() {
            let descriptor = root.join(contract.descriptor());
            let rust = root.join(contract.rust());
            let typescript = root.join(contract.typescript());
            lenso_contract_codegen::check_generated(&descriptor, &rust, &typescript).map_err(
                |error| AuthoringError::Contract {
                    path: descriptor,
                    detail: error.to_string(),
                },
            )?;
        }
        Ok(())
    }

    fn selected_modules(&self, profile: Option<&str>) -> Result<Vec<Module>, AuthoringError> {
        let mut modules = self.composition().modules().to_vec();
        let Some(profile_name) = profile else {
            return Ok(modules);
        };
        let Some(profile) = self.profile(profile_name) else {
            return Err(AuthoringError::InvalidProfile {
                profile: profile_name.to_owned(),
                detail: "profile is not defined".to_owned(),
            });
        };
        let selected: BTreeSet<_> = profile.modules().iter().map(String::as_str).collect();
        for key in &selected {
            if !modules.iter().any(|module| module.key() == *key) {
                return Err(AuthoringError::InvalidProfile {
                    profile: profile_name.to_owned(),
                    detail: format!("unknown Module Instance {key}"),
                });
            }
        }
        modules.retain(|module| selected.contains(module.key()));
        if modules.is_empty() {
            return Err(AuthoringError::InvalidProfile {
                profile: profile_name.to_owned(),
                detail: "profile selects no Module Instances".to_owned(),
            });
        }
        Ok(modules)
    }

    fn check_packages(
        &self,
        root: &Path,
        modules: &[Module],
        options: &CheckOptions,
    ) -> Result<(), AuthoringError> {
        for module in modules {
            let Some(input) = self.packages().get(module.package()) else {
                return Err(AuthoringError::MissingPackageInput {
                    package: module.package().to_owned(),
                });
            };
            let Some(locked) = self.lock().get(module.package()) else {
                return Err(AuthoringError::MissingLockedPackage {
                    package: module.package().to_owned(),
                });
            };
            if input.name() != module.package() || locked.package() != module.package() {
                return Err(AuthoringError::LockMismatch {
                    package: module.package().to_owned(),
                    detail: "package map key and embedded package identity disagree".to_owned(),
                });
            }
            if input.source() != locked.source() {
                return Err(AuthoringError::LockMismatch {
                    package: module.package().to_owned(),
                    detail: format!(
                        "source is {} in input and {} in lock",
                        input.source(),
                        locked.source()
                    ),
                });
            }
            if input.version() != locked.version() {
                return Err(AuthoringError::LockMismatch {
                    package: module.package().to_owned(),
                    detail: format!(
                        "version is {} in input and {} in lock",
                        input.version(),
                        locked.version()
                    ),
                });
            }
            validate_digest(locked)?;
            validate_artifact(root, locked, options.verify_artifact_digests())?;
            let execution_class = module
                .execution_class()
                .or_else(|| input.source().default_execution_class())
                .ok_or_else(|| AuthoringError::UnavailableExecutionClass {
                    instance: module.key().to_owned(),
                    execution_class: "<module-selected>".to_owned(),
                })?;
            if !options
                .available_execution_classes()
                .contains(execution_class)
            {
                return Err(AuthoringError::UnavailableExecutionClass {
                    instance: module.key().to_owned(),
                    execution_class: execution_class.to_owned(),
                });
            }
            if matches!(input.source(), PackageSource::Bun | PackageSource::Npm)
                && module.entrypoint() == "default"
            {
                return Err(AuthoringError::MissingEntrypoint {
                    instance: module.key().to_owned(),
                });
            }
            if matches!(input.source(), PackageSource::Bun | PackageSource::Npm) {
                validate_entrypoint(root, locked, module)?;
            }
            validate_configuration(root, module)?;
        }
        Ok(())
    }
}

/// Runs a resolved project through a caller-supplied immutable Adapter catalog.
pub async fn run_project<D: RuntimeDriver>(
    project: &ProjectFile,
    root: &Path,
    driver: D,
    adapters: ExecutionAdapterCatalog,
    shutdown_timeout: Duration,
    mut options: ResolutionOptions,
) -> Result<TerminalOutcome, AuthoringError> {
    let classes = adapters
        .execution_classes()
        .iter()
        .map(ToString::to_string)
        .collect::<BTreeSet<_>>();
    let check = options
        .check()
        .clone()
        .with_available_execution_classes(classes);
    options = options.with_check_options(check);
    let resolved = project.resolve(root, &options)?;
    lenso_runner::run(resolved.plan().clone(), driver, adapters, shutdown_timeout)
        .await
        .map_err(|error| AuthoringError::Runner {
            detail: format!("{error:?}"),
        })
}

fn validate_digest(package: &LockedPackage) -> Result<(), AuthoringError> {
    let digest = package.digest();
    let valid = digest
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()));
    if !valid {
        return Err(AuthoringError::InvalidDigest {
            package: package.package().to_owned(),
            digest: digest.to_owned(),
        });
    }
    Ok(())
}

fn validate_artifact(
    root: &Path,
    package: &LockedPackage,
    verify_digest: bool,
) -> Result<(), AuthoringError> {
    if package.artifact().contains("://") {
        if remote_locator_contains_credentials(package.artifact()) {
            return Err(AuthoringError::SecretArtifactLocator {
                package: package.package().to_owned(),
            });
        }
        return Ok(());
    }
    let path = root.join(package.artifact());
    if !path.is_file() {
        return Err(AuthoringError::ArtifactMismatch {
            package: package.package().to_owned(),
            detail: format!("local artifact {} does not exist", path.display()),
        });
    }
    if verify_digest {
        let actual = sha256_file(&path)?;
        if actual != package.digest() {
            return Err(AuthoringError::ArtifactMismatch {
                package: package.package().to_owned(),
                detail: format!("expected {}, found {actual}", package.digest()),
            });
        }
    }
    Ok(())
}

fn remote_locator_contains_credentials(locator: &str) -> bool {
    let Some(scheme_end) = locator.find("://") else {
        return false;
    };
    let authority = &locator[scheme_end + 3..];
    let authority_end = authority.find(['/', '?', '#']).unwrap_or(authority.len());
    authority[..authority_end].contains('@')
        || authority[authority_end..].contains('?')
        || authority[authority_end..].contains('#')
}

fn validate_entrypoint(
    root: &Path,
    package: &LockedPackage,
    module: &Module,
) -> Result<(), AuthoringError> {
    if package.artifact().contains("://") {
        return Ok(());
    }
    let artifact = root.join(package.artifact());
    let entrypoint = root.join(module.entrypoint());
    if !entrypoint.is_file() {
        return Err(AuthoringError::ArtifactMismatch {
            package: package.package().to_owned(),
            detail: format!("Bun entrypoint {} does not exist", entrypoint.display()),
        });
    }
    if artifact.is_file() && fs::canonicalize(&artifact).ok() != fs::canonicalize(&entrypoint).ok()
    {
        return Err(AuthoringError::ArtifactMismatch {
            package: package.package().to_owned(),
            detail: "Bun entrypoint is not the exact locked local artifact".to_owned(),
        });
    }
    Ok(())
}

fn build_composition(
    project: &ProjectFile,
    modules: &[Module],
) -> Result<AppComposition, AuthoringError> {
    let selected: BTreeSet<_> = modules.iter().map(Module::key).collect();
    let all_keys: BTreeSet<_> = project
        .composition()
        .modules()
        .iter()
        .map(Module::key)
        .collect();
    let mut instances = Vec::with_capacity(modules.len());
    for module in modules {
        let input = project.packages().get(module.package()).ok_or_else(|| {
            AuthoringError::MissingPackageInput {
                package: module.package().to_owned(),
            }
        })?;
        let locked = project.lock().get(module.package()).ok_or_else(|| {
            AuthoringError::MissingLockedPackage {
                package: module.package().to_owned(),
            }
        })?;
        let execution_class = module
            .execution_class()
            .or_else(|| input.source().default_execution_class())
            .ok_or_else(|| AuthoringError::UnavailableExecutionClass {
                instance: module.key().to_owned(),
                execution_class: "<module-selected>".to_owned(),
            })?;
        let artifact = ModuleArtifact::new(
            input.source().as_str(),
            locked.artifact(),
            locked.version(),
            locked.digest(),
        );
        let mut instance = ModuleInstancePlan::new(module.key(), module.package())
            .with_entrypoint(module.entrypoint())
            .with_configuration(canonical_json_string(module.configuration()))
            .with_execution_class(ExecutionClassId::new(execution_class))
            .with_artifact(artifact);
        for endpoint in module.provides() {
            instance = instance.with_capability(to_plan_endpoint(endpoint));
        }
        for requirement in module.requires() {
            instance = instance.with_requirement(CapabilityRequirementPlan::new(
                requirement.capability_id(),
                requirement.descriptor_version(),
                to_plan_cardinality(requirement.cardinality()),
            ));
        }
        instances.push(instance);
    }
    let mut bindings = Vec::new();
    for binding in project.composition().bindings() {
        if !all_keys.contains(binding.consumer()) || !all_keys.contains(binding.provider()) {
            return Err(AuthoringError::Plan {
                detail: format!(
                    "binding {} -> {} references an unknown Module Instance",
                    binding.consumer(),
                    binding.provider()
                ),
            });
        }
        if selected.contains(binding.consumer()) && selected.contains(binding.provider()) {
            let mut plan_binding = PlanBinding::new(
                binding.consumer(),
                binding.capability_id(),
                binding.descriptor_version(),
                binding.provider(),
            );
            if let Some(admission) = binding.admission() {
                plan_binding = plan_binding.with_admission(RequestAdmissionPlan::new(
                    admission.queue_capacity(),
                    admission.max_concurrency(),
                ));
            }
            if let Some(capacity) = binding.event_capacity() {
                plan_binding = plan_binding.with_event_admission(EventAdmissionPlan::new(capacity));
            }
            bindings.push(plan_binding);
        }
    }
    Ok(AppComposition::new(instances, bindings))
}

fn to_plan_cardinality(cardinality: Cardinality) -> CapabilityCardinality {
    match cardinality {
        Cardinality::One => CapabilityCardinality::One,
        Cardinality::Optional => CapabilityCardinality::Optional,
        Cardinality::Many => CapabilityCardinality::Many,
    }
}

fn to_plan_endpoint(endpoint: &CapabilityEndpoint) -> CapabilityEndpointPlan {
    let mut plan = CapabilityEndpointPlan::new(
        endpoint.capability_id(),
        endpoint.descriptor_version(),
        endpoint.operations(),
    );
    for (operation, kind) in endpoint.operation_kinds() {
        let kind = match kind {
            InteractionKind::Request => CapabilityOperationKind::Request,
            InteractionKind::Stream => CapabilityOperationKind::Stream,
            InteractionKind::Event => CapabilityOperationKind::Event,
        };
        plan = plan.with_operation_kind(operation, kind);
    }
    if let Some(admission) = endpoint.admission() {
        plan = plan.with_admission(RequestAdmissionPlan::new(
            admission.queue_capacity(),
            admission.max_concurrency(),
        ));
    }
    for (operation, admission) in endpoint.operation_admissions() {
        plan = plan.with_operation_admission(
            operation,
            RequestAdmissionPlan::new(admission.queue_capacity(), admission.max_concurrency()),
        );
    }
    if let Some(capacity) = endpoint.event_capacity() {
        plan = plan.with_event_admission(EventAdmissionPlan::new(capacity));
    }
    plan
}

fn resolved_document(
    project: &ProjectFile,
    modules: &[Module],
    plan: &ResolvedAppPlan,
) -> Result<ResolvedPlanFile, AuthoringError> {
    let mut by_key: BTreeMap<&str, &Module> = modules
        .iter()
        .map(|module| (module.key(), module))
        .collect();
    let mut resolved_modules = Vec::with_capacity(plan.module_instances().len());
    for instance in plan.module_instances() {
        let module =
            by_key
                .remove(instance.instance_key())
                .ok_or_else(|| AuthoringError::Plan {
                    detail: format!(
                        "resolved Instance {} is absent from Composition",
                        instance.instance_key()
                    ),
                })?;
        let locked = project.lock().get(module.package()).ok_or_else(|| {
            AuthoringError::MissingLockedPackage {
                package: module.package().to_owned(),
            }
        })?;
        let mut resolved_module = module.clone();
        resolved_module.set_configuration(canonical_value(module.configuration().clone()));
        resolved_module.set_execution_class(instance.execution_class().as_str());
        resolved_modules.push(ResolvedModule {
            module: resolved_module,
            artifact: locked.clone(),
        });
    }
    let bindings = plan
        .capability_bindings()
        .iter()
        .map(|binding| {
            let mut resolved = Binding::new(
                binding.consumer_instance(),
                binding.capability_id(),
                binding.descriptor_version(),
                binding.provider_instance(),
            );
            if binding.has_explicit_admission() {
                let admission = binding.admission();
                resolved = resolved.with_admission(crate::RequestAdmission::new(
                    admission.queue_capacity(),
                    admission.max_concurrency(),
                ));
            }
            if binding.has_explicit_event_admission() {
                resolved = resolved.with_event_capacity(binding.event_admission().capacity());
            }
            resolved
        })
        .collect();
    Ok(ResolvedPlanFile {
        schema_version: RESOLVED_PLAN_SCHEMA_VERSION,
        lock_digest: sha256_bytes(&canonical_json_bytes(project.lock())),
        modules: resolved_modules,
        bindings,
    })
}

fn canonical_pretty_json<T: Serialize>(value: &T) -> Vec<u8> {
    let mut value = serde_json::to_value(value).expect("authoring values are serializable");
    sort_json_value(&mut value);
    let mut bytes = serde_json::to_vec_pretty(&value).expect("authoring values are serializable");
    bytes.push(b'\n');
    bytes
}

fn canonical_json_bytes<T: Serialize>(value: &T) -> Vec<u8> {
    serde_json::to_vec(&canonical_value(
        serde_json::to_value(value).expect("authoring values are serializable"),
    ))
    .expect("authoring values are serializable")
}

fn canonical_json_string(value: &Value) -> String {
    serde_json::to_string(&canonical_value(value.clone())).expect("JSON values are serializable")
}

fn canonical_value(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(canonical_value).collect()),
        Value::Object(object) => {
            let mut sorted = BTreeMap::new();
            for (key, value) in object {
                sorted.insert(key, canonical_value(value));
            }
            Value::Object(sorted.into_iter().collect())
        }
        value => value,
    }
}

fn sort_json_value(value: &mut Value) {
    *value = canonical_value(value.take());
}

/// A successful authoring check summary.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CheckReport {
    /// Number of Module Instances checked.
    pub modules: usize,
    /// Number of explicit bindings checked.
    pub bindings: usize,
    /// Number of generated contract inputs checked.
    pub contracts: usize,
    /// Available host Execution Adapter classes.
    pub execution_classes: BTreeSet<String>,
}
