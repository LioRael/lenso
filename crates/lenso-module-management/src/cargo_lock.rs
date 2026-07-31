use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const CARGO_LOCK_CANDIDATE_PROTOCOL: &str = "lenso.cargo-lock-candidate.v1";
static TEMP_SANDBOX_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoLockResolutionRequest {
    pub read_set: BTreeMap<String, Vec<u8>>,
    pub candidate_files: BTreeMap<String, Vec<u8>>,
    pub root_manifest_path: String,
    pub lock_path: String,
    pub allowed_root_packages: Vec<String>,
    pub current_linked_packages: Vec<ExpectedLinkedPackage>,
    pub expected_linked_packages: Vec<ExpectedLinkedPackage>,
    pub offline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedLinkedPackage {
    pub package: String,
    pub version: String,
    pub archive_checksum: Option<String>,
    pub default_features: bool,
    pub features: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CargoLockCandidate {
    pub protocol: String,
    pub current_lock_digest: String,
    pub candidate_lock_digest: String,
    pub changed_packages: Vec<CargoPackageChange>,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CargoPackageChange {
    pub package: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_version: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_features: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidate_features: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoLockResolution {
    pub candidate_lock: Vec<u8>,
    pub evidence: CargoLockCandidate,
}

#[derive(Debug, thiserror::Error)]
pub enum CargoLockResolutionError {
    #[error("invalid isolated Cargo path `{path}`")]
    InvalidPath { path: String },
    #[error("required isolated Cargo input `{path}` is missing")]
    MissingInput { path: String },
    #[error("failed to prepare isolated Cargo workspace: {0}")]
    Io(#[from] std::io::Error),
    #[error("isolated Cargo resolution failed: {stderr}")]
    CommandFailed { stderr: String },
    #[error("Cargo.lock is invalid: {message}")]
    InvalidLock { message: String },
    #[error("Cargo.lock changed package `{package}` outside the approved module closure")]
    UnrelatedPackageChurn { package: String },
    #[error("Cargo.lock package `{package}` does not match the verified release: {message}")]
    PackageProvenanceMismatch { package: String, message: String },
}

pub trait CargoLockGenerator: Send + Sync {
    fn generate(
        &self,
        sandbox: &Path,
        manifest_path: &Path,
        offline: bool,
    ) -> Result<Vec<String>, CargoLockResolutionError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CargoGenerateLockfile;

impl CargoLockGenerator for CargoGenerateLockfile {
    fn generate(
        &self,
        sandbox: &Path,
        manifest_path: &Path,
        offline: bool,
    ) -> Result<Vec<String>, CargoLockResolutionError> {
        let stable_manifest_path = manifest_path.strip_prefix(sandbox).map_err(|_| {
            CargoLockResolutionError::InvalidPath {
                path: manifest_path.display().to_string(),
            }
        })?;
        let mut command = vec![
            "cargo".to_owned(),
            "generate-lockfile".to_owned(),
            "--manifest-path".to_owned(),
            stable_manifest_path.display().to_string(),
        ];
        if offline {
            command.push("--offline".to_owned());
        }
        let mut process = Command::new(&command[0]);
        process.args(&command[1..]).current_dir(sandbox);
        let output = process.output()?;
        if !output.status.success() {
            return Err(CargoLockResolutionError::CommandFailed {
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            });
        }
        Ok(command)
    }
}

#[derive(Debug, Clone)]
pub struct IsolatedCargoLockResolver<G = CargoGenerateLockfile> {
    generator: G,
}

impl Default for IsolatedCargoLockResolver<CargoGenerateLockfile> {
    fn default() -> Self {
        Self {
            generator: CargoGenerateLockfile,
        }
    }
}

impl<G> IsolatedCargoLockResolver<G>
where
    G: CargoLockGenerator,
{
    pub fn new(generator: G) -> Self {
        Self { generator }
    }

    pub fn resolve(
        &self,
        request: &CargoLockResolutionRequest,
    ) -> Result<CargoLockResolution, CargoLockResolutionError> {
        validate_relative_path(&request.root_manifest_path)?;
        validate_relative_path(&request.lock_path)?;
        let current_lock = request.read_set.get(&request.lock_path).ok_or_else(|| {
            CargoLockResolutionError::MissingInput {
                path: request.lock_path.clone(),
            }
        })?;
        if !request.read_set.contains_key(&request.root_manifest_path)
            && !request
                .candidate_files
                .contains_key(&request.root_manifest_path)
        {
            return Err(CargoLockResolutionError::MissingInput {
                path: request.root_manifest_path.clone(),
            });
        }

        let sandbox = TempSandbox::create()?;
        materialize(&sandbox.path, &request.read_set)?;
        materialize(&sandbox.path, &request.candidate_files)?;
        let manifest_path = sandbox.path.join(&request.root_manifest_path);
        let command = self
            .generator
            .generate(&sandbox.path, &manifest_path, request.offline)?;
        let candidate_lock = fs::read(sandbox.path.join(&request.lock_path))?;
        let evidence = validate_cargo_lock_candidate(
            current_lock,
            &candidate_lock,
            &request.allowed_root_packages,
            &request.current_linked_packages,
            &request.expected_linked_packages,
            command,
        )?;
        Ok(CargoLockResolution {
            candidate_lock,
            evidence,
        })
    }
}

pub fn validate_cargo_lock_candidate(
    current_lock: &[u8],
    candidate_lock: &[u8],
    allowed_root_packages: &[String],
    current_linked_packages: &[ExpectedLinkedPackage],
    expected_linked_packages: &[ExpectedLinkedPackage],
    command: Vec<String>,
) -> Result<CargoLockCandidate, CargoLockResolutionError> {
    let current = ParsedLock::parse(current_lock)?;
    let candidate = ParsedLock::parse(candidate_lock)?;
    let changed_keys = changed_package_keys(&current, &candidate);
    let current_features = feature_selections(current_linked_packages)?;
    let candidate_features = feature_selections(expected_linked_packages)?;
    let allowed = dependency_closure(&current, allowed_root_packages)
        .into_iter()
        .chain(dependency_closure(&candidate, allowed_root_packages))
        .collect::<BTreeSet<_>>();
    if let Some(package) = changed_keys
        .iter()
        .map(|key| key.name.as_str())
        .find(|package| !allowed.contains(*package))
    {
        return Err(CargoLockResolutionError::UnrelatedPackageChurn {
            package: package.to_owned(),
        });
    }
    if let Some(package) = current_features
        .keys()
        .chain(candidate_features.keys())
        .find(|package| {
            current_features.get(*package) != candidate_features.get(*package)
                && !allowed.contains(*package)
        })
    {
        return Err(CargoLockResolutionError::UnrelatedPackageChurn {
            package: package.clone(),
        });
    }

    for expected in expected_linked_packages {
        let matches = candidate
            .packages
            .iter()
            .filter(|(key, _)| key.name == expected.package && key.version == expected.version)
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(CargoLockResolutionError::PackageProvenanceMismatch {
                package: expected.package.clone(),
                message: format!(
                    "expected exactly one {}@{}, found {}",
                    expected.package,
                    expected.version,
                    matches.len()
                ),
            });
        }
        if let Some(expected_checksum) = &expected.archive_checksum {
            let checksum = matches[0].1.checksum.as_deref().unwrap_or_default();
            if normalize_checksum(checksum) != normalize_checksum(expected_checksum) {
                return Err(CargoLockResolutionError::PackageProvenanceMismatch {
                    package: expected.package.clone(),
                    message: "registry checksum differs from the verified archive checksum"
                        .to_owned(),
                });
            }
        }
    }

    let changed_names = changed_keys
        .iter()
        .map(|key| key.name.clone())
        .chain(
            current_features
                .keys()
                .chain(candidate_features.keys())
                .filter(|package| {
                    current_features.get(*package) != candidate_features.get(*package)
                })
                .cloned(),
        )
        .collect::<BTreeSet<_>>();
    let changed_packages = changed_names
        .into_iter()
        .map(|package| CargoPackageChange {
            previous_version: versions_for(&current, &package),
            candidate_version: versions_for(&candidate, &package),
            previous_features: current_features.get(&package).cloned().unwrap_or_default(),
            candidate_features: candidate_features
                .get(&package)
                .cloned()
                .unwrap_or_default(),
            package,
        })
        .collect();

    Ok(CargoLockCandidate {
        protocol: CARGO_LOCK_CANDIDATE_PROTOCOL.to_owned(),
        current_lock_digest: bytes_digest(current_lock),
        candidate_lock_digest: bytes_digest(candidate_lock),
        changed_packages,
        command,
    })
}

fn feature_selections(
    packages: &[ExpectedLinkedPackage],
) -> Result<BTreeMap<String, Vec<String>>, CargoLockResolutionError> {
    let mut selections = BTreeMap::new();
    for package in packages {
        let mut features = package.features.clone();
        if package.default_features {
            features.push("default".to_owned());
        }
        features.sort();
        features.dedup();
        if selections
            .insert(package.package.clone(), features)
            .is_some()
        {
            return Err(CargoLockResolutionError::PackageProvenanceMismatch {
                package: package.package.clone(),
                message: "duplicate linked package feature selection".to_owned(),
            });
        }
    }
    Ok(selections)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PackageKey {
    name: String,
    version: String,
    source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedPackage {
    checksum: Option<String>,
    dependencies: BTreeSet<String>,
    canonical: String,
}

#[derive(Debug, Clone, Default)]
struct ParsedLock {
    packages: BTreeMap<PackageKey, ParsedPackage>,
}

impl ParsedLock {
    fn parse(bytes: &[u8]) -> Result<Self, CargoLockResolutionError> {
        let text =
            std::str::from_utf8(bytes).map_err(|error| CargoLockResolutionError::InvalidLock {
                message: error.to_string(),
            })?;
        let mut lock = Self::default();
        for block in text.split("[[package]]").skip(1) {
            let name = string_field(block, "name").ok_or_else(|| {
                CargoLockResolutionError::InvalidLock {
                    message: "package entry is missing name".to_owned(),
                }
            })?;
            let version = string_field(block, "version").ok_or_else(|| {
                CargoLockResolutionError::InvalidLock {
                    message: format!("package `{name}` is missing version"),
                }
            })?;
            let source = string_field(block, "source").unwrap_or_default();
            let checksum = string_field(block, "checksum");
            let dependencies = dependency_names(block);
            let canonical = block
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            let key = PackageKey {
                name,
                version,
                source,
            };
            if lock
                .packages
                .insert(
                    key.clone(),
                    ParsedPackage {
                        checksum,
                        dependencies,
                        canonical,
                    },
                )
                .is_some()
            {
                return Err(CargoLockResolutionError::InvalidLock {
                    message: format!("duplicate package identity {}@{}", key.name, key.version),
                });
            }
        }
        Ok(lock)
    }
}

fn string_field(block: &str, field: &str) -> Option<String> {
    block.lines().find_map(|line| {
        let (name, value) = line.trim().split_once('=')?;
        (name.trim() == field).then(|| value.trim().trim_matches('"').to_owned())
    })
}

fn dependency_names(block: &str) -> BTreeSet<String> {
    let Some(start) = block.find("dependencies = [") else {
        return BTreeSet::new();
    };
    let tail = &block[start..];
    let Some(end) = tail.find(']') else {
        return BTreeSet::new();
    };
    tail[..end]
        .lines()
        .filter_map(|line| {
            let value = line.trim().trim_end_matches(',').trim_matches('"');
            (!value.is_empty() && value != "dependencies = [").then(|| {
                value
                    .split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_owned()
            })
        })
        .filter(|value| !value.is_empty())
        .collect()
}

fn changed_package_keys(current: &ParsedLock, candidate: &ParsedLock) -> BTreeSet<PackageKey> {
    current
        .packages
        .keys()
        .chain(candidate.packages.keys())
        .filter(|key| current.packages.get(*key) != candidate.packages.get(*key))
        .cloned()
        .collect()
}

fn dependency_closure(lock: &ParsedLock, roots: &[String]) -> BTreeSet<String> {
    let mut closure = BTreeSet::new();
    let mut queue = roots.iter().cloned().collect::<VecDeque<_>>();
    while let Some(name) = queue.pop_front() {
        if !closure.insert(name.clone()) {
            continue;
        }
        for package in lock.packages.iter().filter(|(key, _)| key.name == name) {
            queue.extend(package.1.dependencies.iter().cloned());
        }
    }
    closure
}

fn versions_for(lock: &ParsedLock, package: &str) -> Option<String> {
    let values = lock
        .packages
        .keys()
        .filter(|key| key.name == package)
        .map(|key| key.version.clone())
        .collect::<BTreeSet<_>>();
    (!values.is_empty()).then(|| values.into_iter().collect::<Vec<_>>().join(","))
}

fn normalize_checksum(value: &str) -> &str {
    value.strip_prefix("sha256:").unwrap_or(value)
}

fn bytes_digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let mut digest = String::with_capacity(71);
    digest.push_str("sha256:");
    for byte in hasher.finalize() {
        use std::fmt::Write as _;
        write!(&mut digest, "{byte:02x}").expect("writing to a String cannot fail");
    }
    digest
}

fn validate_relative_path(path: &str) -> Result<(), CargoLockResolutionError> {
    let parsed = Path::new(path);
    if path.is_empty()
        || parsed.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(CargoLockResolutionError::InvalidPath {
            path: path.to_owned(),
        });
    }
    Ok(())
}

fn materialize(
    root: &Path,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<(), CargoLockResolutionError> {
    for (path, contents) in files {
        validate_relative_path(path)?;
        let destination = root.join(path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(destination, contents)?;
    }
    Ok(())
}

struct TempSandbox {
    path: PathBuf,
}

impl TempSandbox {
    fn create() -> Result<Self, std::io::Error> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "lenso-cargo-lock-{}-{nonce}-{}",
            std::process::id(),
            TEMP_SANDBOX_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TempSandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
