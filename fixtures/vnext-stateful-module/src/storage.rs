//! Private file-backed persistence and owned migration workflows.

use std::{collections::BTreeMap, path::PathBuf, rc::Rc};

use serde::{Deserialize, Serialize};

use super::{CURRENT_SCHEMA_VERSION, INITIAL_MIGRATION};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct CounterDocument {
    schema_version: u32,
    revision: u64,
    entries: BTreeMap<String, i64>,
    #[serde(default)]
    secret_fingerprint: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OwnedMigration {
    version: u32,
    initial_document: CounterDocument,
}

#[derive(Debug, Deserialize)]
struct LegacyCounterDocument {
    schema_version: u32,
    entries: BTreeMap<String, i64>,
}

/// A private persistence Adapter owned by the counter Module.
#[derive(Clone, Debug)]
pub(super) struct FileStateAdapter {
    path: PathBuf,
    transaction_lock: Rc<std::cell::RefCell<()>>,
}

impl FileStateAdapter {
    /// Selects one required durable document path. No file is created here.
    pub(super) fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            transaction_lock: Rc::new(std::cell::RefCell::new(())),
        }
    }

    /// Applies the owned initial migration, or reports that it is already applied.
    pub(super) fn setup(&self) -> Result<SetupOutcome, StateStorageError> {
        let _guard = self.transaction_lock.borrow_mut();
        let migration = Self::owned_migration()?;
        if self.path.exists() {
            self.read_current_document()?;
            return Ok(SetupOutcome::AlreadyCurrent {
                schema_version: migration.version,
            });
        }
        let Some(parent) = self.path.parent() else {
            return Err(StateStorageError::InvalidPath {
                path: self.path.clone(),
            });
        };
        std::fs::create_dir_all(parent).map_err(|error| {
            StateStorageError::io(&self.path, "create storage directory", &error)
        })?;
        self.write_document(&migration.initial_document)?;
        Ok(SetupOutcome::Created {
            schema_version: migration.version,
        })
    }

    /// Applies an explicit owned upgrade and never runs from Module preparation.
    pub(super) fn upgrade(&self) -> Result<UpgradeOutcome, StateStorageError> {
        let _guard = self.transaction_lock.borrow_mut();
        let migration = Self::owned_migration()?;
        let value = self.read_json_value()?;
        let version = value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| StateStorageError::InvalidDocument {
                path: self.path.clone(),
                detail: "schema_version is missing".to_owned(),
            })?;
        let version = u32::try_from(version).map_err(|_| StateStorageError::InvalidDocument {
            path: self.path.clone(),
            detail: "schema_version is too large".to_owned(),
        })?;
        match version {
            version if version == migration.version => Ok(UpgradeOutcome::AlreadyCurrent {
                schema_version: migration.version,
            }),
            0 => {
                let legacy: LegacyCounterDocument =
                    serde_json::from_value(value).map_err(|error| {
                        StateStorageError::InvalidDocument {
                            path: self.path.clone(),
                            detail: error.to_string(),
                        }
                    })?;
                let from = legacy.schema_version;
                self.write_document(&CounterDocument {
                    schema_version: migration.version,
                    revision: migration.initial_document.revision,
                    entries: legacy.entries,
                    secret_fingerprint: migration.initial_document.secret_fingerprint,
                })?;
                Ok(UpgradeOutcome::Applied {
                    from,
                    to: migration.version,
                })
            }
            actual => Err(StateStorageError::IncompatibleSchema {
                path: self.path.clone(),
                expected: migration.version,
                actual,
            }),
        }
    }

    /// Verifies required storage and schema compatibility without changing it.
    pub(super) fn verify_ready(&self) -> Result<(), StateStorageError> {
        let _guard = self.transaction_lock.borrow();
        Self::owned_migration()?;
        self.read_current_document().map(|_| ())
    }

    pub(super) fn bind_secret(&self, fingerprint: &[u8]) -> Result<(), StateStorageError> {
        let _guard = self.transaction_lock.borrow_mut();
        let mut document = self.read_current_document()?;
        let fingerprint = fingerprint_hex(fingerprint);
        match document.secret_fingerprint.as_deref() {
            Some(existing) if existing != fingerprint => Err(StateStorageError::SecretMismatch {
                path: self.path.clone(),
            }),
            Some(_) => Ok(()),
            None => {
                document.secret_fingerprint = Some(fingerprint);
                self.write_document(&document)
            }
        }
    }

    pub(crate) fn read_counter(&self, key: &str) -> Result<Option<(i64, u64)>, StateStorageError> {
        let _guard = self.transaction_lock.borrow();
        let document = self.read_current_document()?;
        Ok(document
            .entries
            .get(key)
            .copied()
            .map(|value| (value, document.revision)))
    }

    pub(crate) fn increment_counter(
        &self,
        key: &str,
        amount: i64,
    ) -> Result<(i64, u64), StateStorageError> {
        let _guard = self.transaction_lock.borrow_mut();
        let mut document = self.read_current_document()?;
        let value = document.entries.entry(key.to_owned()).or_default();
        *value = value
            .checked_add(amount)
            .ok_or_else(|| StateStorageError::InvalidDocument {
                path: self.path.clone(),
                detail: "counter value overflow".to_owned(),
            })?;
        document.revision =
            document
                .revision
                .checked_add(1)
                .ok_or_else(|| StateStorageError::InvalidDocument {
                    path: self.path.clone(),
                    detail: "revision overflow".to_owned(),
                })?;
        let result = (*value, document.revision);
        self.write_document(&document)?;
        Ok(result)
    }

    fn read_current_document(&self) -> Result<CounterDocument, StateStorageError> {
        if !self.path.exists() {
            return Err(StateStorageError::Missing {
                path: self.path.clone(),
            });
        }
        let value = self.read_json_value()?;
        let actual = value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            .and_then(|version| u32::try_from(version).ok())
            .ok_or_else(|| StateStorageError::InvalidDocument {
                path: self.path.clone(),
                detail: "schema_version is missing or too large".to_owned(),
            })?;
        if actual != CURRENT_SCHEMA_VERSION {
            return Err(StateStorageError::IncompatibleSchema {
                path: self.path.clone(),
                expected: CURRENT_SCHEMA_VERSION,
                actual,
            });
        }
        let document: CounterDocument =
            serde_json::from_value(value).map_err(|error| StateStorageError::InvalidDocument {
                path: self.path.clone(),
                detail: error.to_string(),
            })?;
        Ok(document)
    }

    fn owned_migration() -> Result<OwnedMigration, StateStorageError> {
        let migration: OwnedMigration =
            serde_json::from_str(INITIAL_MIGRATION).map_err(|error| {
                StateStorageError::InvalidMigration {
                    detail: error.to_string(),
                }
            })?;
        if migration.version != CURRENT_SCHEMA_VERSION
            || migration.initial_document.schema_version != migration.version
        {
            return Err(StateStorageError::InvalidMigration {
                detail: format!(
                    "artifact version {} does not match its initial document or Module schema {}",
                    migration.version, CURRENT_SCHEMA_VERSION
                ),
            });
        }
        Ok(migration)
    }

    fn read_json_value(&self) -> Result<serde_json::Value, StateStorageError> {
        let bytes = std::fs::read(&self.path)
            .map_err(|error| StateStorageError::io(&self.path, "read storage", &error))?;
        serde_json::from_slice(&bytes).map_err(|error| StateStorageError::InvalidDocument {
            path: self.path.clone(),
            detail: error.to_string(),
        })
    }

    fn write_document(&self, document: &CounterDocument) -> Result<(), StateStorageError> {
        let Some(parent) = self.path.parent() else {
            return Err(StateStorageError::InvalidPath {
                path: self.path.clone(),
            });
        };
        std::fs::create_dir_all(parent).map_err(|error| {
            StateStorageError::io(&self.path, "create storage directory", &error)
        })?;
        let temporary_path = self.path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(document).map_err(|error| {
            StateStorageError::InvalidDocument {
                path: self.path.clone(),
                detail: error.to_string(),
            }
        })?;
        std::fs::write(&temporary_path, bytes).map_err(|error| {
            StateStorageError::io(&temporary_path, "write migration result", &error)
        })?;
        std::fs::rename(&temporary_path, &self.path)
            .map_err(|error| StateStorageError::io(&self.path, "commit transaction", &error))
    }
}

