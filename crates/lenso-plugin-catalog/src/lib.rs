//! Marketplace release identity, detached signatures and target-independent verification.
//!
//! Verification does not grant installation or execution authority. Trust anchors and
//! the last accepted checkpoint must come from the caller's durable configuration.

use anyhow::{Context, Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, Signer as _, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub mod identity;

#[cfg(feature = "bundle-verification")]
mod native;

const SCHEMA: &str = "lenso.marketplace.snapshot.v1";
const SIGNATURE_CONTEXT: &[u8] = b"lenso.marketplace.snapshot.v1\0";
const DETAILS_SCHEMA: &str = "lenso.marketplace.release-details.v1";
const DETAILS_SIGNATURE_CONTEXT: &[u8] = b"lenso.marketplace.release-details.v1\0";
const MAX_DETAILS_RELEASE_HISTORY: usize = 16_384;
const MAX_DETAILS_DOCUMENT_HISTORY: usize = 65_536;
const MAX_DETAILS_HISTORY_BYTES: usize = 8 * 1024 * 1024;
const MAX_DETAILS_HISTORY_IDENTITY_BYTES: usize = 640;
pub const MAX_ENVELOPE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_RELEASES: usize = 4096;
const MAX_VALIDITY_SECONDS: u64 = 7 * 24 * 60 * 60;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub url: String,
    pub digest: String,
    pub size: u64,
    pub manifest_digest: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Listed,
    Yanked,
    Revoked,
}

