//! Fixed-input source App adoption through the public CLI's signed admission path.
use std::{
    collections::BTreeMap,
    fs,
    io::Read as _,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicI32},
    },
    time::Duration,
};

use anyhow::{Context as _, ensure};
use lenso_engine::process::{ProcessBudget, execute_cancellable_command_with_budget};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Action {
    Adopt,
    Unadopt,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Distribution {
    LinkedCargo,
    Portable,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct SignedInputs<'a> {
    pub(super) snapshot: &'a Path,
    pub(super) trust: &'a Path,
    pub(super) archive: &'a Path,
}

#[derive(Debug)]
pub(super) struct FrozenSignedInputs {
    pub(super) storage: Arc<tempfile::TempDir>,
    pub(super) snapshot: PathBuf,
    pub(super) trust: PathBuf,
    pub(super) archive: PathBuf,
}

#[derive(Debug)]
struct Record {
    distribution: Distribution,
    action: Action,
    plugin_id: String,
    version: String,
    result: serde_json::Value,
}

#[derive(Debug, Default)]
pub(super) struct AdoptionController {
    records: Mutex<BTreeMap<String, Record>>,
}

impl AdoptionController {
    pub(super) fn apply(
        &self,
        root: &Path,
        distribution: Distribution,
        action: Action,
        plugin_id: &str,
        version: &str,
        request_id: &str,
        inputs: Option<SignedInputs<'_>>,
    ) -> anyhow::Result<serde_json::Value> {
        validate_request(plugin_id, version, request_id)?;
        let mut records = self.records.lock().expect("MCP adoption state lock");
        if let Some(record) = records.get(request_id) {
            ensure!(
                record.distribution == distribution
                    && record.action == action
                    && record.plugin_id == plugin_id
                    && record.version == version,
                "request_id was already used for another signed adoption operation"
            );
            return Ok(record.result.clone());
        }
        ensure!(
            records.len() < 32,
            "MCP adoption history is full; restart the bridge"
        );
        ensure_source_root(root)?;
        if let Some(inputs) = inputs {
            for path in [inputs.snapshot, inputs.trust, inputs.archive] {
                ensure!(
                    fs::symlink_metadata(path)?.file_type().is_file(),
                    "signed adoption input is no longer a regular file"
                );
            }
        }

        let executable = std::env::current_exe().context("locate local Lenso CLI")?;
        let source = format!("{plugin_id}@{version}");
        let mut command = lenso_engine_app::app::build_command(executable);
        command.arg("app");
        match action {
            Action::Adopt => {
                let inputs = inputs.context("signed adoption inputs are not configured")?;
                command.args(["add", &source, "--root"]).arg(root);
                match distribution {
                    Distribution::LinkedCargo => {
                        command
                            .arg("--linked-snapshot")
                            .arg(inputs.snapshot)
                            .arg("--trust")
                            .arg(inputs.trust)
                            .arg("--crate")
                            .arg(inputs.archive)
                            .arg("--no-install");
                    }
                    Distribution::Portable => {
                        command
                            .arg("--portable-snapshot")
                            .arg(inputs.snapshot)
                            .arg("--trust")
                            .arg(inputs.trust)
                            .arg("--archive")
                            .arg(inputs.archive);
                    }
                }
            }
            Action::Unadopt => {
                command.args(["unadopt", &source, "--root"]).arg(root);
                if distribution == Distribution::Portable {
                    command.arg("--portable");
                }
            }
        }
        command.current_dir(root);
        let budget = ProcessBudget::new(Duration::from_secs(60), 64 * 1024)?;
        let outcome = execute_cancellable_command_with_budget(
            command,
            &serde_json::json!({}),
            Arc::new(AtomicI32::new(0)),
            &AtomicBool::new(false),
            budget,
        );
        // The CLI may have published files before an I/O or timeout error. Keep
        // that outcome explicit, and never return its raw stderr over MCP.
        let (state, diagnostic_code) = classify_outcome(distribution, action, &outcome);
        let result = serde_json::json!({
            "schema_version": 1,
            "kind": match distribution {
                Distribution::LinkedCargo => "lenso.mcp-linked-cargo-adoption",
                Distribution::Portable => "lenso.mcp-portable-adoption",
            },
            "request_id": request_id,
            "plugin_id": plugin_id,
            "version": version,
            "state": state,
            "application": "build_required",
            "activation": "not_observed",
            "diagnostic_code": diagnostic_code,
            "next_step": if outcome.is_ok() { "build_and_check" } else { "inspect_source_and_signed_inputs_before_retry" },
        });
        records.insert(
            request_id.to_owned(),
            Record {
                distribution,
                action,
                plugin_id: plugin_id.to_owned(),
                version: version.to_owned(),
                result: result.clone(),
            },
        );
        Ok(result)
    }
}

fn classify_outcome(
    distribution: Distribution,
    action: Action,
    outcome: &anyhow::Result<Vec<u8>>,
) -> (&'static str, Option<&'static str>) {
    let Err(error) = outcome else {
        return (
            match action {
                Action::Adopt => "selected",
                Action::Unadopt => "unadopted",
            },
            None,
        );
    };
    let message = error.to_string();
    let linked_rejection = [
        (
            "crate archive digest does not match signed catalog",
            "LENSO_ADOPTION_CRATE_DIGEST_MISMATCH",
        ),
        (
            "exact linked Cargo release is not in this catalog",
            "LENSO_ADOPTION_VERSION_NOT_LISTED",
        ),
        (
            "linked Cargo release is not available for adoption",
            "LENSO_ADOPTION_RELEASE_UNAVAILABLE",
        ),
        (
            "linked Cargo release does not support Host target",
            "LENSO_ADOPTION_TARGET_MISMATCH",
        ),
        ("App already selects", "LENSO_ADOPTION_SOURCE_CONFLICT"),
    ];
    let portable_rejection = [
        (
            "archive size or digest mismatch",
            "LENSO_ADOPTION_ARCHIVE_DIGEST_MISMATCH",
        ),
        (
            "exact release is not in this catalog",
            "LENSO_ADOPTION_VERSION_NOT_LISTED",
        ),
        ("already supplied by", "LENSO_ADOPTION_SOURCE_CONFLICT"),
    ];
    let rejections: &[(&str, &str)] = match distribution {
        Distribution::LinkedCargo => &linked_rejection,
        Distribution::Portable => &portable_rejection,
    };
    for &(needle, code) in rejections {
        if message.contains(needle) {
            return ("rejected", Some(code));
        }
    }
    if message.contains("processor exceeded its") {
        (
            "outcome_uncertain",
            Some("LENSO_ADOPTION_TIMEOUT_RECONCILE"),
        )
    } else {
        ("outcome_uncertain", Some("LENSO_ADOPTION_RECONCILE_SOURCE"))
    }
}

pub(super) fn freeze_inputs(
    snapshot: &Path,
    trust: &Path,
    archive: &Path,
    distribution: Distribution,
) -> anyhow::Result<FrozenSignedInputs> {
    let storage = Arc::new(
        tempfile::Builder::new()
            .prefix(match distribution {
                Distribution::LinkedCargo => "lenso-mcp-linked-",
                Distribution::Portable => "lenso-mcp-portable-",
            })
            .tempdir()?,
    );
    let snapshot_path = storage.path().join("snapshot.json");
    let trust_path = storage.path().join("trust.json");
    let archive_path = storage.path().join(match distribution {
        Distribution::LinkedCargo => "plugin.crate",
        Distribution::Portable => "plugin.lenso-plugin",
    });
    freeze_file(snapshot, &snapshot_path, 4 * 1024 * 1024)?;
    freeze_file(trust, &trust_path, 64 * 1024)?;
    freeze_file(
        archive,
        &archive_path,
        match distribution {
            Distribution::LinkedCargo => 32 * 1024 * 1024,
            Distribution::Portable => 128 * 1024 * 1024,
        },
    )?;
    Ok(FrozenSignedInputs {
        storage,
        snapshot: snapshot_path,
        trust: trust_path,
        archive: archive_path,
    })
}

#[cfg(unix)]
fn freeze_file(source: &Path, destination: &Path, max_bytes: u64) -> anyhow::Result<()> {
    use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _};

    let metadata = fs::symlink_metadata(source)?;
    ensure!(
        metadata.file_type().is_file() && metadata.len() <= max_bytes,
        "MCP signed adoption input must be a regular file"
    );
    let input = fs::OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(source)?;
    let opened = input.metadata()?;
    ensure!(
        opened.is_file()
            && opened.len() <= max_bytes
            && opened.dev() == metadata.dev()
            && opened.ino() == metadata.ino(),
        "MCP signed adoption input changed while being frozen"
    );
    let mut bytes = Vec::new();
    input.take(max_bytes + 1).read_to_end(&mut bytes)?;
    ensure!(
        !bytes.is_empty() && u64::try_from(bytes.len())? <= max_bytes,
        "MCP signed adoption input exceeds its size limit"
    );
    fs::write(destination, bytes)?;
    Ok(())
}

