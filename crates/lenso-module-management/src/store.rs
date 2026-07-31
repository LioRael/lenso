use crate::{
    MODULE_OPERATION_JOURNAL_PROTOCOL, ModuleOperation, ModuleOperationJournal,
    ModuleOperationJournalEvent, ModuleOperationLease, journal_event_digest,
};
use fs2::FileExt as _;
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead as _, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ModuleOperationStoreError {
    #[error("operation `{0}` was not found")]
    NotFound(String),
    #[error(
        "operation `{operation_id}` revision changed: expected {expected}, observed {observed}"
    )]
    RevisionConflict {
        operation_id: String,
        expected: u64,
        observed: u64,
    },
    #[error("operation `{0}` already exists")]
    AlreadyExists(String),
    #[error("operation journal is invalid: {0}")]
    InvalidJournal(String),
    #[error("operation store I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("operation store JSON failed: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateOperationResult {
    Created,
    Existing(Box<ModuleOperation>),
}

pub trait ModuleOperationStore: std::fmt::Debug + Send + Sync {
    fn create_idempotent(
        &self,
        operation: &ModuleOperation,
        initial_event: &ModuleOperationJournalEvent,
    ) -> Result<CreateOperationResult, ModuleOperationStoreError>;
    fn load(&self, operation_id: &str) -> Result<ModuleOperation, ModuleOperationStoreError>;
    fn journal(
        &self,
        operation_id: &str,
    ) -> Result<ModuleOperationJournal, ModuleOperationStoreError>;
    fn compare_and_append(
        &self,
        expected_revision: u64,
        event: &ModuleOperationJournalEvent,
    ) -> Result<(), ModuleOperationStoreError>;
    fn find_by_idempotency_key(
        &self,
        application_id: &str,
        idempotency_key: &str,
    ) -> Result<Option<ModuleOperation>, ModuleOperationStoreError>;
    fn load_lease(&self) -> Result<Option<ModuleOperationLease>, ModuleOperationStoreError>;
    fn compare_and_set_lease(
        &self,
        expected_revision: Option<u64>,
        lease: Option<&ModuleOperationLease>,
    ) -> Result<(), ModuleOperationStoreError>;
}

#[derive(Debug, Default)]
pub struct MemoryModuleOperationStore {
    inner: Mutex<MemoryStoreState>,
}

#[derive(Debug, Default)]
struct MemoryStoreState {
    journals: BTreeMap<String, Vec<ModuleOperationJournalEvent>>,
    lease: Option<ModuleOperationLease>,
}

impl ModuleOperationStore for MemoryModuleOperationStore {
    fn create_idempotent(
        &self,
        operation: &ModuleOperation,
        initial_event: &ModuleOperationJournalEvent,
    ) -> Result<CreateOperationResult, ModuleOperationStoreError> {
        let mut state = self.inner.lock().expect("memory operation store poisoned");
        if let Some(existing) = state
            .journals
            .values()
            .filter_map(|events| events.last())
            .map(|event| &event.operation_after)
            .find(|existing| {
                existing.application_id == operation.application_id
                    && existing.idempotency_key == operation.idempotency_key
            })
        {
            return Ok(CreateOperationResult::Existing(Box::new(existing.clone())));
        }
        if state.journals.contains_key(&operation.operation_id) {
            return Err(ModuleOperationStoreError::AlreadyExists(
                operation.operation_id.clone(),
            ));
        }
        state
            .journals
            .insert(operation.operation_id.clone(), vec![initial_event.clone()]);
        Ok(CreateOperationResult::Created)
    }

    fn load(&self, operation_id: &str) -> Result<ModuleOperation, ModuleOperationStoreError> {
        Ok(self
            .journal(operation_id)?
            .events
            .last()
            .expect("validated journal is non-empty")
            .operation_after
            .clone())
    }

    fn journal(
        &self,
        operation_id: &str,
    ) -> Result<ModuleOperationJournal, ModuleOperationStoreError> {
        let state = self.inner.lock().expect("memory operation store poisoned");
        let events = state
            .journals
            .get(operation_id)
            .cloned()
            .ok_or_else(|| ModuleOperationStoreError::NotFound(operation_id.to_owned()))?;
        validate_journal(operation_id, &events)?;
        Ok(ModuleOperationJournal {
            protocol: MODULE_OPERATION_JOURNAL_PROTOCOL.to_owned(),
            operation_id: operation_id.to_owned(),
            events,
        })
    }