/// Optional publisher-authored display metadata. URLs are signed references,
/// not a claim that remote image bytes were verified or are immutable.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Presentation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub screenshots: Vec<Screenshot>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub getting_started: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Screenshot {
    pub url: String,
    pub caption: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Release {
    pub plugin_id: String,
    pub version: String,
    pub publisher_id: String,
    pub title: String,
    pub summary: String,
    /// Publisher-authored plain text, covered by the catalog signature.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<Presentation>,
    pub source_url: String,
    pub source_revision: String,
    pub license: String,
    pub artifact: Artifact,
    pub availability: Availability,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DistributionKind {
    PortableBundle,
    CargoPackage,
    NpmPackage,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Distribution {
    pub id: String,
    pub kind: DistributionKind,
    pub package: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integrity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<Artifact>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Documentation {
    pub id: String,
    pub revision: String,
    pub language: String,
    pub topic: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub url: String,
    pub digest: String,
    pub size: u64,
    pub media_type: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReleaseDetails {
    pub plugin_id: String,
    pub version: String,
    pub base_release_identity: String,
    pub distributions: Vec<Distribution>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documentation: Vec<Documentation>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReleaseDetailsSnapshot {
    pub schema: String,
    pub catalog_id: String,
    pub revision: u64,
    pub issued_at: u64,
    pub expires_at: u64,
    pub releases: Vec<ReleaseDetails>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReleaseDetailsCheckpoint {
    pub catalog_id: String,
    pub revision: u64,
    pub payload_digest: String,
    pub release_identities: BTreeMap<String, String>,
    pub document_identities: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct VerifiedReleaseDetails {
    snapshot: ReleaseDetailsSnapshot,
    checkpoint: ReleaseDetailsCheckpoint,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub schema: String,
    pub catalog_id: String,
    pub revision: u64,
    pub issued_at: u64,
    pub expires_at: u64,
    pub releases: Vec<Release>,
}

/// The signature covers the exact decoded payload bytes, not re-serialized JSON.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub key_id: String,
    pub payload_base64: String,
    pub signature_base64: String,
}

#[derive(Clone, Debug)]
pub struct Trust {
    pub catalog_id: String,
    pub keys: BTreeMap<String, VerifyingKey>,
}

/// Persist only after successful verification, atomically with the accepted snapshot.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub catalog_id: String,
    pub revision: u64,
    pub payload_digest: String,
    pub release_identities: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct VerifiedSnapshot {
    snapshot: Snapshot,
    checkpoint: Checkpoint,
}

/// Integrity-verified metadata for browsing. This type cannot select an installation.
#[derive(Clone, Debug)]
pub struct BrowseSnapshot {
    snapshot: Snapshot,
    checkpoint: Checkpoint,
}

impl BrowseSnapshot {
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }
    pub fn checkpoint(&self) -> &Checkpoint {
        &self.checkpoint
    }
    pub fn is_stale(&self, now: u64) -> bool {
        now >= self.snapshot.expires_at
    }

    /// Joins additive details only to an exact release present in this verified catalog.
    pub fn release_details<'a>(
        &self,
        details: &'a VerifiedReleaseDetails,
        plugin_id: &str,
        version: &str,
    ) -> Result<&'a ReleaseDetails> {
        ensure!(
            self.snapshot.catalog_id == details.snapshot.catalog_id,
            "release details belong to another catalog"
        );
        let release = self
            .snapshot
            .releases
            .iter()
            .find(|release| release.plugin_id == plugin_id && release.version == version)
            .context("exact release is not in this catalog")?;
        let selected = details.find(plugin_id, version)?;
        selected.validate_against(release)?;
        Ok(selected)
    }
}

impl VerifiedSnapshot {
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }
    pub fn checkpoint(&self) -> &Checkpoint {
        &self.checkpoint
    }

    /// Exact selection only. Freshness is rechecked at use, even after verification.
    pub fn select(&self, plugin_id: &str, version: &str, now: u64) -> Result<&Release> {
        ensure!(
            now >= self.snapshot.issued_at && now < self.snapshot.expires_at,
            "catalog is not current"
        );
        let release = self
            .snapshot
            .releases
            .iter()
            .find(|release| release.plugin_id == plugin_id && release.version == version)
            .context("exact release is not in this catalog")?;
        ensure!(
            release.availability == Availability::Listed,
            "release is not available for new installation"
        );
        Ok(release)
    }

    /// Search returns published metadata, not target compatibility or installation grants.
    pub fn search(&self, query: &str, offset: usize, limit: usize) -> Result<Vec<&Release>> {
        ensure!(
            query.len() <= 256 && (1..=100).contains(&limit),
            "search exceeds bounds"
        );
        let query = query.to_lowercase();
        let mut found: Vec<_> = self
            .snapshot
            .releases
            .iter()
            .filter(|r| {
                r.availability == Availability::Listed
                    && (r.plugin_id.contains(&query)
                        || r.title.to_lowercase().contains(&query)
                        || r.summary.to_lowercase().contains(&query))
            })
            .collect();
        found.sort_by(|a, b| {
            (a.plugin_id != query, &a.plugin_id, &a.version).cmp(&(
                b.plugin_id != query,
                &b.plugin_id,
                &b.version,
            ))
        });
        Ok(found.into_iter().skip(offset).take(limit).collect())
    }

    /// Selects the base release first, then joins separately signed additive details.
    pub fn select_details<'a>(
        &self,
        details: &'a VerifiedReleaseDetails,
        plugin_id: &str,
        version: &str,
        now: u64,
    ) -> Result<&'a ReleaseDetails> {
        let release = self.select(plugin_id, version, now)?;
        ensure!(
            self.snapshot.catalog_id == details.snapshot.catalog_id,
            "release details belong to another catalog"
        );
        details.ensure_current(now)?;
        let selected = details.find(plugin_id, version)?;
        selected.validate_against(release)?;
        Ok(selected)
    }
}

