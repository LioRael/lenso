use std::{
    collections::BTreeMap,
    fs,
    io::{Read as _, Write as _},
    path::{Component, Path, PathBuf},
};

use anyhow::{Context as _, bail, ensure};
use ed25519_dalek::VerifyingKey;
use lenso_app_authoring::discovery::Candidate;
use lenso_app_plan::authoring::{PluginContract, PluginDescriptor};
use lenso_plugin_bundle::{BundleVerificationLimits, PluginManifest, PluginVariantInputV6};
use lenso_plugin_catalog::{
    Availability, DistributionKind, Trust,
    linked_cargo::{self, LinkedCargoIntegration},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::AddArgs;

mod adoption;
mod checkpoint;
pub(super) mod content;
mod content_checkpoint;
mod replacement;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrustFile {
    catalog_id: String,
    key_id: String,
    public_key_hex: String,
}

const MAX_CRATE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_UNPACKED_BYTES: u64 = 128 * 1024 * 1024;
const MAX_FILES: usize = 4096;
const SOURCE_LOCK: &str = ".lenso-linked-source.json";
const MAX_SOURCE_LOCK_BYTES: u64 = 256 * 1024;
const MAX_V5_SOURCE_LOCK_BYTES: u64 = 4096;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SourceLock {
    schema_version: u32,
    plugin_id: String,
    version: String,
    crate_digest: String,
    source_digest: String,
    /// Present only when the signed archive contains a root Cargo.lock.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    archive_cargo_lock_digest: Option<String>,
    /// Only exclusions added by app add may be removed by app unadopt.
    #[serde(default, skip_serializing_if = "is_false")]
    workspace_exclude_owned: bool,
    /// V5 signed .crate adoption omits this field and retains its lock wire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    v6: Option<V6BuildInputLock>,
}

// The original generated lock wire had no workspace ownership or archive
// Cargo.lock fields. Exact retries may upgrade that canonical wire, but must
// not treat an arbitrary edited lock as equivalent to a signed adoption.
#[derive(Serialize)]
struct LegacySourceLock<'a> {
    schema_version: u32,
    plugin_id: &'a str,
    version: &'a str,
    crate_digest: &'a str,
    source_digest: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    v6: Option<&'a V6BuildInputLock>,
}

fn legacy_source_lock_bytes(new_lock: &[u8]) -> anyhow::Result<Vec<u8>> {
    let lock: SourceLock = serde_json::from_slice(new_lock)?;
    Ok(serde_json::to_vec_pretty(&LegacySourceLock {
        schema_version: lock.schema_version,
        plugin_id: &lock.plugin_id,
        version: &lock.version,
        crate_digest: &lock.crate_digest,
        source_digest: &lock.source_digest,
        v6: lock.v6.as_ref(),
    })?)
}

fn legacy_v6_source_lock_bytes(
    new_lock: &[u8],
    legacy_root_fields: bool,
) -> anyhow::Result<Option<Vec<u8>>> {
    let mut lock: SourceLock = serde_json::from_slice(new_lock)?;
    let Some(v6) = lock.v6.as_mut() else {
        return Ok(None);
    };
    v6.contract = None;
    v6.entrypoint = None;
    let bytes = serde_json::to_vec_pretty(&lock)?;
    if legacy_root_fields {
        return legacy_source_lock_bytes(&bytes).map(Some);
    }
    Ok(Some(bytes))
}

