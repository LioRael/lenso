use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
};

use anyhow::{Context as _, ensure};
use lenso_app_plan::authoring::PluginInstanceId;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

const MAX_PROFILE: u64 = 64 * 1024;
const MAX_FILE: u64 = 1024 * 1024;
const MAX_TOTAL: usize = 16 * 1024 * 1024;
const STAGED_PROFILE: &str = "workers-integration.json";

pub(super) struct Expected<'a> {
    pub plugin_id: &'a str,
    /// The full Plan key, not the profile's local instance suffix.
    pub instance_key: &'a str,
    pub authoring_version: u32,
    pub manifest_digest: &'a str,
    pub artifact_digest: &'a str,
    pub runtime_version: &'a str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    schema: String,
    plugin_id: String,
    instance_key: String,
    authoring_version: u32,
    world: String,
    manifest_digest: String,
    artifact_digest: String,
    runtime_version: String,
    files: BTreeMap<String, String>,
}

/// Captured, operator-pinned Host source. These modules are trusted code, not
/// sandboxed Bundle contents. The world is owner-declared metadata, not WIT proof.
pub(super) struct Integration {
    directory: PathBuf,
    profile_name: String,
    profile_bytes: Vec<u8>,
    profile_digest: String,
    profile: Profile,
    modules: BTreeMap<String, Vec<u8>>,
}

impl Integration {
    pub(super) fn load(
        path: &Path,
        trusted_digest: &str,
        expected: Expected<'_>,
    ) -> anyhow::Result<Self> {
        validate_digest(trusted_digest)?;
        validate_digest(expected.manifest_digest)?;
        validate_digest(expected.artifact_digest)?;
        let profile_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .context("Workers integration requires a profile filename")?
            .to_owned();
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let directory = parent
            .canonicalize()
            .context("canonicalize Workers integration directory")?;
        let profile_bytes = read_bounded(&directory.join(&profile_name), MAX_PROFILE)?;
        let profile_digest = digest(&profile_bytes);
        ensure!(
            profile_digest == trusted_digest,
            "Workers integration profile differs from the operator's trusted digest"
        );
        // Only authenticated bytes reach the JSON parser.
        let profile: Profile = serde_json::from_slice(&profile_bytes)
            .context("invalid Workers integration profile")?;
        validate_profile(&profile, &expected)?;
        ensure!(
            !profile.files.contains_key(&profile_name),
            "profile cannot also be a module"
        );
        check_closure(&directory, &profile_name, &profile.files)?;
        let mut modules = BTreeMap::new();
        let mut total = 0;
        for (name, expected_digest) in &profile.files {
            let bytes = read_bounded(&directory.join(name), MAX_FILE)?;
            total += bytes.len();
            ensure!(
                total <= MAX_TOTAL,
                "Workers integration exceeds total size limit"
            );
            ensure!(
                digest(&bytes) == *expected_digest,
                "Workers integration file digest differs: {name}"
            );
            modules.insert(name.clone(), bytes);
        }
        let integration = Self {
            directory,
            profile_name,
            profile_bytes,
            profile_digest,
            profile,
            modules,
        };
        integration.recheck()?;
        Ok(integration)
    }

    pub(super) fn world(&self) -> &str {
        &self.profile.world
    }

    /// Add captured bytes to the Host's staging directory without replacing
    /// outputs already created by the Host or Jco.
    pub(super) fn stage(&self, destination: &Path) -> anyhow::Result<Value> {
        ensure!(
            fs::symlink_metadata(destination)?.is_dir(),
            "Workers staging destination must be a directory"
        );
        let destination = destination.canonicalize()?;
        for name in self
            .modules
            .keys()
            .map(String::as_str)
            .chain(std::iter::once(STAGED_PROFILE))
        {
            match fs::symlink_metadata(destination.join(name)) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
                Ok(_) => anyhow::bail!("Workers integration would overwrite output: {name}"),
            }
        }
        for (name, bytes) in self
            .modules
            .iter()
            .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
            .chain(std::iter::once((
                STAGED_PROFILE,
                self.profile_bytes.as_slice(),
            )))
        {
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(destination.join(name))
                .with_context(|| format!("create Workers integration output {name}"))?
                .write_all(bytes)?;
        }
        Ok(json!({
            "profile_file": STAGED_PROFILE,
            "profile_digest": self.profile_digest,
            "world": self.profile.world,
            "plugin_id": self.profile.plugin_id,
            "instance_key": self.profile.instance_key,
            "authoring_version": self.profile.authoring_version,
            "manifest_digest": self.profile.manifest_digest,
            "artifact_digest": self.profile.artifact_digest,
            "runtime_version": self.profile.runtime_version,
            "module_digests": self.profile.files,
        }))
    }

    pub(super) fn recheck(&self) -> anyhow::Result<()> {
        check_closure(&self.directory, &self.profile_name, &self.profile.files)?;
        ensure!(
            read_bounded(&self.directory.join(&self.profile_name), MAX_PROFILE)?
                == self.profile_bytes,
            "Workers integration profile changed after capture"
        );
        for (name, bytes) in &self.modules {
            ensure!(
                read_bounded(&self.directory.join(name), MAX_FILE)? == *bytes,
                "Workers integration source changed after capture: {name}"
            );
        }
        check_closure(&self.directory, &self.profile_name, &self.profile.files)
    }
}

