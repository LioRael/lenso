//! Exact generic Component adapters supplied by the JavaScript SDK.

use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

use anyhow::{Context, ensure};
use serde_json::{Value, json};

const V14: &[(&str, &str)] = &[(
    "component-requests.mjs",
    "b7f72c8dd0c14cd2a3e63a4ca2c04fddb08deadc76b7b0c238533dddb845576b",
)];
const V15: &[(&str, &str)] = &[
    (
        "component-admission.mjs",
        "e06d95bc3fe958f72e4eefd4c97a21f6a81b94a65dd6ca32e0e61bfa4de5b14e",
    ),
    (
        "component-requests.mjs",
        "b5e315b999b2ee6fa6ab374c8238dff4428cfa0b792270d0953e12560cac0b43",
    ),
];

#[derive(Debug)]
pub(super) struct PinnedRuntime {
    version: &'static str,
    files: Vec<(&'static str, Vec<u8>)>,
    digests: BTreeMap<&'static str, String>,
}

impl PinnedRuntime {
    pub(super) fn load(package: &Path) -> anyhow::Result<Self> {
        let package = fs::canonicalize(package).context("locate @lenso/workers-runtime package")?;
        let manifest: Value = serde_json::from_slice(&read_file(&package.join("package.json"))?)?;
        ensure!(
            manifest["name"] == "@lenso/workers-runtime"
                && manifest["exports"]["./component-requests"] == "./component-requests.mjs",
            "Workers runtime package must expose @lenso/workers-runtime/component-requests"
        );
        let (version, expected) = match manifest["version"].as_str() {
            Some("0.1.4") => ("0.1.4", V14),
            Some("0.1.5") => ("0.1.5", V15),
            _ => anyhow::bail!("Workers runtime requires pinned version 0.1.4 or 0.1.5"),
        };
        let mut files = Vec::new();
        let mut digests = BTreeMap::new();
        for &(name, expected_sha256) in expected {
            let bytes = read_file(&package.join(name))?;
            let digest = super::digest_bytes(&bytes);
            ensure!(
                digest == format!("sha256:{expected_sha256}"),
                "Workers runtime module {name} differs from pinned {version} bytes: {digest}"
            );
            files.push((name, bytes));
            digests.insert(name, digest);
        }
        Ok(Self {
            version,
            files,
            digests,
        })
    }

    pub(super) fn version(&self) -> &str {
        self.version
    }

    pub(super) fn stage(&self, stage: &Path) -> anyhow::Result<Value> {
        for (name, bytes) in &self.files {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(stage.join(name))
                .with_context(|| format!("create Workers runtime module {name}"))?
                .write_all(bytes)?;
        }
        if self.version == "0.1.4" {
            Ok(json!({
                "package": "@lenso/workers-runtime",
                "version": self.version,
                "module_digest": self.digests["component-requests.mjs"],
            }))
        } else {
            Ok(json!({
                "package": "@lenso/workers-runtime",
                "version": self.version,
                "module_digests": self.digests,
            }))
        }
    }
}

fn read_file(path: &Path) -> anyhow::Result<Vec<u8>> {
    #[cfg(unix)]
    let file = File::from(rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::CLOEXEC
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::NONBLOCK,
        rustix::fs::Mode::empty(),
    )?);
    #[cfg(not(unix))]
    let file = {
        ensure!(
            fs::symlink_metadata(path)?.file_type().is_file(),
            "Workers runtime input must be a regular file: {}",
            path.display()
        );
        File::open(path)?
    };
    let metadata = file.metadata()?;
    ensure!(
        metadata.file_type().is_file() && metadata.len() <= super::MAX_WORKERS_MODULE_BYTES,
        "Workers runtime input must be a bounded regular file: {}",
        path.display()
    );
    let mut bytes = Vec::new();
    file.take(super::MAX_WORKERS_MODULE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= super::MAX_WORKERS_MODULE_BYTES,
        "Workers runtime input grew beyond its bound"
    );
    Ok(bytes)
}