pub(super) fn setup_owned_state(
    path: impl Into<PathBuf>,
) -> Result<SetupOutcome, StateStorageError> {
    FileStateAdapter::new(path).setup()
}

pub(super) fn upgrade_owned_state(
    path: impl Into<PathBuf>,
) -> Result<UpgradeOutcome, StateStorageError> {
    FileStateAdapter::new(path).upgrade()
}

fn fingerprint_hex(fingerprint: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(fingerprint.len() * 2);
    for byte in fingerprint {
        encoded.push(char::from(HEX[(byte >> 4) as usize]));
        encoded.push(char::from(HEX[(byte & 0x0f) as usize]));
    }
    encoded
}

/// Reviewable result from the explicit setup workflow.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SetupOutcome {
    /// The initial owned migration created the storage document.
    Created { schema_version: u32 },
    /// The document already had the current owned schema.
    AlreadyCurrent { schema_version: u32 },
}

/// Reviewable result from the explicit upgrade workflow.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UpgradeOutcome {
    /// No migration was required.
    AlreadyCurrent { schema_version: u32 },
    /// One owned migration was applied.
    Applied { from: u32, to: u32 },
}

/// Failure from the private state Adapter, kept outside the Kernel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StateStorageError {
    /// The required durable document does not exist.
    Missing { path: PathBuf },
    /// The configured path cannot be used as a storage document.
    InvalidPath { path: PathBuf },
    /// The document is malformed or cannot be decoded.
    InvalidDocument { path: PathBuf, detail: String },
    /// The Module's compiled migration artifact is malformed or inconsistent.
    InvalidMigration { detail: String },
    /// The document requires an explicit setup or upgrade decision.
    IncompatibleSchema {
        path: PathBuf,
        expected: u32,
        actual: u32,
    },
    /// The host filesystem rejected one owned operation.
    Io {
        path: PathBuf,
        operation: String,
        detail: String,
    },
    /// The durable state is already bound to another secret.
    SecretMismatch { path: PathBuf },
}

