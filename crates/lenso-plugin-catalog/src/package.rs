//! Signed package-only Plugin releases. This channel has no Portable base.
//!
//! A verified release identifies registry packages; it does not install them,
//! execute lifecycle scripts, or make their source a runtime Bundle. Publishers
//! must enforce Plugin ID/version uniqueness across all catalog channels.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, Signer as _, SigningKey};
use serde::{Deserialize, Serialize};

use crate::{
    Availability, Distribution, DistributionKind, Documentation, Envelope, MAX_ENVELOPE_BYTES,
    MAX_RELEASES, MAX_VALIDITY_SECONDS, Trust, bounded_text, digest, https_url, valid_digest,
};

const SCHEMA: &str = "lenso.marketplace.package-snapshot.v1";
const SIGNATURE_CONTEXT: &[u8] = b"lenso.marketplace.package-snapshot.v1\0";
const MAX_HISTORY: usize = 16_384;
const MAX_HISTORY_BYTES: usize = 8 * 1024 * 1024;

/// One Plugin release with exact npm distributions but no Portable artifact.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackageRelease {
    pub plugin_id: String,
    pub version: String,
    pub publisher_id: String,
    pub title: String,
    pub summary: String,
    pub source_url: String,
    pub source_revision: String,
    pub license: String,
    pub distributions: Vec<Distribution>,
    pub availability: Availability,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documentation: Vec<Documentation>,
}

impl PackageRelease {
    pub fn validate(&self) -> Result<()> {
        crate::identity::validate_plugin_id_v1(&self.plugin_id)?;
        crate::identity::validate_release_version(&self.version)?;
        bounded_text(&self.version, 128)?;
        bounded_text(&self.publisher_id, 128)?;
        bounded_text(&self.title, 160)?;
        bounded_text(&self.summary, 640)?;
        bounded_text(&self.license, 128)?;
        https_url(&self.source_url)?;
        ensure!(
            matches!(self.source_revision.len(), 40 | 64)
                && self
                    .source_revision
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "source must name an exact commit digest"
        );
        ensure!(
            !self.distributions.is_empty() && self.distributions.len() <= 16,
            "package release needs one to sixteen distributions"
        );
        let mut distribution_ids = BTreeSet::new();
        for distribution in &self.distributions {
            distribution.validate()?;
            ensure!(
                distribution.kind == DistributionKind::NpmPackage,
                "npm-only release cannot contain another distribution kind"
            );
            ensure!(
                distribution_ids.insert(&distribution.id),
                "duplicate package distribution identity"
            );
        }
        ensure!(
            self.documentation.len() <= 64,
            "too many documentation resources"
        );
        let mut document_ids = BTreeSet::new();
        for document in &self.documentation {
            document.validate()?;
            ensure!(
                document_ids.insert((&document.id, &document.revision)),
                "duplicate documentation revision"
            );
        }
        Ok(())
    }

    /// Availability may change; package bytes and release metadata may not.
    pub fn immutable_identity(&self) -> Result<String> {
        Ok(digest(&serde_json::to_vec(&(
            &self.plugin_id,
            &self.version,
            &self.publisher_id,
            &self.title,
            &self.summary,
            &self.source_url,
            &self.source_revision,
            &self.license,
            &self.distributions,
        ))?))
    }
}

/// Detached-signature payload for exact package-only releases.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackageSnapshot {
    pub schema: String,
    pub catalog_id: String,
    pub revision: u64,
    pub issued_at: u64,
    pub expires_at: u64,
    pub releases: Vec<PackageRelease>,
}

impl PackageSnapshot {
    pub fn new(
        catalog_id: String,
        revision: u64,
        issued_at: u64,
        expires_at: u64,
        releases: Vec<PackageRelease>,
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
        ensure!(self.schema == SCHEMA, "unsupported package catalog schema");
        bounded_text(&self.catalog_id, 128)?;
        ensure!(
            self.revision > 0
                && self.revision <= 9_007_199_254_740_991
                && self.expires_at > self.issued_at
                && self.expires_at <= 9_007_199_254_740_991
                && self.expires_at - self.issued_at <= MAX_VALIDITY_SECONDS,
            "invalid package catalog revision or validity window"
        );
        ensure!(
            self.releases.len() <= MAX_RELEASES,
            "too many package releases"
        );
        let mut identities = BTreeSet::new();
        for release in &self.releases {
            release.validate()?;
            ensure!(
                identities.insert((&release.plugin_id, &release.version)),
                "duplicate package release"
            );
        }
        Ok(())
    }
}

/// Durable history for this channel, separate from Portable and linked Cargo.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackageCheckpoint {
    pub catalog_id: String,
    pub revision: u64,
    pub payload_digest: String,
    pub release_identities: BTreeMap<String, String>,
    pub document_identities: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct VerifiedPackageSnapshot {
    snapshot: PackageSnapshot,
    checkpoint: PackageCheckpoint,
}

impl VerifiedPackageSnapshot {
    pub const fn snapshot(&self) -> &PackageSnapshot {
        &self.snapshot
    }

    pub const fn checkpoint(&self) -> &PackageCheckpoint {
        &self.checkpoint
    }

