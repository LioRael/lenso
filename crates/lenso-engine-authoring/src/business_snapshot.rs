//! Host-authorized, versioned business data that never rewrites a Plugin Root.

use std::{
    collections::BTreeSet,
    fmt, fs,
    io::Read as _,
    marker::PhantomData,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

mod https;
use https::PollOutcome;
pub use https::{BusinessSnapshotCursor, BusinessSnapshotPoll, HttpsBusinessSnapshotSource};

use anyhow::{Context as _, bail, ensure};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use sha2::{Digest as _, Sha256};

use crate::{
    configuration_snapshot::open_regular_snapshot, host_authoring::compile_ceiling,
    validate_existing_plugin_id, validate_instance_filename,
};

const DOCUMENT_SCHEMA: &str = "lenso.business-snapshot.v1";
const MAX_DOCUMENT_BYTES: u64 = 1024 * 1024;
const MAX_VALUE_BYTES: usize = 256 * 1024;
const MAX_STALE: Duration = Duration::from_secs(24 * 60 * 60);

/// Exact Plugin-owned business object. It is not a Plugin Root configuration field.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessSnapshotObjectId {
    plugin_id: String,
    instance_key: String,
    object_key: String,
}

impl BusinessSnapshotObjectId {
    pub fn new(
        plugin_id: impl Into<String>,
        instance_key: impl Into<String>,
        object_key: impl Into<String>,
    ) -> anyhow::Result<Self> {
        let object = Self {
            plugin_id: plugin_id.into(),
            instance_key: instance_key.into(),
            object_key: object_key.into(),
        };
        object.validate()?;
        Ok(object)
    }

    fn validate(&self) -> anyhow::Result<()> {
        validate_existing_plugin_id(&self.plugin_id)?;
        validate_instance_filename(&self.instance_key)?;
        ensure!(
            !self.object_key.is_empty()
                && self.object_key.len() <= 128
                && self.object_key.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'.' | b'-' | b'_')
                }),
            "business snapshot object key is invalid"
        );
        Ok(())
    }

    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    pub fn instance_key(&self) -> &str {
        &self.instance_key
    }

    pub fn object_key(&self) -> &str {
        &self.object_key
    }
}

/// Exact identity of a Host-selected dynamic source. Revisions are comparable only within it.
#[derive(Clone, Eq, PartialEq)]
pub struct BusinessSnapshotSourceId {
    kind: String,
    reference: String,
}

impl BusinessSnapshotSourceId {
    pub fn new(kind: impl Into<String>, reference: impl Into<String>) -> anyhow::Result<Self> {
        let kind = kind.into();
        let reference = reference.into();
        ensure!(
            !kind.is_empty()
                && kind.len() <= 64
                && kind.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b'-' | b'.')
                }),
            "business snapshot source kind is invalid"
        );
        ensure!(
            !reference.is_empty()
                && reference.len() <= 256
                && !reference.chars().any(char::is_control),
            "business snapshot source reference is invalid"
        );
        Ok(Self { kind, reference })
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn reference(&self) -> &str {
        &self.reference
    }
}

impl fmt::Debug for BusinessSnapshotSourceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BusinessSnapshotSourceId")
            .field("kind", &self.kind)
            .field("reference", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
enum SourceLocation {
    File(PathBuf),
    Https(String),
}

/// An exact reader binding selected by the Host, not asserted by a source document.
#[derive(Clone, Eq, PartialEq)]
pub struct BusinessSnapshotSourceBinding {
    source: BusinessSnapshotSourceId,
    location: SourceLocation,
}

impl BusinessSnapshotSourceBinding {
    pub fn source(&self) -> &BusinessSnapshotSourceId {
        &self.source
    }
}

impl fmt::Debug for BusinessSnapshotSourceBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BusinessSnapshotSourceBinding")
            .field("source", &self.source)
            .field("location", &"<redacted>")
            .finish()
    }
}

/// Untrusted source data, still requiring exact Host authorization before use.
pub struct VersionedBusinessSnapshot {
    source: BusinessSnapshotSourceId,
    location: SourceLocation,
    object: BusinessSnapshotObjectId,
    revision: u64,
    value: Value,
    observed_at: Instant,
}