pub fn digest(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

impl Snapshot {
    pub fn new(
        catalog_id: String,
        revision: u64,
        issued_at: u64,
        expires_at: u64,
        releases: Vec<Release>,
    ) -> Self {
        Self {
            schema: SCHEMA.into(),
            catalog_id,
            revision,
            issued_at,
            expires_at,
            releases,
        }
    }

    fn validate(&self) -> Result<()> {
        ensure!(self.schema == SCHEMA, "unsupported catalog schema");
        bounded_text(&self.catalog_id, 128)?;
        ensure!(
            self.revision <= 9_007_199_254_740_991 && self.expires_at <= 9_007_199_254_740_991,
            "catalog integers exceed portable JSON precision"
        );
        ensure!(
            self.revision > 0
                && self.expires_at > self.issued_at
                && self.expires_at - self.issued_at <= MAX_VALIDITY_SECONDS,
            "invalid revision or validity window"
        );
        ensure!(self.releases.len() <= MAX_RELEASES, "too many releases");
        let mut identities = BTreeSet::new();
        for release in &self.releases {
            release.validate()?;
            ensure!(
                identities.insert((&release.plugin_id, &release.version)),
                "duplicate release identity"
            );
        }
        Ok(())
    }
}

impl Release {
    pub fn immutable_identity(&self) -> Result<String> {
        Ok(digest(&serde_json::to_vec(&(
            &self.publisher_id,
            &self.source_url,
            &self.source_revision,
            &self.artifact.digest,
            self.artifact.size,
            &self.artifact.manifest_digest,
        ))?))
    }

    pub fn validate(&self) -> Result<()> {
        crate::identity::validate_plugin_id_v1(&self.plugin_id)?;
        crate::identity::validate_release_version(&self.version)?;
        for (field, max) in [
            (&self.publisher_id, 128),
            (&self.title, 160),
            (&self.summary, 2048),
            (&self.source_revision, 128),
            (&self.license, 128),
        ] {
            bounded_text(field, max)?;
        }
        ensure!(
            self.description.len() <= 16_384,
            "description exceeds bounds"
        );
        ensure!(
            !self
                .description
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')),
            "description contains control characters"
        );
        if let Some(presentation) = &self.presentation {
            if let Some(url) = &presentation.icon_url {
                https_url(url)?;
            }
            ensure!(presentation.screenshots.len() <= 6, "too many screenshots");
            let mut urls = BTreeSet::new();
            for screenshot in &presentation.screenshots {
                https_url(&screenshot.url)?;
                bounded_text(&screenshot.caption, 320)?;
                ensure!(urls.insert(&screenshot.url), "duplicate screenshot URL");
            }
            ensure!(
                presentation.getting_started.len() <= 16_384,
                "getting started exceeds bounds"
            );
            ensure!(
                !presentation
                    .getting_started
                    .chars()
                    .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')),
                "getting started contains control characters"
            );
        }
        https_url(&self.source_url)?;
        ensure!(
            matches!(self.source_revision.len(), 40 | 64)
                && self
                    .source_revision
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "source must name an exact commit digest"
        );
        https_url(&self.artifact.url)?;
        valid_digest(&self.artifact.digest)?;
        valid_digest(&self.artifact.manifest_digest)?;
        ensure!(
            self.artifact.size > 0 && self.artifact.size <= 256 * 1024 * 1024,
            "artifact size exceeds bounds"
        );
        Ok(())
    }

    /// Compare received archive bytes to the signed release before extraction.
    /// Download/extraction and origin policy remain owned by the existing installer.
    pub fn verify_archive_bytes(&self, bytes: &[u8]) -> Result<()> {
        self.validate()?;
        ensure!(
            bytes.len() as u64 == self.artifact.size && digest(bytes) == self.artifact.digest,
            "archive size or digest mismatch"
        );
        Ok(())
    }
}

impl ReleaseDetailsSnapshot {
    pub fn new(
        catalog_id: String,
        revision: u64,
        issued_at: u64,
        expires_at: u64,
        releases: Vec<ReleaseDetails>,
    ) -> Self {
        Self {
            schema: DETAILS_SCHEMA.into(),
            catalog_id,
            revision,
            issued_at,
            expires_at,
            releases,
        }
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == DETAILS_SCHEMA,
            "unsupported release details schema"
        );
        bounded_text(&self.catalog_id, 128)?;
        ensure!(
            self.revision > 0
                && self.revision <= 9_007_199_254_740_991
                && self.expires_at > self.issued_at
                && self.expires_at <= 9_007_199_254_740_991
                && self.expires_at - self.issued_at <= MAX_VALIDITY_SECONDS,
            "invalid revision or validity window"
        );
        ensure!(
            self.releases.len() <= MAX_RELEASES,
            "too many release details"
        );
        let mut identities = BTreeSet::new();
        for release in &self.releases {
            release.validate()?;
            ensure!(
                identities.insert((&release.plugin_id, &release.version)),
                "duplicate release details identity"
            );
        }
        Ok(())
    }
}