    fn compare_and_append(
        &self,
        expected_revision: u64,
        event: &ModuleOperationJournalEvent,
    ) -> Result<(), ModuleOperationStoreError> {
        let mut state = self.inner.lock().expect("memory operation store poisoned");
        let events = state
            .journals
            .get_mut(&event.operation_id)
            .ok_or_else(|| ModuleOperationStoreError::NotFound(event.operation_id.clone()))?;
        let observed = events
            .last()
            .expect("operation journal is non-empty")
            .revision;
        if observed != expected_revision {
            return Err(ModuleOperationStoreError::RevisionConflict {
                operation_id: event.operation_id.clone(),
                expected: expected_revision,
                observed,
            });
        }
        events.push(event.clone());
        Ok(())
    }

    fn find_by_idempotency_key(
        &self,
        application_id: &str,
        idempotency_key: &str,
    ) -> Result<Option<ModuleOperation>, ModuleOperationStoreError> {
        let state = self.inner.lock().expect("memory operation store poisoned");
        Ok(state
            .journals
            .values()
            .filter_map(|events| events.last())
            .map(|event| &event.operation_after)
            .find(|operation| {
                operation.application_id == application_id
                    && operation.idempotency_key == idempotency_key
            })
            .cloned())
    }

    fn load_lease(&self) -> Result<Option<ModuleOperationLease>, ModuleOperationStoreError> {
        Ok(self
            .inner
            .lock()
            .expect("memory operation store poisoned")
            .lease
            .clone())
    }

    fn compare_and_set_lease(
        &self,
        expected_revision: Option<u64>,
        lease: Option<&ModuleOperationLease>,
    ) -> Result<(), ModuleOperationStoreError> {
        let mut state = self.inner.lock().expect("memory operation store poisoned");
        let observed = state.lease.as_ref().map(|lease| lease.revision);
        if observed != expected_revision {
            return Err(ModuleOperationStoreError::RevisionConflict {
                operation_id: "composition-lease".to_owned(),
                expected: expected_revision.unwrap_or(0),
                observed: observed.unwrap_or(0),
            });
        }
        state.lease = lease.cloned();
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct JsonFileModuleOperationStore {
    root: PathBuf,
}

impl JsonFileModuleOperationStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn with_lock<T>(
        &self,
        operation: impl FnOnce() -> Result<T, ModuleOperationStoreError>,
    ) -> Result<T, ModuleOperationStoreError> {
        fs::create_dir_all(&self.root)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("store.lock"))?;
        lock.lock_exclusive()?;
        let result = operation();
        fs2::FileExt::unlock(&lock)?;
        result
    }

    fn operations_root(&self) -> PathBuf {
        self.root.join("operations")
    }

    fn journal_path(&self, operation_id: &str) -> Result<PathBuf, ModuleOperationStoreError> {
        validate_storage_identity(operation_id)?;
        Ok(self
            .operations_root()
            .join(operation_id)
            .join("journal.jsonl"))
    }

    fn read_events(
        &self,
        operation_id: &str,
    ) -> Result<Vec<ModuleOperationJournalEvent>, ModuleOperationStoreError> {
        let path = self.journal_path(operation_id)?;
        let file = File::open(path).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => {
                ModuleOperationStoreError::NotFound(operation_id.to_owned())
            }
            _ => ModuleOperationStoreError::Io(error),
        })?;
        let events = BufReader::new(file)
            .lines()
            .filter_map(|line| match line {
                Ok(line) if line.trim().is_empty() => None,
                other => Some(other),
            })
            .map(|line| serde_json::from_str(&line?).map_err(ModuleOperationStoreError::from))
            .collect::<Result<Vec<_>, _>>()?;
        validate_journal(operation_id, &events)?;
        Ok(events)
    }

    fn append_event(
        path: &Path,
        event: &ModuleOperationJournalEvent,
    ) -> Result<(), ModuleOperationStoreError> {
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        serde_json::to_writer(&mut file, event)?;
        file.write_all(b"\n")?;
        file.sync_data()?;
        Ok(())
    }
}

impl ModuleOperationStore for JsonFileModuleOperationStore {
    fn create_idempotent(
        &self,
        operation: &ModuleOperation,
        initial_event: &ModuleOperationJournalEvent,
    ) -> Result<CreateOperationResult, ModuleOperationStoreError> {
        self.with_lock(|| {
            if self.operations_root().exists() {
                for entry in fs::read_dir(self.operations_root())? {
                    let entry = entry?;
                    if !entry.file_type()?.is_dir() {
                        continue;
                    }
                    let operation_id = entry.file_name().to_string_lossy().into_owned();
                    let existing = self
                        .read_events(&operation_id)?
                        .last()
                        .expect("validated journal is non-empty")
                        .operation_after
                        .clone();
                    if existing.application_id == operation.application_id
                        && existing.idempotency_key == operation.idempotency_key
                    {
                        return Ok(CreateOperationResult::Existing(Box::new(existing)));
                    }
                }
            }
            let path = self.journal_path(&operation.operation_id)?;
            if path.exists() {
                return Err(ModuleOperationStoreError::AlreadyExists(
                    operation.operation_id.clone(),
                ));
            }
            fs::create_dir_all(path.parent().expect("journal has parent"))?;
            Self::append_event(&path, initial_event)?;
            Ok(CreateOperationResult::Created)
        })
    }