impl VersionedBusinessSnapshot {
    fn new(
        source: BusinessSnapshotSourceId,
        location: SourceLocation,
        object: BusinessSnapshotObjectId,
        revision: u64,
        value: Value,
    ) -> anyhow::Result<Self> {
        object.validate()?;
        ensure!(revision > 0, "business snapshot revision must be positive");
        ensure!(
            serde_json::to_vec(&value)?.len() <= MAX_VALUE_BYTES,
            "business snapshot value exceeds its bound"
        );
        Ok(Self {
            source,
            location,
            object,
            revision,
            value,
            observed_at: Instant::now(),
        })
    }

    pub fn source(&self) -> &BusinessSnapshotSourceId {
        &self.source
    }

    pub fn object(&self) -> &BusinessSnapshotObjectId {
        &self.object
    }

    pub const fn revision(&self) -> u64 {
        self.revision
    }
}

impl fmt::Debug for VersionedBusinessSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VersionedBusinessSnapshot")
            .field("source", &self.source)
            .field("object", &self.object)
            .field("revision", &self.revision)
            .field("value", &"<redacted>")
            .finish()
    }
}

/// Host policy for one exact business object and source, using an existing bounded schema profile.
///
/// Construct this only from a locked Plugin-owned schema and Host deployment
/// policy, never from fields supplied by the snapshot document.
pub struct BusinessSnapshotAuthorization<T> {
    object: BusinessSnapshotObjectId,
    source: BusinessSnapshotSourceId,
    binding: BusinessSnapshotSourceBinding,
    fields: BTreeSet<String>,
    validator: jsonschema::Validator,
    max_stale: Duration,
    typed: PhantomData<fn() -> T>,
}

impl<T> BusinessSnapshotAuthorization<T> {
    pub fn new(
        object: BusinessSnapshotObjectId,
        binding: BusinessSnapshotSourceBinding,
        schema: Value,
        fields: impl IntoIterator<Item = impl Into<String>>,
        max_stale: Duration,
    ) -> anyhow::Result<Self> {
        object.validate()?;
        ensure!(
            !max_stale.is_zero() && max_stale <= MAX_STALE,
            "business snapshot stale limit must be positive and at most 24 hours"
        );
        let fields = fields.into_iter().map(Into::into).collect::<BTreeSet<_>>();
        ensure!(
            !fields.is_empty() && fields.len() <= 256,
            "business snapshot authorization must name 1 to 256 fields"
        );
        let properties = schema
            .as_object()
            .filter(|root| {
                root.get("type").and_then(Value::as_str) == Some("object")
                    && root.get("additionalProperties") == Some(&Value::Bool(false))
            })
            .and_then(|root| root.get("properties"))
            .and_then(Value::as_object)
            .context("business snapshot schema must be a closed object with properties")?;
        ensure!(
            fields.iter().all(|field| {
                !field.is_empty() && field.len() <= 256 && properties.contains_key(field)
            }),
            "business snapshot scope contains an undeclared field"
        );
        let validator = compile_ceiling(&schema)
            .map_err(|_| anyhow::anyhow!("business snapshot schema is invalid"))?;
        Ok(Self {
            object,
            source: binding.source.clone(),
            binding,
            fields,
            validator,
            max_stale,
            typed: PhantomData,
        })
    }
}

impl<T> fmt::Debug for BusinessSnapshotAuthorization<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BusinessSnapshotAuthorization")
            .field("object", &self.object)
            .field("source", &self.source)
            .field("binding", &self.binding)
            .field("fields", &self.fields)
            .field("max_stale", &self.max_stale)
            .finish_non_exhaustive()
    }
}

struct ActiveBusinessSnapshot<T> {
    revision: u64,
    generation: u64,
    value_fingerprint: [u8; 32],
    value: Arc<T>,
    refreshed_at: Instant,
    cursor: Option<BusinessSnapshotCursor>,
}

/// Host-owned atomic publication and request capture for one authorized object.
pub struct BusinessSnapshotAuthority<T> {
    authorization: BusinessSnapshotAuthorization<T>,
    active: RwLock<Option<ActiveBusinessSnapshot<T>>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BusinessSnapshotAcceptance {
    Activated,
    Unchanged,
}

impl<T: DeserializeOwned> BusinessSnapshotAuthority<T> {
    pub fn new(authorization: BusinessSnapshotAuthorization<T>) -> Self {
        Self {
            authorization,
            active: RwLock::new(None),
        }
    }

    /// Validates before replacing the active value, then fences publication with CAS.
    pub fn accept(
        &self,
        candidate: VersionedBusinessSnapshot,
        expected_active_revision: Option<u64>,
    ) -> anyhow::Result<BusinessSnapshotAcceptance> {
        self.accept_with_cursor(candidate, expected_active_revision, None)
    }