fn canonical_prior_source_lock_matches(expected: &[u8], actual: &[u8]) -> anyhow::Result<bool> {
    if actual == legacy_source_lock_bytes(expected)? {
        return Ok(true);
    }
    for legacy_root_fields in [false, true] {
        if legacy_v6_source_lock_bytes(expected, legacy_root_fields)?.as_deref() == Some(actual) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn is_false(value: &bool) -> bool {
    !value
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct V6BuildInputLock {
    bundle_manifest_digest: String,
    implementation_id: String,
    variant_id: String,
    /// Missing on an older V6 lock; an exact signed retry upgrades it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    contract: Option<PluginContract>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    entrypoint: Option<String>,
}

struct SelectedV6Archive {
    bytes: Vec<u8>,
    lock: V6BuildInputLock,
}

fn read_source_lock(path: &Path) -> anyhow::Result<SourceLock> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .with_context(|| format!("read linked Cargo source lock {}", path.display()))?
        .take(MAX_SOURCE_LOCK_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        u64::try_from(bytes.len())? <= MAX_SOURCE_LOCK_BYTES,
        "linked Cargo source lock exceeds size limit"
    );
    let lock: SourceLock = serde_json::from_slice(&bytes)?;
    if lock.v6.is_none() {
        ensure!(
            u64::try_from(bytes.len())? <= MAX_V5_SOURCE_LOCK_BYTES,
            "linked Cargo source lock exceeds size limit"
        );
    }
    Ok(lock)
}

/// One signed source-only catalog, without any claim that its crate can build.
#[derive(Debug, Serialize)]
pub struct LinkedCatalogReport {
    pub schema_version: u32,
    pub kind: &'static str,
    pub catalog_id: String,
    pub revision: u64,
    pub requested_target: String,
    pub releases: Vec<LinkedCatalogCandidate>,
}

/// A hard-negative filter over signed catalog and App facts. Surviving entries
/// are still candidates: no archive, permission, or runtime grant is implied.
#[derive(Debug, Serialize)]
pub struct LinkedRecommendationReport {
    pub schema_version: u32,
    pub kind: &'static str,
    pub catalog_id: String,
    pub revision: u64,
    pub requested_target: String,
    pub project_target: String,
    pub restrictions: RecommendationRestrictions,
    pub releases: Vec<LinkedCatalogCandidate>,
    pub excluded: Vec<LinkedCatalogCandidate>,
}

/// Strict user constraints. This catalog carries no signed permission, service,
/// or fee declarations, so an unknown value is rejected under each constraint.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct RecommendationRestrictions {
    pub require_no_permissions: bool,
    pub require_no_external_services: bool,
    pub require_no_fees: bool,
}

#[derive(Debug, Serialize)]
pub struct LinkedCatalogCandidate {
    pub plugin_id: String,
    pub version: String,
    pub title: String,
    pub summary: String,
    pub package: String,
    pub registry_url: String,
    pub crate_digest: String,
    pub integration: LinkedCargoIntegration,
    pub targets: Vec<String>,
    pub documentation: Vec<lenso_plugin_catalog::Documentation>,
    pub adoption: &'static str,
    pub rejection_reasons: Vec<&'static str>,
    pub unverified: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct LinkedDocumentChunk {
    pub schema_version: u32,
    pub kind: &'static str,
    pub plugin_id: String,
    pub version: String,
    pub release_availability: Availability,
    pub document_id: String,
    pub revision: String,
    pub source_url: String,
    pub digest: String,
    pub media_type: String,
    pub total_bytes: usize,
    pub offset: usize,
    pub next_offset: Option<usize>,
    pub content: String,
    pub content_is_untrusted: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct DocumentRequest<'a> {
    pub snapshot_path: &'a Path,
    pub trust_path: &'a Path,
    pub plugin_id: &'a str,
    pub version: &'a str,
    pub document_id: &'a str,
    pub revision: &'a str,
    pub local_file: Option<&'a Path>,
    pub fetch: bool,
    pub offset: usize,
    pub max_bytes: usize,
}

pub fn document(request: DocumentRequest<'_>) -> anyhow::Result<LinkedDocumentChunk> {
    ensure!(
        (request.local_file.is_some() && !request.fetch)
            || (request.local_file.is_none() && request.fetch),
        "choose exactly one of a local document file or explicit HTTPS fetch"
    );
    ensure!(
        (4..=8192).contains(&request.max_bytes),
        "document chunk must be 4 to 8192 bytes"
    );
    let verified = read_verified(request.snapshot_path, request.trust_path)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let snapshot = verified.snapshot();
    ensure!(
        now >= snapshot.issued_at && now < snapshot.expires_at,
        "linked Cargo catalog is not current"
    );
    let release = snapshot
        .releases
        .iter()
        .find(|release| {
            release.plugin_id == request.plugin_id && release.version == request.version
        })
        .context("exact linked Cargo release is not in this catalog")?;
    ensure!(
        release.availability != Availability::Revoked,
        "revoked linked Cargo documentation is unavailable"
    );
    let document = release
        .documentation
        .iter()
        .find(|document| {
            document.id == request.document_id && document.revision == request.revision
        })
        .context("exact documentation revision is not in the signed release")?;
    let bytes = if let Some(path) = request.local_file {
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(document.size + 1)
            .read_to_end(&mut bytes)?;
        bytes
    } else {
        crate::catalog::fetch_documentation(&document.url, document.size)?
    };
    ensure!(
        bytes.len() as u64 == document.size,
        "documentation size differs from signed release"
    );
    ensure!(
        lenso_plugin_catalog::digest(&bytes) == document.digest,
        "documentation digest differs from signed release"
    );
    let text = std::str::from_utf8(&bytes).context("documentation is not UTF-8 Markdown")?;
    ensure!(
        request.offset <= text.len() && text.is_char_boundary(request.offset),
        "document offset is outside a UTF-8 boundary"
    );
    let mut end = request
        .offset
        .saturating_add(request.max_bytes)
        .min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    ensure!(
        end > request.offset || request.offset == text.len(),
        "document chunk is too small for the next character"
    );
    Ok(LinkedDocumentChunk {
        schema_version: 1,
        kind: "lenso.linked-cargo-document",
        plugin_id: release.plugin_id.clone(),
        version: release.version.clone(),
        release_availability: release.availability.clone(),
        document_id: document.id.clone(),
        revision: document.revision.clone(),
        source_url: document.url.clone(),
        digest: document.digest.clone(),
        media_type: document.media_type.clone(),
        total_bytes: text.len(),
        offset: request.offset,
        next_offset: (end < text.len()).then_some(end),
        content: text[request.offset..end].to_owned(),
        content_is_untrusted: true,
    })
}

pub fn inspect(
    snapshot_path: &Path,
    trust_path: &Path,
    query: &str,
    target: &str,
) -> anyhow::Result<LinkedCatalogReport> {
    ensure!(!target.trim().is_empty(), "requested target is empty");
    let verified = read_verified(snapshot_path, trust_path)?;
    let snapshot = verified.snapshot();
    let query = query.to_lowercase();
    let mut releases = snapshot
        .releases
        .iter()
        .filter(|release| {
            query.is_empty()
                || release.plugin_id.to_lowercase().contains(&query)
                || release.title.to_lowercase().contains(&query)
                || release.summary.to_lowercase().contains(&query)
        })
        .map(|release| {
            let mut rejection_reasons = Vec::new();
            if release.availability != Availability::Listed {
                rejection_reasons.push("not_listed");
            }
            if release.integration != LinkedCargoIntegration::LinkedPlugin {
                rejection_reasons.push("requires_product_host");
            }
            if !release.targets.iter().any(|candidate| candidate == target) {
                rejection_reasons.push("host_target_mismatch");
            }
            if release.registry_url != "https://crates.io" {
                rejection_reasons.push("registry_not_supported_by_app_add");
            }
            LinkedCatalogCandidate {
                plugin_id: release.plugin_id.clone(),
                version: release.version.clone(),
                title: release.title.clone(),
                summary: release.summary.clone(),
                package: release.package.clone(),
                registry_url: release.registry_url.clone(),
                crate_digest: release.crate_digest.clone(),
                integration: release.integration,
                targets: release.targets.clone(),
                documentation: release.documentation.clone(),
                adoption: if rejection_reasons.is_empty() {
                    "candidate_only"
                } else {
                    "rejected"
                },
                rejection_reasons,
                unverified: vec![
                    "crate_archive_integrity_and_identity",
                    "dependency_closure",
                    "permissions_and_external_services",
                    "host_build_and_runtime",
                ],
            }
        })
        .collect::<Vec<_>>();
    releases.sort_by(|left, right| {
        (&left.plugin_id, &left.version).cmp(&(&right.plugin_id, &right.version))
    });
    Ok(LinkedCatalogReport {
        schema_version: 1,
        kind: "lenso.linked-cargo-catalog",
        catalog_id: snapshot.catalog_id.clone(),
        revision: snapshot.revision,
        requested_target: target.to_owned(),
        releases,
    })
}

pub fn recommend(
    root: &Path,
    snapshot_path: &Path,
    trust_path: &Path,
    query: &str,
    target: &str,
    restrictions: RecommendationRestrictions,
) -> anyhow::Result<LinkedRecommendationReport> {
    ensure!(
        query.len() <= 256,
        "linked Cargo recommendation query exceeds 256 bytes"
    );
    ensure!(
        target.len() <= 128,
        "linked Cargo recommendation target exceeds 128 bytes"
    );
    let root = fs::canonicalize(root).context("resolve recommendation App root")?;
    ensure!(root.is_dir(), "recommendation App root is not a directory");
    let facts = crate::app::facts::inspect_project_facts(&root)?;
    let report = inspect(snapshot_path, trust_path, query, target)?;
    let mut releases = Vec::new();
    let mut excluded = Vec::new();
    for mut release in report.releases {
        if facts.host_target != "unknown" && facts.host_target != target {
            release
                .rejection_reasons
                .push("project_host_target_mismatch");
        }
        if facts.status == "resolved"
            && facts.plugins.iter().any(|plugin| {
                plugin.plugin_id == release.plugin_id && plugin.release_version == release.version
            })
        {
            release.rejection_reasons.push("already_adopted");
        }
        if let Some(reason) = local_linked_source_state(&root, &release)?
            && !release.rejection_reasons.contains(&reason)
        {
            release.rejection_reasons.push(reason);
        }
        if restrictions.require_no_permissions {
            release
                .rejection_reasons
                .push("permission_requirements_unverified");
        }
        if restrictions.require_no_external_services {
            release
                .rejection_reasons
                .push("external_service_requirements_unverified");
        }
        if restrictions.require_no_fees {
            release
                .rejection_reasons
                .push("fee_requirements_unverified");
        }
        if release.rejection_reasons.is_empty() {
            release
                .unverified
                .extend(["required_capabilities", "fees_and_cloud_prerequisites"]);
            if facts.status != "resolved" {
                release.unverified.push("project_selection");
            }
            releases.push(release);
        } else {
            release.adoption = "rejected";
            excluded.push(release);
        }
    }
    Ok(LinkedRecommendationReport {
        schema_version: 1,
        kind: "lenso.linked-cargo-recommendations",
        catalog_id: report.catalog_id,
        revision: report.revision,
        requested_target: report.requested_target,
        project_target: facts.host_target,
        restrictions,
        releases,
        excluded,
    })
}

fn local_linked_source_state(
    root: &Path,
    release: &LinkedCatalogCandidate,
) -> anyhow::Result<Option<&'static str>> {
    let mut source = root.to_path_buf();
    for segment in [
        "vendor",
        "lenso",
        release.plugin_id.as_str(),
        release.version.as_str(),
    ] {
        source.push(segment);
        let metadata = match fs::symlink_metadata(&source) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        ensure!(
            metadata.is_dir(),
            "linked source path is not a real directory"
        );
    }
    let lock_path = source.join(SOURCE_LOCK);
    let metadata = match fs::symlink_metadata(&lock_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Some("local_source_conflict"));
        }
        Err(error) => return Err(error.into()),
    };
    ensure!(metadata.is_file(), "linked source lock is not a real file");
    let lock = read_source_lock(&lock_path)?;
    if lock.plugin_id == release.plugin_id
        && lock.version == release.version
        && lock.crate_digest == release.crate_digest
    {
        Ok(Some("already_adopted"))
    } else {
        Ok(Some("local_source_conflict"))
    }
}

fn read_verified(
    snapshot_path: &Path,
    trust_path: &Path,
) -> anyhow::Result<linked_cargo::VerifiedLinkedCargoSnapshot> {
    let trust = read_trust(trust_path)?;
    linked_cargo::verify(&read_envelope(snapshot_path)?, &trust, None, now()?)
}

fn read_trust(trust_path: &Path) -> anyhow::Result<Trust> {
    let mut trust_bytes = Vec::new();
    fs::File::open(trust_path)?
        .take(4097)
        .read_to_end(&mut trust_bytes)?;
    ensure!(
        trust_bytes.len() <= 4096,
        "linked Cargo trust file exceeds size limit"
    );
    let trust_file: TrustFile = serde_json::from_slice(&trust_bytes)?;
    let key: [u8; 32] = hex::decode(trust_file.public_key_hex)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("public trust key must have 32 bytes"))?;
    Ok(Trust {
        catalog_id: trust_file.catalog_id,
        keys: BTreeMap::from([(trust_file.key_id, VerifyingKey::from_bytes(&key)?)]),
    })
}

fn read_envelope(snapshot_path: &Path) -> anyhow::Result<Vec<u8>> {
    let mut envelope = Vec::new();
    fs::File::open(snapshot_path)?
        .take(lenso_plugin_catalog::MAX_ENVELOPE_BYTES as u64 + 1)
        .read_to_end(&mut envelope)?;
    ensure!(
        envelope.len() <= lenso_plugin_catalog::MAX_ENVELOPE_BYTES,
        "linked Cargo snapshot exceeds size limit"
    );
    Ok(envelope)
}

fn now() -> anyhow::Result<u64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs())
}

