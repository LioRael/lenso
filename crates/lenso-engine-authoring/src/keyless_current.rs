//! Canonical online status joined to verified bytes and durable consumer history.

use std::{
    fs,
    io::{Read as _, Write as _},
    path::Path,
};

use anyhow::{Context as _, ensure};
use lenso_plugin_catalog::{
    Release, linked_cargo::LinkedCargoRelease, package::PackageRelease,
    release_content::ReleaseContent,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::keyless_catalog::{VerifiedKeylessCatalog, digest, validate_catalog};

const ORIGIN: &str = "https://marketplace.lenso.dev";
const CURRENT: &str = "/api/marketplace/v3/current";
const MAX_OBJECT: u64 = 4 * 1024 * 1024;
const CHECKPOINT: &str = "official-keyless-history.json";

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Object {
    path: String,
    sha256: String,
    size: u64,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Provenance {
    repository: String,
    workflow: String,
    r#ref: String,
    source_sha: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Head {
    schema: String,
    catalog_id: String,
    revision: u64,
    catalog: Object,
    bundle: Object,
    provenance: Provenance,
}

impl Head {
    fn parse(bytes: &[u8]) -> anyhow::Result<Self> {
        let head: Self = serde_json::from_slice(bytes)?;
        ensure!(
            head.schema == "lenso.marketplace.keyless-current.v1"
                && head.catalog_id == "lenso-official-v2"
                && head.revision > 0
                && head.revision <= 9_007_199_254_740_991,
            "invalid official current identity or revision"
        );
        ensure!(
            head.provenance.repository == "LioRael/lenso-marketplace"
                && head.provenance.workflow == ".github/workflows/publish-keyless-catalog.yml"
                && head.provenance.r#ref == "refs/heads/main"
                && lower_hex(&head.provenance.source_sha, 40),
            "invalid official current provenance"
        );
        for object in [&head.catalog, &head.bundle] {
            ensure!(
                lower_hex(&object.sha256, 64)
                    && object.size > 0
                    && object.size <= MAX_OBJECT
                    && object.path == format!("/api/marketplace/v3/objects/{}.json", object.sha256),
                "invalid official current object descriptor"
            );
        }
        Ok(head)
    }
}

fn lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn fetch(path: &str, limit: u64) -> anyhow::Result<Vec<u8>> {
    let agent = crate::archive_download::restricted_https_agent_builder()
        .resolver(crate::archive_download::public_resolve)
        .build();
    let response = agent
        .get(&format!("{ORIGIN}{path}"))
        .set("Accept-Encoding", "identity")
        .set("Cache-Control", "no-cache")
        .call()
        .map_err(|_| anyhow::anyhow!("official Marketplace HTTPS status is unavailable"))?;
    ensure!(
        response.status() == 200
            && response
                .header("Content-Encoding")
                .is_none_or(|value| value.eq_ignore_ascii_case("identity")),
        "official Marketplace returned an unsupported response"
    );
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        !bytes.is_empty() && bytes.len() as u64 <= limit,
        "official Marketplace response exceeds bounds"
    );
    Ok(bytes)
}

fn fetch_object(object: &Object) -> anyhow::Result<Vec<u8>> {
    let bytes = fetch(&object.path, object.size)?;
    ensure!(
        bytes.len() as u64 == object.size && digest(&bytes) == object.sha256,
        "official Marketplace object does not match current digest and size"
    );
    Ok(bytes)
}

/// Bounded archive transport admitted by Lenso policy, not by catalog URLs.
pub fn download_official_artifact(input: &str, limit: u64) -> anyhow::Result<Vec<u8>> {
    let mut url = crate::archive_download::checked_url(input, "official artifact")?;
    ensure!(
        artifact_url_allowed(&url, false) && limit > 0 && limit <= 256 * 1024 * 1024,
        "artifact origin or size is not admitted by Lenso"
    );
    let github_release = url.host_str() == Some("github.com");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    for _ in 0..=3 {
        ensure!(
            artifact_url_allowed(&url, github_release),
            "artifact redirect is not admitted by Lenso"
        );
        let remaining = deadline
            .checked_duration_since(std::time::Instant::now())
            .context("artifact download deadline exceeded")?;
        let agent = crate::archive_download::restricted_https_agent_builder()
            .timeout(remaining)
            .resolver(crate::archive_download::public_resolve)
            .build();
        let response = agent
            .get(url.as_str())
            .set("Accept-Encoding", "identity")
            .call()
            .map_err(|_| anyhow::anyhow!("official artifact HTTPS download failed"))?;
        if (300..400).contains(&response.status()) {
            ensure!(
                github_release,
                "registry and Marketplace artifact redirects are not admitted"
            );
            url = url.join(
                response
                    .header("Location")
                    .context("artifact redirect lacks Location")?,
            )?;
            continue;
        }
        ensure!(
            response.status() == 200
                && response
                    .header("Content-Encoding")
                    .is_none_or(|value| value.eq_ignore_ascii_case("identity")),
            "official artifact returned an unsupported response"
        );
        let mut bytes = Vec::new();
        let mut reader = response.into_reader();
        let mut chunk = [0_u8; 16 * 1024];
        loop {
            ensure!(
                std::time::Instant::now() < deadline,
                "artifact download deadline exceeded"
            );
            let count = reader.read(&mut chunk)?;
            ensure!(
                std::time::Instant::now() < deadline,
                "artifact download deadline exceeded"
            );
            if count == 0 {
                break;
            }
            ensure!(
                (bytes.len() + count) as u64 <= limit,
                "official artifact archive exceeds bounds"
            );
            bytes.extend_from_slice(&chunk[..count]);
        }
        ensure!(
            !bytes.is_empty() && bytes.len() as u64 <= limit,
            "official artifact archive exceeds bounds"
        );
        return Ok(bytes);
    }
    anyhow::bail!("official artifact exceeded redirect limit")
}

fn artifact_url_allowed(url: &url::Url, github_redirect: bool) -> bool {
    url.scheme() == "https"
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
        && match url.host_str() {
            Some("marketplace.lenso.dev" | "static.crates.io" | "registry.npmjs.org") => {
                url.query().is_none()
            }
            Some("github.com") => {
                url.query().is_none()
                    && url
                        .path()
                        .starts_with("/LioRael/lenso-marketplace/releases/download/")
            }
            Some("release-assets.githubusercontent.com") => {
                github_redirect && url.path().starts_with("/github-production-release-asset/")
            }
            _ => false,
        }
}

pub fn cargo_artifact_url(release: &LinkedCargoRelease) -> anyhow::Result<String> {
    let registry = crate::archive_download::checked_url(&release.registry_url, "Cargo registry")?;
    ensure!(
        registry.origin().ascii_serialization() == "https://crates.io"
            && registry.query().is_none()
            && (registry.path() == "/"
                || registry.path() == format!("/crates/{}/{}", release.package, release.version)),
        "Cargo registry must name the exact official package and version"
    );
    Ok(format!(
        "https://static.crates.io/crates/{0}/{0}-{1}.crate",
        release.package, release.version
    ))
}

pub fn npm_artifact_url(
    distribution: &lenso_plugin_catalog::Distribution,
) -> anyhow::Result<String> {
    let registry = crate::archive_download::checked_url(
        distribution
            .registry_url
            .as_deref()
            .context("npm registry URL required")?,
        "npm registry",
    )?;
    let name = distribution
        .package
        .rsplit('/')
        .next()
        .context("npm package name")?;
    let path = format!(
        "/{}/-/{}-{}.tgz",
        distribution.package, name, distribution.version
    );
    ensure!(
        registry.origin().ascii_serialization() == "https://registry.npmjs.org"
            && registry.query().is_none()
            && (registry.path() == "/" || registry.path() == path),
        "npm registry must name the exact official package and version"
    );
    Ok(format!("https://registry.npmjs.org{path}"))
}

/// A listed exact release admitted through authenticated current status.
/// Construction is private; decoded JSON or inspector output is not admission.
#[derive(Debug)]
pub struct AdmittedRelease {
    record: ReleaseRecord,
    catalog_id: String,
}

#[derive(Debug)]
pub enum ReleaseRecord {
    Portable(Release),
    LinkedCargo(LinkedCargoRelease),
    Package(PackageRelease),
    Content(ReleaseContent),
}

impl AdmittedRelease {
    pub const fn record(&self) -> &ReleaseRecord {
        &self.record
    }
    pub fn catalog_id(&self) -> &str {
        &self.catalog_id
    }
}

/// Holds the independent history-directory lock throughout App acquisition.
pub struct CurrentAdmission {
    catalog: Value,
    head: Head,
    _history_lock: fs::File,
}

impl CurrentAdmission {
    /// Rechecks canonical current status after preparation and immediately
    /// before publishing App-owned selection or source files.
    pub fn before_commit<T>(
        &self,
        publish: impl FnOnce() -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        checked_commit(|| self.recheck(), publish)
    }
    pub fn recheck(&self) -> anyhow::Result<()> {
        ensure!(
            Head::parse(&fetch(CURRENT, 16 * 1024)?)? == self.head,
            "official current status changed during acquisition; retry"
        );
        Ok(())
    }
    pub fn select(&self, plugin_id: &str, version: &str) -> anyhow::Result<AdmittedRelease> {
        crate::identity::validate_plugin_id_v1(plugin_id)?;
        crate::identity::validate_release_version(version)?;
        let status = rows(&self.catalog, "statuses")?
            .iter()
            .find(|row| identity(row) == (plugin_id, version))
            .context("exact release is not in the current official catalog")?;
        ensure!(
            status["state"] == "listed",
            "exact release is withdrawn from the current official catalog"
        );
        let release = rows(&self.catalog, "releases")?
            .iter()
            .find(|row| identity(row) == (plugin_id, version))
            .context("exact current release record is missing")?;
        let record = release["record"].clone();
        let record = match release["channel"].as_str() {
            Some("portable") => ReleaseRecord::Portable(serde_json::from_value(record)?),
            Some("linked_cargo") => ReleaseRecord::LinkedCargo(serde_json::from_value(record)?),
            Some("package") => ReleaseRecord::Package(serde_json::from_value(record)?),
            Some("release_content") => ReleaseRecord::Content(serde_json::from_value(record)?),
            _ => anyhow::bail!("unsupported admitted release channel"),
        };
        Ok(AdmittedRelease {
            record,
            catalog_id: "lenso-official-v2".into(),
        })
    }
}

fn checked_commit<T>(
    check: impl FnOnce() -> anyhow::Result<()>,
    publish: impl FnOnce() -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    check()?;
    publish()
}

fn rows<'a>(catalog: &'a Value, name: &str) -> anyhow::Result<&'a Vec<Value>> {
    catalog[name]
        .as_array()
        .context("invalid admitted catalog rows")
}

