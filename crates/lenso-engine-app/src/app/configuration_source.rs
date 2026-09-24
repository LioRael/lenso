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
    PluginConfigurationSnapshotAuthorization, PluginConfigurationSnapshotCursor,
    PluginConfigurationSnapshotIntent, PluginConfigurationSnapshotObjectScope,
    PluginConfigurationSnapshotPoll, PluginConfigurationSnapshotPublicationState,
    PluginConfigurationSnapshotReconciliation, propose_versioned_plugin_configuration_snapshot,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

const POLICY_SCHEMA: &str = "lenso.configuration-source-policy.v1";
const STATE_SCHEMA: &str = "lenso.configuration-source-state.v2";
const STATE_FILE: &str = "configuration-source-state.json";
const MAX_POLICY_BYTES: u64 = 64 * 1024;
const MAX_STATE_BYTES: u64 = 64 * 1024 * 1024;

fn digest_policy(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

#[derive(Clone, Debug, Args)]
pub struct SyncArgs {
    /// Built App distribution, or an external Plugin Root with --host-build.
    #[arg(long)]
    root: PathBuf,
    /// Exact distribution Host build for an external Plugin Root.
    #[arg(long)]
    host_build: Option<PathBuf>,
    /// Host-operator-owned source address and writable field scopes.
    #[arg(long)]
    policy: PathBuf,
}

#[derive(Clone, Debug, Args)]
pub struct StatusArgs {
    /// Built App distribution, or external Plugin Root with --host-build.
    #[arg(long)]
    root: PathBuf,
    /// Exact distribution Host build for an external Plugin Root.
    #[arg(long)]
    host_build: Option<PathBuf>,
    /// Emit a stable, non-secret JSON projection.
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Debug, Args)]
pub struct ActivatedArgs {
    /// External Plugin Root whose active Generation passed the Host Ready Gate.
    #[arg(long)]
    root: PathBuf,
    /// Exact distribution Host build used to resolve the active Generation.
    #[arg(long)]
    host_build: PathBuf,
    /// Root revision resolved into the active Generation.
    #[arg(long)]
    plugin_root_revision: String,
}