pub(super) fn add(root: &Path, args: &AddArgs) -> anyhow::Result<()> {
    let snapshot_path = args
        .linked_snapshot
        .as_ref()
        .context("--linked-snapshot required")?;
    let trust_path = args.trust.as_ref().context("--trust required")?;
    ensure!(
        args.crate_archive.is_some() != args.bundle.is_some(),
        "choose exactly one of --crate or --bundle"
    );
    let (plugin_id, version) = args
        .source
        .split_once('@')
        .context("linked Cargo source must be an exact PLUGIN_ID@VERSION")?;
    let app_lock = adoption::lock_app(root)?;
    let trust = read_trust(trust_path)?;
    let previous = checkpoint::read(root, &app_lock, &trust.catalog_id)?;
    let now = now()?;
    let verified = linked_cargo::verify(
        &read_envelope(snapshot_path)?,
        &trust,
        previous.as_ref(),
        now,
    )?;
    checkpoint::persist(root, &app_lock, verified.checkpoint(), previous.as_ref())?;
    let release = verified.select(plugin_id, version, now)?;
    ensure!(
        release.integration == LinkedCargoIntegration::LinkedPlugin,
        "Host integration required: this release cannot be added as a generic linked Plugin"
    );
    let target = lenso_app_authoring::native_host_target();
    ensure!(
        release.targets.iter().any(|candidate| candidate == target),
        "linked Cargo release does not support Host target {target}"
    );
    ensure!(
        release.registry_url == "https://crates.io",
        "linked Cargo registry is unsupported; use an authorized custom Host"
    );
    let (archive, v6_lock) = if let Some(bundle) = &args.bundle {
        let selected = verified_v6_archive(bundle, release, target)?;
        (selected.bytes, Some(selected.lock))
    } else {
        let mut bytes = Vec::new();
        fs::File::open(args.crate_archive.as_ref().context("--crate required")?)?
            .take(MAX_CRATE_BYTES + 1)
            .read_to_end(&mut bytes)?;
        (bytes, None)
    };
    ensure!(
        !archive.is_empty() && u64::try_from(archive.len())? <= MAX_CRATE_BYTES,
        "crate archive exceeds size limit"
    );
    ensure!(
        lenso_plugin_catalog::digest(&archive) == release.crate_digest,
        "crate archive digest does not match signed catalog"
    );
    adopt_archive(
        root,
        args,
        plugin_id,
        version,
        &release.package,
        &release.crate_digest,
        &archive,
        v6_lock,
        app_lock,
    )
}

pub(super) fn add_from_release_details(root: &Path, args: &AddArgs) -> anyhow::Result<()> {
    ensure!(
        args.linked_snapshot.is_none()
            && args.bundle.is_none()
            && args.archive.is_none()
            && args.origin.is_none()
            && args.crate_archive.is_some(),
        "release details Cargo adoption requires --portable-snapshot, --release-details, --trust and --crate only"
    );
    let (plugin_id, version) = args
        .source
        .split_once('@')
        .context("release details Cargo source must be an exact PLUGIN_ID@VERSION")?;
    let app_lock = adoption::lock_app(root)?;
    let trust =
        crate::app::read_signed_portable_trust(args.trust.as_deref().context("--trust required")?)?;
    let portable_previous =
        crate::plugins::signed_install::checkpoint::read(root, &app_lock, &trust.catalog_id)?;
    let now = now()?;
    let portable = lenso_plugin_catalog::verify(
        &crate::app::read_signed_portable_snapshot(
            args.portable_snapshot
                .as_deref()
                .context("--portable-snapshot required")?,
        )?,
        &trust,
        portable_previous.as_ref(),
        now,
    )?;
    crate::plugins::signed_install::checkpoint::persist(
        root,
        &app_lock,
        portable.checkpoint(),
        portable_previous.as_ref(),
    )?;
    let base = portable.select(plugin_id, version, now)?;
    let details_previous = crate::plugins::signed_install::checkpoint::read_details(
        root,
        &app_lock,
        &trust.catalog_id,
    )?;
    let details = lenso_plugin_catalog::verify_release_details(
        &crate::app::read_signed_portable_snapshot(
            args.release_details
                .as_deref()
                .context("--release-details required")?,
        )?,
        &trust,
        details_previous.as_ref(),
        now,
    )?;
    crate::plugins::signed_install::checkpoint::persist_details(
        root,
        &app_lock,
        details.checkpoint(),
        details_previous.as_ref(),
    )?;
    let release = details
        .snapshot()
        .releases
        .iter()
        .find(|candidate| candidate.plugin_id == plugin_id && candidate.version == version)
        .context("exact release details are not in this catalog")?;
    release.validate_against(base)?;
    let target = lenso_app_authoring::native_host_target();
    let candidates: Vec<_> = release
        .distributions
        .iter()
        .filter(|distribution| {
            distribution.kind == DistributionKind::CargoPackage
                && args
                    .distribution
                    .as_ref()
                    .is_none_or(|id| distribution.id == *id)
                && distribution
                    .targets
                    .iter()
                    .any(|candidate| candidate == target)
        })
        .collect();
    let [distribution] = candidates.as_slice() else {
        bail!(
            "select exactly one signed Cargo distribution for Host target {target}; use --distribution when needed"
        );
    };
    ensure!(
        distribution.registry_url.as_deref() == Some("https://crates.io"),
        "Cargo distribution registry is unsupported; use an authorized custom Host"
    );
    let crate_digest = distribution
        .integrity
        .as_deref()
        .context("Cargo distribution is missing its signed crate digest")?;
    let mut archive = Vec::new();
    fs::File::open(args.crate_archive.as_ref().context("--crate required")?)?
        .take(MAX_CRATE_BYTES + 1)
        .read_to_end(&mut archive)?;
    ensure!(
        !archive.is_empty() && u64::try_from(archive.len())? <= MAX_CRATE_BYTES,
        "crate archive exceeds size limit"
    );
    ensure!(
        lenso_plugin_catalog::digest(&archive) == crate_digest,
        "crate archive digest does not match signed release details"
    );
    adopt_archive(
        root,
        args,
        plugin_id,
        version,
        &distribution.package,
        crate_digest,
        &archive,
        None,
        app_lock,
    )
}

fn adopt_archive(
    root: &Path,
    args: &AddArgs,
    plugin_id: &str,
    version: &str,
    package: &str,
    crate_digest: &str,
    archive: &[u8],
    v6_lock: Option<V6BuildInputLock>,
    app_lock: fs::File,
) -> anyhow::Result<()> {
    super::preflight_source_adoption(root, plugin_id)?;
    let parent = root.join("vendor/lenso").join(plugin_id);
    super::writable_path(root, Path::new("vendor/lenso"))?;
    super::writable_path(
        root,
        &Path::new("vendor/lenso").join(plugin_id).join(version),
    )?;
    let destination = parent.join(version);
    let previous =
        preflight_selected_identity(root, plugin_id, version, &destination, args.replace)?;
    let stage = tempfile::Builder::new()
        .prefix(".linked-cargo-")
        .tempdir_in(root)?;
    unpack_archive(archive, stage.path(), package, version, plugin_id)?;
    let report = lenso_app_authoring::discovery::discover(stage.path())?;
    let [candidate] = report.candidates.as_slice() else {
        bail!("linked Cargo archive must contain one Plugin source package");
    };
    ensure!(
        candidate.plugin_id == plugin_id
            && candidate.release_version == version
            && candidate.format == "cargo"
            && candidate
                .implementations
                .iter()
                .any(|entry| entry.runtime == "native-linked"),
        "linked Cargo source does not match an adoptable native Plugin"
    );
    if let Some(v6) = &v6_lock {
        let manifest: toml::Value =
            toml::from_str(&fs::read_to_string(stage.path().join("Cargo.toml"))?)?;
        let source_slot = manifest["package"]["metadata"]["lenso"]["root-slot"]
            .as_str()
            .context("linked Cargo source has no root Slot")?;
        ensure!(
            v6.contract
                .as_ref()
                .is_some_and(|contract| contract.root_slot() == source_slot),
            "V6 Bundle Contract root Slot differs from exact Cargo source"
        );
    }
    let prepared = if let Some(previous) = previous {
        PreparedSelection::Replacement(replacement::PreparedLinkedReplacement::new_locked(
            root,
            &previous,
            &destination,
            plugin_id,
            app_lock,
        )?)
    } else {
        PreparedSelection::Adoption(adoption::PreparedLinkedAdoption::new_locked(
            root,
            &destination,
            plugin_id,
            app_lock,
        )?)
    };
    let lock = SourceLock {
        schema_version: 1,
        plugin_id: plugin_id.to_owned(),
        version: version.to_owned(),
        crate_digest: crate_digest.to_owned(),
        source_digest: source_digest(stage.path())?,
        archive_cargo_lock_digest: archive_cargo_lock_digest(stage.path())?,
        workspace_exclude_owned: prepared.workspace_exclude_owned(),
        v6: v6_lock,
    };
    let lock_bytes = serde_json::to_vec_pretty(&lock)?;
    let lock_limit = if lock.v6.is_some() {
        MAX_SOURCE_LOCK_BYTES
    } else {
        MAX_V5_SOURCE_LOCK_BYTES
    };
    ensure!(
        u64::try_from(lock_bytes.len())? <= lock_limit,
        "linked Cargo source lock exceeds size limit"
    );
    fs::write(stage.path().join(SOURCE_LOCK), lock_bytes)?;
    prepared
        .commit(stage.path())
        .with_context(|| format!(
            "linked Cargo source selection failed; inspect the error and retry the same exact signed app add input after resolving any filesystem conflict; source={}, cargo={}, config={}, intent={}",
            destination.display(),
            root.join("Cargo.toml").display(),
            root.join("lenso.toml").display(),
            root.join("plugins").join(plugin_id).display(),
        ))?;
    println!(
        "Linked Cargo {}@{} selected for Host compilation; review its build-time code before app build",
        plugin_id, version
    );
    println!(
        "Complete plugins/{plugin_id}/default.toml and select required Capability providers before app build when this Plugin's Contract requires them"
    );
    Ok(())
}