fn validate_profile(profile: &Profile, expected: &Expected<'_>) -> anyhow::Result<()> {
    ensure!(
        profile.schema == "lenso.workers-integration.v1",
        "unsupported Workers integration schema"
    );
    ensure!(
        !profile.plugin_id.is_empty()
            && profile.plugin_id == expected.plugin_id
            && valid_instance_suffix(&profile.instance_key)
            && PluginInstanceId::new(&profile.plugin_id, &profile.instance_key).plan_key()
                == expected.instance_key,
        "Workers integration Plugin identity or instance differs from the selected Plan"
    );
    ensure!(
        matches!(profile.authoring_version, 1 | 2)
            && profile.authoring_version == expected.authoring_version,
        "Workers integration authoring version differs or is unsupported"
    );
    ensure!(
        !profile.world.trim().is_empty()
            && profile.world.len() <= 256
            && !profile.world.chars().any(char::is_control),
        "Workers integration world must be a nonempty string of at most 256 bytes"
    );
    validate_digest(&profile.manifest_digest)?;
    validate_digest(&profile.artifact_digest)?;
    ensure!(
        profile.manifest_digest == expected.manifest_digest,
        "Workers integration Bundle manifest digest differs"
    );
    ensure!(
        profile.artifact_digest == expected.artifact_digest,
        "Workers integration Component artifact digest differs"
    );
    ensure!(
        matches!(profile.runtime_version.as_str(), "0.1.4" | "0.1.5")
            && profile.runtime_version == expected.runtime_version,
        "Workers integration runtime version differs or is unsupported"
    );
    ensure!(
        (2..=16).contains(&profile.files.len()),
        "Workers integration requires 2..16 files"
    );
    ensure!(
        profile.files.contains_key("worker.mjs") && profile.files.contains_key("README.md"),
        "Workers integration requires worker.mjs and README.md"
    );
    for (name, digest) in &profile.files {
        validate_name(name)?;
        validate_digest(digest)?;
    }
    Ok(())
}

fn valid_instance_suffix(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && !value.contains("..")
        && !value.contains(['/', '\\'])
        && !value.chars().any(char::is_control)
}

fn validate_name(name: &str) -> anyhow::Result<()> {
    ensure!(
        name == "README.md"
            || (name.ends_with(".mjs")
                && name.len() > 4
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))),
        "Workers integration file must be a flat ASCII .mjs filename or README.md"
    );
    ensure!(
        !name.contains("..")
            && !name.starts_with("guest.")
            && !matches!(
                name,
                "plan.mjs"
                    | "descriptor-digests.mjs"
                    | "artifact.mjs"
                    | "component-requests.mjs"
                    | "component-admission.mjs"
                    | "workers-http.mjs"
            ),
        "Workers integration file uses a reserved or unsafe name: {name}"
    );
    Ok(())
}

fn validate_digest(value: &str) -> anyhow::Result<()> {
    ensure!(
        value
            .strip_prefix("sha256:")
            .is_some_and(|hex| hex.len() == 64
                && hex
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))),
        "Workers integration digest must be sha256: followed by 64 lowercase hexadecimal digits"
    );
    Ok(())
}

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

fn check_closure(
    directory: &Path,
    profile: &str,
    files: &BTreeMap<String, String>,
) -> anyhow::Result<()> {
    ensure!(
        fs::symlink_metadata(directory)?.is_dir(),
        "Workers integration source must remain a regular directory"
    );
    let mut remaining: BTreeSet<&str> = files.keys().map(String::as_str).collect();
    remaining.insert(profile);
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name
            .to_str()
            .context("Workers integration has a non-UTF-8 filename")?;
        ensure!(
            remaining.remove(name),
            "Workers integration contains an unlisted file: {name}"
        );
        ensure!(
            entry.file_type()?.is_file(),
            "Workers integration requires regular files, not directories or symlinks: {name}"
        );
    }
    ensure!(
        remaining.is_empty(),
        "Workers integration is missing declared files"
    );
    Ok(())
}

fn read_bounded(path: &Path, limit: u64) -> anyhow::Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).context("inspect Workers integration source")?;
    ensure!(
        metadata.is_file() && metadata.len() <= limit,
        "Workers integration source is not a regular file or exceeds size limit"
    );
    #[cfg(unix)]
    let file = {
        use rustix::fs::{Mode, OFlags, open};
        fs::File::from(open(
            path,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )?)
    };
    #[cfg(windows)]
    let file = {
        use std::os::windows::fs::OpenOptionsExt as _;
        fs::OpenOptions::new()
            .read(true)
            .custom_flags(0x0020_0000)
            .open(path)?
    };
    #[cfg(not(any(unix, windows)))]
    let file = fs::File::open(path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= limit,
        "Workers integration opened source is not a regular file or exceeds size limit"
    );
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "Workers integration source exceeds size limit"
    );
    Ok(bytes)
}

#[cfg(test)]
#[path = "integration/tests.rs"]
mod tests;
