//! Separately signed, source-only linked Cargo releases.
//!
//! This channel does not change the portable v1 catalog or its additive
//! release-details signatures. A crate is a Host build input, never a runtime
//! artifact that can be installed into a running Plugin Root.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, Signer as _, SigningKey};
use serde::{Deserialize, Serialize};

use crate::{
    Availability, Documentation, Envelope, MAX_ENVELOPE_BYTES, MAX_RELEASES, MAX_VALIDITY_SECONDS,
    ReleaseDetails, Trust, VerifiedReleaseDetails, bounded_text, digest, https_url, valid_digest,
};

const SCHEMA: &str = "lenso.marketplace.linked-cargo-snapshot.v1";
const SIGNATURE_CONTEXT: &[u8] = b"lenso.marketplace.linked-cargo-snapshot.v1\0";
const MAX_HISTORY: usize = 16_384;
const MAX_HISTORY_BYTES: usize = 8 * 1024 * 1024;

/// Whether the package exposes a generated linked-Plugin entrypoint or
/// requires a product Host's own integration.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkedCargoIntegration {
    LinkedPlugin,
    HostProvided,
}

/// One exact registry crate whose linked factory must be verified after Host build.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LinkedCargoRelease {
    pub plugin_id: String,
    pub version: String,
    pub publisher_id: String,
    pub title: String,
    pub summary: String,
    pub source_url: String,
    pub source_revision: String,
    pub license: String,
    pub package: String,
    pub registry_url: String,
    /// SHA-256 of the registry crate archive, not a compiled Host artifact.
    pub crate_digest: String,
    pub integration: LinkedCargoIntegration,
    pub targets: Vec<String>,
    pub availability: Availability,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documentation: Vec<Documentation>,
}

impl LinkedCargoRelease {
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
                    .all(|byte| { byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte) }),
            "source must name an exact commit digest"
        );
        crate::validate_package_name(&crate::DistributionKind::CargoPackage, &self.package)?;
        https_url(&self.registry_url)?;
        valid_digest(&self.crate_digest)?;
        ensure!(
            !self.targets.is_empty() && self.targets.len() <= 32,
            "linked Cargo release needs one to 32 targets"
        );
        let mut targets = BTreeSet::new();
        for target in &self.targets {
            bounded_text(target, 128)?;
            ensure!(targets.insert(target), "duplicate linked Cargo target");
        }
        ensure!(
            self.documentation.len() <= 64,
            "too many documentation resources"
        );
        let mut documents = BTreeSet::new();
        for document in &self.documentation {
            document.validate()?;
            ensure!(
                documents.insert((&document.id, &document.revision)),
                "duplicate documentation revision"
            );
        }
        Ok(())
    }

    /// Availability may change; package identity and build input may not.
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
            &self.package,
            &self.registry_url,
            &self.crate_digest,
            self.integration,
            &self.targets,
        ))?))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LinkedCargoSnapshot {
    pub schema: String,
    pub catalog_id: String,
    pub revision: u64,
    pub issued_at: u64,
    pub expires_at: u64,
    pub releases: Vec<LinkedCargoRelease>,
}