enum PreparedSelection {
    Adoption(adoption::PreparedLinkedAdoption),
    Replacement(replacement::PreparedLinkedReplacement),
}

impl PreparedSelection {
    fn workspace_exclude_owned(&self) -> bool {
        match self {
            Self::Adoption(prepared) => prepared.workspace_exclude_owned(),
            Self::Replacement(prepared) => prepared.workspace_exclude_owned(),
        }
    }

    fn commit(self, source_stage: &Path) -> anyhow::Result<()> {
        match self {
            Self::Adoption(prepared) => prepared.commit(source_stage),
            Self::Replacement(prepared) => prepared.commit(source_stage),
        }
    }
}

fn preflight_selected_identity(
    root: &Path,
    plugin_id: &str,
    version: &str,
    destination: &Path,
    replace: bool,
) -> anyhow::Result<Option<PathBuf>> {
    let current = lenso_app_authoring::discovery::discover(root)
        .context("inspect current App Plugin selection before linked Cargo adoption")?;
    let Some(selected) = current
        .candidates
        .iter()
        .find(|candidate| candidate.plugin_id == plugin_id)
    else {
        ensure!(
            !replace,
            "--replace requires an already selected linked Cargo Plugin source"
        );
        return Ok(None);
    };
    if selected.project == destination && selected.release_version == version {
        ensure!(
            !replace,
            "--replace requires a different exact Plugin version"
        );
        return Ok(None);
    }
    let signed_source = root
        .join("vendor/lenso")
        .join(plugin_id)
        .join(&selected.release_version);
    if selected.project == signed_source {
        if replace {
            verify_sources(root, std::slice::from_ref(selected))?;
            return Ok(Some(signed_source));
        }
        bail!(
            "App already selects {plugin_id}@{} from {}; run `lenso app add {plugin_id}@{version} --replace` with the exact signed inputs, or `lenso app unadopt {plugin_id}@{} --root {}` before adding; automatic replacement is not supported",
            selected.release_version,
            selected.project.display(),
            selected.release_version,
            root.display()
        );
    }
    bail!(
        "App already selects {plugin_id}@{} from {}; remove that App source selection before adding {plugin_id}@{version}; automatic replacement is not supported",
        selected.release_version,
        selected.project.display()
    )
}

fn verified_v6_archive(
    bundle: &Path,
    release: &linked_cargo::LinkedCargoRelease,
    target: &str,
) -> anyhow::Result<SelectedV6Archive> {
    crate::archive::with_bundle_directory(bundle, |directory| {
        verified_v6_directory(directory, release, target)
    })
}

fn verified_v6_directory(
    bundle: &Path,
    release: &linked_cargo::LinkedCargoRelease,
    target: &str,
) -> anyhow::Result<SelectedV6Archive> {
    let limits = BundleVerificationLimits {
        max_file_bytes: MAX_CRATE_BYTES,
        max_total_bytes: MAX_UNPACKED_BYTES,
        ..BundleVerificationLimits::default()
    };
    let (verified, manifest) =
        lenso_plugin_bundle::read_verified_bundle_with_limits(bundle, &limits)
            .context("verify V6 Bundle before linked Cargo adoption")?;
    let PluginManifest::V6(manifest) = manifest else {
        bail!("--bundle requires a V6 Plugin Release with a Cargo build input");
    };
    ensure!(
        manifest.contract.plugin_id() == release.plugin_id
            && manifest.contract.release_version() == release.version,
        "V6 Bundle Contract differs from exact signed linked Cargo release"
    );
    let matching = manifest
        .implementations
        .iter()
        .flat_map(|implementation| {
            implementation
                .variants
                .iter()
                .map(move |variant| (implementation, variant))
        })
        .filter(|(_, variant)| {
            matches!(variant.input, PluginVariantInputV6::CargoBuildInput { .. })
                && variant
                    .host_targets
                    .iter()
                    .any(|candidate| candidate == "*" || candidate == target)
        })
        .collect::<Vec<_>>();
    if matching.is_empty() {
        bail!("V6 Bundle has no Cargo build input for Host target {target}");
    }
    let abi_compatible = matching
        .into_iter()
        .filter(|(_, variant)| {
            variant.runtime.execution_class().as_str() == "lenso.native-rust@1"
                && variant.runtime.runtime_profile() == "lenso.native-rust@1"
        })
        .collect::<Vec<_>>();
    if abi_compatible.is_empty() {
        bail!("V6 Cargo build input requires exact native-linked Host ABI lenso.native-rust@1");
    }
    let compatible = abi_compatible
        .into_iter()
        .filter(|(_, variant)| variant.runtime.required_target_capabilities().is_empty())
        .collect::<Vec<_>>();
    if compatible.is_empty() {
        bail!("V6 Cargo build input has target capability requirements without Host proof");
    }
    for (_, variant) in &compatible {
        ensure!(
            variant.execution_requirements.is_empty(),
            "V6 Cargo build input has execution requirements without verified Host enforcement"
        );
        let PluginVariantInputV6::CargoBuildInput { build_input } = &variant.input else {
            unreachable!("compatible variants have Cargo build inputs")
        };
        ensure!(
            build_input.package == release.package
                && build_input.version == release.version
                && build_input.digest == release.crate_digest
                && build_input.size > 0
                && build_input.size <= MAX_CRATE_BYTES,
            "V6 Cargo build input coordinate, size, or digest differs from signed release"
        );
    }
    let [(implementation, variant)] = compatible.as_slice() else {
        bail!("V6 Bundle has ambiguous Cargo build inputs for Host target {target}");
    };
    let PluginVariantInputV6::CargoBuildInput { build_input } = &variant.input else {
        unreachable!("compatible variants have Cargo build inputs")
    };
    let bytes = lenso_plugin_bundle::read_verified_cargo_build_input(
        bundle,
        build_input,
        &release.plugin_id,
        MAX_CRATE_BYTES,
    )
    .context("reopen exact verified V6 Cargo build input")?;
    Ok(SelectedV6Archive {
        bytes,
        lock: V6BuildInputLock {
            bundle_manifest_digest: verified.manifest_digest,
            implementation_id: implementation.id.clone(),
            variant_id: variant.id.clone(),
            contract: Some(manifest.contract.clone()),
            entrypoint: Some(variant.runtime.entrypoint().to_owned()),
        },
    })
}

pub(crate) fn verify_native_descriptor(
    root: &Path,
    candidate: &Candidate,
    descriptor: &PluginDescriptor,
) -> anyhow::Result<()> {
    let adopted = root
        .join("vendor/lenso")
        .join(&candidate.plugin_id)
        .join(&candidate.release_version);
    let metadata = match fs::symlink_metadata(&adopted) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let lock = read_source_lock(&adopted.join(SOURCE_LOCK))?;
    let Some(v6) = lock.v6.as_ref() else {
        return Ok(());
    };
    ensure!(
        metadata.file_type().is_dir() && candidate.project == fs::canonicalize(&adopted)?,
        "adopted linked Cargo source path differs from discovered Plugin source"
    );
    ensure!(
        lock.schema_version == 1
            && lock.plugin_id == candidate.plugin_id
            && lock.version == candidate.release_version,
        "V6 linked Cargo source lock differs from discovered Plugin identity"
    );
    ensure!(
        source_digest(&adopted)? == lock.source_digest,
        "V6 linked Cargo source changed after adoption"
    );
    verify_archive_cargo_lock(&adopted, &lock)?;
    let contract = v6.contract.as_ref().context(
        "V6 linked Cargo source lock lacks its expected Contract; retry exact signed app add",
    )?;
    let entrypoint = v6.entrypoint.as_ref().context(
        "V6 linked Cargo source lock lacks its selected entrypoint; retry exact signed app add",
    )?;
    ensure!(
        descriptor.contract() == *contract,
        "compiled native Plugin Descriptor differs from V6 Bundle Contract"
    );
    ensure!(
        descriptor.entrypoint() == entrypoint,
        "compiled native Plugin Descriptor differs from V6 Bundle selected entrypoint"
    );
    ensure!(
        descriptor.authoring_version() == 2,
        "compiled native Plugin Descriptor requires authoring version 2"
    );
    ensure!(
        descriptor.execution_class().as_str() == "lenso.native-rust@1",
        "compiled native Plugin Descriptor requires native-linked execution class"
    );
    ensure!(
        descriptor.runtime_profile() == "lenso.native-authoring@2",
        "compiled native Plugin Descriptor has incompatible runtime profile"
    );
    ensure!(
        descriptor.required_target_capabilities().is_empty(),
        "compiled native Plugin Descriptor has unproven target capabilities"
    );
    ensure!(
        descriptor.runtime_package_id() == candidate.plugin_id
            && descriptor.runtime_package_revision() == candidate.release_version,
        "compiled native Plugin Descriptor differs from V6 linked Cargo package identity"
    );
    Ok(())
}