fn identity(row: &Value) -> (&str, &str) {
    (
        row["plugin_id"].as_str().unwrap_or_default(),
        row["version"].as_str().unwrap_or_default(),
    )
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct History {
    schema: String,
    catalog_sha256: String,
    catalog: Value,
}

fn compare_history(previous: &History, next: &Value, next_digest: &str) -> anyhow::Result<()> {
    ensure!(
        previous.schema == "lenso.marketplace.consumer-history.v1"
            && lower_hex(&previous.catalog_sha256, 64),
        "invalid consumer history; refusing to forget accepted releases"
    );
    validate_catalog(&previous.catalog)?;
    let prior = previous.catalog["revision"]
        .as_u64()
        .context("invalid stored revision")?;
    let revision = next["revision"]
        .as_u64()
        .context("invalid current revision")?;
    ensure!(revision >= prior, "official catalog revision rollback");
    if revision == prior {
        ensure!(
            next_digest == previous.catalog_sha256 && next == &previous.catalog,
            "official catalog equivocation at an accepted revision"
        );
    }
    for old in rows(&previous.catalog, "releases")? {
        let current = rows(next, "releases")?
            .iter()
            .find(|row| identity(row) == identity(old))
            .context("official catalog dropped an accepted release")?;
        ensure!(
            current == old,
            "official catalog rewrote an immutable release"
        );
    }
    for old in rows(&previous.catalog, "statuses")?
        .iter()
        .filter(|row| row["state"] == "revoked")
    {
        let current = rows(next, "statuses")?
            .iter()
            .find(|row| identity(row) == identity(old))
            .context("official catalog dropped a terminal revocation")?;
        ensure!(
            current["state"] == "revoked",
            "official catalog resurrected a revoked release"
        );
    }
    Ok(())
}

/// Fetches only the canonical endpoint; historical bundles alone cannot admit.
/// `state_dir` belongs to the CLI installation, independently of any App root.
/// The verifier checks the head-selected source SHA against certificate claims.
pub fn admit_current(
    state_dir: &Path,
    verifier: impl FnOnce(&Path, &Path, &str) -> anyhow::Result<VerifiedKeylessCatalog>,
) -> anyhow::Result<CurrentAdmission> {
    let head = Head::parse(&fetch(CURRENT, 16 * 1024)?)?;
    let catalog = fetch_object(&head.catalog)?;
    let bundle = fetch_object(&head.bundle)?;
    let scratch = tempfile::tempdir()?;
    let catalog_path = scratch.path().join("catalog.json");
    let bundle_path = scratch.path().join("bundle.jsonl");
    fs::write(&catalog_path, catalog)?;
    fs::write(&bundle_path, bundle)?;
    let verified = verifier(&catalog_path, &bundle_path, &head.provenance.source_sha)?;
    ensure!(
        verified.sha256() == head.catalog.sha256
            && verified.catalog()["revision"].as_u64() == Some(head.revision),
        "verified catalog does not match current status"
    );
    ensure!(
        Head::parse(&fetch(CURRENT, 16 * 1024)?)? == head,
        "official current status changed during verification; retry"
    );
    let lock = open_history_dir(state_dir)?;
    lock.try_lock().context(
        "consumer history is busy; retry after the other Marketplace operation completes",
    )?;
    let previous = read_history(&lock)?;
    if let Some(previous) = previous.as_ref() {
        compare_history(previous, verified.catalog(), verified.sha256())?;
    }
    persist_history(
        &lock,
        &History {
            schema: "lenso.marketplace.consumer-history.v1".into(),
            catalog_sha256: verified.sha256().into(),
            catalog: verified.catalog().clone(),
        },
    )?;
    Ok(CurrentAdmission {
        catalog: verified.catalog().clone(),
        head,
        _history_lock: lock,
    })
}

#[cfg(unix)]
fn open_history_dir(path: &Path) -> anyhow::Result<fs::File> {
    use rustix::{
        fs::{Mode, OFlags, mkdirat, open, openat},
        io::Errno,
    };
    use std::os::unix::fs::MetadataExt as _;
    use std::path::Component;
    ensure!(
        path.is_absolute(),
        "consumer history directory must be absolute"
    );
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mut dir = fs::File::from(open("/", flags, Mode::empty())?);
    for component in path.components() {
        match component {
            Component::RootDir => continue,
            Component::Normal(name) => {
                let fd = match openat(&dir, name, flags, Mode::empty()) {
                    Ok(fd) => fd,
                    Err(Errno::NOENT) => {
                        match mkdirat(&dir, name, Mode::RUSR | Mode::WUSR | Mode::XUSR) {
                            Ok(()) => dir.sync_all()?,
                            Err(Errno::EXIST) => {}
                            Err(error) => {
                                return Err(error)
                                    .context("create independent consumer history directory");
                            }
                        }
                        openat(&dir, name, flags, Mode::empty())?
                    }
                    Err(error) => {
                        return Err(error)
                            .context("open non-symlink independent consumer history directory");
                    }
                };
                dir = fs::File::from(fd);
            }
            _ => anyhow::bail!("consumer history directory cannot contain traversal"),
        }
    }
    let metadata = dir.metadata()?;
    ensure!(
        metadata.uid() == rustix::process::getuid().as_raw() && metadata.mode() & 0o022 == 0,
        "consumer history directory must be owned by this user and not group/world writable"
    );
    Ok(dir)
}

#[cfg(unix)]
fn read_history(dir: &fs::File) -> anyhow::Result<Option<History>> {
    use rustix::{
        fs::{Mode, OFlags, openat},
        io::Errno,
    };
    use std::os::unix::fs::MetadataExt as _;
    let fd = match openat(
        dir,
        CHECKPOINT,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(Errno::NOENT) => return Ok(None),
        Err(error) => return Err(error).context("open independent consumer history"),
    };
    let mut file = fs::File::from(fd);
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file()
            && metadata.nlink() == 1
            && metadata.uid() == rustix::process::getuid().as_raw()
            && metadata.mode() & 0o022 == 0,
        "consumer history must be a user-owned non-writable single-link regular file"
    );
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(MAX_OBJECT + 64 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_OBJECT + 64 * 1024,
        "consumer history exceeds bounds"
    );
    Ok(Some(serde_json::from_slice(&bytes).context(
        "invalid consumer history; refusing to forget accepted history",
    )?))
}