    fn load(&self, operation_id: &str) -> Result<ModuleOperation, ModuleOperationStoreError> {
        Ok(self
            .journal(operation_id)?
            .events
            .last()
            .expect("validated journal is non-empty")
            .operation_after
            .clone())
    }

    fn journal(
        &self,
        operation_id: &str,
    ) -> Result<ModuleOperationJournal, ModuleOperationStoreError> {
        self.with_lock(|| {
            let events = self.read_events(operation_id)?;
            Ok(ModuleOperationJournal {
                protocol: MODULE_OPERATION_JOURNAL_PROTOCOL.to_owned(),
                operation_id: operation_id.to_owned(),
                events,
            })
        })
    }

    fn compare_and_append(
        &self,
        expected_revision: u64,
        event: &ModuleOperationJournalEvent,
    ) -> Result<(), ModuleOperationStoreError> {
        self.with_lock(|| {
            let events = self.read_events(&event.operation_id)?;
            let observed = events
                .last()
                .expect("validated journal is non-empty")
                .revision;
            if observed != expected_revision {
                return Err(ModuleOperationStoreError::RevisionConflict {
                    operation_id: event.operation_id.clone(),
                    expected: expected_revision,
                    observed,
                });
            }
            Self::append_event(&self.journal_path(&event.operation_id)?, event)
        })
    }

    fn find_by_idempotency_key(
        &self,
        application_id: &str,
        idempotency_key: &str,
    ) -> Result<Option<ModuleOperation>, ModuleOperationStoreError> {
        self.with_lock(|| {
            if !self.operations_root().exists() {
                return Ok(None);
            }
            for entry in fs::read_dir(self.operations_root())? {
                let entry = entry?;
                if !entry.file_type()?.is_dir() {
                    continue;
                }
                let operation_id = entry.file_name().to_string_lossy().into_owned();
                let operation = self
                    .read_events(&operation_id)?
                    .last()
                    .expect("validated journal is non-empty")
                    .operation_after
                    .clone();
                if operation.application_id == application_id
                    && operation.idempotency_key == idempotency_key
                {
                    return Ok(Some(operation));
                }
            }
            Ok(None)
        })
    }

    fn load_lease(&self) -> Result<Option<ModuleOperationLease>, ModuleOperationStoreError> {
        let path = self.root.join("composition-lease.json");
        match fs::read(path) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn compare_and_set_lease(
        &self,
        expected_revision: Option<u64>,
        lease: Option<&ModuleOperationLease>,
    ) -> Result<(), ModuleOperationStoreError> {
        self.with_lock(|| {
            let observed = self.load_lease()?.map(|lease| lease.revision);
            if observed != expected_revision {
                return Err(ModuleOperationStoreError::RevisionConflict {
                    operation_id: "composition-lease".to_owned(),
                    expected: expected_revision.unwrap_or(0),
                    observed: observed.unwrap_or(0),
                });
            }
            let path = self.root.join("composition-lease.json");
            if let Some(lease) = lease {
                let temporary = self.root.join("composition-lease.next.json");
                fs::write(&temporary, serde_json::to_vec_pretty(lease)?)?;
                File::open(&temporary)?.sync_all()?;
                fs::rename(temporary, path)?;
            } else if path.exists() {
                fs::remove_file(path)?;
            }
            Ok(())
        })
    }
}

fn validate_storage_identity(value: &str) -> Result<(), ModuleOperationStoreError> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(ModuleOperationStoreError::InvalidJournal(
            "operation identity is unsafe for target-owned storage".to_owned(),
        ));
    }
    Ok(())
}

fn validate_journal(
    operation_id: &str,
    events: &[ModuleOperationJournalEvent],
) -> Result<(), ModuleOperationStoreError> {
    if events.is_empty() {
        return Err(ModuleOperationStoreError::InvalidJournal(
            "journal has no events".to_owned(),
        ));
    }
    let mut prior_digest: Option<String> = None;
    for (index, event) in events.iter().enumerate() {
        let expected_revision = u64::try_from(index).expect("journal index fits u64");
        if event.operation_id != operation_id
            || event.revision != expected_revision
            || event.operation_after.revision != event.revision
            || event.prior_event_digest != prior_digest
        {
            return Err(ModuleOperationStoreError::InvalidJournal(format!(
                "event {index} breaks identity, revision, or digest chaining"
            )));
        }
        prior_digest = Some(journal_event_digest(event)?);
    }
    Ok(())
}
