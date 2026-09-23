use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::Read as _,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context as _, ensure};
use ed25519_dalek::VerifyingKey;
use lenso_plugin_catalog::{Availability, MAX_ENVELOPE_BYTES, Trust, verify_for_browse};
use serde::{Deserialize, Serialize};

const MAX_TRUST_BYTES: u64 = 4096;
const MAX_QUERY_BYTES: usize = 256;
const MAX_PAGE_SIZE: usize = 20;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrustFile {
    catalog_id: String,
    key_id: String,
    public_key_hex: String,
}

#[derive(Clone, Copy, Debug)]
pub struct PortableCatalogQuery<'a> {
    pub snapshot: &'a Path,
    pub trust: &'a Path,
    pub query: &'a str,
    pub offset: usize,
    pub limit: usize,
}

#[derive(Debug, Serialize)]
pub struct PortableCatalogPage {
    pub schema_version: u32,
    pub kind: &'static str,
    pub catalog_id: String,
    pub revision: u64,
    pub payload_digest: String,
    pub issued_at: u64,
    pub expires_at: u64,
    pub stale: bool,
    pub provenance: &'static str,
    pub history: &'static str,
    pub target_compatibility: &'static str,
    pub installation: &'static str,
    pub publisher_text_is_untrusted: bool,
    pub total_releases: usize,
    pub offset: usize,
    pub next_offset: Option<usize>,
    pub releases: Vec<PortableCatalogCandidate>,
}

#[derive(Debug, Serialize)]
pub struct PortableCatalogCandidate {
    pub plugin_id: String,
    pub version: String,
    pub publisher_id: String,
    pub title: String,
    pub summary: String,
    pub source_url: String,
    pub source_revision: String,
    pub license: String,
    pub availability: Availability,
    pub artifact_digest: String,
    pub artifact_size: u64,
}

pub fn inspect(request: PortableCatalogQuery<'_>) -> anyhow::Result<PortableCatalogPage> {
    ensure!(
        request.query.len() <= MAX_QUERY_BYTES,
        "signed Portable catalog query exceeds 256 bytes"
    );
    ensure!(
        request.offset <= lenso_plugin_catalog::MAX_RELEASES,
        "signed Portable catalog offset exceeds release bound"
    );
    ensure!(
        (1..=MAX_PAGE_SIZE).contains(&request.limit),
        "signed Portable catalog page limit must be from 1 to 20"
    );
    let trust: TrustFile = serde_json::from_slice(&read_bounded(request.trust, MAX_TRUST_BYTES)?)
        .context("decode signed Portable catalog trust")?;
    let key: [u8; 32] = hex::decode(trust.public_key_hex)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("public trust key must have 32 bytes"))?;
    let trust = Trust {
        catalog_id: trust.catalog_id,
        keys: BTreeMap::from([(trust.key_id, VerifyingKey::from_bytes(&key)?)]),
    };
    let bytes = read_bounded(request.snapshot, u64::try_from(MAX_ENVELOPE_BYTES)?)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time is before Unix epoch")?
        .as_secs();
    let verified = verify_for_browse(&bytes, &trust, None, now)?;
    let snapshot = verified.snapshot();
    let query = request.query.to_lowercase();
    let mut matching = snapshot
        .releases
        .iter()
        .filter(|release| {
            query.is_empty()
                || release.plugin_id.to_lowercase().contains(&query)
                || release.title.to_lowercase().contains(&query)
                || release.summary.to_lowercase().contains(&query)
        })
        .collect::<Vec<_>>();
    matching.sort_by(|left, right| {
        (&left.plugin_id, &left.version).cmp(&(&right.plugin_id, &right.version))
    });
    let total_releases = matching.len();
    let releases = matching
        .into_iter()
        .skip(request.offset)
        .take(request.limit)
        .map(|release| PortableCatalogCandidate {
            plugin_id: release.plugin_id.clone(),
            version: release.version.clone(),
            publisher_id: release.publisher_id.clone(),
            title: release.title.clone(),
            summary: release.summary.clone(),
            source_url: release.source_url.clone(),
            source_revision: release.source_revision.clone(),
            license: release.license.clone(),
            availability: release.availability.clone(),
            artifact_digest: release.artifact.digest.clone(),
            artifact_size: release.artifact.size,
        })
        .collect::<Vec<_>>();
    let next = request.offset.saturating_add(releases.len());
    Ok(PortableCatalogPage {
        schema_version: 1,
        kind: "lenso.signed-portable-catalog",
        catalog_id: snapshot.catalog_id.clone(),
        revision: snapshot.revision,
        payload_digest: verified.checkpoint().payload_digest.clone(),
        issued_at: snapshot.issued_at,
        expires_at: snapshot.expires_at,
        stale: verified.is_stale(now),
        provenance: "signature_verified",
        history: "not_checked",
        target_compatibility: "not_verified",
        installation: "not_authorized",
        publisher_text_is_untrusted: true,
        total_releases,
        offset: request.offset,
        next_offset: (next < total_releases).then_some(next),
        releases,
    })
}

fn read_bounded(path: &Path, max_bytes: u64) -> anyhow::Result<Vec<u8>> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .with_context(|| format!("open {}", path.display()))?;
    ensure!(
        file.metadata()?.is_file(),
        "signed catalog input is not a regular file"
    );
    let mut bytes = Vec::new();
    file.take(
        max_bytes
            .checked_add(1)
            .context("signed catalog size overflow")?,
    )
    .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= max_bytes,
        "signed catalog input exceeds size limit"
    );
    Ok(bytes)
}