#[cfg(unix)]
fn persist_history(dir: &fs::File, history: &History) -> anyhow::Result<()> {
    use rustix::fs::{AtFlags, Mode, OFlags, openat, renameat, unlinkat};
    let name = format!(".keyless-history-{}.tmp", uuid::Uuid::now_v7());
    let fd = openat(
        dir,
        name.as_str(),
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::RUSR | Mode::WUSR,
    )?;
    let mut file = fs::File::from(fd);
    let result = (|| {
        file.write_all(&serde_json::to_vec(history)?)?;
        file.sync_all()?;
        renameat(dir, name.as_str(), dir, CHECKPOINT)?;
        dir.sync_all()?;
        anyhow::Ok(())
    })();
    if result.is_err() {
        let _ = unlinkat(dir, name.as_str(), AtFlags::empty());
    }
    result
}

#[cfg(not(unix))]
fn open_history_dir(_path: &Path) -> anyhow::Result<fs::File> {
    anyhow::bail!("keyless adoption requires supported durable history storage")
}
#[cfg(not(unix))]
fn read_history(_dir: &fs::File) -> anyhow::Result<Option<History>> {
    anyhow::bail!("keyless adoption requires supported durable history storage")
}
#[cfg(not(unix))]
fn persist_history(_dir: &fs::File, _history: &History) -> anyhow::Result<()> {
    anyhow::bail!("keyless adoption requires supported durable history storage")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history() -> History {
        History {
            schema: "lenso.marketplace.consumer-history.v1".into(),
            catalog_sha256: "a".repeat(64),
            catalog: crate::keyless_catalog::tests::fixture_catalog(),
        }
    }

    #[test]
    fn changed_current_after_preparation_never_commits_selection() {
        let published = std::cell::Cell::new(false);
        let result = checked_commit(
            || anyhow::bail!("current revision changed"),
            || {
                published.set(true);
                Ok(())
            },
        );
        assert!(result.is_err());
        assert!(!published.get());
    }

    #[test]
    fn checkpoint_rejects_rollback_equivocation_and_release_rewrite() {
        let old = history();
        let mut next = old.catalog.clone();
        next["revision"] = 0.into();
        assert!(compare_history(&old, &next, &"a".repeat(64)).is_err());
        next["revision"] = 1.into();
        assert!(compare_history(&old, &next, &"b".repeat(64)).is_err());
        next["revision"] = 2.into();
        next["releases"][0]["record"]["title"] = "Changed".into();
        assert!(compare_history(&old, &next, &"b".repeat(64)).is_err());
    }

    #[test]
    fn checkpoint_rejects_missing_history_and_terminal_revocation_reversal() {
        let mut old = history();
        old.catalog["statuses"][0]["state"] = "revoked".into();
        old.catalog["statuses"][0]["reason"] = "Security".into();
        let mut next = old.catalog.clone();
        next["revision"] = 2.into();
        next["statuses"][0]["state"] = "listed".into();
        assert!(compare_history(&old, &next, &"b".repeat(64)).is_err());
        next["statuses"][0]["state"] = "revoked".into();
        next["releases"] = serde_json::json!([]);
        assert!(compare_history(&old, &next, &"b".repeat(64)).is_err());
    }

    #[test]
    fn checkpoint_accepts_exact_retry_and_monotonic_unchanged_records() {
        let old = history();
        assert!(compare_history(&old, &old.catalog, &old.catalog_sha256).is_ok());
        let mut next = old.catalog.clone();
        next["revision"] = 2.into();
        assert!(compare_history(&old, &next, &"b".repeat(64)).is_ok());
    }

    #[test]
    fn current_descriptors_cannot_redirect_or_change_official_identity() {
        let mut value = serde_json::json!({
            "schema":"lenso.marketplace.keyless-current.v1", "catalog_id":"lenso-official-v2", "revision":1,
            "catalog":{"path":format!("/api/marketplace/v3/objects/{}.json", "a".repeat(64)),"sha256":"a".repeat(64),"size":1},
            "bundle":{"path":format!("/api/marketplace/v3/objects/{}.json", "b".repeat(64)),"sha256":"b".repeat(64),"size":1},
            "provenance":{"repository":"LioRael/lenso-marketplace","workflow":".github/workflows/publish-keyless-catalog.yml","ref":"refs/heads/main","source_sha":"c".repeat(40)}
        });
        assert!(Head::parse(&serde_json::to_vec(&value).unwrap()).is_ok());
        value["catalog"]["path"] = "https://evil.example/catalog".into();
        assert!(Head::parse(&serde_json::to_vec(&value).unwrap()).is_err());
    }

    #[test]
    fn artifact_transport_denies_unapproved_origins_before_network() {
        assert!(download_official_artifact("https://example.com/evil", 1).is_err());
        assert!(download_official_artifact("https://127.0.0.1/evil", 1).is_err());
    }

    #[test]
    fn six_published_records_keep_their_real_registry_and_artifact_urls() {
        let fixture = crate::keyless_catalog::tests::fixture_catalog();
        let mut cargo: LinkedCargoRelease =
            serde_json::from_value(fixture["releases"][0]["record"].clone()).unwrap();
        for (package, version, registry) in [
            (
                "lenso-secrets-env-plugin",
                "0.1.7",
                "https://crates.io/crates/lenso-secrets-env-plugin/0.1.7",
            ),
            (
                "lenso-web-ingress-plugin",
                "0.4.9",
                "https://crates.io/crates/lenso-web-ingress-plugin/0.4.9",
            ),
        ] {
            cargo.package = package.into();
            cargo.version = version.into();
            cargo.registry_url = registry.into();
            assert_eq!(
                cargo_artifact_url(&cargo).unwrap(),
                format!("https://static.crates.io/crates/{package}/{package}-{version}.crate")
            );
        }
        let npm = lenso_plugin_catalog::Distribution {
            id: "bun".into(),
            kind: lenso_plugin_catalog::DistributionKind::NpmPackage,
            package: "@lenso/knowledge-excerpt".into(),
            version: "0.1.2".into(),
            integrity: Some(format!("sha256:{}", "a".repeat(64))),
            registry_url: Some(
                "https://registry.npmjs.org/@lenso/knowledge-excerpt/-/knowledge-excerpt-0.1.2.tgz"
                    .into(),
            ),
            artifact: None,
            targets: vec![],
        };
        assert_eq!(
            npm_artifact_url(&npm).unwrap(),
            npm.registry_url.as_ref().unwrap().as_str()
        );
        for artifact in [
            "https://github.com/LioRael/lenso-marketplace/releases/download/marketplace-echo-v0.1.3/echo-0.1.3.lenso-plugin",
            "https://marketplace.lenso.dev/artifacts/5ba597240979579cc5d054522cf672f3e398d2cf61c792c1713eec723eaaccd2.tar.gz",
            "https://marketplace.lenso.dev/artifacts/6361f17ff634a1d132c52871b546666bf7b043bc5e27d8943bff9543f3c18170.tar.gz",
        ] {
            assert!(artifact_url_allowed(
                &url::Url::parse(artifact).unwrap(),
                false
            ));
        }
        let redirected = url::Url::parse("https://release-assets.githubusercontent.com/github-production-release-asset/123/file?signature=example").unwrap();
        assert!(artifact_url_allowed(&redirected, true));
        assert!(!artifact_url_allowed(&redirected, false));
        assert!(!artifact_url_allowed(
            &url::Url::parse("https://github.com/evil/repo/releases/download/v1/file").unwrap(),
            true
        ));
    }

    #[cfg(unix)]
    #[test]
    fn independent_history_is_durable_and_corruption_is_fail_closed() {
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().canonicalize().unwrap().join("history");
        let dir = open_history_dir(&path).unwrap();
        dir.try_lock().unwrap();
        persist_history(&dir, &history()).unwrap();
        assert_eq!(
            read_history(&dir).unwrap().unwrap().catalog,
            history().catalog
        );
        let second = open_history_dir(&path).unwrap();
        assert!(second.try_lock().is_err());
        fs::write(path.join(CHECKPOINT), b"not JSON").unwrap();
        assert!(read_history(&dir).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn history_directory_rejects_symlinks_and_shared_write_access() {
        use std::os::unix::fs::{PermissionsExt as _, symlink};
        let scratch = tempfile::tempdir().unwrap();
        let parent = scratch.path().canonicalize().unwrap();
        let target = parent.join("target");
        fs::create_dir(&target).unwrap();
        let link = parent.join("linked");
        symlink(&target, &link).unwrap();
        assert!(open_history_dir(&link).is_err());
        fs::set_permissions(&target, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(open_history_dir(&target).is_err());
    }
}