pub(crate) fn verify_sources(root: &Path, candidates: &[Candidate]) -> anyhow::Result<()> {
    let vendor_root = fs::canonicalize(root)?.join("vendor/lenso");
    verify_selected_portable_paths(root, candidates)?;
    for candidate in candidates {
        if !candidate.project.starts_with(&vendor_root) {
            continue;
        }
        if candidate.project.starts_with(vendor_root.join("portable")) {
            crate::plugins::signed_install::verify_source_candidate(root, candidate)?;
            continue;
        }
        let lock_path = candidate.project.join(SOURCE_LOCK);
        let lock = read_source_lock(&lock_path)?;
        ensure!(
            lock.schema_version == 1
                && lock.plugin_id == candidate.plugin_id
                && lock.version == candidate.release_version,
            "linked Cargo source identity changed: {}",
            candidate.project.display()
        );
        ensure!(
            source_digest(&candidate.project)? == lock.source_digest,
            "linked Cargo source changed after adoption: {}",
            candidate.project.display()
        );
        verify_archive_cargo_lock(&candidate.project, &lock)?;
    }
    Ok(())
}

fn verify_selected_portable_paths(root: &Path, candidates: &[Candidate]) -> anyhow::Result<()> {
    let root = fs::canonicalize(root)?;
    let config = root.join("lenso.toml");
    let document: toml::Value = match fs::read_to_string(&config) {
        Ok(text) => toml::from_str(&text)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let Some(sources) = document
        .get("plugin_sources")
        .and_then(toml::Value::as_array)
    else {
        return Ok(());
    };
    for source in sources {
        let source = source
            .as_str()
            .context("Plugin source must be a path string")?;
        let relative = Path::new(source);
        if !relative.starts_with("vendor/lenso/portable") {
            continue;
        }
        ensure!(
            relative
                .components()
                .all(|component| matches!(component, Component::Normal(_))),
            "signed Portable source path must be a direct relative path"
        );
        let mut path = root.clone();
        for component in relative.components() {
            path.push(component.as_os_str());
            ensure!(
                !fs::symlink_metadata(&path)?.file_type().is_symlink(),
                "signed Portable source path cannot traverse a symlink: {}",
                path.display()
            );
        }
        ensure!(
            fs::symlink_metadata(&path)?.file_type().is_file(),
            "signed Portable source must be a regular archive: {}",
            path.display()
        );
        ensure!(
            candidates.iter().any(|candidate| candidate.project == path),
            "selected signed Portable source was not discovered"
        );
    }
    Ok(())
}

fn source_digest(root: &Path) -> anyhow::Result<String> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    let mut total = 0u64;
    while let Some(path) = pending.pop() {
        ensure!(
            pending.len() + files.len() <= MAX_FILES,
            "linked Cargo source has too many files"
        );
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            if path == root.join("target") {
                continue;
            }
            for entry in fs::read_dir(path)? {
                pending.push(entry?.path());
                ensure!(
                    pending.len() + files.len() <= MAX_FILES,
                    "linked Cargo source has too many files"
                );
            }
        } else if metadata.is_file() {
            if path != root.join(SOURCE_LOCK) && path != root.join("Cargo.lock") {
                total = total
                    .checked_add(metadata.len())
                    .context("linked Cargo source size overflow")?;
                ensure!(
                    total <= MAX_UNPACKED_BYTES,
                    "linked Cargo source exceeds size limit"
                );
                files.push(path);
            }
        } else {
            bail!("linked Cargo source contains a symlink or special file");
        }
    }
    files.sort();
    let mut hasher = Sha256::new();
    for path in files {
        let relative = path
            .strip_prefix(root)?
            .to_str()
            .context("linked Cargo path UTF-8")?;
        let bytes = fs::read(&path)?;
        hasher.update((relative.len() as u64).to_be_bytes());
        hasher.update(relative.as_bytes());
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    }
    Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
}

