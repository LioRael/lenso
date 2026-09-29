//! Managed official GitHub verifier provisioning, independent of App inputs.

use std::{
    fs::{self, File},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::{Context, bail, ensure};
use url::Url;

use crate::keyless_catalog::{VerifiedKeylessCatalog, digest, verify_catalog_with_managed_tool};

const VERSION: &str = "2.101.0";
const MAX_BINARY: u64 = 96 * 1024 * 1024;
const MAX_ARCHIVE: u64 = 20 * 1024 * 1024;

struct Asset<'a> {
    name: &'a str,
    archive_sha256: &'a str,
    executable_sha256: &'a str,
    leaf: &'a str,
}

const MAC_ARM: Asset<'static> = Asset {
    name: "gh_2.101.0_macOS_arm64.zip",
    archive_sha256: "e4303e39d8f07141c4bad4b99b01079f05029c59b27076e8fbc825c985ecdd8b",
    executable_sha256: "aa97dfb4a82f7c56063cdcbfaa39a738b923c1bdca29bd5a041b8e959ca38e04",
    leaf: "gh_2.101.0_macOS_arm64/bin/gh",
};
const MAC_X64: Asset<'static> = Asset {
    name: "gh_2.101.0_macOS_amd64.zip",
    archive_sha256: "a6fd66c88e2f07d6e4e058173db341d07dd74d58cf8f19ae668293d2bb614ca3",
    executable_sha256: "58e034ed3be2e75def74c15870b6179deee89903437b90b0ba4a829b5d186a79",
    leaf: "gh_2.101.0_macOS_amd64/bin/gh",
};
const LINUX_ARM: Asset<'static> = Asset {
    name: "gh_2.101.0_linux_arm64.tar.gz",
    archive_sha256: "b57e8063f18862647c9d22727c32e9da1b963f8bf9db648fe123a6975695640f",
    executable_sha256: "76f657388c2270fb5049305afe683b741fb7c50fba0e15b09338189123f66557",
    leaf: "gh_2.101.0_linux_arm64/bin/gh",
};
const LINUX_X64: Asset<'static> = Asset {
    name: "gh_2.101.0_linux_amd64.tar.gz",
    archive_sha256: "9bca2d1c16825f109907a23307628a2f0698fbf99662b73a5cf0b020293072b8",
    executable_sha256: "ea857a3f0f7d4276cf5848b236542c5048e2eaa7bdd1b6ddec238f8793e74bff",
    leaf: "gh_2.101.0_linux_amd64/bin/gh",
};

/// Returns the user-owned private state directory, never a Plugin Root path.
///
/// # Errors
/// Rejects unsupported targets and symlinked, foreign-owned or permissive state.
pub fn managed_state_directory() -> anyhow::Result<PathBuf> {
    platform_asset(std::env::consts::OS, std::env::consts::ARCH)?;
    let scratch = tempfile::tempdir().context("check managed verifier ownership")?;
    managed_cache(scratch.path())
}

/// Verifies downloaded catalog bytes with an automatically managed official tool.
///
/// `source_sha` must come from the canonical current-head transport, not an App
/// manifest. Tool hashes and publisher identity are compiled policy. First use
/// downloads the pinned official release; public roots are maintained by the
/// verifier's embedded TUF trust anchors. No GitHub login or private key is used.
///
/// # Errors
/// Rejects unsupported targets, insecure caches, changed tool bytes, unavailable
/// downloads or roots, and any catalog failing cryptographic publisher policy.
pub fn verify_managed_catalog(
    catalog: &Path,
    bundle: &Path,
    source_sha: &str,
) -> anyhow::Result<VerifiedKeylessCatalog> {
    ensure!(
        source_sha.len() == 40
            && source_sha
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "official source SHA must be 40 lowercase hexadecimal characters"
    );
    let asset = platform_asset(std::env::consts::OS, std::env::consts::ARCH)?;
    let scratch = tempfile::tempdir().context("create managed verifier snapshot")?;
    let cache = managed_cache(scratch.path())?;
    let tool_cache = cache.join(asset.name);
    private_directory(&tool_cache, scratch.path())?;
    let executable = ensure_cached_tool(&tool_cache, asset, download)?;
    // The process executes a private snapshot, never a mutable cached pathname.
    let bytes = read_executable(&executable, asset)?;
    let gh = scratch.path().join("gh");
    write_executable(&gh, &bytes)?;
    let roots_cache = cache.join("tuf");
    private_directory(&roots_cache, scratch.path())?;
    verify_catalog_with_managed_tool(catalog, bundle, source_sha, &gh, &roots_cache)
}