    /// Accepts one actual HTTPS poll result. A 304 can refresh only its accepted ETag.
    pub fn accept_poll(
        &self,
        poll: BusinessSnapshotPoll,
        expected_active_revision: Option<u64>,
    ) -> anyhow::Result<BusinessSnapshotAcceptance> {
        match poll.outcome {
            PollOutcome::Updated { snapshot, cursor } => {
                if let Some(cursor) = &cursor {
                    ensure!(
                        cursor.source() == snapshot.source()
                            && cursor.revision() == snapshot.revision()
                            && cursor.generation() == 0,
                        "business snapshot ETag belongs to a different source or revision"
                    );
                }
                self.accept_with_cursor(*snapshot, expected_active_revision, cursor)
            }
            PollOutcome::NotModified {
                cursor,
                observed_at,
            } => {
                let mut active = self
                    .active
                    .write()
                    .map_err(|_| anyhow::anyhow!("business snapshot state is unavailable"))?;
                let active = active
                    .as_mut()
                    .context("business snapshot HTTP 304 has no accepted revision")?;
                ensure!(
                    Some(active.revision) == expected_active_revision,
                    "business snapshot active revision changed before revalidation"
                );
                ensure!(
                    active.cursor.as_ref() == Some(&cursor)
                        && cursor.source() == &self.authorization.source
                        && cursor.revision() == active.revision
                        && cursor.generation() == active.generation,
                    "business snapshot HTTP 304 does not match the accepted ETag"
                );
                ensure!(
                    active.refreshed_at.elapsed() <= self.authorization.max_stale,
                    "business snapshot source proof has expired; fetch a complete snapshot"
                );
                ensure!(
                    observed_at.elapsed() <= self.authorization.max_stale,
                    "business snapshot HTTPS proof expired before acceptance"
                );
                active.refreshed_at = active.refreshed_at.max(observed_at);
                Ok(BusinessSnapshotAcceptance::Unchanged)
            }
        }
    }

    /// Returns only the ETag cursor of the currently accepted HTTPS value.
    pub fn cursor(&self) -> anyhow::Result<Option<BusinessSnapshotCursor>> {
        Ok(self
            .active
            .read()
            .map_err(|_| anyhow::anyhow!("business snapshot state is unavailable"))?
            .as_ref()
            .filter(|active| active.refreshed_at.elapsed() <= self.authorization.max_stale)
            .and_then(|active| active.cursor.clone()))
    }

    /// Returns the fenced revision even when its source proof has expired.
    pub fn active_revision(&self) -> anyhow::Result<Option<u64>> {
        Ok(self
            .active
            .read()
            .map_err(|_| anyhow::anyhow!("business snapshot state is unavailable"))?
            .as_ref()
            .map(|active| active.revision))
    }

    fn accept_with_cursor(
        &self,
        candidate: VersionedBusinessSnapshot,
        expected_active_revision: Option<u64>,
        cursor: Option<BusinessSnapshotCursor>,
    ) -> anyhow::Result<BusinessSnapshotAcceptance> {
        ensure!(
            candidate.source == self.authorization.source,
            "business snapshot source is not authorized"
        );
        ensure!(
            candidate.location == self.authorization.binding.location,
            "business snapshot reader is not authorized"
        );
        ensure!(
            candidate.object == self.authorization.object,
            "business snapshot object is not authorized"
        );
        ensure!(
            candidate.observed_at.elapsed() <= self.authorization.max_stale,
            "business snapshot source proof expired before acceptance"
        );
        let value = candidate
            .value
            .as_object()
            .context("business snapshot value must be an object")?;
        ensure!(
            value
                .keys()
                .all(|field| self.authorization.fields.contains(field)),
            "business snapshot contains a field outside its authorized scope"
        );
        ensure!(
            self.authorization.validator.is_valid(&candidate.value),
            "business snapshot value is invalid"
        );
        let typed: T = serde_json::from_value(candidate.value.clone())
            .map_err(|_| anyhow::anyhow!("business snapshot typed value is invalid"))?;
        let fingerprint = fingerprint(&candidate.value);
        let mut active = self
            .active
            .write()
            .map_err(|_| anyhow::anyhow!("business snapshot state is unavailable"))?;
        ensure!(
            active.as_ref().map(|current| current.revision) == expected_active_revision,
            "business snapshot active revision changed before publication"
        );
        let generation = active
            .as_ref()
            .map_or(Some(1), |current| current.generation.checked_add(1))
            .context("business snapshot generation is exhausted")?;
        let cursor = cursor.map(|cursor| cursor.bind_generation(generation));
        if let Some(current) = active.as_mut() {
            if candidate.revision < current.revision {
                bail!("business snapshot revision is stale");
            }
            if candidate.revision == current.revision {
                ensure!(
                    fingerprint == current.value_fingerprint,
                    "business snapshot revision was reused with changed content"
                );
                current.refreshed_at = current.refreshed_at.max(candidate.observed_at);
                current.generation = generation;
                current.cursor = cursor;
                return Ok(BusinessSnapshotAcceptance::Unchanged);
            }
        }
        *active = Some(ActiveBusinessSnapshot {
            revision: candidate.revision,
            generation,
            value_fingerprint: fingerprint,
            value: Arc::new(typed),
            refreshed_at: candidate.observed_at,
            cursor,
        });
        Ok(BusinessSnapshotAcceptance::Activated)
    }