impl ReleaseDetails {
    pub fn validate(&self) -> Result<()> {
        identity::validate_plugin_id_v1(&self.plugin_id)?;
        bounded_text(&self.version, 128)?;
        identity::validate_release_version(&self.version)?;
        valid_digest(&self.base_release_identity)?;
        ensure!(
            !self.distributions.is_empty() && self.distributions.len() <= 16,
            "release details need one to sixteen distributions"
        );
        ensure!(
            self.documentation.len() <= 64,
            "too many documentation resources"
        );
        let mut distribution_ids = BTreeSet::new();
        for distribution in &self.distributions {
            distribution.validate()?;
            ensure!(
                distribution.version == self.version,
                "distribution version does not match release details"
            );
            ensure!(
                distribution_ids.insert(&distribution.id),
                "duplicate distribution identity"
            );
        }
        let mut document_ids = BTreeSet::new();
        for document in &self.documentation {
            document.validate()?;
            ensure!(
                document_ids.insert(format!("{}@{}", document.id, document.revision)),
                "duplicate documentation identity"
            );
        }
        Ok(())
    }

    /// Validate this additive document against the exact immutable v1 release.
    /// Publishers use this before accepting details; consumers repeat it after
    /// independently verifying both signed documents.
    pub fn validate_against(&self, release: &Release) -> Result<()> {
        self.validate()?;
        release.validate()?;
        ensure!(
            self.plugin_id == release.plugin_id && self.version == release.version,
            "release details identity does not match the base release"
        );
        ensure!(
            self.base_release_identity == release.immutable_identity()?,
            "release details do not match the immutable base release"
        );
        ensure!(
            self.distributions.iter().any(|distribution| {
                distribution.kind == DistributionKind::PortableBundle
                    && distribution.artifact.as_ref().is_some_and(|artifact| {
                        artifact.digest == release.artifact.digest
                            && artifact.size == release.artifact.size
                            && artifact.manifest_digest == release.artifact.manifest_digest
                    })
            }),
            "release details do not retain the base portable artifact"
        );
        Ok(())
    }
}

impl Distribution {
    pub fn validate(&self) -> Result<()> {
        bounded_text(&self.id, 128)?;
        bounded_text(&self.package, 256)?;
        bounded_text(&self.version, 128)?;
        identity::validate_release_version(&self.version)?;
        ensure!(self.targets.len() <= 32, "too many distribution targets");
        let mut targets = BTreeSet::new();
        for target in &self.targets {
            bounded_text(target, 128)?;
            ensure!(targets.insert(target), "duplicate distribution target");
        }
        match self.kind {
            DistributionKind::PortableBundle => {
                ensure!(
                    self.registry_url.is_none(),
                    "portable bundle cannot name a registry"
                );
                ensure!(
                    self.integrity.is_none(),
                    "portable bundle uses artifact digest"
                );
                validate_artifact(
                    self.artifact
                        .as_ref()
                        .context("portable bundle needs artifact")?,
                )?;
            }
            DistributionKind::CargoPackage | DistributionKind::NpmPackage => {
                ensure!(
                    self.artifact.is_none(),
                    "package distribution cannot embed an artifact"
                );
                https_url(
                    self.registry_url
                        .as_deref()
                        .context("package distribution needs registry URL")?,
                )?;
                valid_digest(
                    self.integrity
                        .as_deref()
                        .context("package distribution needs integrity digest")?,
                )?;
                validate_package_name(&self.kind, &self.package)?;
            }
        }
        Ok(())
    }
}

impl Documentation {
    pub fn validate(&self) -> Result<()> {
        bounded_text(&self.id, 128)?;
        bounded_text(&self.revision, 128)?;
        bounded_text(&self.language, 32)?;
        bounded_text(&self.topic, 128)?;
        if let Some(target) = &self.target {
            bounded_text(target, 128)?;
        }
        https_url(&self.url)?;
        valid_digest(&self.digest)?;
        ensure!(
            self.size > 0 && self.size <= 1024 * 1024,
            "documentation size exceeds bounds"
        );
        ensure!(
            self.media_type == "text/markdown",
            "unsupported documentation media type"
        );
        Ok(())
    }
}

