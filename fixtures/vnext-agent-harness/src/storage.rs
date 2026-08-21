//! Private durable storage owned by the memory Module.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct MemoryDocument {
    schema_version: u32,
    revision: u64,
    entries: BTreeMap<String, Vec<String>>,
}

/// A file-backed persistence Adapter whose schema is private to the memory Module.
#[derive(Clone, Debug)]
pub(crate) struct FileMemoryAdapter {
    path: PathBuf,
}

impl FileMemoryAdapter {
    pub(crate) fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub(crate) fn setup(&self) -> Result<MemorySetupOutcome, MemoryStorageError> {
        let parent = self.parent()?;
        fs::create_dir_all(parent).map_err(|error| {
            MemoryStorageError::io(&self.path, "create memory directory", &error)
        })?;
        if self.path.exists() {
            self.read_document()?;
            return Ok(MemorySetupOutcome::AlreadyCurrent {
                schema_version: CURRENT_SCHEMA_VERSION,
            });
        }
        self.write_document(&MemoryDocument {
            schema_version: CURRENT_SCHEMA_VERSION,
            revision: 0,
            entries: BTreeMap::new(),
        })?;
        Ok(MemorySetupOutcome::Created {
            schema_version: CURRENT_SCHEMA_VERSION,
        })
    }

    pub(crate) fn verify_ready(&self) -> Result<(), MemoryStorageError> {
        if !self.path.exists() {
            return Err(MemoryStorageError::Missing {
                path: self.path.clone(),
            });
        }
        self.read_document()?;
        let parent = self.parent()?;
        let probe = parent.join(format!(
            ".{}.probe-{}",
            self.path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("memory"),
            std::process::id()
        ));
        fs::write(&probe, [])
            .map_err(|error| MemoryStorageError::io(&probe, "probe memory directory", &error))?;
        fs::remove_file(&probe)
            .map_err(|error| MemoryStorageError::io(&probe, "remove memory probe", &error))?;
        Ok(())
    }

    pub(crate) fn read(&self, key: &str) -> Result<Option<(Vec<String>, u64)>, MemoryStorageError> {
        let document = self.read_document()?;
        Ok(document
            .entries
            .get(key)
            .cloned()
            .map(|entries| (entries, document.revision)))
    }

    pub(crate) fn append(&self, key: &str, entry: &str) -> Result<u64, MemoryStorageError> {
        let mut document = self.read_document()?;
        let next_revision = document.revision.checked_add(1).ok_or_else(|| {
            MemoryStorageError::InvalidDocument {
                path: self.path.clone(),
                detail: "memory revision overflow".to_owned(),
            }
        })?;
        document
            .entries
            .entry(key.to_owned())
            .or_default()
            .push(entry.to_owned());
        document.revision = next_revision;
        self.write_document(&document)?;
        Ok(next_revision)
    }

    fn parent(&self) -> Result<&Path, MemoryStorageError> {
        self.path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or_else(|| MemoryStorageError::InvalidPath {
                path: self.path.clone(),
            })
    }

    fn read_document(&self) -> Result<MemoryDocument, MemoryStorageError> {
        let bytes = fs::read(&self.path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                MemoryStorageError::Missing {
                    path: self.path.clone(),
                }
            } else {
                MemoryStorageError::io(&self.path, "read memory", &error)
            }
        })?;
        let document: MemoryDocument = serde_json::from_slice(&bytes).map_err(|error| {
            MemoryStorageError::InvalidDocument {
                path: self.path.clone(),
                detail: error.to_string(),
            }
        })?;
        if document.schema_version != CURRENT_SCHEMA_VERSION {
            return Err(MemoryStorageError::IncompatibleSchema {
                path: self.path.clone(),
                expected: CURRENT_SCHEMA_VERSION,
                actual: document.schema_version,
            });
        }
        Ok(document)
    }

    fn write_document(&self, document: &MemoryDocument) -> Result<(), MemoryStorageError> {
        let temporary = self.temporary_path();
        let bytes = serde_json::to_vec_pretty(document).map_err(|error| {
            MemoryStorageError::InvalidDocument {
                path: self.path.clone(),
                detail: error.to_string(),
            }
        })?;
        fs::write(&temporary, bytes).map_err(|error| {
            MemoryStorageError::io(&temporary, "write memory transaction", &error)
        })?;
        fs::rename(&temporary, &self.path).map_err(|error| {
            MemoryStorageError::io(&self.path, "commit memory transaction", &error)
        })
    }

    fn temporary_path(&self) -> PathBuf {
        let mut path = self.path.clone().into_os_string();
        path.push(".tmp");
        PathBuf::from(path)
    }
}

pub fn setup_owned_memory(
    path: impl Into<PathBuf>,
) -> Result<MemorySetupOutcome, MemoryStorageError> {
    FileMemoryAdapter::new(path).setup()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MemorySetupOutcome {
    Created { schema_version: u32 },
    AlreadyCurrent { schema_version: u32 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MemoryStorageError {
    Missing {
        path: PathBuf,
    },
    InvalidPath {
        path: PathBuf,
    },
    InvalidDocument {
        path: PathBuf,
        detail: String,
    },
    IncompatibleSchema {
        path: PathBuf,
        expected: u32,
        actual: u32,
    },
    Io {
        path: PathBuf,
        operation: String,
        detail: String,
    },
}

impl MemoryStorageError {
    fn io(path: &Path, operation: &str, error: &std::io::Error) -> Self {
        Self::Io {
            path: path.to_owned(),
            operation: operation.to_owned(),
            detail: error.to_string(),
        }
    }
}

impl std::fmt::Display for MemoryStorageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing { path } => write!(
                formatter,
                "required durable agent memory `{}` is missing; run setup_owned_memory",
                path.display()
            ),
            Self::InvalidPath { path } => {
                write!(formatter, "memory path `{}` is invalid", path.display())
            }
            Self::InvalidDocument { path, detail } => write!(
                formatter,
                "memory document `{}` is invalid: {detail}",
                path.display()
            ),
            Self::IncompatibleSchema {
                path,
                expected,
                actual,
            } => write!(
                formatter,
                "memory document `{}` uses schema {actual}; expected {expected}",
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
        }
    }
}

impl std::error::Error for MemoryStorageError {}