    /// Captures one immutable value and version for the entire business request.
    pub fn capture_request(&self) -> anyhow::Result<BusinessRequestSnapshot<T>> {
        let active = self
            .active
            .read()
            .map_err(|_| anyhow::anyhow!("business snapshot state is unavailable"))?;
        let active = active
            .as_ref()
            .context("business snapshot has no admitted active revision")?;
        ensure!(
            active.refreshed_at.elapsed() <= self.authorization.max_stale,
            "business snapshot source proof has expired"
        );
        Ok(BusinessRequestSnapshot {
            object: self.authorization.object.clone(),
            source: self.authorization.source.clone(),
            revision: active.revision,
            value: Arc::clone(&active.value),
        })
    }
}

fn fingerprint(value: &Value) -> [u8; 32] {
    fn write(value: &Value, digest: &mut Sha256) {
        match value {
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {
                digest.update(serde_json::to_vec(value).expect("JSON value serializes"));
            }
            Value::Array(items) => {
                digest.update(b"[");
                for item in items {
                    write(item, digest);
                    digest.update(b",");
                }
                digest.update(b"]");
            }
            Value::Object(object) => {
                digest.update(b"{");
                let mut keys = object.keys().collect::<Vec<_>>();
                keys.sort();
                for key in keys {
                    digest.update(serde_json::to_vec(key).expect("JSON key serializes"));
                    digest.update(b":");
                    write(&object[key], digest);
                    digest.update(b",");
                }
                digest.update(b"}");
            }
        }
    }

    let mut digest = Sha256::new();
    write(value, &mut digest);
    digest.finalize().into()
}

/// One request-pinned version; later publications cannot alter its value.
pub struct BusinessRequestSnapshot<T> {
    object: BusinessSnapshotObjectId,
    source: BusinessSnapshotSourceId,
    revision: u64,
    value: Arc<T>,
}

impl<T> BusinessRequestSnapshot<T> {
    pub fn object(&self) -> &BusinessSnapshotObjectId {
        &self.object
    }

