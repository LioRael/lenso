//! Host-owned external configuration admission for a built App distribution.
use std::{
    fs,
    io::Read as _,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, bail, ensure};
use clap::Args;
use lenso_app_authoring::{
    FilePluginConfigurationSnapshotSource, HttpsPluginConfigurationSnapshotSource,
    LocalPluginRootAuthority, PluginConfigurationAuthority, PluginConfigurationAuthoritySource,
    PluginConfigurationSnapshotAuthorization, PluginConfigurationSnapshotIntent,
    PluginConfigurationSnapshotObjectScope, PluginConfigurationSnapshotPoll,
    PluginConfigurationSnapshotPublicationState, PluginConfigurationSnapshotReconciliation,
    propose_versioned_plugin_configuration_snapshot,
};
use serde::{Deserialize, Serialize};

const POLICY_SCHEMA: &str = "lenso.configuration-source-policy.v1";
const STATE_SCHEMA: &str = "lenso.configuration-source-state.v1";
const STATE_FILE: &str = "configuration-source-state.json";
const MAX_POLICY_BYTES: u64 = 64 * 1024;
const MAX_STATE_BYTES: u64 = 16 * 1024;

#[derive(Clone, Debug, Args)]
pub struct SyncArgs {
    /// Built App distribution containing the exact Host authority.
    #[arg(long)]
    root: PathBuf,
    /// Host-operator-owned source address and writable field scopes.
    #[arg(long)]
    policy: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    schema: String,
    source_reference: String,
    source: Source,
    objects: Vec<ObjectScope>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Source {
    File {
        path: PathBuf,
    },
    Https {
        url: String,
        admitted_origins: Vec<String>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObjectScope {
    plugin_id: String,
    instance_key: String,
    fields: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct State {
    schema: String,
    desired: PluginConfigurationSnapshotIntent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_activated: Option<ActivatedConfiguration>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ActivatedConfiguration {
    revision: u64,
    snapshot_digest: String,
    plugin_root_revision: String,
}

pub fn sync_command(args: SyncArgs) -> anyhow::Result<()> {
    let root = fs::canonicalize(&args.root)?;
    sync(&root, &args.policy)?;
    println!(
        "External configuration accepted for {}; Host activation remains separate",
        root.display()
    );
    Ok(())
}

pub(super) fn require_or_sync(root: &Path, policy: Option<&Path>) -> anyhow::Result<()> {
    if let Some(policy) = policy {
        sync(root, policy)?;
    } else {
        match fs::symlink_metadata(root.join("intent/.lenso").join(STATE_FILE)) {
            Ok(_) => bail!(
                "this App has an external configuration source; provide --configuration-policy"
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

pub(super) fn sync(root: &Path, policy_path: &Path) -> anyhow::Result<()> {
    let root = fs::canonicalize(root)?;
    let intent = root.join("intent");
    ensure!(
        fs::symlink_metadata(&intent)?.file_type().is_dir(),
        "App runtime intent must be a real directory"
    );
    let control = intent.join(".lenso");
    ensure!(
        fs::symlink_metadata(&control)?.file_type().is_dir(),
        "App control directory must be a real directory"
    );
    let lock = open_lock(&control.join("configuration-source.lock"))?;
    lock.lock()?;
    verify_intent_authority(&root, &intent)?;

    let policy: Policy = serde_json::from_slice(&read_bounded(policy_path, MAX_POLICY_BYTES)?)
        .context("parse Host configuration source policy")?;
    ensure!(
        policy.schema == POLICY_SCHEMA,
        "unsupported configuration source policy"
    );
    let source_kind = match policy.source {
        Source::File { .. } => "file_snapshot",
        Source::Https { .. } => "https_poll",
    };
    let identity = PluginConfigurationAuthoritySource::new(source_kind, &policy.source_reference)?;
    let scopes = policy
        .objects
        .into_iter()
        .map(|scope| {
            PluginConfigurationSnapshotObjectScope::new(
                scope.plugin_id,
                scope.instance_key,
                scope.fields,
            )
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let authorization = PluginConfigurationSnapshotAuthorization::new(identity.clone(), scopes)?;
    let snapshot = match policy.source {
        Source::File { path } => {
            ensure!(
                path.is_absolute(),
                "configuration snapshot path must be absolute"
            );
            FilePluginConfigurationSnapshotSource::new(path, identity).read()?
        }
        Source::Https {
            url,
            admitted_origins,
        } => {
            let source =
                HttpsPluginConfigurationSnapshotSource::new(&url, identity, &admitted_origins)?;
            match source.poll(None)? {
                PluginConfigurationSnapshotPoll::Updated { snapshot, .. } => snapshot,
                PluginConfigurationSnapshotPoll::NotModified { .. } => {
                    bail!("configuration source returned 304 without a saved cursor")
                }
            }
        }
    };
    let state_path = control.join(STATE_FILE);
    let previous = read_state(&state_path)?;
    let authority = LocalPluginRootAuthority::new(&intent);
    let current = authority.inspect()?;
    let pending = previous.as_ref().is_some_and(|state| {
        state.desired.publication_state(current.revision()).ok()
            == Some(PluginConfigurationSnapshotPublicationState::AwaitingPublication)
    });
    if let Some(previous) = &previous {
        previous.desired.publication_state(current.revision())?;
    }
    let result = propose_versioned_plugin_configuration_snapshot(
        &authority,
        &authorization,
        if pending {
            None
        } else {
            previous.as_ref().map(|state| &state.desired)
        },
        &snapshot,
    )?;
    if pending {
        ensure!(
            previous
                .as_ref()
                .is_some_and(|state| state.desired == *result.intent()),
            "pending configuration intent does not match the current source revision"
        );
    }
    publish_result(&authority, &state_path, result)
}

fn verify_intent_authority(distribution: &Path, intent: &Path) -> anyhow::Result<()> {
    let pairs = [".lenso/host-build.json", ".lenso/host-catalog.json"];
    let selected = pairs
        .into_iter()
        .find(|path| distribution.join(path).is_file())
        .context("built App lacks a Host authority")?;
    ensure!(
        !pairs
            .into_iter()
            .any(|path| path != selected && distribution.join(path).exists()),
        "built App has competing Host authorities"
    );
    let source = distribution.join(selected);
    let destination = intent.join(selected);
    let original = read_bounded(&source, 16 * 1024 * 1024)?;
    match fs::symlink_metadata(&destination) {
        Ok(_) => ensure!(
            read_bounded(&destination, 16 * 1024 * 1024)? == original,
            "runtime intent Host authority differs from the built distribution"
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut stage = tempfile::NamedTempFile::new_in(intent.join(".lenso"))?;
            std::io::Write::write_all(&mut stage, &original)?;
            stage.as_file().sync_all()?;
            stage.persist(&destination)?;
            fs::File::open(intent.join(".lenso"))?.sync_all()?;
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn publish_result(
    authority: &LocalPluginRootAuthority,
    state_path: &Path,
    result: PluginConfigurationSnapshotReconciliation,
) -> anyhow::Result<()> {
    match result {
        PluginConfigurationSnapshotReconciliation::Unchanged(_) => Ok(()),
        PluginConfigurationSnapshotReconciliation::NoRootChange(intent) => {
            write_state(state_path, &intent)
        }
        PluginConfigurationSnapshotReconciliation::Proposed { intent, proposal } => {
            write_state(state_path, &intent)?;
            authority.publish_changes(&proposal)?;
            ensure!(
                intent.publication_state(authority.inspect()?.revision())?
                    == PluginConfigurationSnapshotPublicationState::Published,
                "external configuration publication did not reach the proposed Root revision"
            );
            Ok(())
        }
    }
}

fn read_state(path: &Path) -> anyhow::Result<Option<State>> {
    if !path.try_exists()? {
        return Ok(None);
    }
    let state: State = serde_json::from_slice(&read_bounded(path, MAX_STATE_BYTES)?)?;
    ensure!(
        state.schema == STATE_SCHEMA,
        "unsupported configuration source state"
    );
    state.desired.source()?;
    Ok(Some(state))
}

fn write_state(path: &Path, desired: &PluginConfigurationSnapshotIntent) -> anyhow::Result<()> {
    let last_activated = read_state(path)?.and_then(|state| state.last_activated);
    let parent = path.parent().context("configuration state parent")?;
    let mut stage = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(
        &mut stage,
        &State {
            schema: STATE_SCHEMA.into(),
            desired: desired.clone(),
            last_activated,
        },
    )?;
    stage.as_file().sync_all()?;
    stage.persist(path)?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

fn read_bounded(path: &Path, max: u64) -> anyhow::Result<Vec<u8>> {
    let mut file = open_regular(path)?;
    let mut bytes = Vec::new();
    file.by_ref().take(max + 1).read_to_end(&mut bytes)?;
    ensure!(
        u64::try_from(bytes.len())? <= max,
        "configuration policy/state exceeds its size limit"
    );
    Ok(bytes)
}

#[cfg(unix)]
fn open_lock(path: &Path) -> anyhow::Result<fs::File> {
    use rustix::fs::{Mode, OFlags};

    let descriptor = rustix::fs::open(
        path,
        OFlags::RDWR | OFlags::CREATE | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::RUSR | Mode::WUSR,
    )?;
    let file = fs::File::from(descriptor);
    ensure!(
        file.metadata()?.file_type().is_file(),
        "configuration lock must be a regular file"
    );
    Ok(file)
}

#[cfg(unix)]
fn open_regular(path: &Path) -> anyhow::Result<fs::File> {
    use rustix::fs::{Mode, OFlags};

    let descriptor = rustix::fs::open(
        path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )?;
    let file = fs::File::from(descriptor);
    ensure!(
        file.metadata()?.file_type().is_file(),
        "configuration policy/state must be a regular file"
    );
    Ok(file)
}

#[cfg(not(unix))]
fn open_lock(_path: &Path) -> anyhow::Result<fs::File> {
    bail!("external configuration startup is unsupported on this platform")
}

#[cfg(not(unix))]
fn open_regular(_path: &Path) -> anyhow::Result<fs::File> {
    bail!("external configuration startup is unsupported on this platform")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use lenso_app_plan::authoring::{
        HostCatalog, HostDefaultPlugin, HostPluginRelease, HostSlot, PluginDescriptor,
    };

    use super::*;

    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".lenso")).unwrap();
        fs::create_dir_all(root.path().join("intent/.lenso")).unwrap();
        let descriptor = PluginDescriptor::new("example.agent", "1.0.0", "agent")
            .with_configuration_schema(serde_json::json!({
                "type": "object", "properties": {
                    "greeting": {"type": "string"},
                    "token": {"x-lenso-sensitive": true}
                }, "additionalProperties": false
            }));
        let host = HostCatalog::new(
            [HostSlot::one("agent")],
            [HostPluginRelease::new(descriptor)],
            [HostDefaultPlugin::new("example.agent", "default")],
        );
        fs::write(
            root.path().join(".lenso/host-catalog.json"),
            serde_json::to_vec(&host).unwrap(),
        )
        .unwrap();
        let snapshot = root.path().join("snapshot.json");
        let policy = root.path().join("policy.json");
        fs::write(&policy, serde_json::to_vec(&serde_json::json!({
            "schema": POLICY_SCHEMA,
            "source_reference": "development",
            "source": {"type": "file", "path": snapshot},
            "objects": [{"plugin_id": "example.agent", "instance_key": "default", "fields": ["greeting"]}]
        })).unwrap()).unwrap();
        (root, snapshot, policy)
    }

    fn snapshot(path: &Path, revision: u64, toml: &str) {
        fs::write(path, serde_json::to_vec(&serde_json::json!({
            "schema": "lenso.plugin-configuration-snapshot.v1",
            "revision": revision,
            "configurations": [{"plugin_id": "example.agent", "instance_key": "default", "toml": toml}]
        })).unwrap()).unwrap();
    }

    #[test]
    fn file_snapshot_reconciles_exact_revisions_before_host_start() {
        let (root, source, policy) = fixture();
        snapshot(&source, 1, "greeting = 'first'\n");
        sync(root.path(), &policy).unwrap();
        sync(root.path(), &policy).unwrap();
        snapshot(&source, 2, "greeting = 'second'\n");
        sync(root.path(), &policy).unwrap();

        let configuration = fs::read_to_string(
            root.path()
                .join("intent/plugins/example.agent/default.toml"),
        )
        .unwrap();
        assert!(configuration.contains("second"));
        let state = read_state(&root.path().join("intent/.lenso").join(STATE_FILE))
            .unwrap()
            .unwrap();
        assert_eq!(state.desired.revision(), 2);
    }

    #[test]
    fn stale_file_snapshot_fails_without_changing_root() {
        let (root, source, policy) = fixture();
        snapshot(&source, 2, "greeting = 'second'\n");
        sync(root.path(), &policy).unwrap();
        snapshot(&source, 1, "greeting = 'first'\n");

        let error = sync(root.path(), &policy).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("stale external configuration revision")
        );
        assert!(
            fs::read_to_string(
                root.path()
                    .join("intent/plugins/example.agent/default.toml")
            )
            .unwrap()
            .contains("second")
        );
    }

    #[test]
    fn new_desired_revision_preserves_last_successful_activation() {
        let (root, source, policy) = fixture();
        let state_path = root.path().join("intent/.lenso").join(STATE_FILE);
        snapshot(&source, 1, "greeting = 'first'\n");
        sync(root.path(), &policy).unwrap();
        let mut state = read_state(&state_path).unwrap().unwrap();
        state.last_activated = Some(ActivatedConfiguration {
            revision: state.desired.revision(),
            snapshot_digest: state.desired.snapshot_digest().to_owned(),
            plugin_root_revision: state.desired.candidate_plugin_root_revision().to_owned(),
        });
        fs::write(&state_path, serde_json::to_vec(&state).unwrap()).unwrap();

        snapshot(&source, 2, "greeting = 'second'\n");
        sync(root.path(), &policy).unwrap();
        let current = read_state(&state_path).unwrap().unwrap();
        assert_eq!(current.desired.revision(), 2);
        assert_eq!(current.last_activated.unwrap().revision, 1);
    }

    #[test]
    fn unauthorized_field_fails_without_publication() {
        let (root, source, policy) = fixture();
        snapshot(&source, 1, "token = 'secret://test'\n");

        let error = sync(root.path(), &policy).unwrap_err();
        assert!(error.to_string().contains("authorized scope"));
        assert!(
            !root
                .path()
                .join("intent/plugins/example.agent/default.toml")
                .exists()
        );
        assert!(!root.path().join("intent/.lenso").join(STATE_FILE).exists());
    }

    #[test]
    fn pending_intent_is_recovered_only_for_the_same_snapshot() {
        let (root, source, policy) = fixture();
        snapshot(&source, 1, "greeting = 'first'\n");
        let identity =
            PluginConfigurationAuthoritySource::new("file_snapshot", "development").unwrap();
        let authorization =
            PluginConfigurationSnapshotAuthorization::new(
                identity.clone(),
                [PluginConfigurationSnapshotObjectScope::new(
                    "example.agent",
                    "default",
                    ["greeting"],
                )
                .unwrap()],
            )
            .unwrap();
        let current_snapshot = FilePluginConfigurationSnapshotSource::new(&source, identity)
            .read()
            .unwrap();
        verify_intent_authority(root.path(), &root.path().join("intent")).unwrap();
        let authority = LocalPluginRootAuthority::new(root.path().join("intent"));
        let proposal = propose_versioned_plugin_configuration_snapshot(
            &authority,
            &authorization,
            None,
            &current_snapshot,
        )
        .unwrap();
        write_state(
            &root.path().join("intent/.lenso").join(STATE_FILE),
            proposal.intent(),
        )
        .unwrap();

        snapshot(&source, 1, "greeting = 'changed'\n");
        assert!(sync(root.path(), &policy).is_err());
        assert!(
            !root
                .path()
                .join("intent/plugins/example.agent/default.toml")
                .exists()
        );
        snapshot(&source, 1, "greeting = 'first'\n");
        sync(root.path(), &policy).unwrap();
        assert!(
            root.path()
                .join("intent/plugins/example.agent/default.toml")
                .exists()
        );
    }
}