    /// Select one currently listed npm distribution by exact Plugin and distribution IDs.
    pub fn select_npm(
        &self,
        plugin_id: &str,
        version: &str,
        distribution_id: &str,
        now: u64,
    ) -> Result<&Distribution> {
        ensure!(
            now >= self.snapshot.issued_at && now < self.snapshot.expires_at,
            "package catalog is not current"
        );
        let release = self
            .snapshot
            .releases
            .iter()
            .find(|release| release.plugin_id == plugin_id && release.version == version)
            .context("exact package release is not in this catalog")?;
        ensure!(
            release.availability == Availability::Listed,
            "package release is not available for adoption"
        );
        release
            .distributions
            .iter()
            .find(|distribution| {
                distribution.id == distribution_id
                    && distribution.kind == DistributionKind::NpmPackage
            })
            .context("exact npm distribution is not in this release")
    }
}

pub fn sign(snapshot: &PackageSnapshot, key_id: &str, key: &SigningKey) -> Result<Vec<u8>> {
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
        "package catalog exceeds size limit"
    );
    Ok(bytes)
}

pub fn verify(
    bytes: &[u8],
    trust: &Trust,
    previous: Option<&PackageCheckpoint>,
    now: u64,
) -> Result<VerifiedPackageSnapshot> {
    ensure!(
        bytes.len() <= MAX_ENVELOPE_BYTES,
        "package catalog exceeds size limit"
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
    let snapshot: PackageSnapshot = serde_json::from_slice(&payload)?;
    snapshot.validate()?;
    ensure!(
        snapshot.catalog_id == trust.catalog_id,
        "unexpected catalog identity"
    );
    ensure!(
        now >= snapshot.issued_at && now < snapshot.expires_at,
        "package catalog is expired or not yet valid"
    );
    if let Some(previous) = previous {
        validate_checkpoint(previous)?;
    }
    let mut release_identities =
        previous.map_or_else(BTreeMap::new, |old| old.release_identities.clone());
    let mut document_identities =
        previous.map_or_else(BTreeMap::new, |old| old.document_identities.clone());
    for release in &snapshot.releases {
        let identity = format!("{}@{}", release.plugin_id, release.version);
        let immutable = release.immutable_identity()?;
        if let Some(old) = release_identities.get(&identity) {
            ensure!(
                old == &immutable,
                "published package release changed: {identity}"
            );
        }
        release_identities.insert(identity, immutable);
        for document in &release.documentation {
            // Documentation IDs and revisions may contain separators. A tuple
            // key keeps their history unambiguous without restricting old IDs.
            let identity = serde_json::to_string(&(
                &release.plugin_id,
                &release.version,
                &document.id,
                &document.revision,
            ))?;
            let immutable = digest(&serde_json::to_vec(document)?);
            if let Some(old) = document_identities.get(&identity) {
                ensure!(
                    old == &immutable,
                    "published package documentation changed: {identity}"
                );
            }
            document_identities.insert(identity, immutable);
        }
    }
    let checkpoint = PackageCheckpoint {
        catalog_id: snapshot.catalog_id.clone(),
        revision: snapshot.revision,
        payload_digest: digest(&payload),
        release_identities,
        document_identities,
    };
    validate_checkpoint(&checkpoint)?;
    if let Some(previous) = previous {
        ensure!(
            previous.catalog_id == checkpoint.catalog_id
                && previous.revision <= checkpoint.revision,
            "package catalog rollback rejected"
        );
        ensure!(
            previous.revision != checkpoint.revision
                || previous.payload_digest == checkpoint.payload_digest,
            "package catalog revision equivocation rejected"
        );
    }
    Ok(VerifiedPackageSnapshot {
        snapshot,
        checkpoint,
    })
}

fn validate_checkpoint(checkpoint: &PackageCheckpoint) -> Result<()> {
    bounded_text(&checkpoint.catalog_id, 128)?;
    ensure!(
        checkpoint.revision > 0 && checkpoint.revision <= 9_007_199_254_740_991,
        "invalid package checkpoint revision"
    );
    valid_digest(&checkpoint.payload_digest)?;
    ensure!(
        checkpoint.release_identities.len() <= MAX_HISTORY
            && checkpoint.document_identities.len() <= MAX_HISTORY * 4,
        "package history exceeds limit"
    );
    let mut bytes = 0usize;
    for (identity, digest) in checkpoint
        .release_identities
        .iter()
        .chain(checkpoint.document_identities.iter())
    {
        ensure!(
            identity.len() <= 640,
            "package history identity exceeds limit"
        );
        valid_digest(digest)?;
        bytes = bytes
            .checked_add(identity.len())
            .and_then(|size| size.checked_add(digest.len()))
            .context("package history size overflow")?;
        ensure!(
            bytes <= MAX_HISTORY_BYTES,
            "package history exceeds byte limit"
        );
    }
    Ok(())
}

fn signing_bytes(key_id: &str, payload: &[u8]) -> Vec<u8> {
    let mut bytes = SIGNATURE_CONTEXT.to_vec();
    bytes.extend_from_slice(key_id.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(payload);
    bytes
}
