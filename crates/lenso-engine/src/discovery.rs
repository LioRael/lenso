//! Opt-in bounded acquisition shared by authoring readers within one epoch.
//! Selection belongs to callers. Start a fresh epoch for live freshness checks.
use anyhow::{Context, ensure};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize)]
pub struct AcquisitionStats {
    pub directory_reads: usize,
    pub file_reads: usize,
    pub json_parses: usize,
}

/// Acquisition budgets are policy; each reader still applies its own selection limits.
#[derive(Clone, Copy, Debug)]
pub struct DiscoveryLimits {
    pub entries: usize,
    pub files: usize,
    pub bytes: usize,
    pub file_bytes: usize,
}
impl Default for DiscoveryLimits {
    fn default() -> Self {
        Self {
            entries: 50_000,
            files: 50_000,
            bytes: 256 * 1024 * 1024,
            file_bytes: 16 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub path: String,
    pub kind: fs::FileType,
}

#[derive(Debug, Default)]
struct Acquired {
    directories: BTreeMap<PathBuf, Vec<(PathBuf, fs::FileType)>>,
    files: BTreeMap<PathBuf, Arc<[u8]>>,
    json: BTreeMap<PathBuf, Arc<serde_json::Value>>,
    stats: AcquisitionStats,
    bytes: usize,
    entries: usize,
}

/// One acquisition epoch; never execution or composition authority.
/// Views of separately selected roots share physical input identities.
#[derive(Debug)]
pub struct DiscoverySession {
    root: PathBuf,
    limits: DiscoveryLimits,
    acquired: Arc<Mutex<Acquired>>,
}

impl DiscoverySession {
    fn acquired(&self) -> MutexGuard<'_, Acquired> {
        self.acquired
            .lock()
            .expect("discovery acquisition poisoned")
    }
    pub fn new(root: &Path) -> anyhow::Result<Self> {
        Self::with_limits(root, DiscoveryLimits::default())
    }

    pub fn with_limits(root: &Path, limits: DiscoveryLimits) -> anyhow::Result<Self> {
        ensure!(
            fs::symlink_metadata(root)?.is_dir(),
            "discovery root must be a real directory"
        );
        Ok(Self {
            root: fs::canonicalize(root)?,
            limits,
            acquired: Arc::new(Mutex::new(Acquired::default())),
        })
    }

    /// Select another explicit root in the same epoch. The caller retains root
    /// admission policy; this does not discover, adopt, or execute anything.
    pub fn scope(&self, root: &Path) -> anyhow::Result<Self> {
        ensure!(
            fs::symlink_metadata(root)?.is_dir(),
            "discovery root must be a real directory"
        );
        Ok(Self {
            root: fs::canonicalize(root)?,
            limits: self.limits,
            acquired: self.acquired.clone(),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn stats(&self) -> AcquisitionStats {
        self.acquired().stats
    }

    /// Independent live observation for every scoped view of this session.
    pub fn begin_epoch(&mut self) {
        *self.acquired() = Acquired::default();
    }

    /// Invalidate an event path/subtree and ancestor listings across all views.
    /// Configuration/unknown events should instead start a fresh epoch.
    pub fn invalidate(&mut self, path: &str) -> anyhow::Result<()> {
        crate::validate_name(path)?;
        let path = self.root.join(path);
        let mut acquired = self.acquired();
        acquired.files.retain(|key, _| !key.starts_with(&path));
        acquired.json.retain(|key, _| !key.starts_with(&path));
        acquired
            .directories
            .retain(|key, _| !key.starts_with(&path) && !path.starts_with(key));
        acquired.bytes = acquired.files.values().map(|bytes| bytes.len()).sum();
        acquired.entries = acquired.directories.values().map(Vec::len).sum();
        Ok(())
    }

    fn contained(&self, path: &str) -> anyhow::Result<PathBuf> {
        if !path.is_empty() {
            crate::validate_name(path)?;
        }
        let mut current = self.root.clone();
        // Also reject a scoped root replaced by a symlink after acquisition.
        ensure!(
            fs::symlink_metadata(&current)?.is_dir(),
            "discovery root changed"
        );
        for component in Path::new(path).components() {
            current.push(component);
            ensure!(
                !fs::symlink_metadata(&current)?.file_type().is_symlink(),
                "symlink in discovery input: {path}"
            );
        }
        Ok(current)
    }

    /// Lists only the requested directory; overlapping scoped roots reuse it.
    pub fn directory(&mut self, path: &str) -> anyhow::Result<Vec<Entry>> {
        let source = self.contained(path)?;
        let mut acquired = self.acquired();
        if !acquired.directories.contains_key(&source) {
            ensure!(
                fs::symlink_metadata(&source)?.is_dir(),
                "discovery input is not a directory: {path}"
            );
            let mut entries = Vec::new();
            acquired.stats.directory_reads += 1;
            for entry in fs::read_dir(&source)? {
                let entry = entry?;
                ensure!(
                    acquired.entries + entries.len() < self.limits.entries,
                    "discovery exceeds directory entry budget"
                );
                ensure!(
                    entry.file_name().to_str().is_some(),
                    "non-UTF8 discovery input"
                );
                entries.push((entry.path(), entry.file_type()?));
            }
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            acquired.entries += entries.len();
            acquired.directories.insert(source.clone(), entries);
        }
        acquired.directories[&source]
            .iter()
            .map(|(path, kind)| {
                Ok(Entry {
                    path: path
                        .strip_prefix(&self.root)?
                        .to_str()
                        .context("UTF8 discovery path")?
                        .replace('\\', "/"),
                    kind: *kind,
                })
            })
            .collect()
    }

    /// Reads one bounded regular file once; tighter caller limits apply on hits.
    pub fn read(&mut self, path: &str, limit: usize) -> anyhow::Result<Arc<[u8]>> {
        ensure!(
            limit <= self.limits.file_bytes,
            "discovery file limit exceeds session budget"
        );
        crate::validate_name(path)?;
        let source = self.contained(path)?;
        let mut acquired = self.acquired();
        if let Some(bytes) = acquired.files.get(&source) {
            ensure!(
                bytes.len() <= limit,
                "discovery input exceeds byte budget: {path}"
            );
            return Ok(bytes.clone());
        }
        let metadata = fs::symlink_metadata(&source)?;
        ensure!(
            metadata.is_file() && metadata.len() <= limit as u64,
            "discovery input must be a bounded regular file: {path}"
        );
        ensure!(
            acquired.files.len() < self.limits.files,
            "discovery exceeds file budget"
        );
        let mut bytes = Vec::new();
        fs::File::open(&source)?
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= limit && acquired.bytes + bytes.len() <= self.limits.bytes,
            "discovery exceeds byte budget: {path}"
        );
        acquired.stats.file_reads += 1;
        acquired.bytes += bytes.len();
        let bytes: Arc<[u8]> = bytes.into();
        acquired.files.insert(source, bytes.clone());
        Ok(bytes)
    }

    pub fn json(&mut self, path: &str, limit: usize) -> anyhow::Result<Arc<serde_json::Value>> {
        let bytes = self.read(path, limit)?;
        let source = self.root.join(path);
        let mut acquired = self.acquired();
        if let Some(value) = acquired.json.get(&source) {
            return Ok(value.clone());
        }
        let value = Arc::new(
            serde_json::from_slice::<serde_json::Value>(&bytes)
                .with_context(|| format!("parse JSON input {path}"))?,
        );
        acquired.stats.json_parses += 1;
        acquired.json.insert(source, value.clone());
        Ok(value)
    }
}