impl VerifiedReleaseDetails {
    pub fn snapshot(&self) -> &ReleaseDetailsSnapshot {
        &self.snapshot
    }
    pub fn checkpoint(&self) -> &ReleaseDetailsCheckpoint {
        &self.checkpoint
    }
    fn ensure_current(&self, now: u64) -> Result<()> {
        ensure!(
            now >= self.snapshot.issued_at && now < self.snapshot.expires_at,
            "release details are not current"
        );
        Ok(())
    }

    fn find(&self, plugin_id: &str, version: &str) -> Result<&ReleaseDetails> {
        self.snapshot
            .releases
            .iter()
            .find(|release| release.plugin_id == plugin_id && release.version == version)
            .context("exact release details are not in this catalog")
    }
}

pub fn sign_release_details(
    snapshot: &ReleaseDetailsSnapshot,
    key_id: &str,
    key: &SigningKey,
) -> Result<Vec<u8>> {
    snapshot.validate()?;
    bounded_text(key_id, 128)?;
    let payload = serde_json::to_vec(snapshot)?;
    let envelope = Envelope {
        key_id: key_id.into(),
        signature_base64: STANDARD.encode(
            key.sign(&details_signing_bytes(key_id, &payload))
                .to_bytes(),
        ),
        payload_base64: STANDARD.encode(payload),
    };
    let bytes = serde_json::to_vec(&envelope)?;
    ensure!(
        bytes.len() <= MAX_ENVELOPE_BYTES,
        "release details exceed size limit"
    );
    Ok(bytes)
}

pub fn verify_release_details(
    bytes: &[u8],
    trust: &Trust,
    previous: Option<&ReleaseDetailsCheckpoint>,
    now: u64,
) -> Result<VerifiedReleaseDetails> {
    ensure!(
        bytes.len() <= MAX_ENVELOPE_BYTES,
        "release details exceed size limit"
    );
    let envelope: Envelope = serde_json::from_slice(bytes)?;
    bounded_text(&envelope.key_id, 128)?;
    let key = trust
        .keys
        .get(&envelope.key_id)
        .context("unknown signing key")?;
    let payload = STANDARD.decode(&envelope.payload_base64)?;
    let signature = Signature::from_slice(&STANDARD.decode(&envelope.signature_base64)?)?;
    key.verify_strict(
        &details_signing_bytes(&envelope.key_id, &payload),
        &signature,
    )?;
    let snapshot: ReleaseDetailsSnapshot = serde_json::from_slice(&payload)?;
    snapshot.validate()?;
    ensure!(
        snapshot.catalog_id == trust.catalog_id,
        "unexpected catalog identity"
    );
    ensure!(
        now >= snapshot.issued_at && now < snapshot.expires_at,
        "release details are expired or not yet valid"
    );
    if let Some(previous) = previous {
        validate_details_checkpoint(previous)?;
    }
    let mut release_identities = previous
        .map(|checkpoint| checkpoint.release_identities.clone())
        .unwrap_or_default();
    let mut document_identities = previous
        .map(|checkpoint| checkpoint.document_identities.clone())
        .unwrap_or_default();
    for release in &snapshot.releases {
        let identity = format!("{}@{}", release.plugin_id, release.version);
        let immutable = digest(&serde_json::to_vec(&(
            &release.plugin_id,
            &release.version,
            &release.base_release_identity,
            &release.distributions,
        ))?);
        if let Some(old) = release_identities.get(&identity) {
            ensure!(
                old == &immutable,
                "published release details changed: {identity}"
            );
        }
        release_identities.insert(identity, immutable);
        for document in &release.documentation {
            let identity = format!(
                "{}@{}/{}@{}",
                release.plugin_id, release.version, document.id, document.revision
            );
            let immutable = digest(&serde_json::to_vec(document)?);
            if let Some(old) = document_identities.get(&identity) {
                ensure!(
                    old == &immutable,
                    "published documentation revision changed: {identity}"
                );
            }
            document_identities.insert(identity, immutable);
        }
    }
    let checkpoint = ReleaseDetailsCheckpoint {
        catalog_id: snapshot.catalog_id.clone(),
        revision: snapshot.revision,
        payload_digest: digest(&payload),
        release_identities,
        document_identities,
    };
    validate_details_checkpoint(&checkpoint)?;
    if let Some(previous) = previous {
        ensure!(
            previous.catalog_id == checkpoint.catalog_id,
            "checkpoint belongs to another catalog"
        );
        ensure!(
            checkpoint.revision >= previous.revision,
            "release details rollback rejected"
        );
        ensure!(
            checkpoint.revision != previous.revision
                || checkpoint.payload_digest == previous.payload_digest,
            "release details revision equivocation rejected"
        );
    }
    Ok(VerifiedReleaseDetails {
        snapshot,
        checkpoint,
    })
}