#[cfg(not(unix))]
fn freeze_file(_source: &Path, _destination: &Path, _max_bytes: u64) -> anyhow::Result<()> {
    anyhow::bail!("MCP signed adoption requires Unix no-follow file admission")
}

fn ensure_source_root(root: &Path) -> anyhow::Result<()> {
    ensure!(
        fs::symlink_metadata(root)?.file_type().is_dir(),
        "MCP signed adoption requires a real source App root"
    );
    ensure!(
        fs::symlink_metadata(root.join("plugins"))?
            .file_type()
            .is_dir(),
        "MCP signed adoption requires a source App with a real plugins directory"
    );
    ensure!(
        !root.join(".lenso/host-build.json").exists()
            && !root.join(".lenso/host-catalog.json").exists(),
        "MCP signed adoption requires a source App, not a built Host distribution"
    );
    Ok(())
}

fn validate_request(plugin_id: &str, version: &str, request_id: &str) -> anyhow::Result<()> {
    lenso_app_authoring::identity::validate_plugin_id_v1(plugin_id)?;
    ensure!(
        !version.is_empty()
            && version.len() <= 64
            && version
                .bytes()
                .all(|byte| { byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+') }),
        "version must be an exact bounded ASCII release version"
    );
    ensure!(
        !request_id.is_empty()
            && request_id.len() <= 128
            && request_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')),
        "request_id must be 1 to 128 ASCII letters, digits, dots, hyphens or underscores"
    );
    Ok(())
}

pub(super) fn public_request_error(error: &anyhow::Error) -> &'static str {
    let message = error.to_string();
    if message.contains("request_id was already used") {
        "request_id was already used for a different signed adoption operation; choose a new request_id"
    } else if message.contains("request_id must") {
        "request_id must be 1 to 128 ASCII letters, digits, dots, hyphens or underscores"
    } else if message.contains("version must") || message.contains("Plugin ID") {
        "choose one valid exact Plugin ID and release version"
    } else if message.contains("requires a source App") {
        "start the MCP bridge at the source App root, not a built distribution"
    } else if message.contains("no longer a regular file") {
        "fixed signed adoption inputs are unavailable; restart the MCP bridge with regular files"
    } else if message.contains("history is full") {
        "MCP adoption request history is full; restart the bridge"
    } else {
        "signed adoption request is invalid; inspect the fixed source App and signed inputs locally"
    }
}