    pub fn source(&self) -> &BusinessSnapshotSourceId {
        &self.source
    }

    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub fn value(&self) -> &T {
        &self.value
    }
}

impl<T> fmt::Debug for BusinessRequestSnapshot<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BusinessRequestSnapshot")
            .field("object", &self.object)
            .field("source", &self.source)
            .field("revision", &self.revision)
            .field("value", &"<redacted>")
            .finish()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceDocument {
    schema: String,
    revision: u64,
    object: BusinessSnapshotObjectId,
    value: Value,
}

/// Bounded regular-file input. Producers must publish by atomic replacement,
/// not by rewriting the active inode. Path admission and filesystem confinement
/// remain Host duties; metadata checks cannot stop a hostile paused writer.
pub struct FileBusinessSnapshotSource {
    path: PathBuf,
    source: BusinessSnapshotSourceId,
}

impl FileBusinessSnapshotSource {
    pub fn new(path: impl Into<PathBuf>, source: BusinessSnapshotSourceId) -> Self {
        Self {
            path: path.into(),
            source,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn binding(&self) -> BusinessSnapshotSourceBinding {
        BusinessSnapshotSourceBinding {
            source: self.source.clone(),
            location: SourceLocation::File(self.path.clone()),
        }
    }

    pub fn read(&self) -> anyhow::Result<VersionedBusinessSnapshot> {
        self.read_with_after_bytes(|| {})
    }

    fn read_with_after_bytes(
        &self,
        after_bytes: impl FnOnce(),
    ) -> anyhow::Result<VersionedBusinessSnapshot> {
        ensure!(
            self.path.is_absolute(),
            "business snapshot file path must be absolute"
        );
        let file = open_regular_snapshot(&self.path)?;
        let metadata = file.metadata().context("inspect business snapshot file")?;
        ensure!(
            metadata.len() <= MAX_DOCUMENT_BYTES,
            "business snapshot file exceeds its bound"
        );
        let mut bytes = Vec::new();
        (&file)
            .take(MAX_DOCUMENT_BYTES + 1)
            .read_to_end(&mut bytes)
            .context("read business snapshot file")?;
        ensure!(
            u64::try_from(bytes.len()).unwrap_or(u64::MAX) <= MAX_DOCUMENT_BYTES,
            "business snapshot file exceeds its bound"
        );
        after_bytes();
        let after = file.metadata().context("inspect business snapshot file")?;
        ensure!(
            u64::try_from(bytes.len()).ok() == Some(metadata.len())
                && same_file_version(&metadata, &after),
            "business snapshot file changed while reading"
        );
        let document: SourceDocument = serde_json::from_slice(&bytes)
            .map_err(|_| anyhow::anyhow!("business snapshot file is invalid JSON"))?;
        snapshot_from_document(self.binding(), document)
    }
}

fn same_file_version(before: &fs::Metadata, after: &fs::Metadata) -> bool {
    let (Ok(before_modified), Ok(after_modified)) = (before.modified(), after.modified()) else {
        return false;
    };
    if !before.file_type().is_file()
        || !after.file_type().is_file()
        || before.len() != after.len()
        || before_modified != after_modified
    {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        before.dev() == after.dev()
            && before.ino() == after.ino()
            && before.ctime() == after.ctime()
            && before.ctime_nsec() == after.ctime_nsec()
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn snapshot_from_document(
    binding: BusinessSnapshotSourceBinding,
    document: SourceDocument,
) -> anyhow::Result<VersionedBusinessSnapshot> {
    ensure!(
        document.schema == DOCUMENT_SCHEMA,
        "business snapshot document schema is unsupported"
    );
    VersionedBusinessSnapshot::new(
        binding.source,
        binding.location,
        document.object,
        document.revision,
        document.value,
    )
}

#[cfg(test)]
mod tests {
    use std::{fs, thread, time::Duration};

    use serde_json::json;

    use super::{BusinessSnapshotSourceId, FileBusinessSnapshotSource};

    fn document(revision: u64, excerpt_limit: u32) -> String {
        json!({
            "schema": "lenso.business-snapshot.v1",
            "revision": revision,
            "object": {
                "plugin_id": "company.notes",
                "instance_key": "default",
                "object_key": "excerpt-policy"
            },
            "value": { "excerpt_limit": excerpt_limit }
        })
        .to_string()
    }

    #[test]
    fn file_reader_rejects_detected_same_inode_rewrite() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let original = document(1, 96);
        let replacement = document(2, 48);
        assert_eq!(original.len(), replacement.len());
        fs::write(&path, original).unwrap();
        let before = fs::metadata(&path).unwrap();
        let source = FileBusinessSnapshotSource::new(
            &path,
            BusinessSnapshotSourceId::new("file", "operator-settings").unwrap(),
        );
        let read = source.read_with_after_bytes(|| {
            thread::sleep(Duration::from_millis(20));
            fs::write(&path, replacement).unwrap();
        });
        let after = fs::metadata(&path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
        }
        assert!(
            read.unwrap_err()
                .to_string()
                .contains("changed while reading")
        );
    }

    #[test]
    fn atomic_file_replacement_never_exposes_mixed_document() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let replacement = directory.path().join("settings-next.json");
        fs::write(&path, document(1, 96)).unwrap();
        fs::write(&replacement, document(2, 48)).unwrap();
        let source = FileBusinessSnapshotSource::new(
            &path,
            BusinessSnapshotSourceId::new("file", "operator-settings").unwrap(),
        );
        let read = source.read_with_after_bytes(|| fs::rename(&replacement, &path).unwrap());
        if let Ok(snapshot) = read {
            assert_eq!(snapshot.revision(), 1);
        }
        assert_eq!(source.read().unwrap().revision(), 2);
    }
}
