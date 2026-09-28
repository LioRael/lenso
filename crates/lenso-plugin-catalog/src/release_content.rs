//! Optional, separately signed content for an exact existing Plugin Release.
//!
//! This v2 channel does not change either v1 signed catalog payload. Content
//! references are inert until an App owner selects, verifies, and copies them.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, Signer as _, SigningKey};
use serde::{Deserialize, Serialize};

use crate::{
    Envelope, MAX_ENVELOPE_BYTES, MAX_RELEASES, MAX_VALIDITY_SECONDS, Release, Trust,
    VerifiedSnapshot as VerifiedPortableSnapshot, bounded_text, digest, https_url,
    linked_cargo::{LinkedCargoRelease, VerifiedLinkedCargoSnapshot},
    package::{PackageRelease, VerifiedPackageSnapshot},
    valid_digest,
};

const SCHEMA: &str = "lenso.marketplace.release-content.v2";
const SIGNATURE_CONTEXT: &[u8] = b"lenso.marketplace.release-content.v2\0";
const MAX_HISTORY: usize = 16_384;
const MAX_HISTORY_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BaseKind {
    Portable,
    LinkedCargo,
    Package,
    ContentOnly,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentKind {
    EditableTemplate,
    DevelopmentExtension,
}

/// One bounded `.tar.gz` source tree. It is not executable merely because it
/// appears in a signed catalog, and it must never replace App-owned files.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Content {
    pub id: String,
    pub kind: ContentKind,
    pub url: String,
    pub digest: String,
    pub size: u64,
}

