//! Offline verification of official Marketplace catalog provenance.
//!
//! This transition interface requires a trusted GitHub CLI installation and
//! independently pinned Sigstore roots. It does not authorize App adoption.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Read,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, bail, ensure};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest as _, Sha256};

const MAX_INPUT: u64 = 8 * 1024 * 1024;
const MAX_OUTPUT: u64 = 2 * 1024 * 1024;
const REPOSITORY: &str = "LioRael/lenso-marketplace";
const WORKFLOW: &str = "LioRael/lenso-marketplace/.github/workflows/publish-keyless-catalog.yml";
const IDENTITY: &str = "https://github.com/LioRael/lenso-marketplace/.github/workflows/publish-keyless-catalog.yml@refs/heads/main";

/// Operator-selected verifier inputs, never derived from the catalog itself.
#[derive(Debug)]
pub struct KeylessCatalogVerification<'a> {
    /// Absolute path to a trusted `gh` executable, outside downloaded inputs.
    pub gh: &'a Path,
    /// Exact bytes of the catalog being verified.
    pub catalog: &'a Path,
    /// Sigstore attestation bundle covering the catalog bytes.
    pub bundle: &'a Path,
    /// Independently provisioned public trusted root set.
    pub trusted_root: &'a Path,
    /// Lowercase SHA-256 pin for the trusted root set, without a prefix.
    pub trusted_root_sha256: &'a str,
    /// Independently reviewed source commit of the publishing workflow.
    pub source_sha: &'a str,
}

/// Cryptographically verified catalog bytes; not permission to install a release.
#[derive(Debug)]
pub struct VerifiedKeylessCatalog {
    catalog: Value,
    sha256: String,
}

impl VerifiedKeylessCatalog {
    /// Returns the verified catalog document for subsequent schema admission.
    pub const fn catalog(&self) -> &Value {
        &self.catalog
    }