fn platform_asset(os: &str, arch: &str) -> anyhow::Result<&'static Asset<'static>> {
    match (os, arch) {
        ("macos", "aarch64") => Ok(&MAC_ARM),
        ("macos", "x86_64") => Ok(&MAC_X64),
        ("linux", "aarch64") if cfg!(target_env = "gnu") => Ok(&LINUX_ARM),
        ("linux", "x86_64") if cfg!(target_env = "gnu") => Ok(&LINUX_X64),
        _ => bail!(
            "managed Marketplace verification supports macOS ARM64/x64 and Linux GNU ARM64/x64; unsupported target {os}/{arch}"
        ),
    }
}

fn managed_cache(scratch: &Path) -> anyhow::Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME is required for managed verifier cache")?;
    let home = PathBuf::from(home);
    ensure!(home.is_absolute(), "managed verifier HOME must be absolute");
    let home = home
        .canonicalize()
        .context("resolve managed verifier home")?;
    let cache = home.join(".lenso-keyless-cache-v1");
    private_directory(&cache, scratch)?;
    Ok(cache)
}

#[cfg(unix)]
fn private_directory(path: &Path, scratch: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::{DirBuilderExt as _, MetadataExt as _};
    match fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error).context("create private verifier cache"),
    }
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_dir(),
        "managed verifier cache must not be a symlink"
    );
    ensure!(
        metadata.uid() == fs::metadata(scratch)?.uid() && metadata.mode() & 0o077 == 0,
        "managed verifier cache must be owned by the current user with mode 0700"
    );
    Ok(())
}

#[cfg(not(unix))]
fn private_directory(_path: &Path, _scratch: &Path) -> anyhow::Result<()> {
    bail!("managed verifier private cache is unsupported on this target")
}

fn ensure_cached_tool(
    cache: &Path,
    asset: &Asset<'_>,
    fetch: impl FnOnce(&Asset<'_>) -> anyhow::Result<Vec<u8>>,
) -> anyhow::Result<PathBuf> {
    let executable = cache.join("gh");
    match fs::symlink_metadata(&executable) {
        Ok(_) => {
            read_executable(&executable, asset)?;
            return Ok(executable);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("inspect managed verifier cache"),
    }
    let archive = fetch(asset)?;
    ensure!(
        archive.len() as u64 <= MAX_ARCHIVE,
        "verifier archive exceeded byte limit"
    );
    ensure!(
        digest(&archive) == asset.archive_sha256,
        "official verifier archive digest mismatch"
    );
    let bytes = extract_executable(&archive, asset)?;
    ensure!(
        digest(&bytes) == asset.executable_sha256,
        "official verifier executable digest mismatch"
    );
    let staging = tempfile::tempdir_in(cache)?;
    let staged = staging.path().join("gh");
    write_executable(&staged, &bytes)?;
    // Hard-link publication is atomic and create-only; concurrent first use cannot overwrite.
    match fs::hard_link(&staged, &executable) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            read_executable(&executable, asset)?;
        }
        Err(error) => return Err(error).context("publish managed verifier cache"),
    }
    Ok(executable)
}