pub fn bounded_text(value: &str, max: usize) -> Result<()> {
    ensure!(
        !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control),
        "invalid text field"
    );
    Ok(())
}
fn valid_digest(value: &str) -> Result<()> {
    let hex = value
        .strip_prefix("sha256:")
        .context("unsupported digest")?;
    ensure!(
        hex.len() == 64
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid SHA-256 digest"
    );
    Ok(())
}
fn validate_artifact(artifact: &Artifact) -> Result<()> {
    https_url(&artifact.url)?;
    valid_digest(&artifact.digest)?;
    valid_digest(&artifact.manifest_digest)?;
    ensure!(
        artifact.size > 0 && artifact.size <= 256 * 1024 * 1024,
        "artifact size exceeds bounds"
    );
    Ok(())
}
fn validate_details_checkpoint(checkpoint: &ReleaseDetailsCheckpoint) -> Result<()> {
    bounded_text(&checkpoint.catalog_id, 128)?;
    ensure!(
        checkpoint.revision > 0 && checkpoint.revision <= 9_007_199_254_740_991,
        "invalid release details checkpoint revision"
    );
    valid_digest(&checkpoint.payload_digest)?;
    ensure!(
        checkpoint.release_identities.len() <= MAX_DETAILS_RELEASE_HISTORY,
        "release details history exceeds entry limit"
    );
    ensure!(
        checkpoint.document_identities.len() <= MAX_DETAILS_DOCUMENT_HISTORY,
        "documentation history exceeds entry limit"
    );
    let mut bytes = 0usize;
    for (identity, digest) in checkpoint
        .release_identities
        .iter()
        .chain(checkpoint.document_identities.iter())
    {
        ensure!(
            identity.len() <= MAX_DETAILS_HISTORY_IDENTITY_BYTES,
            "checkpoint identity exceeds limit"
        );
        valid_digest(digest)?;
        bytes = bytes
            .checked_add(identity.len())
            .and_then(|size| size.checked_add(digest.len()))
            .context("release details history size overflow")?;
        ensure!(
            bytes <= MAX_DETAILS_HISTORY_BYTES,
            "release details history exceeds byte limit"
        );
    }
    Ok(())
}
fn validate_package_name(kind: &DistributionKind, package: &str) -> Result<()> {
    let valid = match kind {
        DistributionKind::CargoPackage => {
            package.len() <= 64
                && package
                    .bytes()
                    .next()
                    .is_some_and(|byte| byte.is_ascii_lowercase())
                && package.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b'-')
                })
        }
        DistributionKind::NpmPackage => validate_npm_package_name(package),
        DistributionKind::PortableBundle => true,
    };
    ensure!(valid, "invalid package name");
    Ok(())
}
fn validate_npm_package_name(package: &str) -> bool {
    if package.len() > 214 {
        return false;
    }
    let valid_segment = |segment: &str| {
        !segment.is_empty()
            && !matches!(segment.as_bytes().first(), Some(b'.' | b'_'))
            && segment.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'_' | b'-')
            })
    };
    if let Some(scoped) = package.strip_prefix('@') {
        let Some((scope, name)) = scoped.split_once('/') else {
            return false;
        };
        !name.contains('/') && valid_segment(scope) && valid_segment(name)
    } else {
        !package.contains('/') && valid_segment(package)
    }
}
fn https_url(value: &str) -> Result<()> {
    ensure!(value.len() <= 2048, "URL exceeds bounds");
    let url = url::Url::parse(value)?;
    ensure!(
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.fragment().is_none(),
        "expected credential-free HTTPS URL"
    );
    Ok(())
}