    /// Returns the SHA-256 digest of the exact verified bytes.
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

/// Verifies a catalog using the official repository, workflow, ref and source.
///
/// All input bytes are snapshotted before launching the verifier. The verifier
/// receives no inherited credentials, uses explicit offline roots and bundles,
/// and has a 30-second wall-clock deadline. Verification output supplied by a
/// caller is never accepted. Current online status must be checked separately.
pub fn verify_official_catalog(
    input: &KeylessCatalogVerification<'_>,
) -> anyhow::Result<VerifiedKeylessCatalog> {
    validate_hex(input.source_sha, 40, "reviewed source SHA")?;
    validate_hex(input.trusted_root_sha256, 64, "trusted root SHA-256")?;
    ensure!(input.gh.is_absolute(), "trusted gh path must be absolute");
    let gh = input
        .gh
        .canonicalize()
        .context("resolve trusted gh executable")?;
    ensure!(gh.is_file(), "trusted gh executable must be a regular file");
    let catalog = read_bounded(input.catalog, 4 * 1024 * 1024)?;
    let bundle = read_bounded(input.bundle, MAX_INPUT)?;
    let roots = read_bounded(input.trusted_root, MAX_INPUT)?;
    let document: Value = serde_json::from_slice(&catalog).context("parse catalog candidate")?;
    validate_catalog(&document)?;
    ensure!(
        digest(&roots) == input.trusted_root_sha256,
        "trusted root SHA-256 does not match its independent pin"
    );
    let scratch = tempfile::tempdir().context("create verifier scratch")?;
    let artifact = scratch.path().join("catalog.json");
    let bundle_path = scratch.path().join("bundle.jsonl");
    let roots_path = scratch.path().join("trusted-root.jsonl");
    fs::write(&artifact, &catalog)?;
    fs::write(&bundle_path, bundle)?;
    fs::write(&roots_path, roots)?;
    let output = scratch.path().join("verified.json");
    let errors = scratch.path().join("stderr.txt");
    let mut command =
        verification_command(&gh, &artifact, &bundle_path, &roots_path, input.source_sha);
    command
        .current_dir(scratch.path())
        .env_clear()
        .env("HOME", scratch.path())
        .env("TMPDIR", scratch.path())
        .env("GH_CONFIG_DIR", scratch.path())
        .env("GH_PROMPT_DISABLED", "1")
        .stdin(Stdio::null())
        .stdout(File::create(&output)?)
        .stderr(File::create(&errors)?);
    let mut child = command
        .spawn()
        .context("start trusted gh attestation verifier")?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if [&output, &errors]
            .iter()
            .any(|path| fs::metadata(path).is_ok_and(|metadata| metadata.len() > MAX_OUTPUT))
        {
            child.kill().context("stop oversized verifier output")?;
            child.wait().context("reap oversized verifier output")?;
            bail!("attestation verifier output exceeded byte limit");
        }
        if Instant::now() >= deadline {
            child
                .kill()
                .context("stop timed-out attestation verifier")?;
            child
                .wait()
                .context("reap timed-out attestation verifier")?;
            bail!("attestation verifier exceeded 30-second deadline");
        }
        thread::sleep(Duration::from_millis(25));
    };
    ensure!(
        status.success(),
        "official catalog attestation verification failed ({status})"
    );
    let verification: Value = serde_json::from_slice(&read_bounded(&output, MAX_OUTPUT)?)
        .context("parse trusted verifier output")?;
    ensure!(
        verification
            .as_array()
            .is_some_and(|results| !results.is_empty()),
        "trusted verifier returned no verified attestation"
    );
    Ok(VerifiedKeylessCatalog {
        catalog: document,
        sha256: digest(&catalog),
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    schema: String,
    catalog_id: String,
    revision: u64,
    issued_at: u64,
    releases: Vec<CatalogRelease>,
    statuses: Vec<CatalogStatus>,
    legacy_sources: BTreeMap<String, LegacySource>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogRelease {
    channel: String,
    plugin_id: String,
    version: String,
    record: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogStatus {
    plugin_id: String,
    version: String,
    state: String,
    reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacySource {
    schema: String,
    revision: u64,
    payload_digest: String,
}

fn validate_catalog(document: &Value) -> anyhow::Result<()> {
    let catalog: Catalog = serde_json::from_value(document.clone())?;
    ensure!(
        catalog.schema == "lenso.marketplace.keyless-catalog.v1",
        "unsupported catalog schema"
    );
    ensure!(
        catalog.catalog_id == "lenso-official-v2",
        "unexpected official catalog identity"
    );
    let safe_integer = 9_007_199_254_740_991;
    ensure!(
        catalog.revision > 0 && catalog.revision <= safe_integer,
        "invalid catalog revision"
    );
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    ensure!(
        catalog.issued_at > 0
            && catalog.issued_at <= safe_integer
            && catalog.issued_at <= now + 300,
        "invalid catalog issue time"
    );
    ensure!(
        !catalog.releases.is_empty()
            && catalog.releases.len() <= 10_000
            && catalog.statuses.len() <= 10_000,
        "too many catalog releases"
    );
    let mut identities = BTreeSet::new();
    for release in catalog.releases {
        ensure!(
            identities.insert((release.plugin_id.clone(), release.version.clone())),
            "duplicate release identity across catalog channels"
        );
        validate_record(release)?;
    }
    let mut statuses = BTreeSet::new();
    for status in catalog.statuses {
        ensure!(
            matches!(status.state.as_str(), "listed" | "yanked" | "revoked"),
            "invalid release state"
        );
        ensure!(
            status
                .reason
                .as_ref()
                .is_none_or(|reason| !reason.is_empty()
                    && reason.len() <= 1024
                    && !reason.contains('\0')),
            "invalid release state reason"
        );
        ensure!(
            status.state == "listed" || status.reason.is_some(),
            "withdrawn release needs a reason"
        );
        ensure!(
            statuses.insert((status.plugin_id, status.version)),
            "duplicate release status"
        );
    }
    ensure!(
        identities == statuses,
        "catalog statuses must cover exactly all release identities"
    );
    ensure!(
        catalog.legacy_sources.len() == 4,
        "catalog needs four legacy audit sources"
    );
    for (channel, source) in catalog.legacy_sources {
        let expected = match channel.as_str() {
            "portable" => "lenso.marketplace.snapshot.v1",
            "linked_cargo" => "lenso.marketplace.linked-cargo-snapshot.v1",
            "package" => "lenso.marketplace.package-snapshot.v1",
            "release_content" => "lenso.marketplace.release-content.v2",
            _ => bail!("unsupported legacy audit channel"),
        };
        ensure!(
            source.schema == expected && source.revision > 0 && source.revision <= safe_integer,
            "invalid legacy audit source"
        );
        let payload_digest = source
            .payload_digest
            .strip_prefix("sha256:")
            .context("legacy payload digest needs sha256 prefix")?;
        validate_hex(payload_digest, 64, "legacy payload digest")?;
    }
    Ok(())
}

fn validate_record(release: CatalogRelease) -> anyhow::Result<()> {
    use lenso_plugin_catalog::{
        Release, linked_cargo::LinkedCargoRelease, package::PackageRelease,
        release_content::ReleaseContent,
    };
    let (plugin, version) = match release.channel.as_str() {
        "portable" => {
            let record: Release = serde_json::from_value(release.record)?;
            record.validate()?;
            (record.plugin_id, record.version)
        }
        "linked_cargo" => {
            let record: LinkedCargoRelease = serde_json::from_value(release.record)?;
            record.validate()?;
            (record.plugin_id, record.version)
        }
        "package" => {
            let record: PackageRelease = serde_json::from_value(release.record)?;
            record.validate()?;
            (record.plugin_id, record.version)
        }
        "release_content" => {
            let record: ReleaseContent = serde_json::from_value(release.record)?;
            record.validate()?;
            (record.plugin_id, record.version)
        }
        _ => bail!("unsupported catalog channel"),
    };
    ensure!(
        plugin == release.plugin_id && version == release.version,
        "catalog record identity differs from outer identity"
    );
    Ok(())
}

fn verification_command(
    gh: &Path,
    artifact: &Path,
    bundle: &Path,
    roots: &Path,
    source: &str,
) -> Command {
    let mut command = Command::new(gh);
    command
        .args(["attestation", "verify"])
        .arg(artifact)
        .arg("--bundle")
        .arg(bundle)
        .arg("--custom-trusted-root")
        .arg(roots)
        .args([
            "--repo",
            REPOSITORY,
            "--hostname",
            "github.com",
            "--signer-repo",
            REPOSITORY,
            "--signer-workflow",
            WORKFLOW,
            "--source-ref",
            "refs/heads/main",
            "--source-digest",
            source,
            "--signer-digest",
            source,
            "--cert-identity",
            IDENTITY,
            "--cert-oidc-issuer",
            "https://token.actions.githubusercontent.com",
            "--predicate-type",
            "https://slsa.dev/provenance/v1",
            "--deny-self-hosted-runners",
            "--format",
            "json",
        ]);
    command
}

fn read_bounded(path: &Path, maximum: u64) -> anyhow::Result<Vec<u8>> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("inspect {}", path.display()))?;
    ensure!(
        metadata.is_file(),
        "verification input must be a regular file"
    );
    ensure!(
        metadata.len() <= maximum,
        "verification input exceeds {maximum} bytes"
    );
    let mut bytes = Vec::new();
    File::open(path)?
        .take(maximum + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= maximum,
        "verification input grew beyond byte limit"
    );
    Ok(bytes)
}

fn validate_hex(value: &str, length: usize, name: &str) -> anyhow::Result<()> {
    ensure!(
        value.len() == length
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "{name} must be {length} lowercase hexadecimal characters"
    );
    Ok(())
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_catalog() -> Value {
        let mut sources = serde_json::Map::new();
        for (channel, schema) in [
            ("portable", "lenso.marketplace.snapshot.v1"),
            ("linked_cargo", "lenso.marketplace.linked-cargo-snapshot.v1"),
            ("package", "lenso.marketplace.package-snapshot.v1"),
            ("release_content", "lenso.marketplace.release-content.v2"),
        ] {
            sources.insert(
                channel.into(),
                serde_json::json!({"schema":schema,"revision":1,"payload_digest":format!("sha256:{}", "a".repeat(64))}),
            );
        }
        serde_json::json!({
            "schema":"lenso.marketplace.keyless-catalog.v1",
            "catalog_id":"lenso-official-v2", "revision":1, "issued_at":1,
            "releases":[{"channel":"linked_cargo","plugin_id":"example.plugin","version":"1.0.0","record":{
                "plugin_id":"example.plugin","version":"1.0.0","publisher_id":"example",
                "title":"Example","summary":"Example linked build input","license":"MIT",
                "source_url":"https://github.com/example/plugin","source_revision":"b".repeat(40),
                "package":"example-plugin","registry_url":"https://crates.io",
                "crate_digest":format!("sha256:{}", "a".repeat(64)),"integration":"host_provided",
                "targets":["aarch64-unknown-linux-gnu"],"availability":"listed"
            }}],
            "statuses":[{"plugin_id":"example.plugin","version":"1.0.0","state":"listed"}],
            "legacy_sources":sources,
        })
    }

    #[test]
    fn catalog_with_complete_status_set_is_valid() {
        assert!(validate_catalog(&fixture_catalog()).is_ok());
    }

    #[test]
    fn status_for_unknown_release_is_rejected() {
        let mut catalog = fixture_catalog();
        catalog["statuses"] =
            serde_json::json!([{"plugin_id":"example.other","version":"1.0.0","state":"listed"}]);
        assert!(
            validate_catalog(&catalog)
                .unwrap_err()
                .to_string()
                .contains("cover exactly")
        );
    }

    #[test]
    fn forged_receipt_field_is_rejected() {
        let mut catalog = fixture_catalog();
        catalog["verified"] = Value::Bool(true);
        assert!(validate_catalog(&catalog).is_err());
    }

    #[test]
    fn duplicate_global_release_identity_is_rejected() {
        let mut catalog = fixture_catalog();
        let duplicate = catalog["releases"][0].clone();
        catalog["releases"].as_array_mut().unwrap().push(duplicate);
        assert!(
            validate_catalog(&catalog)
                .unwrap_err()
                .to_string()
                .contains("duplicate release")
        );
    }

    #[test]
    fn inner_record_identity_mismatch_is_rejected() {
        let mut catalog = fixture_catalog();
        catalog["releases"][0]["record"]["plugin_id"] = serde_json::json!("example.other");
        assert!(
            validate_catalog(&catalog)
                .unwrap_err()
                .to_string()
                .contains("differs from outer")
        );
    }

    #[test]
    fn revoked_release_without_a_reason_is_rejected() {
        let mut catalog = fixture_catalog();
        catalog["statuses"][0]["state"] = serde_json::json!("revoked");
        assert!(
            validate_catalog(&catalog)
                .unwrap_err()
                .to_string()
                .contains("needs a reason")
        );
    }

    #[test]
    fn catalog_revision_above_javascript_safe_integer_is_rejected() {
        let mut catalog = fixture_catalog();
        catalog["revision"] = serde_json::json!(9_007_199_254_740_992_u64);
        assert!(validate_catalog(&catalog).is_err());
    }

    #[test]
    fn verifier_arguments_pin_official_identity_without_a_shell() {
        let command = verification_command(
            Path::new("/trusted/gh"),
            Path::new("/input/catalog"),
            Path::new("/input/bundle"),
            Path::new("/trusted/roots"),
            &"a".repeat(40),
        );
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy())
            .collect();
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--cert-identity", IDENTITY])
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--source-ref", "refs/heads/main"])
        );
        assert!(args.contains(&"--deny-self-hosted-runners".into()));
        assert!(!args.contains(&"--owner".into()));
    }

    #[test]
    fn source_pin_rejects_flag_injection_and_noncanonical_hashes() {
        for invalid in ["--repo", "", &"A".repeat(40), &"a".repeat(39)] {
            assert!(validate_hex(invalid, 40, "source").is_err());
        }
    }

    #[test]
    fn oversized_input_is_rejected_before_verifier_execution() {
        let file = tempfile::NamedTempFile::new().unwrap();
        file.as_file().set_len(MAX_INPUT + 1).unwrap();
        assert!(read_bounded(file.path(), MAX_INPUT).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_input_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("file");
        fs::write(&file, b"input").unwrap();
        let link = root.path().join("link");
        std::os::unix::fs::symlink(file, &link).unwrap();
        assert!(read_bounded(&link, MAX_INPUT).is_err());
    }
}