impl StateStorageError {
    fn io(path: &std::path::Path, operation: &str, error: &std::io::Error) -> Self {
        Self::Io {
            path: path.to_owned(),
            operation: operation.to_owned(),
            detail: error.to_string(),
        }
    }
}

impl std::fmt::Display for StateStorageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing { path } => write!(
                formatter,
                "required durable storage `{}` is missing; run setup",
                path.display()
            ),
            Self::InvalidPath { path } => {
                write!(formatter, "storage path `{}` is invalid", path.display())
            }
            Self::InvalidDocument { path, detail } => write!(
                formatter,
                "storage document `{}` is invalid: {detail}",
                path.display()
            ),
            Self::InvalidMigration { detail } => {
                write!(formatter, "owned migration artifact is invalid: {detail}")
            }
            Self::IncompatibleSchema {
                path,
                expected,
                actual,
            } => write!(
                formatter,
                "storage document `{}` uses schema {actual}; expected {expected}, run the explicit upgrade workflow",
                path.display()
            ),
            Self::Io {
                path,
                operation,
                detail,
            } => write!(
                formatter,
                "cannot {operation} `{}`: {detail}",
                path.display()
            ),
            Self::SecretMismatch { path } => write!(
                formatter,
                "durable storage `{}` is bound to a different secret",
                path.display()
            ),
        }
    }
}

impl std::error::Error for StateStorageError {}