fn download(asset: &Asset<'_>) -> anyhow::Result<Vec<u8>> {
    let mut url = Url::parse(&format!(
        "https://github.com/cli/cli/releases/download/v{VERSION}/{}",
        asset.name
    ))?;
    let deadline = Instant::now() + Duration::from_secs(60);
    for _ in 0..=3 {
        ensure!(
            allowed_download_url(&url),
            "untrusted official verifier redirect"
        );
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .context("verifier download deadline exceeded")?;
        let agent = ureq::AgentBuilder::new()
            .redirects(0)
            .timeout(remaining)
            .build();
        let response = agent
            .get(url.as_str())
            .set("Accept-Encoding", "identity")
            .call()
            .context("download fixed official verifier")?;
        if (300..400).contains(&response.status()) {
            url = url.join(
                response
                    .header("Location")
                    .context("verifier redirect missing location")?,
            )?;
            continue;
        }
        ensure!(
            response.status() == 200,
            "official verifier download did not return 200"
        );
        if let Some(length) = response.header("Content-Length") {
            ensure!(
                length.parse::<u64>()? <= MAX_ARCHIVE,
                "verifier download exceeds byte limit"
            );
        }
        let mut bytes = Vec::new();
        let mut reader = response.into_reader();
        let mut chunk = [0_u8; 16 * 1024];
        loop {
            ensure!(
                Instant::now() < deadline,
                "verifier download deadline exceeded"
            );
            let count = reader.read(&mut chunk)?;
            ensure!(
                Instant::now() < deadline,
                "verifier download deadline exceeded"
            );
            if count == 0 {
                break;
            }
            ensure!(
                (bytes.len() + count) as u64 <= MAX_ARCHIVE,
                "verifier download exceeded byte limit"
            );
            bytes.extend_from_slice(&chunk[..count]);
        }
        ensure!(
            bytes.len() as u64 <= MAX_ARCHIVE,
            "verifier download exceeded byte limit"
        );
        return Ok(bytes);
    }
    bail!("official verifier download exceeded redirect limit")
}

fn allowed_download_url(url: &Url) -> bool {
    url.scheme() == "https"
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
        && match url.host_str() {
            Some("github.com") => url
                .path()
                .starts_with("/cli/cli/releases/download/v2.101.0/"),
            Some("release-assets.githubusercontent.com") => {
                url.path().starts_with("/github-production-release-asset/")
            }
            _ => false,
        }
}

fn extract_executable(archive: &[u8], asset: &Asset<'_>) -> anyhow::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    if asset.name.ends_with(".zip") {
        let mut zip = zip::ZipArchive::new(Cursor::new(archive))?;
        ensure!(
            zip.file_names().filter(|name| *name == asset.leaf).count() == 1,
            "verifier archive must contain one exact executable"
        );
        let mut entry = zip.by_name(asset.leaf)?;
        ensure!(
            entry.is_file()
                && entry
                    .unix_mode()
                    .is_none_or(|mode| mode & 0o170000 == 0o100000),
            "verifier executable must be a regular file"
        );
        ensure!(
            entry.size() <= MAX_BINARY,
            "verifier executable exceeds byte limit"
        );
        entry
            .by_ref()
            .take(MAX_BINARY + 1)
            .read_to_end(&mut bytes)?;
    } else {
        let decoder = flate2::read::GzDecoder::new(archive).take(256 * 1024 * 1024);
        let mut tar = tar::Archive::new(decoder);
        let mut found = false;
        for entry in tar.entries()? {
            let mut entry = entry?;
            if entry.path_bytes().as_ref() != asset.leaf.as_bytes() {
                continue;
            }
            ensure!(
                !found && entry.header().entry_type().is_file(),
                "verifier archive must contain one regular executable"
            );
            ensure!(
                entry.size() <= MAX_BINARY,
                "verifier executable exceeds byte limit"
            );
            entry
                .by_ref()
                .take(MAX_BINARY + 1)
                .read_to_end(&mut bytes)?;
            found = true;
        }
        ensure!(found, "verifier archive is missing its exact executable");
    }
    ensure!(
        !bytes.is_empty() && bytes.len() as u64 <= MAX_BINARY,
        "invalid verifier executable size"
    );
    Ok(bytes)
}

fn read_executable(path: &Path, asset: &Asset<'_>) -> anyhow::Result<Vec<u8>> {
    #[cfg(unix)]
    let file = File::from(rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK,
        rustix::fs::Mode::empty(),
    )?);
    #[cfg(not(unix))]
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        ensure!(
            metadata.uid() == rustix::process::geteuid().as_raw()
                && metadata.mode() & 0o077 == 0
                && metadata.mode() & 0o100 != 0,
            "managed verifier executable must be private, user-owned and executable"
        );
    }
    ensure!(
        metadata.is_file() && metadata.len() <= MAX_BINARY,
        "managed verifier must be a bounded regular file"
    );
    let mut bytes = Vec::new();
    file.take(MAX_BINARY + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_BINARY && digest(&bytes) == asset.executable_sha256,
        "managed verifier executable digest mismatch"
    );
    Ok(bytes)
}