fn signing_bytes(key_id: &str, payload: &[u8]) -> Vec<u8> {
    let mut bytes = SIGNATURE_CONTEXT.to_vec();
    bytes.extend_from_slice(key_id.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(payload);
    bytes
}

fn details_signing_bytes(key_id: &str, payload: &[u8]) -> Vec<u8> {
    let mut bytes = DETAILS_SIGNATURE_CONTEXT.to_vec();
    bytes.extend_from_slice(key_id.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(payload);
    bytes
}

pub fn sign(snapshot: &Snapshot, key_id: &str, key: &SigningKey) -> Result<Vec<u8>> {
    snapshot.validate()?;
    bounded_text(key_id, 128)?;
    let payload = serde_json::to_vec(snapshot)?;
    let envelope = Envelope {
        key_id: key_id.into(),
        signature_base64: STANDARD.encode(key.sign(&signing_bytes(key_id, &payload)).to_bytes()),
        payload_base64: STANDARD.encode(payload),
    };
    let bytes = serde_json::to_vec(&envelope)?;
    ensure!(
        bytes.len() <= MAX_ENVELOPE_BYTES,
        "catalog exceeds size limit"
    );
    Ok(bytes)
}

pub fn verify(
    bytes: &[u8],
    trust: &Trust,
    previous: Option<&Checkpoint>,
    now: u64,
) -> Result<VerifiedSnapshot> {
    let browse = verify_for_browse(bytes, trust, previous, now)?;
    ensure!(!browse.is_stale(now), "catalog is expired or not yet valid");
    Ok(VerifiedSnapshot {
        snapshot: browse.snapshot,
        checkpoint: browse.checkpoint,
    })
}

/// Verify provenance and history for display, permitting expired metadata only.
/// Future issuance, invalid signatures, rollback and equivocation remain errors.
pub fn verify_for_browse(
    bytes: &[u8],
    trust: &Trust,
    previous: Option<&Checkpoint>,
    now: u64,
) -> Result<BrowseSnapshot> {
    ensure!(
        bytes.len() <= MAX_ENVELOPE_BYTES,
        "catalog exceeds size limit"
    );
    let envelope: Envelope = serde_json::from_slice(bytes)?;
    bounded_text(&envelope.key_id, 128)?;
    let key = trust
        .keys
        .get(&envelope.key_id)
        .context("unknown signing key")?;
    let payload = STANDARD.decode(&envelope.payload_base64)?;
    let signature = Signature::from_slice(&STANDARD.decode(&envelope.signature_base64)?)?;
    key.verify_strict(&signing_bytes(&envelope.key_id, &payload), &signature)?;
    let snapshot: Snapshot = serde_json::from_slice(&payload)?;
    snapshot.validate()?;
    ensure!(
        snapshot.catalog_id == trust.catalog_id,
        "unexpected catalog identity"
    );
    ensure!(
        snapshot.issued_at <= now,
        "catalog is expired or not yet valid"
    );
    let checkpoint = Checkpoint {
        catalog_id: snapshot.catalog_id.clone(),
        revision: snapshot.revision,
        payload_digest: digest(&payload),
        release_identities: release_identities(&snapshot, previous)?,
    };
    if let Some(previous) = previous {
        ensure!(
            previous.catalog_id == checkpoint.catalog_id,
            "checkpoint belongs to another catalog"
        );
        ensure!(
            checkpoint.revision >= previous.revision,
            "catalog rollback rejected"
        );
        ensure!(
            checkpoint.revision != previous.revision
                || checkpoint.payload_digest == previous.payload_digest,
            "catalog revision equivocation rejected"
        );
    }
    Ok(BrowseSnapshot {
        snapshot,
        checkpoint,
    })
}

fn release_identities(
    snapshot: &Snapshot,
    previous: Option<&Checkpoint>,
) -> Result<BTreeMap<String, String>> {
    let mut identities = previous
        .map(|checkpoint| checkpoint.release_identities.clone())
        .unwrap_or_default();
    for release in &snapshot.releases {
        let identity = format!("{}@{}", release.plugin_id, release.version);
        let immutable = release.immutable_identity()?;
        if let Some(old) = identities.get(&identity) {
            ensure!(
                old == &immutable,
                "published release identity changed: {identity}"
            );
        }
        identities.insert(identity, immutable);
    }
    ensure!(
        identities.len() <= 65_536,
        "release history exceeds checkpoint limit"
    );
    Ok(identities)
}