impl LinkedCargoSnapshot {
    pub fn new(
        catalog_id: String,
        revision: u64,
        issued_at: u64,
        expires_at: u64,
        releases: Vec<LinkedCargoRelease>,
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
        ensure!(
            self.schema == SCHEMA,
            "unsupported linked Cargo catalog schema"
        );
        bounded_text(&self.catalog_id, 128)?;
        ensure!(
            self.revision > 0
                && self.revision <= 9_007_199_254_740_991
                && self.expires_at > self.issued_at
                && self.expires_at <= 9_007_199_254_740_991
                && self.expires_at - self.issued_at <= MAX_VALIDITY_SECONDS,
            "invalid linked Cargo revision or validity window"
        );
        ensure!(
            self.releases.len() <= MAX_RELEASES,
            "too many linked Cargo releases"
        );
        let mut identities = BTreeSet::new();
        for release in &self.releases {
            release.validate()?;
            ensure!(
                identities.insert((&release.plugin_id, &release.version)),
                "duplicate linked Cargo release"
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LinkedCargoCheckpoint {
    pub catalog_id: String,
    pub revision: u64,
    pub payload_digest: String,
    pub release_identities: BTreeMap<String, String>,
    pub document_identities: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct VerifiedLinkedCargoSnapshot {
    snapshot: LinkedCargoSnapshot,
    checkpoint: LinkedCargoCheckpoint,
}

impl VerifiedLinkedCargoSnapshot {
    pub const fn snapshot(&self) -> &LinkedCargoSnapshot {
        &self.snapshot
    }

    pub const fn checkpoint(&self) -> &LinkedCargoCheckpoint {
        &self.checkpoint
    }

    /// Exact identity and freshness are checked at selection, not only verification.
    pub fn select(&self, plugin_id: &str, version: &str, now: u64) -> Result<&LinkedCargoRelease> {
        ensure!(
            now >= self.snapshot.issued_at && now < self.snapshot.expires_at,
            "linked Cargo catalog is not current"
        );
        let release = self
            .snapshot
            .releases
            .iter()
            .find(|release| release.plugin_id == plugin_id && release.version == version)
            .context("exact linked Cargo release is not in this catalog")?;
        ensure!(
            release.availability == Availability::Listed,
            "linked Cargo release is not available for adoption"
        );
        Ok(release)
    }

    /// Join separately signed details only to this listed, current linked release.
    pub fn select_details<'a>(
        &self,
        details: &'a VerifiedReleaseDetails,
        plugin_id: &str,
        version: &str,
        now: u64,
    ) -> Result<&'a ReleaseDetails> {
        let release = self.select(plugin_id, version, now)?;
        ensure!(
            self.snapshot.catalog_id == details.snapshot().catalog_id,
            "release details belong to another catalog"
        );
        details.ensure_current(now)?;
        let selected = details.find(plugin_id, version)?;
        selected.validate_against_linked(release)?;
        Ok(selected)
    }
}

pub fn sign(snapshot: &LinkedCargoSnapshot, key_id: &str, key: &SigningKey) -> Result<Vec<u8>> {
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
        "linked Cargo catalog exceeds size limit"
    );
    Ok(bytes)
}

pub fn verify(
    bytes: &[u8],
    trust: &Trust,
    previous: Option<&LinkedCargoCheckpoint>,
    now: u64,
) -> Result<VerifiedLinkedCargoSnapshot> {
    ensure!(
        bytes.len() <= MAX_ENVELOPE_BYTES,
        "linked Cargo catalog exceeds size limit"
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
    let snapshot: LinkedCargoSnapshot = serde_json::from_slice(&payload)?;
    snapshot.validate()?;
    ensure!(
        snapshot.catalog_id == trust.catalog_id,
        "unexpected catalog identity"
    );
    ensure!(
        now >= snapshot.issued_at && now < snapshot.expires_at,
        "linked Cargo catalog is expired or not yet valid"
    );
    if let Some(previous) = previous {
        validate_checkpoint(previous)?;
    }
    let mut release_identities =
        previous.map_or_else(BTreeMap::new, |old| old.release_identities.clone());
    let mut document_identities =
        previous.map_or_else(BTreeMap::new, |old| old.document_identities.clone());
    ensure!(
        release_identities.len() <= MAX_HISTORY,
        "linked Cargo history exceeds limit"
    );
    for release in &snapshot.releases {
        let identity = format!("{}@{}", release.plugin_id, release.version);
        let immutable = release.immutable_identity()?;
        if let Some(old) = release_identities.get(&identity) {
            ensure!(
                old == &immutable,
                "published linked Cargo release changed: {identity}"
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
                    "published linked Cargo documentation changed: {identity}"
                );
            }
            document_identities.insert(identity, immutable);
        }
    }
    ensure!(
        release_identities.len() <= MAX_HISTORY && document_identities.len() <= MAX_HISTORY * 4,
        "linked Cargo history exceeds limit"
    );
    let checkpoint = LinkedCargoCheckpoint {
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
                && previous.revision > 0
                && previous.revision <= checkpoint.revision,
            "linked Cargo catalog rollback rejected"
        );
        ensure!(
            previous.revision != checkpoint.revision
                || previous.payload_digest == checkpoint.payload_digest,
            "linked Cargo catalog revision equivocation rejected"
        );
    }
    Ok(VerifiedLinkedCargoSnapshot {
        snapshot,
        checkpoint,
    })
}

fn validate_checkpoint(checkpoint: &LinkedCargoCheckpoint) -> Result<()> {
    bounded_text(&checkpoint.catalog_id, 128)?;
    ensure!(
        checkpoint.revision > 0 && checkpoint.revision <= 9_007_199_254_740_991,
        "invalid linked Cargo checkpoint revision"
    );
    valid_digest(&checkpoint.payload_digest)?;
    ensure!(
        checkpoint.release_identities.len() <= MAX_HISTORY
            && checkpoint.document_identities.len() <= MAX_HISTORY * 4,
        "linked Cargo history exceeds limit"
    );
    let mut bytes = 0usize;
    for (identity, digest) in checkpoint
        .release_identities
        .iter()
        .chain(checkpoint.document_identities.iter())
    {
        ensure!(
            identity.len() <= 640,
            "linked Cargo history identity exceeds limit"
        );
        valid_digest(digest)?;
        bytes = bytes
            .checked_add(identity.len())
            .and_then(|size| size.checked_add(digest.len()))
            .context("linked Cargo history size overflow")?;
        ensure!(
            bytes <= MAX_HISTORY_BYTES,
            "linked Cargo history exceeds byte limit"
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