fn write_executable(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o700);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_release_asset_redirect_is_allowed() {
        assert!(allowed_download_url(&Url::parse("https://release-assets.githubusercontent.com/github-production-release-asset/212613049/asset?sig=public").unwrap()));
    }

    #[test]
    fn arbitrary_download_hosts_and_credentials_are_rejected() {
        for url in [
            "https://evil.test/bin/gh",
            "https://github.com/evil/cli/releases/download/v2.101.0/gh.zip",
            "https://user@github.com/cli/cli/releases/download/v2.101.0/gh.zip",
            "http://github.com/cli/cli/releases/download/v2.101.0/gh.zip",
        ] {
            assert!(!allowed_download_url(&Url::parse(url).unwrap()));
        }
    }

    #[test]
    fn windows_is_explicitly_unsupported_until_private_acl_support() {
        assert!(platform_asset("windows", "x86_64").is_err());
    }

    #[test]
    fn wrong_archive_digest_is_rejected_before_parsing() {
        let cache = tempfile::tempdir().unwrap();
        assert!(
            ensure_cached_tool(cache.path(), &MAC_ARM, |_| Ok(b"untrusted zip".to_vec()))
                .unwrap_err()
                .to_string()
                .contains("archive digest")
        );
    }

    #[test]
    fn missing_exact_executable_is_rejected_without_unpacking() {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("other/bin/gh", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"not the expected executable").unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        assert!(extract_executable(&bytes, &MAC_ARM).is_err());
    }

    #[test]
    fn changed_cached_tool_is_rejected_without_redownloading() {
        let cache = tempfile::tempdir().unwrap();
        write_executable(&cache.path().join("gh"), b"changed tool").unwrap();
        assert!(
            ensure_cached_tool(cache.path(), &MAC_ARM, |_| bail!("must not fetch"))
                .unwrap_err()
                .to_string()
                .contains("executable digest")
        );
    }

    #[test]
    fn valid_cached_bytes_do_not_fetch() {
        let cache = tempfile::tempdir().unwrap();
        let bytes = b"pinned test tool";
        let asset = Asset {
            name: "test.zip",
            archive_sha256: "",
            executable_sha256: "02790c1ab1a1479379ab99ed8d195841cd2951d5fdfab559d676b45088951897",
            leaf: "bin/gh",
        };
        write_executable(&cache.path().join("gh"), bytes).unwrap();
        assert!(ensure_cached_tool(cache.path(), &asset, |_| bail!("must not fetch")).is_ok());
    }

    #[test]
    fn first_use_fetches_verified_archive_and_atomically_caches_exact_leaf() {
        let cache = tempfile::tempdir().unwrap();
        let executable = b"verified fixture tool";
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file(
                "bin/gh",
                zip::write::SimpleFileOptions::default().unix_permissions(0o755),
            )
            .unwrap();
        writer.write_all(executable).unwrap();
        let archive = writer.finish().unwrap().into_inner();
        let archive_hash = digest(&archive);
        let executable_hash = digest(executable);
        let asset = Asset {
            name: "fixture.zip",
            archive_sha256: &archive_hash,
            executable_sha256: &executable_hash,
            leaf: "bin/gh",
        };
        let path = ensure_cached_tool(cache.path(), &asset, |_| Ok(archive)).unwrap();
        assert_eq!(read_executable(&path, &asset).unwrap(), executable);
    }

    #[cfg(unix)]
    #[test]
    fn cached_symlink_is_not_executed_or_followed() {
        let cache = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink("/bin/sh", cache.path().join("gh")).unwrap();
        assert!(ensure_cached_tool(cache.path(), &MAC_ARM, |_| bail!("must not fetch")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn permissive_cache_directory_is_rejected() {
        use std::os::unix::fs::PermissionsExt as _;
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join("cache");
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(private_directory(&path, scratch.path()).is_err());
    }
}