impl Content {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.id.is_empty()
                && self.id.len() <= 128
                && self
                    .id
                    .bytes()
                    .next()
                    .is_some_and(|byte| byte.is_ascii_lowercase())
                && self.id.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'-' | b'_')
                }),
            "invalid release content ID"
        );
        https_url(&self.url)?;
        valid_digest(&self.digest)?;
        ensure!(
            self.size > 0 && self.size <= 16 * 1024 * 1024,
            "release content size exceeds bounds"
        );
        Ok(())
    }

    pub fn verify_bytes(&self, bytes: &[u8]) -> Result<()> {
        self.validate()?;
        ensure!(
            bytes.len() as u64 == self.size && digest(bytes) == self.digest,
            "release content size or digest mismatch"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseContent {
    pub plugin_id: String,
    pub version: String,
    pub base_kind: BaseKind,
    pub base_release_identity: String,
    pub content: Vec<Content>,
}

impl ReleaseContent {
    pub fn validate(&self) -> Result<()> {
        crate::identity::validate_plugin_id_v1(&self.plugin_id)?;
        crate::identity::validate_release_version(&self.version)?;
        bounded_text(&self.version, 128)?;
        valid_digest(&self.base_release_identity)?;
        ensure!(
            !self.content.is_empty() && self.content.len() <= 32,
            "release content needs one to 32 entries"
        );
        let mut ids = BTreeSet::new();
        for content in &self.content {
            content.validate()?;
            ensure!(
                ids.insert(&content.id),
                "duplicate release content identity"
            );
        }
        if self.base_kind == BaseKind::ContentOnly {
            ensure!(
                self.base_release_identity == self.content_only_identity()?,
                "content-only release identity does not match its signed content"
            );
        }
        Ok(())
    }

    /// A content-only release has no fabricated runtime or package base. Its
    /// exact ordered source references bind the existing base-identity field.
    pub fn content_only_identity(&self) -> Result<String> {
        let content = self
            .content
            .iter()
            .map(|item| (&item.id, item.kind, &item.url, &item.digest, item.size))
            .collect::<Vec<_>>();
        Ok(digest(&serde_json::to_vec(&(
            &self.plugin_id,
            &self.version,
            content,
        ))?))
    }

    pub fn select(&self, id: &str) -> Result<&Content> {
        self.content
            .iter()
            .find(|content| content.id == id)
            .context("exact content ID is not in this release")
    }

    /// Canonical cross-language identity independent of JSON object key order.
    pub fn immutable_identity(&self) -> Result<String> {
        self.validate()?;
        let content = self
            .content
            .iter()
            .map(|item| (&item.id, item.kind, &item.url, &item.digest, item.size))
            .collect::<Vec<_>>();
        Ok(digest(&serde_json::to_vec(&(
            &self.plugin_id,
            &self.version,
            self.base_kind,
            &self.base_release_identity,
            content,
        ))?))
    }

    fn validate_portable(&self, base: &Release) -> Result<()> {
        self.validate()?;
        base.validate()?;
        ensure!(
            self.base_kind == BaseKind::Portable
                && self.plugin_id == base.plugin_id
                && self.version == base.version
                && self.base_release_identity == base.immutable_identity()?,
            "release content does not match exact portable base"
        );
        Ok(())
    }

    fn validate_linked(&self, base: &LinkedCargoRelease) -> Result<()> {
        self.validate()?;
        base.validate()?;
        ensure!(
            self.base_kind == BaseKind::LinkedCargo
                && self.plugin_id == base.plugin_id
                && self.version == base.version
                && self.base_release_identity == base.immutable_identity()?,
            "release content does not match exact linked Cargo base"
        );
        Ok(())
    }

    fn validate_package(&self, base: &PackageRelease) -> Result<()> {
        self.validate()?;
        base.validate()?;
        ensure!(
            self.base_kind == BaseKind::Package
                && self.plugin_id == base.plugin_id
                && self.version == base.version
                && self.base_release_identity == base.immutable_identity()?,
            "release content does not match exact package base"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub schema: String,
    pub catalog_id: String,
    pub revision: u64,
    pub issued_at: u64,
    pub expires_at: u64,
    pub releases: Vec<ReleaseContent>,
}

impl Snapshot {
    pub fn new(
        catalog_id: String,
        revision: u64,
        issued_at: u64,
        expires_at: u64,
        releases: Vec<ReleaseContent>,
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
        ensure!(self.schema == SCHEMA, "unsupported release content schema");
        bounded_text(&self.catalog_id, 128)?;
        ensure!(
            self.revision > 0
                && self.revision <= 9_007_199_254_740_991
                && self.expires_at > self.issued_at
                && self.expires_at <= 9_007_199_254_740_991
                && self.expires_at - self.issued_at <= MAX_VALIDITY_SECONDS,
            "invalid release content revision or validity window"
        );
        ensure!(
            self.releases.len() <= MAX_RELEASES,
            "too many release content entries"
        );
        let mut identities = BTreeSet::new();
        for release in &self.releases {
            release.validate()?;
            ensure!(
                identities.insert((&release.plugin_id, &release.version)),
                "duplicate release content identity"
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

impl VerifiedSnapshot {
    pub const fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    pub const fn checkpoint(&self) -> &Checkpoint {
        &self.checkpoint
    }

    fn select(&self, plugin_id: &str, version: &str, now: u64) -> Result<&ReleaseContent> {
        ensure!(
            now >= self.snapshot.issued_at && now < self.snapshot.expires_at,
            "release content catalog is not current"
        );
        self.snapshot
            .releases
            .iter()
            .find(|release| release.plugin_id == plugin_id && release.version == version)
            .context("exact release content is not in this catalog")
    }

    pub fn select_portable(
        &self,
        base: &VerifiedPortableSnapshot,
        plugin_id: &str,
        version: &str,
        now: u64,
    ) -> Result<&ReleaseContent> {
        ensure!(
            self.snapshot.catalog_id == base.snapshot().catalog_id,
            "release content belongs to another catalog"
        );
        let base = base.select(plugin_id, version, now)?;
        let release = self.select(plugin_id, version, now)?;
        release.validate_portable(base)?;
        Ok(release)
    }

    pub fn select_linked(
        &self,
        base: &VerifiedLinkedCargoSnapshot,
        plugin_id: &str,
        version: &str,
        now: u64,
    ) -> Result<&ReleaseContent> {
        ensure!(
            self.snapshot.catalog_id == base.snapshot().catalog_id,
            "release content belongs to another catalog"
        );
        let base = base.select(plugin_id, version, now)?;
        let release = self.select(plugin_id, version, now)?;
        release.validate_linked(base)?;
        Ok(release)
    }

    pub fn select_package(
        &self,
        base: &VerifiedPackageSnapshot,
        plugin_id: &str,
        version: &str,
        now: u64,
    ) -> Result<&ReleaseContent> {
        ensure!(
            self.snapshot.catalog_id == base.snapshot().catalog_id,
            "release content belongs to another catalog"
        );
        let base = base.select_release(plugin_id, version, now)?;
        let release = self.select(plugin_id, version, now)?;
        release.validate_package(base)?;
        Ok(release)
    }

    pub fn select_content_only(
        &self,
        plugin_id: &str,
        version: &str,
        now: u64,
    ) -> Result<&ReleaseContent> {
        let release = self.select(plugin_id, version, now)?;
        ensure!(
            release.base_kind == BaseKind::ContentOnly,
            "release content requires an exact signed base"
        );
        release.validate()?;
        Ok(release)
    }
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
        "release content exceeds size limit"
    );
    Ok(bytes)
}

pub fn verify(
    bytes: &[u8],
    trust: &Trust,
    previous: Option<&Checkpoint>,
    now: u64,
) -> Result<VerifiedSnapshot> {
    ensure!(
        bytes.len() <= MAX_ENVELOPE_BYTES,
        "release content exceeds size limit"
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
        now >= snapshot.issued_at && now < snapshot.expires_at,
        "release content is expired or not yet valid"
    );
    if let Some(previous) = previous {
        validate_checkpoint(previous)?;
    }
    let mut release_identities = previous
        .map(|checkpoint| checkpoint.release_identities.clone())
        .unwrap_or_default();
    for release in &snapshot.releases {
        let identity = format!("{}@{}", release.plugin_id, release.version);
        let immutable = release.immutable_identity()?;
        if let Some(old) = release_identities.get(&identity) {
            ensure!(
                old == &immutable,
                "published release content changed: {identity}"
            );
        }
        release_identities.insert(identity, immutable);
    }
    let checkpoint = Checkpoint {
        catalog_id: snapshot.catalog_id.clone(),
        revision: snapshot.revision,
        payload_digest: digest(&payload),
        release_identities,
    };
    validate_checkpoint(&checkpoint)?;
    if let Some(previous) = previous {
        ensure!(
            previous.catalog_id == checkpoint.catalog_id
                && checkpoint.revision >= previous.revision,
            "release content rollback rejected"
        );
        ensure!(
            checkpoint.revision != previous.revision
                || checkpoint.payload_digest == previous.payload_digest,
            "release content revision equivocation rejected"
        );
    }
    Ok(VerifiedSnapshot {
        snapshot,
        checkpoint,
    })
}

fn validate_checkpoint(checkpoint: &Checkpoint) -> Result<()> {
    bounded_text(&checkpoint.catalog_id, 128)?;
    ensure!(
        checkpoint.revision > 0 && checkpoint.revision <= 9_007_199_254_740_991,
        "invalid release content checkpoint revision"
    );
    valid_digest(&checkpoint.payload_digest)?;
    ensure!(
        checkpoint.release_identities.len() <= MAX_HISTORY,
        "release content history exceeds limit"
    );
    let mut bytes = 0usize;
    for (identity, digest) in &checkpoint.release_identities {
        ensure!(
            identity.len() <= 512,
            "release content history identity exceeds limit"
        );
        valid_digest(digest)?;
        bytes = bytes
            .checked_add(identity.len())
            .and_then(|size| size.checked_add(digest.len()))
            .context("release content history size overflow")?;
        ensure!(
            bytes <= MAX_HISTORY_BYTES,
            "release content history exceeds byte limit"
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