fn archive_cargo_lock_digest(root: &Path) -> anyhow::Result<Option<String>> {
    let path = root.join("Cargo.lock");
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_file() && metadata.len() <= MAX_CRATE_BYTES => {
            Ok(Some(lenso_plugin_catalog::digest(&fs::read(path)?)))
        }
        Ok(_) => bail!("linked Cargo root Cargo.lock is not a bounded regular file"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn verify_archive_cargo_lock(root: &Path, lock: &SourceLock) -> anyhow::Result<()> {
    if let Some(expected) = &lock.archive_cargo_lock_digest {
        ensure!(
            archive_cargo_lock_digest(root)?.as_deref() == Some(expected),
            "signed archive Cargo.lock changed after adoption: {}",
            root.display()
        );
    }
    Ok(())
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum UnadoptCheckpoint {
    AfterSourceMove,
    AfterIntentMove,
    AfterConfigPublish,
}

pub(super) fn unadopt(root: &Path, source: &str) -> anyhow::Result<()> {
    unadopt_with(root, source, |_| Ok(()))
}

fn unadopt_with(
    root: &Path,
    source: &str,
    mut checkpoint: impl FnMut(UnadoptCheckpoint) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let (plugin_id, version) = source
        .split_once('@')
        .context("linked Cargo source must be an exact PLUGIN_ID@VERSION")?;
    lenso_app_authoring::identity::validate_plugin_id_v1(plugin_id)?;
    lenso_app_authoring::identity::validate_release_version(version)?;
    let _app_lock = adoption::lock_app(root)?;
    let relative = Path::new("vendor/lenso").join(plugin_id).join(version);
    let source_path = root.join(&relative);
    let intent_relative = Path::new("plugins").join(plugin_id);
    let intent_path = root.join(&intent_relative);
    for path in [
        relative.as_path(),
        intent_relative.as_path(),
        Path::new("lenso.toml"),
        Path::new("Cargo.toml"),
        Path::new(".lenso/trash/linked-cargo"),
    ] {
        super::writable_path(root, path)?;
    }
    ensure!(source_path.is_dir(), "linked Cargo source is not adopted");
    let lock = read_source_lock(&source_path.join(SOURCE_LOCK))
        .context("linked Cargo source lock is missing")?;
    ensure!(
        lock.schema_version == 1 && lock.plugin_id == plugin_id && lock.version == version,
        "linked Cargo source lock does not match requested identity"
    );
    ensure!(
        source_digest(&source_path)? == lock.source_digest,
        "linked Cargo source has user changes; preserve it and review before unadopting"
    );
    verify_archive_cargo_lock(&source_path, &lock)?;
    ensure!(
        fs::symlink_metadata(&intent_path)?.file_type().is_dir(),
        "linked Cargo Plugin Root intent must be a regular directory"
    );
    let mut default = false;
    for entry in fs::read_dir(&intent_path)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_str().context("Plugin Root intent filename UTF-8")?;
        ensure!(
            name == "default.toml" || name == "default.disabled",
            "Plugin Root intent has user changes; remove it explicitly before unadopting source"
        );
        let metadata = fs::symlink_metadata(entry.path())?;
        ensure!(
            metadata.file_type().is_file() && (name != "default.disabled" || metadata.len() == 0),
            "Plugin Root intent has user changes; remove it explicitly before unadopting source"
        );
        if name == "default.toml" {
            ensure!(
                fs::read_to_string(entry.path())? == "# Explicit local Plugin adoption\n",
                "Plugin Root intent has user changes; remove it explicitly before unadopting source"
            );
            default = true;
        }
    }
    ensure!(
        default,
        "linked Cargo Plugin Root default intent is missing"
    );
    let config_path = root.join("lenso.toml");
    let mut document: toml::Value = toml::from_str(&fs::read_to_string(&config_path)?)?;
    let sources = document
        .get_mut("plugin_sources")
        .and_then(toml::Value::as_array_mut)
        .context("lenso.toml plugin_sources array")?;
    let selected = relative.to_str().context("linked source path UTF-8")?;
    ensure!(
        sources
            .iter()
            .filter(|value| value.as_str() == Some(selected))
            .count()
            == 1,
        "linked Cargo source is not uniquely selected by lenso.toml"
    );
    sources.retain(|value| value.as_str() != Some(selected));
    let config_before = fs::read(&config_path)?;
    let config_after = toml::to_string_pretty(&document)?.into_bytes();
    let mut staged = tempfile::NamedTempFile::new_in(root)?;
    staged.write_all(&config_after)?;
    let cargo_path = root.join("Cargo.toml");
    let cargo_before = adoption::read_optional_regular(&cargo_path)?;
    let cargo_after = adoption::remove_workspace_exclude(
        cargo_before.as_deref(),
        &relative,
        lock.workspace_exclude_owned,
    )?;
    let staged_cargo = if cargo_after != cargo_before {
        let mut staged = tempfile::NamedTempFile::new_in(root)?;
        staged.write_all(
            cargo_after
                .as_deref()
                .context("updated Cargo.toml is missing")?,
        )?;
        Some(staged)
    } else {
        None
    };
    let trash_parent = root.join(".lenso/trash/linked-cargo");
    fs::create_dir_all(&trash_parent)?;
    let trash = tempfile::Builder::new()
        .prefix("unadopt-")
        .tempdir_in(&trash_parent)?
        .keep();
    ensure!(
        adoption::read_optional_regular(&cargo_path)? == cargo_before,
        "Cargo.toml changed during linked Cargo unadopt; preserving concurrent edit"
    );
    ensure!(
        fs::read(&config_path)? == config_before,
        "lenso.toml changed during linked Cargo unadopt; preserving concurrent edit"
    );
    let mut source_moved = false;
    let mut intent_moved = false;
    let mut config_published = false;
    let result = (|| -> anyhow::Result<()> {
        super::super::build::publish_new_output(&source_path, &trash.join("source"))?;
        source_moved = true;
        checkpoint(UnadoptCheckpoint::AfterSourceMove)?;
        super::super::build::publish_new_output(&intent_path, &trash.join("plugin-root"))?;
        intent_moved = true;
        checkpoint(UnadoptCheckpoint::AfterIntentMove)?;
        ensure!(
            adoption::read_optional_regular(&cargo_path)? == cargo_before
                && fs::read(&config_path)? == config_before,
            "App manifests changed during linked Cargo unadopt; preserving concurrent edit"
        );
        staged.persist(&config_path)?;
        config_published = true;
        checkpoint(UnadoptCheckpoint::AfterConfigPublish)?;
        ensure!(
            adoption::read_optional_regular(&cargo_path)? == cargo_before,
            "Cargo.toml changed during linked Cargo unadopt; preserving concurrent edit"
        );
        if let Some(staged_cargo) = staged_cargo {
            staged_cargo.persist(&cargo_path)?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        if let Err(rollback_error) = rollback_unadopt(
            root,
            &trash,
            &source_path,
            &intent_path,
            &config_path,
            &config_before,
            &config_after,
            source_moved,
            intent_moved,
            config_published,
        ) {
            return Err(error.context(format!(
                "linked Cargo unadopt rollback incomplete; recover from {}: {rollback_error:#}",
                trash.display()
            )));
        }
        return Err(error);
    }
    println!(
        "Unadopted {plugin_id}@{version}; source and Plugin Root intent are recoverable at {}",
        trash.display()
    );
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "restores one attempted linked Cargo unadopt"
)]
fn rollback_unadopt(
    root: &Path,
    trash: &Path,
    source_path: &Path,
    intent_path: &Path,
    config_path: &Path,
    config_before: &[u8],
    config_after: &[u8],
    source_moved: bool,
    intent_moved: bool,
    config_published: bool,
) -> anyhow::Result<()> {
    let mut failures = Vec::new();
    for (moved, staged, original) in [
        (intent_moved, trash.join("plugin-root"), intent_path),
        (source_moved, trash.join("source"), source_path),
    ] {
        if moved {
            if let Err(error) = super::super::build::publish_new_output(&staged, original) {
                failures.push(format!("restore {}: {error:#}", original.display()));
            }
        }
    }
    if config_published {
        match adoption::read_optional_regular(config_path) {
            Ok(Some(current)) if current == config_after => {
                let restored = (|| -> anyhow::Result<()> {
                    let mut staged = tempfile::NamedTempFile::new_in(root)?;
                    staged.write_all(config_before)?;
                    ensure!(
                        adoption::read_optional_regular(config_path)?.as_deref()
                            == Some(config_after),
                        "lenso.toml changed during rollback"
                    );
                    staged.persist(config_path)?;
                    Ok(())
                })();
                if let Err(error) = restored {
                    failures.push(format!("restore {}: {error:#}", config_path.display()));
                }
            }
            Ok(_) => failures.push(format!(
                "preserved concurrent edit at {}",
                config_path.display()
            )),
            Err(error) => failures.push(format!("inspect {}: {error:#}", config_path.display())),
        }
    }
    ensure!(failures.is_empty(), "{}", failures.join("; "));
    Ok(())
}

/// Extracts one exact Cargo source archive into an empty authoring directory.
/// Every member is constrained to the declared package root before it is written.
pub(crate) fn unpack_archive(
    bytes: &[u8],
    stage: &Path,
    package: &str,
    version: &str,
    plugin_id: &str,
) -> anyhow::Result<()> {
    let root = format!("{package}-{version}/");
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    let mut files = 0usize;
    let mut total = 0u64;
    for entry in archive.entries().context("invalid crate archive")? {
        let mut entry = entry.context("invalid crate archive entry")?;
        files += 1;
        ensure!(files <= MAX_FILES, "crate archive has too many files");
        total = total
            .checked_add(entry.size())
            .context("crate size overflow")?;
        ensure!(
            total <= MAX_UNPACKED_BYTES,
            "crate archive exceeds uncompressed size limit"
        );
        ensure!(
            entry.header().entry_type().is_file(),
            "crate archive has a non-file entry"
        );
        let path = std::str::from_utf8(&entry.path_bytes())?.to_owned();
        ensure!(
            path.starts_with(&root),
            "crate archive path is outside its package root"
        );
        let relative = &path[root.len()..];
        ensure!(
            !relative.is_empty()
                && !relative.contains('\\')
                && relative
                    .split('/')
                    .all(|part| !matches!(part, "" | "." | ".."))
                && relative != SOURCE_LOCK
                && !relative.starts_with("target/"),
            "crate archive has an invalid path"
        );
        let destination = stage.join(relative);
        fs::create_dir_all(destination.parent().context("crate file parent")?)?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)?;
        let copied = std::io::copy(&mut entry, &mut file)?;
        ensure!(copied == entry.size(), "crate archive entry is truncated");
        file.flush()?;
    }
    let manifest = fs::read_to_string(stage.join("Cargo.toml"))
        .context("crate archive is missing Cargo.toml")?;
    ensure!(
        manifest.len() <= 1024 * 1024,
        "crate manifest exceeds size limit"
    );
    let manifest: toml::Value = toml::from_str(&manifest)?;
    ensure!(
        manifest["package"]["name"].as_str() == Some(package),
        "crate package name does not match signed release"
    );
    ensure!(
        manifest["package"]["version"].as_str() == Some(version),
        "crate package version does not match signed release"
    );
    ensure!(
        manifest["package"]["metadata"]["lenso"]["plugin-id"].as_str() == Some(plugin_id),
        "crate Plugin ID does not match signed release"
    );
    Ok(())
}

fn same_tree(expected: &Path, actual: &Path) -> anyhow::Result<bool> {
    same_tree_children(expected, actual, true)
}

fn same_tree_children(expected: &Path, actual: &Path, root: bool) -> anyhow::Result<bool> {
    if !actual.is_dir() || actual.is_symlink() {
        return Ok(false);
    }
    let mut expected_entries = fs::read_dir(expected)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<Result<Vec<_>, _>>()?;
    let mut actual_entries = fs::read_dir(actual)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<Result<Vec<_>, _>>()?;
    if root && !expected_entries.iter().any(|name| name == "Cargo.lock") {
        // Cargo may materialize a root lock after adoption. Only a lock
        // absent from the signed archive may be ignored during an exact retry.
        if actual_entries.iter().any(|name| name == "Cargo.lock") {
            let metadata = fs::symlink_metadata(actual.join("Cargo.lock"))?;
            if !metadata.is_file() || metadata.len() > MAX_CRATE_BYTES {
                return Ok(false);
            }
        }
        actual_entries.retain(|name| name != "Cargo.lock");
    }
    expected_entries.sort();
    actual_entries.sort();
    if expected_entries != actual_entries {
        return Ok(false);
    }
    for name in expected_entries {
        let expected_child: PathBuf = expected.join(&name);
        let actual_child: PathBuf = actual.join(&name);
        let metadata = fs::symlink_metadata(&actual_child)?;
        if metadata.file_type().is_symlink() {
            return Ok(false);
        }
        if expected_child.is_dir() {
            if !same_tree_children(&expected_child, &actual_child, false)? {
                return Ok(false);
            }
        } else {
            if !metadata.is_file() {
                return Ok(false);
            }
            let expected_len = fs::metadata(&expected_child)?.len();
            if root && name == SOURCE_LOCK {
                if metadata.len() > MAX_SOURCE_LOCK_BYTES || expected_len > MAX_SOURCE_LOCK_BYTES {
                    return Ok(false);
                }
            } else if metadata.len() != expected_len {
                return Ok(false);
            }
            let expected_bytes = fs::read(&expected_child)?;
            let actual_bytes = fs::read(&actual_child)?;
            if actual_bytes != expected_bytes
                && !(root
                    && name == SOURCE_LOCK
                    && canonical_prior_source_lock_matches(&expected_bytes, &actual_bytes)?)
            {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lenso_app_authoring::host_authoring::{GeneratedHostBuild, LocalPluginInput};
    use lenso_app_plan::authoring::{DependencyChoice, PluginDescriptor, PluginInstanceId};
    use lenso_app_plan::{CapabilityEndpointPlan, CapabilityRequirementPlan};

    fn adopted_source() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("vendor/lenso/example.web/0.4.5");
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join("Cargo.toml"),
            b"[package]\nname = 'example-web'\n",
        )
        .unwrap();
        let lock = SourceLock {
            schema_version: 1,
            plugin_id: "example.web".into(),
            version: "0.4.5".into(),
            crate_digest: format!("sha256:{}", "a".repeat(64)),
            source_digest: source_digest(&source).unwrap(),
            archive_cargo_lock_digest: None,
            workspace_exclude_owned: false,
            v6: None,
        };
        fs::write(
            source.join(SOURCE_LOCK),
            serde_json::to_vec_pretty(&lock).unwrap(),
        )
        .unwrap();
        let intent = root.path().join("plugins/example.web");
        fs::create_dir_all(&intent).unwrap();
        fs::write(
            intent.join("default.toml"),
            b"# Explicit local Plugin adoption\n",
        )
        .unwrap();
        fs::write(
            root.path().join("lenso.toml"),
            b"plugin_sources = ['vendor/lenso/example.web/0.4.5']\n",
        )
        .unwrap();
        root
    }

    fn assert_adoption_intact(root: &Path) {
        assert!(
            root.join("vendor/lenso/example.web/0.4.5/Cargo.toml")
                .is_file()
        );
        assert!(root.join("plugins/example.web/default.toml").is_file());
        assert!(
            fs::read_to_string(root.join("lenso.toml"))
                .unwrap()
                .contains("vendor/lenso/example.web/0.4.5")
        );
    }

    #[test]
    fn v6_native_descriptor_requires_contract_and_selected_entrypoint() {
        let app = adopted_source();
        let root = fs::canonicalize(app.path()).unwrap();
        let source = fs::canonicalize(root.join("vendor/lenso/example.web/0.4.5")).unwrap();
        let candidate = Candidate {
            surface_owner: None,
            composite: None,
            plugin_id: "example.web".into(),
            release_version: "0.4.5".into(),
            project: source.clone(),
            metadata: source.join("Cargo.toml"),
            format: "cargo".into(),
            role: lenso_app_authoring::discovery::SourceRole::Shared,
            implementations: Vec::new(),
            published_resources: Vec::new(),
            evidence: "test".into(),
        };
        let descriptor = PluginDescriptor::new("example.web", "0.4.5", "web")
            .with_authoring(2, "lenso.native-authoring@2");
        let lock_path = source.join(SOURCE_LOCK);
        let mut lock = read_source_lock(&lock_path).unwrap();
        lock.v6 = Some(V6BuildInputLock {
            bundle_manifest_digest: format!("sha256:{}", "a".repeat(64)),
            implementation_id: "native".into(),
            variant_id: "cargo".into(),
            contract: None,
            entrypoint: None,
        });
        fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();
        let error = verify_native_descriptor(&root, &candidate, &descriptor).unwrap_err();
        assert!(format!("{error:#}").contains("lacks its expected Contract"));

        let v6 = lock.v6.as_mut().unwrap();
        v6.contract = Some(descriptor.contract());
        v6.entrypoint = Some("forged-entrypoint".into());
        fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();
        let error = verify_native_descriptor(&root, &candidate, &descriptor).unwrap_err();
        assert!(format!("{error:#}").contains("selected entrypoint"));

        lock.v6.as_mut().unwrap().entrypoint = Some("default".into());
        fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();
        verify_native_descriptor(&root, &candidate, &descriptor).unwrap();
        let profile_drift = descriptor
            .clone()
            .with_authoring(2, "lenso.native-authoring@1");
        let error = verify_native_descriptor(&root, &candidate, &profile_drift).unwrap_err();
        assert!(format!("{error:#}").contains("runtime profile"));
    }

    #[test]
    fn unadopt_preserves_optional_absence_and_unrelated_saved_binding() {
        let root = adopted_source();
        let consumer_id = PluginInstanceId::new("example.consumer", "default");
        let auditor_id = PluginInstanceId::new("example.auditor", "default");
        let choices = vec![
            DependencyChoice {
                consumer: consumer_id.clone(),
                requirement_id: "audit".into(),
                provider: Some(auditor_id),
            },
            DependencyChoice {
                consumer: consumer_id,
                requirement_id: "jobs".into(),
                provider: None,
            },
        ];
        let path = root.path().join("plugins/.dependencies.json");
        let before =
            serde_json::to_vec_pretty(&lenso_app_authoring::DependencySelectionsDocument {
                schema_version: lenso_app_authoring::DEPENDENCY_SELECTIONS_SCHEMA_VERSION,
                choices: choices.clone(),
            })
            .unwrap();
        fs::write(&path, &before).unwrap();

        let input = |descriptor: PluginDescriptor, app_owned| LocalPluginInput {
            source: descriptor.plugin_id().into(),
            descriptor,
            manifest_digest: format!("sha256:{}", "a".repeat(64)),
            app_owned,
        };
        let consumer = input(
            PluginDescriptor::new("example.consumer", "1.0.0", "tools")
                .with_authoring(2, "lenso.native-authoring@2")
                .with_requirement(
                    CapabilityRequirementPlan::optional("example.audit@1", "1")
                        .with_requirement_id("audit"),
                )
                .with_requirement(
                    CapabilityRequirementPlan::optional("example.jobs@1", "1")
                        .with_requirement_id("jobs"),
                ),
            true,
        );
        let auditor = input(
            PluginDescriptor::new("example.auditor", "1.0.0", "tools").with_capability(
                CapabilityEndpointPlan::new("example.audit@1", "1", ["check"]),
            ),
            true,
        );
        let jobs = input(
            PluginDescriptor::new("example.web", "0.4.5", "tools").with_capability(
                CapabilityEndpointPlan::new("example.jobs@1", "1", ["enqueue"]),
            ),
            false,
        );
        let (_, before_removal) =
            GeneratedHostBuild::lower_local("example.app", vec![consumer, auditor, jobs])
                .unwrap()
                .with_local_root(root.path())
                .unwrap();
        assert_eq!(before_removal.dependency_choices(), choices);
        assert_eq!(before_removal.plan().capability_bindings().len(), 1);

        unadopt(root.path(), "example.web@0.4.5").unwrap();
        assert_eq!(fs::read(&path).unwrap(), before);
        let (_, after_removal) = GeneratedHostBuild::lower_local(
            "example.app",
            vec![
                input(
                    PluginDescriptor::new("example.consumer", "1.0.0", "tools")
                        .with_authoring(2, "lenso.native-authoring@2")
                        .with_requirement(
                            CapabilityRequirementPlan::optional("example.audit@1", "1")
                                .with_requirement_id("audit"),
                        )
                        .with_requirement(
                            CapabilityRequirementPlan::optional("example.jobs@1", "1")
                                .with_requirement_id("jobs"),
                        ),
                    true,
                ),
                input(
                    PluginDescriptor::new("example.auditor", "1.0.0", "tools").with_capability(
                        CapabilityEndpointPlan::new("example.audit@1", "1", ["check"]),
                    ),
                    true,
                ),
            ],
        )
        .unwrap()
        .with_local_root(root.path())
        .unwrap();
        assert_eq!(after_removal.dependency_choices(), choices);
        assert_eq!(after_removal.plan().capability_bindings().len(), 1);
    }

    #[test]
    fn unadopt_rejects_edited_or_non_regular_disabled_intent() {
        let root = adopted_source();
        let disabled = root.path().join("plugins/example.web/default.disabled");
        fs::write(&disabled, b"user edit\n").unwrap();
        assert!(unadopt(root.path(), "example.web@0.4.5").is_err());
        assert_adoption_intact(root.path());
        #[cfg(unix)]
        {
            fs::remove_file(&disabled).unwrap();
            std::os::unix::fs::symlink(root.path().join("lenso.toml"), &disabled).unwrap();
            assert!(unadopt(root.path(), "example.web@0.4.5").is_err());
            assert_adoption_intact(root.path());
            fs::remove_file(&disabled).unwrap();
        }
        fs::write(&disabled, []).unwrap();
        unadopt(root.path(), "example.web@0.4.5").unwrap();
        let trash = root.path().join(".lenso/trash/linked-cargo");
        let entry = fs::read_dir(trash).unwrap().next().unwrap().unwrap().path();
        assert!(entry.join("plugin-root/default.disabled").is_file());
    }

    #[test]
    fn unadopt_rolls_back_each_published_phase() {
        for interrupted in [
            UnadoptCheckpoint::AfterSourceMove,
            UnadoptCheckpoint::AfterIntentMove,
            UnadoptCheckpoint::AfterConfigPublish,
        ] {
            let root = adopted_source();
            let before = fs::read(root.path().join("lenso.toml")).unwrap();
            let error = unadopt_with(root.path(), "example.web@0.4.5", |step| {
                if step == interrupted {
                    bail!("injected interruption");
                }
                Ok(())
            })
            .unwrap_err();
            assert!(error.to_string().contains("injected interruption"));
            assert_adoption_intact(root.path());
            assert_eq!(fs::read(root.path().join("lenso.toml")).unwrap(), before);
        }
    }

    #[test]
    fn unadopt_restores_directories_when_manifest_read_fails_after_move() {
        let root = adopted_source();
        let config = root.path().join("lenso.toml");
        let error = unadopt_with(root.path(), "example.web@0.4.5", |step| {
            if step == UnadoptCheckpoint::AfterIntentMove {
                fs::remove_file(&config)?;
                fs::create_dir(&config)?;
            }
            Ok(())
        })
        .unwrap_err();
        assert!(!error.to_string().contains("rollback incomplete"));
        assert!(root.path().join("vendor/lenso/example.web/0.4.5").is_dir());
        assert!(
            root.path()
                .join("plugins/example.web/default.toml")
                .is_file()
        );
        assert!(
            config.is_dir(),
            "concurrent path change must not be overwritten"
        );
    }

    #[test]
    fn unadopt_preserves_conflicting_restore_path_in_trash() {
        let root = adopted_source();
        let source = root.path().join("vendor/lenso/example.web/0.4.5");
        let error = unadopt_with(root.path(), "example.web@0.4.5", |step| {
            if step == UnadoptCheckpoint::AfterIntentMove {
                fs::create_dir(&source)?;
                bail!("injected interruption");
            }
            Ok(())
        })
        .unwrap_err();
        assert!(error.to_string().contains("rollback incomplete"));
        assert!(
            source.is_dir(),
            "concurrent empty directory must not be replaced"
        );
        assert!(
            root.path()
                .join("plugins/example.web/default.toml")
                .is_file()
        );
        let trash = root.path().join(".lenso/trash/linked-cargo");
        let entry = fs::read_dir(trash).unwrap().next().unwrap().unwrap().path();
        assert!(entry.join("source/Cargo.toml").is_file());
    }

    #[test]
    fn crate_archive_rejects_windows_style_path_escape() {
        let release = linked_cargo::LinkedCargoRelease {
            plugin_id: "example.web".into(),
            version: "0.4.5".into(),
            publisher_id: "example".into(),
            title: "Web".into(),
            summary: "Web Plugin".into(),
            source_url: "https://example.test/web".into(),
            source_revision: "a".repeat(40),
            license: "MIT".into(),
            package: "example-web-plugin".into(),
            registry_url: "https://crates.io".into(),
            crate_digest: format!("sha256:{}", "a".repeat(64)),
            integration: linked_cargo::LinkedCargoIntegration::LinkedPlugin,
            targets: vec!["aarch64-apple-darwin".into()],
            availability: Availability::Listed,
            documentation: vec![],
        };
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        let path = "example-web-plugin-0.4.5/src\\..\\escape.rs";
        let contents = b"pub fn escaped() {}";
        let mut header = tar::Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, path, contents.as_slice())
            .unwrap();
        let archive = builder.into_inner().unwrap().finish().unwrap();
        let stage = tempfile::tempdir().unwrap();
        assert!(
            unpack_archive(
                &archive,
                stage.path(),
                &release.package,
                &release.version,
                &release.plugin_id,
            )
            .unwrap_err()
            .to_string()
            .contains("invalid path")
        );
        assert!(!stage.path().join("escape.rs").exists());
    }

    #[test]
    fn repeated_adoption_ignores_only_generated_root_cargo_lock() {
        let root = tempfile::tempdir().unwrap();
        let expected = root.path().join("expected");
        let actual = root.path().join("actual");
        fs::create_dir_all(expected.join("nested")).unwrap();
        fs::create_dir_all(actual.join("nested")).unwrap();
        fs::write(expected.join("Cargo.toml"), b"[package]\n").unwrap();
        fs::write(actual.join("Cargo.toml"), b"[package]\n").unwrap();
        fs::write(actual.join("Cargo.lock"), b"generated\n").unwrap();
        assert!(same_tree(&expected, &actual).unwrap());
        fs::write(actual.join("nested/Cargo.lock"), b"unexpected\n").unwrap();
        assert!(!same_tree(&expected, &actual).unwrap());
    }

    #[test]
    fn crate_archive_accepts_registry_root_cargo_lock() {
        let release = linked_cargo::LinkedCargoRelease {
            plugin_id: "example.web".into(),
            version: "0.4.5".into(),
            publisher_id: "example".into(),
            title: "Web".into(),
            summary: "Web Plugin".into(),
            source_url: "https://example.test/web".into(),
            source_revision: "a".repeat(40),
            license: "MIT".into(),
            package: "example-web-plugin".into(),
            registry_url: "https://crates.io".into(),
            crate_digest: format!("sha256:{}", "a".repeat(64)),
            integration: linked_cargo::LinkedCargoIntegration::LinkedPlugin,
            targets: vec!["aarch64-apple-darwin".into()],
            availability: Availability::Listed,
            documentation: vec![],
        };
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        for (name, contents) in [
            (
                "Cargo.toml",
                "[package]\nname = \"example-web-plugin\"\nversion = \"0.4.5\"\n[package.metadata.lenso]\nplugin-id = \"example.web\"\n",
            ),
            ("Cargo.lock", "# published lock\n"),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(
                    &mut header,
                    format!("example-web-plugin-0.4.5/{name}"),
                    contents.as_bytes(),
                )
                .unwrap();
        }
        let archive = builder.into_inner().unwrap().finish().unwrap();
        let stage = tempfile::tempdir().unwrap();
        unpack_archive(
            &archive,
            stage.path(),
            &release.package,
            &release.version,
            &release.plugin_id,
        )
        .unwrap();
        assert_eq!(
            fs::read(stage.path().join("Cargo.lock")).unwrap(),
            b"# published lock\n"
        );
        let actual = tempfile::tempdir().unwrap();
        fs::write(
            actual.path().join("Cargo.toml"),
            fs::read(stage.path().join("Cargo.toml")).unwrap(),
        )
        .unwrap();
        fs::write(actual.path().join("Cargo.lock"), b"# published lock\n").unwrap();
        assert!(same_tree(stage.path(), actual.path()).unwrap());
        fs::write(actual.path().join("Cargo.lock"), b"# Cargo-updated lock\n").unwrap();
        assert!(!same_tree(stage.path(), actual.path()).unwrap());
        let lock = SourceLock {
            schema_version: 1,
            plugin_id: release.plugin_id,
            version: release.version,
            crate_digest: release.crate_digest,
            source_digest: source_digest(stage.path()).unwrap(),
            archive_cargo_lock_digest: archive_cargo_lock_digest(stage.path()).unwrap(),
            workspace_exclude_owned: false,
            v6: None,
        };
        assert!(verify_archive_cargo_lock(actual.path(), &lock).is_err());
        fs::write(actual.path().join("Cargo.lock"), b"# published lock\n").unwrap();
        verify_archive_cargo_lock(actual.path(), &lock).unwrap();
    }

    #[test]
    fn exact_retry_only_ignores_a_regular_generated_root_lock() {
        let expected = tempfile::tempdir().unwrap();
        let actual = tempfile::tempdir().unwrap();
        for root in [expected.path(), actual.path()] {
            fs::write(
                root.join("Cargo.toml"),
                b"[package]\nname = \"example\"\nversion = \"0.1.0\"\n",
            )
            .unwrap();
        }
        fs::write(actual.path().join("Cargo.lock"), b"# generated lock\n").unwrap();
        assert!(same_tree(expected.path(), actual.path()).unwrap());
        fs::remove_file(actual.path().join("Cargo.lock")).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                actual.path().join("Cargo.toml"),
                actual.path().join("Cargo.lock"),
            )
            .unwrap();
            assert!(!same_tree(expected.path(), actual.path()).unwrap());
        }
    }
}