#[derive(Debug, Serialize)]
pub struct Status {
    pub schema: &'static str,
    pub state: &'static str,
    pub source_kind: Option<String>,
    pub desired_revision: Option<u64>,
    pub last_activated_revision: Option<u64>,
    pub pending_publication: bool,
    pub pending_activation: bool,
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
    prior_desired: Option<PluginConfigurationSnapshotIntent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    policy_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cursor: Option<PluginConfigurationSnapshotCursor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_activated: Option<ActivatedConfiguration>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ActivatedConfiguration {
    revision: u64,
    snapshot_digest: String,
    plugin_root_revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    policy_digest: Option<String>,
}

pub fn sync_command(args: SyncArgs) -> anyhow::Result<()> {
    let root = fs::canonicalize(&args.root)?;
    if let Some(host_build) = args.host_build {
        sync_external_configuration(&root, &host_build, &args.policy)?;
    } else {
        sync(&root, &args.policy)?;
    }
    println!(
        "External configuration accepted for {}; Host activation remains separate",
        root.display()
    );
    Ok(())
}

pub fn activated_command(args: ActivatedArgs) -> anyhow::Result<()> {
    record_external_activation(&args.root, &args.host_build, &args.plugin_root_revision)
}

/// A private Host receipt, not a request to activate. The caller must invoke
/// this only after its controller proves the resolved Generation is active.
pub fn record_external_activation(
    plugin_root: &Path,
    distribution_host_build: &Path,
    expected_revision: &str,
) -> anyhow::Result<()> {
    let intent = fs::canonicalize(plugin_root)?;
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
    verify_external_host_authority(distribution_host_build, &intent)?;
    record_activation_locked(&intent, expected_revision)
}

/// The local development supervisor calls this only after its candidate Host
/// passed readiness and the preceding Host stopped. A source publication by
/// itself never advances the activation receipt.
pub(super) fn record_distribution_activation(
    distribution: &Path,
    expected_revision: &str,
) -> anyhow::Result<()> {
    let distribution = fs::canonicalize(distribution)?;
    let intent = fs::canonicalize(distribution.join("intent"))?;
    let control = intent.join(".lenso");
    ensure!(
        fs::symlink_metadata(&control)?.file_type().is_dir(),
        "App control directory must be a real directory"
    );
    let lock = open_lock(&control.join("configuration-source.lock"))?;
    lock.lock()?;
    verify_intent_authority(&distribution, &intent)?;
    record_activation_locked(&intent, expected_revision)
}

/// Return the exact accepted Root revision to fence a supervised candidate.
pub(super) fn desired_root_revision(distribution: &Path) -> anyhow::Result<Option<String>> {
    let distribution = fs::canonicalize(distribution)?;
    let intent = fs::canonicalize(distribution.join("intent"))?;
    let control = intent.join(".lenso");
    let lock = open_lock(&control.join("configuration-source.lock"))?;
    lock.lock()?;
    verify_intent_authority(&distribution, &intent)?;
    let Some(state) = read_state(&control.join(STATE_FILE))? else {
        return Ok(None);
    };
    let current = LocalPluginRootAuthority::new(intent).inspect()?;
    ensure!(
        matches!(
            state.desired.publication_state(current.revision())?,
            PluginConfigurationSnapshotPublicationState::Published
                | PluginConfigurationSnapshotPublicationState::NoRootChange
        ),
        "configuration candidate has not reached its proposed Plugin Root revision"
    );
    Ok(Some(
        state.desired.candidate_plugin_root_revision().to_owned(),
    ))
}

fn record_activation_locked(intent: &Path, expected_revision: &str) -> anyhow::Result<()> {
    let control = intent.join(".lenso");
    let path = control.join(STATE_FILE);
    let mut state = read_state(&path)?.context("missing accepted external configuration")?;
    let current = LocalPluginRootAuthority::new(intent).inspect()?;
    ensure!(
        current.revision().as_str() == expected_revision
            && state.desired.candidate_plugin_root_revision() == expected_revision
            && matches!(
                state.desired.publication_state(current.revision())?,
                PluginConfigurationSnapshotPublicationState::Published
                    | PluginConfigurationSnapshotPublicationState::NoRootChange
            ),
        "activation receipt no longer matches the accepted Plugin Root revision"
    );
    state.last_activated = Some(ActivatedConfiguration {
        revision: state.desired.revision(),
        snapshot_digest: state.desired.snapshot_digest().to_owned(),
        plugin_root_revision: expected_revision.to_owned(),
        policy_digest: Some(
            state
                .policy_digest
                .clone()
                .context("accepted configuration has no Host policy digest")?,
        ),
    });
    persist_state(&path, &state)
}

pub fn status_command(args: StatusArgs) -> anyhow::Result<()> {
    let status = if let Some(host_build) = args.host_build {
        inspect_external_status(&args.root, &host_build)?
    } else {
        inspect_status(&args.root)?
    };
    if args.json {
        println!("{}", serde_json::to_string(&status)?);
    } else {
        println!(
            "Configuration: {} (desired: {}, last activated: {})",
            status.state,
            status
                .desired_revision
                .map_or_else(|| "none".to_owned(), |v| v.to_string()),
            status
                .last_activated_revision
                .map_or_else(|| "none".to_owned(), |v| v.to_string())
        );
    }
    Ok(())
}

pub(super) fn inspect_status(root: &Path) -> anyhow::Result<Status> {
    let root = fs::canonicalize(root)?;
    let intent = root.join("intent");
    status_for_intent(&intent)
}

pub fn inspect_external_status(plugin_root: &Path, host_build: &Path) -> anyhow::Result<Status> {
    let intent = fs::canonicalize(plugin_root)?;
    let source = read_bounded(host_build, 16 * 1024 * 1024)?;
    let copied = read_bounded(&intent.join(".lenso/host-build.json"), 16 * 1024 * 1024)?;
    ensure!(
        source == copied,
        "runtime intent Host authority differs from the built distribution"
    );
    ensure!(
        !intent.join(".lenso/host-catalog.json").exists(),
        "runtime intent has a competing Host authority"
    );
    status_for_intent(&intent)
}

fn status_for_intent(intent: &Path) -> anyhow::Result<Status> {
    ensure!(
        fs::symlink_metadata(intent)?.file_type().is_dir(),
        "App runtime intent must be a real directory"
    );
    let state = read_state(&intent.join(".lenso").join(STATE_FILE))?;
    let status = if let Some(state) = state {
        let current = LocalPluginRootAuthority::new(intent).inspect()?;
        let pending_publication = state.desired.publication_state(current.revision())?
            == PluginConfigurationSnapshotPublicationState::AwaitingPublication;
        let last = state.last_activated.as_ref();
        let pending = last.is_none_or(|last| {
            last.revision != state.desired.revision()
                || last.snapshot_digest != state.desired.snapshot_digest()
                || last.plugin_root_revision != state.desired.candidate_plugin_root_revision()
                || state.policy_digest.is_none()
                || last.policy_digest != state.policy_digest
        });
        Status {
            schema: "lenso.configuration-status.v1",
            state: if pending_publication {
                "pending_publication"
            } else if pending {
                "pending_activation"
            } else {
                "last_activated"
            },
            desired_revision: Some(state.desired.revision()),
            source_kind: Some(state.desired.source()?.kind().to_owned()),
            last_activated_revision: last.map(|last| last.revision),
            pending_publication,
            pending_activation: pending,
        }
    } else {
        Status {
            schema: "lenso.configuration-status.v1",
            state: "no_external_source",
            desired_revision: None,
            source_kind: None,
            last_activated_revision: None,
            pending_publication: false,
            pending_activation: false,
        }
    };
    Ok(status)
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

/// A local supervisor must retire an active Generation when its Host policy
/// changes, even when the newly scoped source cannot yet produce an accepted
/// replacement. Source outages under the same policy do not require this.
pub(super) fn policy_changed_since_active(
    distribution: &Path,
    policy_path: &Path,
) -> anyhow::Result<bool> {
    let distribution = fs::canonicalize(distribution)?;
    let intent = fs::canonicalize(distribution.join("intent"))?;
    verify_intent_authority(&distribution, &intent)?;
    let state = read_state(&intent.join(".lenso").join(STATE_FILE))?
        .context("active App has no accepted configuration source state")?;
    let active = state
        .last_activated
        .context("active App has no configuration activation receipt")?;
    let policy_digest = digest_policy(&read_bounded(policy_path, MAX_POLICY_BYTES)?);
    Ok(active.policy_digest.as_deref() != Some(&policy_digest))
}

pub(super) fn sync(root: &Path, policy_path: &Path) -> anyhow::Result<()> {
    sync_with_https_poll(root, policy_path, |source, cursor| source.poll(cursor))
}

/// Reconciles an operator-owned source into an external Plugin Root against one
/// immutable distribution Host build. Product Hosts may call this before
/// resolving a candidate Generation; this does not activate it.
pub fn sync_external_configuration(
    plugin_root: &Path,
    distribution_host_build: &Path,
    policy_path: &Path,
) -> anyhow::Result<()> {
    sync_for_intent(
        plugin_root,
        Authority::ExternalHostBuild(distribution_host_build),
        policy_path,
        |source, cursor| source.poll(cursor),
    )
}

enum Authority<'a> {
    Distribution(&'a Path),
    ExternalHostBuild(&'a Path),
}

fn sync_with_https_poll<F>(root: &Path, policy_path: &Path, poll: F) -> anyhow::Result<()>
where
    F: FnOnce(
        &HttpsPluginConfigurationSnapshotSource,
        Option<&PluginConfigurationSnapshotCursor>,
    ) -> anyhow::Result<PluginConfigurationSnapshotPoll>,
{
    let root = fs::canonicalize(root)?;
    sync_for_intent(
        &root.join("intent"),
        Authority::Distribution(&root),
        policy_path,
        poll,
    )
}

fn sync_for_intent<F>(
    intent: &Path,
    authority: Authority<'_>,
    policy_path: &Path,
    poll: F,
) -> anyhow::Result<()>
where
    F: FnOnce(
        &HttpsPluginConfigurationSnapshotSource,
        Option<&PluginConfigurationSnapshotCursor>,
    ) -> anyhow::Result<PluginConfigurationSnapshotPoll>,
{
    let intent = fs::canonicalize(intent)?;
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
    match authority {
        Authority::Distribution(root) => verify_intent_authority(root, &intent)?,
        Authority::ExternalHostBuild(host_build) => {
            verify_external_host_authority(host_build, &intent)?;
        }
    }

    let policy_bytes = read_bounded(policy_path, MAX_POLICY_BYTES)?;
    let policy_digest = digest_policy(&policy_bytes);
    let policy: Policy =
        serde_json::from_slice(&policy_bytes).context("parse Host configuration source policy")?;
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
    let (snapshot, cursor) = match policy.source {
        Source::File { path } => {
            ensure!(
                path.is_absolute(),
                "configuration snapshot path must be absolute"
            );
            (
                FilePluginConfigurationSnapshotSource::new(path, identity).read()?,
                None,
            )
        }
        Source::Https {
            url,
            admitted_origins,
        } => {
            let source = HttpsPluginConfigurationSnapshotSource::new(
                &url,
                identity.clone(),
                &admitted_origins,
            )?;
            let previous_cursor = revalidation_cursor(previous.as_ref(), pending, &policy_digest);
            match poll(&source, previous_cursor)? {
                PluginConfigurationSnapshotPoll::Updated { snapshot, cursor } => (snapshot, cursor),
                PluginConfigurationSnapshotPoll::NotModified { .. } => {
                    ensure!(
                        previous_cursor.is_some(),
                        "configuration source returned 304 without an accepted cursor"
                    );
                    ensure!(
                        previous.as_ref().is_some_and(
                            |state| state.desired.source().ok().as_ref() == Some(&identity)
                        ),
                        "configuration source returned 304 for a different authority"
                    );
                    return Ok(());
                }
            }
        }
    };
    let prior_desired = if pending {
        previous
            .as_ref()
            .and_then(|state| state.prior_desired.as_ref())
    } else {
        previous.as_ref().map(|state| &state.desired)
    };
    let result = propose_versioned_plugin_configuration_snapshot(
        &authority,
        &authorization,
        prior_desired,
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
    let accepted_intent = result.intent().clone();
    publish_result(
        &authority,
        &state_path,
        result,
        &policy_digest,
        prior_desired,
    )?;
    if source_kind == "https_poll" {
        write_cursor(&state_path, cursor, &policy_digest, &accepted_intent)?;
    }
    Ok(())
}

fn revalidation_cursor<'a>(
    previous: Option<&'a State>,
    pending_publication: bool,
    policy_digest: &str,
) -> Option<&'a PluginConfigurationSnapshotCursor> {
    previous
        .filter(|state| {
            !pending_publication && state.policy_digest.as_deref() == Some(policy_digest)
        })
        .and_then(|state| state.cursor.as_ref())
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
    verify_authority_copy(&distribution.join(selected), intent, selected)
}

fn verify_external_host_authority(host_build: &Path, intent: &Path) -> anyhow::Result<()> {
    ensure!(
        fs::symlink_metadata(host_build)?.file_type().is_file(),
        "distribution Host build must be a regular file"
    );
    verify_authority_copy(host_build, intent, ".lenso/host-build.json")
}

fn verify_authority_copy(source: &Path, intent: &Path, selected: &str) -> anyhow::Result<()> {
    for path in [".lenso/host-build.json", ".lenso/host-catalog.json"] {
        if path != selected {
            match fs::symlink_metadata(intent.join(path)) {
                Ok(_) => bail!("runtime intent has a competing Host authority"),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
    let destination = intent.join(selected);
    let original = read_bounded(source, 16 * 1024 * 1024)?;
    if selected == ".lenso/host-build.json" {
        let build: lenso_app_authoring::host_authoring::GeneratedHostBuild =
            serde_json::from_slice(&original).context("invalid distribution Host build")?;
        build.validate()?;
    }
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
    policy_digest: &str,
    prior_desired: Option<&PluginConfigurationSnapshotIntent>,
) -> anyhow::Result<()> {
    match result {
        PluginConfigurationSnapshotReconciliation::Unchanged(intent) => {
            let state = read_state(state_path)?.context("missing accepted configuration state")?;
            if state.policy_digest.as_deref() != Some(policy_digest)
                || state.prior_desired.is_some()
            {
                write_state(state_path, &intent, policy_digest, None)?;
            }
            Ok(())
        }
        PluginConfigurationSnapshotReconciliation::NoRootChange(intent) => {
            write_state(state_path, &intent, policy_digest, None)
        }
        PluginConfigurationSnapshotReconciliation::Proposed { intent, proposal } => {
            write_state(state_path, &intent, policy_digest, prior_desired)?;
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
    state.desired.validate()?;
    if let Some(prior) = &state.prior_desired {
        prior.validate()?;
        ensure!(
            prior.source()? == state.desired.source()?,
            "previous configuration intent belongs to a different authority"
        );
    }
    if let Some(cursor) = &state.cursor {
        ensure!(
            cursor.source()? == state.desired.source()?,
            "configuration cursor source differs from desired authority"
        );
    }
    Ok(Some(state))
}

fn write_state(
    path: &Path,
    desired: &PluginConfigurationSnapshotIntent,
    policy_digest: &str,
    prior_desired: Option<&PluginConfigurationSnapshotIntent>,
) -> anyhow::Result<()> {
    let previous = read_state(path)?;
    let last_activated = previous
        .as_ref()
        .and_then(|state| state.last_activated.clone());
    let cursor = previous.and_then(|state| {
        (state.desired == *desired && state.policy_digest.as_deref() == Some(policy_digest))
            .then_some(state.cursor)
            .flatten()
    });
    persist_state(
        path,
        &State {
            schema: STATE_SCHEMA.into(),
            desired: desired.clone(),
            prior_desired: prior_desired.cloned(),
            policy_digest: Some(policy_digest.to_owned()),
            cursor,
            last_activated,
        },
    )
}

fn write_cursor(
    path: &Path,
    cursor: Option<PluginConfigurationSnapshotCursor>,
    policy_digest: &str,
    expected: &PluginConfigurationSnapshotIntent,
) -> anyhow::Result<()> {
    let mut state = read_state(path)?.context("missing accepted configuration state")?;
    ensure!(
        state.desired == *expected,
        "configuration cursor no longer matches the accepted snapshot"
    );
    state.policy_digest = Some(policy_digest.to_owned());
    state.cursor = cursor;
    persist_state(path, &state)
}

fn persist_state(path: &Path, state: &State) -> anyhow::Result<()> {
    let parent = path.parent().context("configuration state parent")?;
    let mut stage = tempfile::NamedTempFile::new_in(parent)?;
    let bytes = serde_json::to_vec(state)?;
    ensure!(
        u64::try_from(bytes.len())? <= MAX_STATE_BYTES,
        "configuration state exceeds its size limit"
    );
    std::io::Write::write_all(&mut stage, &bytes)?;
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

    use lenso_app_authoring::host_authoring::{GeneratedHostBuild, HostPluginInput};
    use lenso_app_authoring::{VersionedPluginConfiguration, VersionedPluginConfigurationSnapshot};
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
                    "ephemeral": {"type": "string"},
                    "owner": {"type": "string"},
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
    fn external_root_reconciles_only_against_exact_distribution_host_build() {
        let distribution = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        fs::create_dir(distribution.path().join(".lenso")).unwrap();
        fs::create_dir(external.path().join(".lenso")).unwrap();
        let descriptor = PluginDescriptor::new("example.agent", "1.0.0", "agent")
            .with_configuration_schema(serde_json::json!({
                "type": "object",
                "properties": {"greeting": {"type": "string"}},
                "additionalProperties": false
            }));
        let host = GeneratedHostBuild::lower(
            "example.app",
            vec![HostPluginInput {
                descriptor,
                instance: "default".into(),
                configuration: serde_json::json!({}),
                source: "fixture".into(),
            }],
            vec![],
        )
        .unwrap();
        let host_build = distribution.path().join(".lenso/host-build.json");
        fs::write(&host_build, b"{}").unwrap();
        let source = distribution.path().join("snapshot.json");
        let policy = distribution.path().join("policy.json");
        let rejected = sync_external_configuration(external.path(), &host_build, &policy);
        assert!(rejected.is_err());
        assert!(!external.path().join(".lenso/host-build.json").exists());
        fs::write(&host_build, serde_json::to_vec(&host).unwrap()).unwrap();
        fs::write(
            &policy,
            serde_json::to_vec(&serde_json::json!({
                "schema": POLICY_SCHEMA,
                "source_reference": "operator-settings",
                "source": {"type": "file", "path": source},
                "objects": [{
                    "plugin_id": "example.agent",
                    "instance_key": "default",
                    "fields": ["greeting"]
                }]
            }))
            .unwrap(),
        )
        .unwrap();
        snapshot(&source, 1, "greeting = 'first'\n");
        sync_external_configuration(external.path(), &host_build, &policy).unwrap();
        assert_eq!(
            fs::read(external.path().join(".lenso/host-build.json")).unwrap(),
            fs::read(&host_build).unwrap()
        );
        let instance = external.path().join("plugins/example.agent/default.toml");
        assert!(fs::read_to_string(&instance).unwrap().contains("first"));
        snapshot(&source, 2, "greeting = 'second'\n");
        sync_external_configuration(external.path(), &host_build, &policy).unwrap();
        assert!(fs::read_to_string(&instance).unwrap().contains("second"));

        fs::write(external.path().join(".lenso/host-build.json"), b"{}").unwrap();
        snapshot(&source, 3, "greeting = 'third'\n");
        let error = sync_external_configuration(external.path(), &host_build, &policy).unwrap_err();
        assert!(error.to_string().contains("Host authority differs"));
        assert!(fs::read_to_string(&instance).unwrap().contains("second"));
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
        let unconfigured = inspect_status(root.path()).unwrap();
        assert_eq!(unconfigured.state, "no_external_source");
        snapshot(&source, 1, "greeting = 'first'\n");
        sync(root.path(), &policy).unwrap();
        let pending = inspect_status(root.path()).unwrap();
        assert_eq!(pending.state, "pending_activation");
        assert_eq!(pending.desired_revision, Some(1));
        assert_eq!(pending.last_activated_revision, None);
        let mut state = read_state(&state_path).unwrap().unwrap();
        state.last_activated = Some(ActivatedConfiguration {
            revision: state.desired.revision(),
            snapshot_digest: state.desired.snapshot_digest().to_owned(),
            plugin_root_revision: state.desired.candidate_plugin_root_revision().to_owned(),
            policy_digest: state.policy_digest.clone(),
        });
        fs::write(&state_path, serde_json::to_vec(&state).unwrap()).unwrap();
        let activated = inspect_status(root.path()).unwrap();
        assert_eq!(activated.state, "last_activated");
        assert!(!activated.pending_activation);

        snapshot(&source, 2, "greeting = 'second'\n");
        sync(root.path(), &policy).unwrap();
        let current = read_state(&state_path).unwrap().unwrap();
        assert_eq!(current.desired.revision(), 2);
        assert_eq!(current.last_activated.unwrap().revision, 1);
        let pending = inspect_status(root.path()).unwrap();
        assert_eq!(pending.state, "pending_activation");
        assert_eq!(pending.desired_revision, Some(2));
        assert_eq!(pending.last_activated_revision, Some(1));
        let projected = serde_json::to_string(&pending).unwrap();
        assert!(!projected.contains("first"));
        assert!(!projected.contains("second"));
        assert!(!projected.contains("sha256:"));
    }

    #[test]
    fn revision_without_root_change_can_record_a_new_ready_generation() {
        let (root, source, policy) = fixture();
        snapshot(&source, 1, "greeting = 'first'\n");
        sync(root.path(), &policy).unwrap();
        let first = desired_root_revision(root.path()).unwrap().unwrap();
        record_distribution_activation(root.path(), &first).unwrap();

        snapshot(&source, 2, "greeting = 'first'\n");
        sync(root.path(), &policy).unwrap();
        let second = desired_root_revision(root.path()).unwrap().unwrap();
        assert_eq!(second, first);
        let pending = inspect_status(root.path()).unwrap();
        assert_eq!(pending.desired_revision, Some(2));
        assert_eq!(pending.last_activated_revision, Some(1));
        record_distribution_activation(root.path(), &second).unwrap();
        let activated = inspect_status(root.path()).unwrap();
        assert_eq!(activated.last_activated_revision, Some(2));
        assert!(!activated.pending_activation);
    }

    #[test]
    fn etag_cursor_is_retained_only_for_the_same_accepted_snapshot() {
        let (root, source, policy) = fixture();
        let state_path = root.path().join("intent/.lenso").join(STATE_FILE);
        snapshot(&source, 1, "greeting = 'first'\n");
        sync(root.path(), &policy).unwrap();
        let cursor: PluginConfigurationSnapshotCursor = serde_json::from_value(serde_json::json!({
            "endpoint": "https://configuration.example/snapshot",
            "source_kind": "file_snapshot",
            "source_reference": "development",
            "etag": "\"revision-1\""
        }))
        .unwrap();
        let digest = digest_policy(&fs::read(&policy).unwrap());
        let desired = read_state(&state_path).unwrap().unwrap().desired;
        write_cursor(&state_path, Some(cursor.clone()), &digest, &desired).unwrap();
        write_state(&state_path, &desired, &digest, None).unwrap();
        assert_eq!(
            read_state(&state_path).unwrap().unwrap().cursor,
            Some(cursor)
        );
        let accepted = read_state(&state_path).unwrap().unwrap();
        assert!(revalidation_cursor(Some(&accepted), false, &digest).is_some());
        assert!(revalidation_cursor(Some(&accepted), true, &digest).is_none());
        assert!(revalidation_cursor(Some(&accepted), false, "sha256:changed").is_none());

        snapshot(&source, 2, "greeting = 'second'\n");
        sync(root.path(), &policy).unwrap();
        let current = read_state(&state_path).unwrap().unwrap();
        assert_eq!(current.desired.revision(), 2);
        assert!(current.cursor.is_none());
        assert!(write_cursor(&state_path, None, &digest, &desired).is_err());
    }

    #[test]
    fn cursor_from_a_different_source_is_rejected() {
        let (root, source, policy) = fixture();
        let state_path = root.path().join("intent/.lenso").join(STATE_FILE);
        snapshot(&source, 1, "greeting = 'first'\n");
        sync(root.path(), &policy).unwrap();
        let mut state = read_state(&state_path).unwrap().unwrap();
        state.cursor = Some(
            serde_json::from_value(serde_json::json!({
                "endpoint": "https://configuration.example/snapshot",
                "source_kind": "https_poll",
                "source_reference": "other",
                "etag": "\"revision-1\""
            }))
            .unwrap(),
        );
        persist_state(&state_path, &state).unwrap();
        assert!(
            read_state(&state_path)
                .unwrap_err()
                .to_string()
                .contains("cursor source")
        );
    }

    #[test]
    fn https_reconciliation_handles_304_outage_and_policy_reauthorization() {
        let (root, _source, policy) = fixture();
        let url = "https://configuration.example/snapshot";
        let policy_document = |field: &str| {
            serde_json::json!({
                "schema": POLICY_SCHEMA,
                "source_reference": "development",
                "source": {"type": "https", "url": url, "admitted_origins": ["https://configuration.example/"]},
                "objects": [{"plugin_id": "example.agent", "instance_key": "default", "fields": [field]}]
            })
        };
        let approved_policy = serde_json::to_vec(&policy_document("greeting")).unwrap();
        fs::write(&policy, &approved_policy).unwrap();
        let identity =
            PluginConfigurationAuthoritySource::new("https_poll", "development").unwrap();
        let cursor = |etag: &str| -> PluginConfigurationSnapshotCursor {
            serde_json::from_value(serde_json::json!({
                "endpoint": url,
                "source_kind": "https_poll",
                "source_reference": "development",
                "etag": etag
            }))
            .unwrap()
        };
        let snapshot = |revision: u64, greeting: &str| {
            VersionedPluginConfigurationSnapshot::new(
                identity.clone(),
                revision,
                [VersionedPluginConfiguration::new(
                    "example.agent",
                    "default",
                    format!("greeting = {greeting:?}\n"),
                )],
            )
            .unwrap()
        };
        let first = cursor("\"revision-1\"");
        sync_with_https_poll(root.path(), &policy, |source, previous| {
            assert_eq!(source.url().as_str(), url);
            assert!(previous.is_none());
            Ok(PluginConfigurationSnapshotPoll::Updated {
                snapshot: snapshot(1, "first"),
                cursor: Some(first.clone()),
            })
        })
        .unwrap();
        let state_path = root.path().join("intent/.lenso").join(STATE_FILE);
        let current = read_state(&state_path).unwrap().unwrap();
        assert_eq!(current.desired.revision(), 1);
        assert_eq!(current.cursor, Some(first.clone()));
        assert!(
            fs::read_to_string(
                root.path()
                    .join("intent/plugins/example.agent/default.toml")
            )
            .unwrap()
            .contains("first")
        );

        sync_with_https_poll(root.path(), &policy, |_, previous| {
            assert_eq!(previous, Some(&first));
            Ok(PluginConfigurationSnapshotPoll::NotModified {
                cursor: first.clone(),
            })
        })
        .unwrap();
        assert!(
            sync_with_https_poll(root.path(), &policy, |_, previous| {
                assert_eq!(previous, Some(&first));
                Err(anyhow::anyhow!("configuration source offline"))
            })
            .is_err()
        );
        assert_eq!(
            read_state(&state_path).unwrap().unwrap().desired.revision(),
            1
        );

        fs::write(
            &policy,
            serde_json::to_vec(&policy_document("token")).unwrap(),
        )
        .unwrap();
        let rejected = sync_with_https_poll(root.path(), &policy, |_, previous| {
            assert!(
                previous.is_none(),
                "a changed Host policy must force a full fetch"
            );
            Ok(PluginConfigurationSnapshotPoll::Updated {
                snapshot: snapshot(2, "unauthorized"),
                cursor: Some(cursor("\"revision-2\"")),
            })
        });
        assert!(
            rejected
                .unwrap_err()
                .to_string()
                .contains("authorized scope")
        );
        assert_eq!(
            read_state(&state_path).unwrap().unwrap().desired.revision(),
            1
        );

        fs::write(&policy, approved_policy).unwrap();
        let second = cursor("\"revision-2\"");
        sync_with_https_poll(root.path(), &policy, |_, previous| {
            assert_eq!(previous, Some(&first));
            Ok(PluginConfigurationSnapshotPoll::Updated {
                snapshot: snapshot(2, "second"),
                cursor: Some(second.clone()),
            })
        })
        .unwrap();
        let current = read_state(&state_path).unwrap().unwrap();
        assert_eq!(current.desired.revision(), 2);
        assert_eq!(current.cursor, Some(second));
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
    fn narrower_policy_restores_app_owned_values_and_removes_old_source_values() {
        let (root, source, policy) = fixture();
        let configuration = root
            .path()
            .join("intent/plugins/example.agent/default.toml");
        fs::create_dir_all(configuration.parent().unwrap()).unwrap();
        fs::write(&configuration, "greeting = 'app'\nowner = 'keep'\n").unwrap();

        fs::write(
            &policy,
            serde_json::to_vec(&serde_json::json!({
                "schema": POLICY_SCHEMA,
                "source_reference": "development",
                "source": {"type": "file", "path": source},
                "objects": [{"plugin_id": "example.agent", "instance_key": "default", "fields": ["greeting", "ephemeral"]}]
            }))
            .unwrap(),
        )
        .unwrap();
        snapshot(
            &source,
            1,
            "greeting = 'external'\nephemeral = 'source-only'\n",
        );
        sync(root.path(), &policy).unwrap();
        let first: toml::Table =
            toml::from_str(&fs::read_to_string(&configuration).unwrap()).unwrap();
        assert_eq!(first["greeting"].as_str(), Some("external"));
        assert_eq!(first["ephemeral"].as_str(), Some("source-only"));
        assert_eq!(first["owner"].as_str(), Some("keep"));
        assert!(
            !fs::read_to_string(root.path().join("intent/.lenso").join(STATE_FILE))
                .unwrap()
                .contains("keep")
        );

        fs::write(
            &policy,
            serde_json::to_vec(&serde_json::json!({
                "schema": POLICY_SCHEMA,
                "source_reference": "development",
                "source": {"type": "file", "path": source},
                "objects": [{"plugin_id": "example.agent", "instance_key": "default", "fields": ["token"]}]
            }))
            .unwrap(),
        )
        .unwrap();
        snapshot(&source, 2, "token = { secret_ref = 'credential' }\n");
        sync(root.path(), &policy).unwrap();
        let second: toml::Table =
            toml::from_str(&fs::read_to_string(&configuration).unwrap()).unwrap();
        assert_eq!(second["greeting"].as_str(), Some("app"));
        assert!(!second.contains_key("ephemeral"));
        assert_eq!(second["owner"].as_str(), Some("keep"));
        assert_eq!(second["token"]["secret_ref"].as_str(), Some("credential"));
        assert_eq!(
            inspect_status(root.path()).unwrap().desired_revision,
            Some(2)
        );
    }

    #[test]
    fn policy_only_narrowing_blocks_a_new_start_when_the_source_still_sends_revoked_fields() {
        let (root, source, policy) = fixture();
        snapshot(&source, 1, "greeting = 'external'\n");
        sync(root.path(), &policy).unwrap();
        let state_path = root.path().join("intent/.lenso").join(STATE_FILE);
        let first = read_state(&state_path).unwrap().unwrap();

        fs::write(
            &policy,
            serde_json::to_vec(&serde_json::json!({
                "schema": POLICY_SCHEMA,
                "source_reference": "development",
                "source": {"type": "file", "path": source},
                "objects": [{"plugin_id": "example.agent", "instance_key": "default", "fields": ["token"]}]
            }))
            .unwrap(),
        )
        .unwrap();
        let error = require_or_sync(root.path(), Some(&policy)).unwrap_err();
        assert!(error.to_string().contains("authorized scope"));
        let after = read_state(&state_path).unwrap().unwrap();
        assert_eq!(after.desired, first.desired);
        assert_eq!(after.last_activated.as_ref().map(|v| v.revision), None);
    }

    #[test]
    fn same_revision_reauthorization_updates_desired_policy_but_not_the_active_receipt() {
        let (root, source, policy) = fixture();
        snapshot(&source, 1, "greeting = 'external'\n");
        sync(root.path(), &policy).unwrap();
        let revision = desired_root_revision(root.path()).unwrap().unwrap();
        record_distribution_activation(root.path(), &revision).unwrap();
        let state_path = root.path().join("intent/.lenso").join(STATE_FILE);
        let original = read_state(&state_path).unwrap().unwrap();

        fs::write(
            &policy,
            serde_json::to_vec(&serde_json::json!({
                "schema": POLICY_SCHEMA,
                "source_reference": "development",
                "source": {"type": "file", "path": source},
                "objects": [{"plugin_id": "example.agent", "instance_key": "default", "fields": ["greeting", "token"]}]
            }))
            .unwrap(),
        )
        .unwrap();
        sync(root.path(), &policy).unwrap();
        let updated = read_state(&state_path).unwrap().unwrap();
        assert_eq!(updated.desired, original.desired);
        assert_eq!(
            updated.policy_digest,
            Some(digest_policy(&fs::read(&policy).unwrap()))
        );
        assert_eq!(
            updated.last_activated.unwrap().policy_digest,
            original.policy_digest
        );
        let pending = inspect_status(root.path()).unwrap();
        assert_eq!(pending.state, "pending_activation");
        assert!(pending.pending_activation);
        assert!(policy_changed_since_active(root.path(), &policy).unwrap());
        record_distribution_activation(root.path(), &revision).unwrap();
        let activated = inspect_status(root.path()).unwrap();
        assert_eq!(activated.state, "last_activated");
        assert!(!activated.pending_activation);
    }

    #[test]
    fn concurrent_app_owned_edit_is_not_replaced_by_a_stale_source_base() {
        let (root, source, policy) = fixture();
        let configuration = root
            .path()
            .join("intent/plugins/example.agent/default.toml");
        fs::create_dir_all(configuration.parent().unwrap()).unwrap();
        fs::write(&configuration, "owner = 'first'\n").unwrap();
        snapshot(&source, 1, "greeting = 'external'\n");
        sync(root.path(), &policy).unwrap();

        fs::write(
            &configuration,
            "greeting = 'external'\nowner = 'new-app-value'\n",
        )
        .unwrap();
        snapshot(&source, 2, "greeting = 'second'\n");
        let error = sync(root.path(), &policy).unwrap_err();
        assert!(error.to_string().contains("Plugin Root no longer matches"));
        let current: toml::Table =
            toml::from_str(&fs::read_to_string(&configuration).unwrap()).unwrap();
        assert_eq!(current["owner"].as_str(), Some("new-app-value"));
        assert_eq!(current["greeting"].as_str(), Some("external"));
        assert_eq!(
            inspect_status(root.path()).unwrap_err().to_string(),
            error.to_string()
        );
    }

    #[test]
    fn legacy_intent_without_provenance_fails_closed_before_replacement() {
        let (root, source, policy) = fixture();
        snapshot(&source, 1, "greeting = 'external'\n");
        sync(root.path(), &policy).unwrap();
        let state_path = root.path().join("intent/.lenso").join(STATE_FILE);
        let mut state: serde_json::Value =
            serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
        state["desired"]
            .as_object_mut()
            .unwrap()
            .remove("displaced_fields");
        fs::write(&state_path, serde_json::to_vec(&state).unwrap()).unwrap();

        snapshot(&source, 2, "greeting = 'second'\n");
        let error = sync(root.path(), &policy).unwrap_err();
        assert!(error.to_string().contains("lacks the App-owned base"));
        let raw: serde_json::Value =
            serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
        assert_eq!(raw["desired"]["revision"], 1);
        assert!(
            fs::read_to_string(
                root.path()
                    .join("intent/plugins/example.agent/default.toml")
            )
            .unwrap()
            .contains("external")
        );
    }

    #[test]
    fn old_configuration_state_schema_requires_a_new_distribution() {
        let (root, source, policy) = fixture();
        snapshot(&source, 1, "greeting = 'external'\n");
        sync(root.path(), &policy).unwrap();
        let state_path = root.path().join("intent/.lenso").join(STATE_FILE);
        let mut state: serde_json::Value =
            serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
        state["schema"] = "lenso.configuration-source-state.v1".into();
        fs::write(&state_path, serde_json::to_vec(&state).unwrap()).unwrap();

        let error = require_or_sync(root.path(), Some(&policy)).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unsupported configuration source state")
        );
        assert!(
            fs::read_to_string(
                root.path()
                    .join("intent/plugins/example.agent/default.toml")
            )
            .unwrap()
            .contains("external")
        );
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
            &digest_policy(&fs::read(&policy).unwrap()),
            None,
        )
        .unwrap();
        let status = inspect_status(root.path()).unwrap();
        assert_eq!(status.state, "pending_publication");
        assert!(status.pending_publication);

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

    #[test]
    fn pending_second_revision_recovers_with_the_previous_app_owned_base() {
        let (root, source, policy) = fixture();
        let configuration = root
            .path()
            .join("intent/plugins/example.agent/default.toml");
        fs::create_dir_all(configuration.parent().unwrap()).unwrap();
        fs::write(&configuration, "greeting = 'app'\nowner = 'keep'\n").unwrap();
        snapshot(&source, 1, "greeting = 'external'\n");
        sync(root.path(), &policy).unwrap();
        let state_path = root.path().join("intent/.lenso").join(STATE_FILE);
        let first = read_state(&state_path).unwrap().unwrap().desired;

        snapshot(&source, 2, "token = { secret_ref = 'credential' }\n");
        fs::write(
            &policy,
            serde_json::to_vec(&serde_json::json!({
                "schema": POLICY_SCHEMA,
                "source_reference": "development",
                "source": {"type": "file", "path": source},
                "objects": [{"plugin_id": "example.agent", "instance_key": "default", "fields": ["token"]}]
            }))
            .unwrap(),
        )
        .unwrap();
        let identity =
            PluginConfigurationAuthoritySource::new("file_snapshot", "development").unwrap();
        let authorization = PluginConfigurationSnapshotAuthorization::new(
            identity.clone(),
            [
                PluginConfigurationSnapshotObjectScope::new("example.agent", "default", ["token"])
                    .unwrap(),
            ],
        )
        .unwrap();
        let incoming = FilePluginConfigurationSnapshotSource::new(&source, identity)
            .read()
            .unwrap();
        let authority = LocalPluginRootAuthority::new(root.path().join("intent"));
        let proposal = propose_versioned_plugin_configuration_snapshot(
            &authority,
            &authorization,
            Some(&first),
            &incoming,
        )
        .unwrap();
        write_state(
            &state_path,
            proposal.intent(),
            &digest_policy(&fs::read(&policy).unwrap()),
            Some(&first),
        )
        .unwrap();
        assert!(inspect_status(root.path()).unwrap().pending_publication);

        sync(root.path(), &policy).unwrap();
        let current: toml::Table =
            toml::from_str(&fs::read_to_string(&configuration).unwrap()).unwrap();
        assert_eq!(current["greeting"].as_str(), Some("app"));
        assert_eq!(current["owner"].as_str(), Some("keep"));
        assert_eq!(current["token"]["secret_ref"].as_str(), Some("credential"));
        assert_eq!(
            inspect_status(root.path()).unwrap().desired_revision,
            Some(2)
        );
    }
}
