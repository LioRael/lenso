use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    fs,
    io::Read as _,
    path::{Path, PathBuf},
    str::FromStr as _,
};

use anyhow::{Context as _, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use url::Url;

use crate::{
    MAX_CONFIGURATION_BYTES, PluginConfigurationApplication, PluginConfigurationAuthority,
    PluginConfigurationAuthoritySource, PluginConfigurationProposalStatus,
    PluginRootChangeProposal, PluginRootChangeSet, PluginRootConfigurationChange,
    PluginRootRevision,
    archive_download::{checked_url, public_resolve, restricted_https_agent_builder},
    validate_existing_plugin_id, validate_instance_filename,
};

const SNAPSHOT_SCHEMA: &str = "lenso.plugin-configuration-snapshot.v1";
const SNAPSHOT_DIGEST_SCHEMA: &str = "lenso.plugin-configuration-snapshot-digest.v1";
const MAX_SNAPSHOT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_SNAPSHOT_CONFIGURATIONS: usize = 4_096;

/// One schema-validated Plugin configuration carried by an external snapshot.
///
/// Package fields marked `x-lenso-sensitive` accept secret references rather
/// than raw secret material through the ordinary Host admission path.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VersionedPluginConfiguration {
    plugin_id: String,
    instance_key: String,
    toml: String,
}

impl VersionedPluginConfiguration {
    pub fn new(
        plugin_id: impl Into<String>,
        instance_key: impl Into<String>,
        toml: impl Into<String>,
    ) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            instance_key: instance_key.into(),
            toml: toml.into(),
        }
    }

    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    pub fn instance_key(&self) -> &str {
        &self.instance_key
    }

    pub fn toml(&self) -> &str {
        &self.toml
    }
}

/// Host-issued scope for one external source to update one Plugin Instance.
///
/// Fields name top-level typed configuration fields. Granting one field grants
/// its complete value, including nested object members. The Host constructs
/// these scopes from deployment policy; snapshot documents cannot add scopes.
#[derive(Clone, Debug)]
pub struct PluginConfigurationSnapshotObjectScope {
    plugin_id: String,
    instance_key: String,
    fields: BTreeSet<String>,
}

impl PluginConfigurationSnapshotObjectScope {
    pub fn new(
        plugin_id: impl Into<String>,
        instance_key: impl Into<String>,
        fields: impl IntoIterator<Item = impl Into<String>>,
    ) -> anyhow::Result<Self> {
        let plugin_id = plugin_id.into();
        let instance_key = instance_key.into();
        validate_existing_plugin_id(&plugin_id)?;
        validate_instance_filename(&instance_key)?;
        let fields = fields.into_iter().map(Into::into).collect::<BTreeSet<_>>();
        ensure!(
            !fields.is_empty() && fields.len() <= 256,
            "external configuration scope must contain 1 to 256 fields"
        );
        ensure!(
            fields.iter().all(|field| {
                !field.is_empty() && field.len() <= 256 && !field.chars().any(char::is_control)
            }),
            "external configuration scope contains an invalid field"
        );
        Ok(Self {
            plugin_id,
            instance_key,
            fields,
        })
    }

    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    pub fn instance_key(&self) -> &str {
        &self.instance_key
    }

    pub fn fields(&self) -> &BTreeSet<String> {
        &self.fields
    }
}

/// Explicit Host authorization for one external source and its writable scope.
#[derive(Clone, Debug)]
pub struct PluginConfigurationSnapshotAuthorization {
    source: PluginConfigurationAuthoritySource,
    objects: BTreeMap<(String, String), BTreeSet<String>>,
}

impl PluginConfigurationSnapshotAuthorization {
    pub fn new(
        source: PluginConfigurationAuthoritySource,
        scopes: impl IntoIterator<Item = PluginConfigurationSnapshotObjectScope>,
    ) -> anyhow::Result<Self> {
        let mut objects = BTreeMap::new();
        for scope in scopes {
            ensure!(
                objects
                    .insert((scope.plugin_id, scope.instance_key), scope.fields,)
                    .is_none(),
                "duplicate external configuration object scope"
            );
        }
        ensure!(
            !objects.is_empty() && objects.len() <= MAX_SNAPSHOT_CONFIGURATIONS,
            "external configuration authorization must contain a bounded object scope"
        );
        Ok(Self { source, objects })
    }

    pub const fn source(&self) -> &PluginConfigurationAuthoritySource {
        &self.source
    }

    fn authorize(&self, snapshot: &VersionedPluginConfigurationSnapshot) -> anyhow::Result<()> {
        ensure!(
            self.source == snapshot.source,
            "external configuration source is not authorized"
        );
        for configuration in &snapshot.configurations {
            let fields = self
                .objects
                .get(&(
                    configuration.plugin_id.clone(),
                    configuration.instance_key.clone(),
                ))
                .context("external configuration object is not authorized")?;
            let table: toml::Table = toml::from_str(&configuration.toml)
                .map_err(|_| anyhow::anyhow!("external Plugin configuration TOML is invalid"))?;
            ensure!(
                table.keys().all(|field| fields.contains(field)),
                "external configuration contains a field outside its authorized scope"
            );
        }
        Ok(())
    }
}

/// One immutable external configuration revision from a Host-approved source.
#[derive(Clone, Debug)]
pub struct VersionedPluginConfigurationSnapshot {
    source: PluginConfigurationAuthoritySource,
    revision: u64,
    configurations: Vec<VersionedPluginConfiguration>,
}

impl VersionedPluginConfigurationSnapshot {
    pub fn new(
        source: PluginConfigurationAuthoritySource,
        revision: u64,
        configurations: impl IntoIterator<Item = VersionedPluginConfiguration>,
    ) -> anyhow::Result<Self> {
        let snapshot = Self {
            source,
            revision,
            configurations: configurations.into_iter().collect(),
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub const fn source(&self) -> &PluginConfigurationAuthoritySource {
        &self.source
    }

    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub fn configurations(&self) -> &[VersionedPluginConfiguration] {
        &self.configurations
    }

    fn digest(&self) -> anyhow::Result<String> {
        let mut configurations = self.configurations.iter().collect::<Vec<_>>();
        configurations.sort_by(|left, right| {
            (&left.plugin_id, &left.instance_key, &left.toml).cmp(&(
                &right.plugin_id,
                &right.instance_key,
                &right.toml,
            ))
        });
        let bytes = serde_json::to_vec(&(
            SNAPSHOT_DIGEST_SCHEMA,
            self.source.kind(),
            self.source.reference(),
            self.revision,
            configurations,
        ))
        .context("encode external Plugin configuration snapshot")?;
        Ok(sha256(&bytes))
    }

    fn validate(&self) -> anyhow::Result<()> {
        ensure!(
            self.revision > 0,
            "external configuration revision must be positive"
        );
        ensure!(
            !self.configurations.is_empty(),
            "external configuration snapshot must contain at least one configuration"
        );
        ensure!(
            self.configurations.len() <= MAX_SNAPSHOT_CONFIGURATIONS,
            "external configuration snapshot exceeds {MAX_SNAPSHOT_CONFIGURATIONS} configurations"
        );
        let mut byte_count = 0_u64;
        for configuration in &self.configurations {
            validate_existing_plugin_id(&configuration.plugin_id)?;
            validate_instance_filename(&configuration.instance_key)?;
            let toml_bytes = u64::try_from(configuration.toml.len()).unwrap_or(u64::MAX);
            ensure!(
                toml_bytes <= MAX_CONFIGURATION_BYTES,
                "external Plugin configuration exceeds {MAX_CONFIGURATION_BYTES} bytes"
            );
            byte_count = byte_count
                .checked_add(toml_bytes)
                .and_then(|count| {
                    count.checked_add(
                        u64::try_from(
                            configuration.plugin_id.len() + configuration.instance_key.len(),
                        )
                        .unwrap_or(u64::MAX),
                    )
                })
                .context("external configuration snapshot size overflow")?;
        }
        ensure!(
            byte_count <= MAX_SNAPSHOT_BYTES,
            "external configuration snapshot exceeds {MAX_SNAPSHOT_BYTES} bytes"
        );
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileSnapshotDocument {
    schema: String,
    revision: u64,
    configurations: Vec<VersionedPluginConfiguration>,
}

/// Reads one bounded, regular JSON file as an external versioned snapshot.
///
/// The Host supplies the source identity; the untrusted document cannot name or
/// authorize its own provider.
#[derive(Clone, Debug)]
pub struct FilePluginConfigurationSnapshotSource {
    path: PathBuf,
    source: PluginConfigurationAuthoritySource,
}

impl FilePluginConfigurationSnapshotSource {
    pub fn new(path: impl Into<PathBuf>, source: PluginConfigurationAuthoritySource) -> Self {
        Self {
            path: path.into(),
            source,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub const fn source(&self) -> &PluginConfigurationAuthoritySource {
        &self.source
    }

    pub fn read(&self) -> anyhow::Result<VersionedPluginConfigurationSnapshot> {
        let file = open_regular_snapshot(&self.path)?;
        let metadata = file
            .metadata()
            .with_context(|| format!("inspect configuration snapshot {}", self.path.display()))?;
        ensure!(
            metadata.len() <= MAX_SNAPSHOT_BYTES,
            "configuration snapshot exceeds {MAX_SNAPSHOT_BYTES} bytes"
        );
        let mut bytes = Vec::new();
        file.take(MAX_SNAPSHOT_BYTES + 1).read_to_end(&mut bytes)?;
        ensure!(
            u64::try_from(bytes.len()).unwrap_or(u64::MAX) <= MAX_SNAPSHOT_BYTES,
            "configuration snapshot exceeds {MAX_SNAPSHOT_BYTES} bytes"
        );
        let document: FileSnapshotDocument =
            serde_json::from_slice(&bytes).context("parse configuration snapshot JSON")?;
        snapshot_from_document(self.source.clone(), document)
    }
}

fn snapshot_from_document(
    source: PluginConfigurationAuthoritySource,
    document: FileSnapshotDocument,
) -> anyhow::Result<VersionedPluginConfigurationSnapshot> {
    ensure!(
        document.schema == SNAPSHOT_SCHEMA,
        "unsupported configuration snapshot schema"
    );
    VersionedPluginConfigurationSnapshot::new(source, document.revision, document.configurations)
}

#[cfg(unix)]
fn open_regular_snapshot(path: &Path) -> anyhow::Result<fs::File> {
    use rustix::fs::{Mode, OFlags};

    let descriptor = rustix::fs::open(
        path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .with_context(|| format!("open regular configuration snapshot {}", path.display()))?;
    let file = fs::File::from(descriptor);
    ensure!(
        file.metadata()?.file_type().is_file(),
        "configuration snapshot must be a regular file: {}",
        path.display()
    );
    Ok(file)
}

/// One explicitly admitted HTTPS endpoint for polling versioned snapshots.
///
/// Production polling rejects redirects, proxies, credentials, non-HTTPS URLs,
/// and private or ambiguous DNS results. The document still passes through the
/// same Host authorization and proposal path as file snapshots.
#[derive(Clone, Debug)]
pub struct HttpsPluginConfigurationSnapshotSource {
    url: Url,
    source: PluginConfigurationAuthoritySource,
    admitted_origins: BTreeSet<String>,
}

impl HttpsPluginConfigurationSnapshotSource {
    pub fn new(
        url: &str,
        source: PluginConfigurationAuthoritySource,
        admitted_origins: &[String],
    ) -> anyhow::Result<Self> {
        ensure!(
            !admitted_origins.is_empty() && admitted_origins.len() <= 16,
            "expected 1 to 16 configuration snapshot origins"
        );
        let mut origins = BTreeSet::new();
        for origin in admitted_origins {
            let origin = checked_url(origin, "configuration snapshot")?;
            ensure!(
                origin.path() == "/" && origin.query().is_none(),
                "configuration snapshot policy requires an origin, not a path or query"
            );
            origins.insert(origin.origin().ascii_serialization());
        }
        let url = checked_url(url, "configuration snapshot")?;
        ensure!(
            origins.contains(&url.origin().ascii_serialization()),
            "configuration snapshot origin is not admitted by the Host"
        );
        Ok(Self {
            url,
            source,
            admitted_origins: origins,
        })
    }

    pub fn url(&self) -> &Url {
        &self.url
    }

    pub const fn source(&self) -> &PluginConfigurationAuthoritySource {
        &self.source
    }

    pub fn poll(
        &self,
        previous: Option<&PluginConfigurationSnapshotCursor>,
    ) -> anyhow::Result<PluginConfigurationSnapshotPoll> {
        let agent = restricted_https_agent_builder()
            .resolver(public_resolve)
            .build();
        self.poll_with_agent(previous, &agent)
    }

    fn poll_with_agent(
        &self,
        previous: Option<&PluginConfigurationSnapshotCursor>,
        agent: &ureq::Agent,
    ) -> anyhow::Result<PluginConfigurationSnapshotPoll> {
        ensure!(
            self.admitted_origins
                .contains(&self.url.origin().ascii_serialization()),
            "configuration snapshot origin is not admitted by the Host"
        );
        if let Some(cursor) = previous {
            cursor.validate_for(self)?;
        }
        let mut request = agent
            .get(self.url.as_str())
            .set("Accept", "application/json")
            .set("Accept-Encoding", "identity");
        if let Some(cursor) = previous {
            request = request.set("If-None-Match", cursor.etag());
        }
        let response = match request.call() {
            Ok(response) if response.status() == 304 => {
                return not_modified_poll(previous, &response);
            }
            Ok(response) => response,
            Err(ureq::Error::Status(304, response)) => {
                return not_modified_poll(previous, &response);
            }
            Err(_) => bail!("configuration snapshot HTTPS request failed"),
        };
        ensure!(
            response.status() == 200,
            "configuration snapshot response must be HTTP 200 or 304"
        );
        ensure!(
            response
                .header("Content-Encoding")
                .is_none_or(|value| value.eq_ignore_ascii_case("identity")),
            "encoded configuration snapshot responses are not accepted"
        );
        if let Some(length) = response.header("Content-Length") {
            ensure!(
                length
                    .parse::<u64>()
                    .is_ok_and(|length| length <= MAX_SNAPSHOT_BYTES),
                "configuration snapshot response length exceeds its bound"
            );
        }
        let cursor = response
            .header("ETag")
            .map(|value| {
                validate_etag(value)?;
                Ok::<_, anyhow::Error>(PluginConfigurationSnapshotCursor {
                    endpoint: self.url.as_str().to_owned(),
                    source_kind: self.source.kind().to_owned(),
                    source_reference: self.source.reference().to_owned(),
                    etag: value.to_owned(),
                })
            })
            .transpose()?;
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(MAX_SNAPSHOT_BYTES + 1)
            .read_to_end(&mut bytes)
            .context("read configuration snapshot HTTPS response")?;
        ensure!(
            u64::try_from(bytes.len()).unwrap_or(u64::MAX) <= MAX_SNAPSHOT_BYTES,
            "configuration snapshot response exceeds {MAX_SNAPSHOT_BYTES} bytes"
        );
        let document: FileSnapshotDocument = serde_json::from_slice(&bytes)
            .context("parse configuration snapshot HTTPS response")?;
        Ok(PluginConfigurationSnapshotPoll::Updated {
            snapshot: snapshot_from_document(self.source.clone(), document)?,
            cursor,
        })
    }
}

fn not_modified_poll(
    previous: Option<&PluginConfigurationSnapshotCursor>,
    response: &ureq::Response,
) -> anyhow::Result<PluginConfigurationSnapshotPoll> {
    let previous =
        previous.context("configuration snapshot returned HTTP 304 without a previous ETag")?;
    if let Some(returned) = response.header("ETag") {
        validate_etag(returned)?;
        ensure!(
            returned == previous.etag,
            "configuration snapshot HTTP 304 changed its ETag"
        );
    }
    Ok(PluginConfigurationSnapshotPoll::NotModified {
        cursor: previous.clone(),
    })
}

/// Revalidation cursor bound to one exact endpoint and Host source identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginConfigurationSnapshotCursor {
    endpoint: String,
    source_kind: String,
    source_reference: String,
    etag: String,
}

impl PluginConfigurationSnapshotCursor {
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn source(&self) -> anyhow::Result<PluginConfigurationAuthoritySource> {
        PluginConfigurationAuthoritySource::new(&self.source_kind, &self.source_reference)
    }

    pub fn etag(&self) -> &str {
        &self.etag
    }

    fn validate_for(&self, source: &HttpsPluginConfigurationSnapshotSource) -> anyhow::Result<()> {
        validate_etag(&self.etag)?;
        ensure!(
            self.endpoint == source.url.as_str()
                && self.source_kind == source.source.kind()
                && self.source_reference == source.source.reference(),
            "configuration snapshot cursor belongs to a different source"
        );
        Ok(())
    }
}

#[derive(Debug)]
pub enum PluginConfigurationSnapshotPoll {
    NotModified {
        cursor: PluginConfigurationSnapshotCursor,
    },
    Updated {
        snapshot: VersionedPluginConfigurationSnapshot,
        cursor: Option<PluginConfigurationSnapshotCursor>,
    },
}

impl PluginConfigurationSnapshotPoll {
    pub fn etag(&self) -> Option<&str> {
        self.cursor().map(PluginConfigurationSnapshotCursor::etag)
    }

    pub const fn cursor(&self) -> Option<&PluginConfigurationSnapshotCursor> {
        match self {
            Self::NotModified { cursor } => Some(cursor),
            Self::Updated { cursor, .. } => cursor.as_ref(),
        }
    }

    pub const fn snapshot(&self) -> Option<&VersionedPluginConfigurationSnapshot> {
        match self {
            Self::NotModified { .. } => None,
            Self::Updated { snapshot, .. } => Some(snapshot),
        }
    }
}

fn validate_etag(etag: &str) -> anyhow::Result<()> {
    ensure!(
        !etag.is_empty() && etag.len() <= 512 && !etag.chars().any(char::is_control),
        "configuration snapshot ETag is invalid"
    );
    let opaque = etag.strip_prefix("W/").unwrap_or(etag);
    ensure!(
        opaque.len() >= 2
            && opaque.starts_with('"')
            && opaque.ends_with('"')
            && opaque[1..opaque.len() - 1]
                .bytes()
                .all(|byte| byte == 0x21 || (0x23..=0x7e).contains(&byte)),
        "configuration snapshot ETag is invalid"
    );
    Ok(())
}

#[cfg(windows)]
fn open_regular_snapshot(path: &Path) -> anyhow::Result<fs::File> {
    use std::os::windows::fs::OpenOptionsExt as _;

    // FILE_FLAG_OPEN_REPARSE_POINT makes the handle name the link itself
    // instead of following it. The handle metadata check then rejects it.
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .with_context(|| format!("open regular configuration snapshot {}", path.display()))?;
    ensure!(
        file.metadata()?.file_type().is_file(),
        "configuration snapshot must be a regular file: {}",
        path.display()
    );
    Ok(file)
}

#[cfg(not(any(unix, windows)))]
fn open_regular_snapshot(path: &Path) -> anyhow::Result<fs::File> {
    bail!(
        "configuration snapshot no-follow open is unsupported on this platform: {}",
        path.display()
    )
}

/// Durable intent for one reviewed external snapshot publication.
///
/// Persist this before publishing the paired Plugin Root proposal. Its base and
/// candidate revisions let recovery distinguish a proposal that was not yet
/// published from one published just before a crash. App Generation activation
/// and external source acknowledgement remain separate Host state transitions.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginConfigurationSnapshotIntent {
    source_kind: String,
    source_reference: String,
    revision: u64,
    snapshot_digest: String,
    base_plugin_root_revision: String,
    candidate_plugin_root_revision: String,
}

impl PluginConfigurationSnapshotIntent {
    pub fn source(&self) -> anyhow::Result<PluginConfigurationAuthoritySource> {
        PluginConfigurationAuthoritySource::new(&self.source_kind, &self.source_reference)
    }

    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub fn snapshot_digest(&self) -> &str {
        &self.snapshot_digest
    }

    pub fn base_plugin_root_revision(&self) -> &str {
        &self.base_plugin_root_revision
    }

    pub fn candidate_plugin_root_revision(&self) -> &str {
        &self.candidate_plugin_root_revision
    }

    fn validate(&self) -> anyhow::Result<()> {
        PluginConfigurationAuthoritySource::new(&self.source_kind, &self.source_reference)?;
        ensure!(
            self.revision > 0,
            "external configuration intent revision must be positive"
        );
        validate_sha256(&self.snapshot_digest, "snapshot digest")?;
        PluginRootRevision::from_str(&self.base_plugin_root_revision)?;
        PluginRootRevision::from_str(&self.candidate_plugin_root_revision)?;
        Ok(())
    }

    pub fn publication_state(
        &self,
        current: &PluginRootRevision,
    ) -> anyhow::Result<PluginConfigurationSnapshotPublicationState> {
        self.validate()?;
        if self.base_plugin_root_revision == self.candidate_plugin_root_revision
            && current.as_str() == self.base_plugin_root_revision
        {
            return Ok(PluginConfigurationSnapshotPublicationState::NoRootChange);
        }
        if current.as_str() == self.base_plugin_root_revision {
            return Ok(PluginConfigurationSnapshotPublicationState::AwaitingPublication);
        }
        if current.as_str() == self.candidate_plugin_root_revision {
            return Ok(PluginConfigurationSnapshotPublicationState::Published);
        }
        bail!("Plugin Root no longer matches the external configuration intent")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginConfigurationSnapshotPublicationState {
    AwaitingPublication,
    Published,
    NoRootChange,
}

/// Result of reviewing one external snapshot through the ordinary authority.
#[derive(Debug)]
pub enum PluginConfigurationSnapshotReconciliation {
    Unchanged(PluginConfigurationSnapshotIntent),
    NoRootChange(PluginConfigurationSnapshotIntent),
    Proposed {
        intent: PluginConfigurationSnapshotIntent,
        proposal: Box<PluginRootChangeProposal>,
    },
}

impl PluginConfigurationSnapshotReconciliation {
    pub const fn intent(&self) -> &PluginConfigurationSnapshotIntent {
        match self {
            Self::Unchanged(intent)
            | Self::NoRootChange(intent)
            | Self::Proposed { intent, .. } => intent,
        }
    }

    pub fn proposal(&self) -> Option<&PluginRootChangeProposal> {
        match self {
            Self::Unchanged(_) | Self::NoRootChange(_) => None,
            Self::Proposed { proposal, .. } => Some(proposal.as_ref()),
        }
    }
}

/// Reviews an external revision through the same typed proposal and Host
/// admission path used by local Plugin Root edits, without publishing it.
///
/// Persist the returned intent before calling `publish_changes` with the paired
/// proposal. This ordering closes the crash gap between Root publication and
/// durable recovery evidence.
pub fn propose_versioned_plugin_configuration_snapshot(
    authority: &dyn PluginConfigurationAuthority,
    authorization: &PluginConfigurationSnapshotAuthorization,
    previous: Option<&PluginConfigurationSnapshotIntent>,
    snapshot: &VersionedPluginConfigurationSnapshot,
) -> anyhow::Result<PluginConfigurationSnapshotReconciliation> {
    snapshot.validate()?;
    authorization.authorize(snapshot)?;
    let snapshot_digest = snapshot.digest()?;
    let current = authority.inspect()?;

    if let Some(previous) = previous {
        previous.validate()?;
        ensure!(
            previous.source_kind == snapshot.source.kind()
                && previous.source_reference == snapshot.source.reference(),
            "external configuration source changed without a new Host authority"
        );
        ensure!(
            matches!(
                previous.publication_state(current.revision())?,
                PluginConfigurationSnapshotPublicationState::Published
                    | PluginConfigurationSnapshotPublicationState::NoRootChange
            ),
            "previous external configuration intent has not been published"
        );
        match snapshot.revision.cmp(&previous.revision) {
            Ordering::Less => bail!(
                "stale external configuration revision {} is older than {}",
                snapshot.revision,
                previous.revision
            ),
            Ordering::Equal => {
                ensure!(
                    snapshot_digest == previous.snapshot_digest,
                    "external configuration revision was reused with different content"
                );
                return Ok(PluginConfigurationSnapshotReconciliation::Unchanged(
                    previous.clone(),
                ));
            }
            Ordering::Greater => {}
        }
    }

    let mut changes = PluginRootChangeSet::new();
    for configuration in &snapshot.configurations {
        let mut merged = current_root_configuration(
            &current,
            &configuration.plugin_id,
            &configuration.instance_key,
        )?;
        let incoming: toml::Table = toml::from_str(&configuration.toml)
            .map_err(|_| anyhow::anyhow!("external Plugin configuration TOML is invalid"))?;
        for (field, value) in incoming {
            merged.insert(field, value);
        }
        let toml = toml::to_string(&merged).context("encode merged Plugin configuration")?;
        changes = changes.with_configuration(PluginRootConfigurationChange::new(
            &configuration.plugin_id,
            &configuration.instance_key,
            toml.into_bytes(),
        ));
    }
    let proposal = authority.propose_changes(current.revision(), changes)?;
    if proposal.status() != PluginConfigurationProposalStatus::Ready
        || proposal.application() == PluginConfigurationApplication::Blocked
    {
        let detail = proposal
            .diagnostics()
            .first()
            .map_or("candidate did not pass the Ready Gate", |diagnostic| {
                diagnostic.detail()
            });
        bail!("external configuration snapshot was rejected: {detail}");
    }
    let intent = PluginConfigurationSnapshotIntent {
        source_kind: snapshot.source.kind().to_owned(),
        source_reference: snapshot.source.reference().to_owned(),
        revision: snapshot.revision,
        snapshot_digest,
        base_plugin_root_revision: proposal.base_revision().as_str().to_owned(),
        candidate_plugin_root_revision: proposal.candidate_revision().as_str().to_owned(),
    };
    if intent.base_plugin_root_revision == intent.candidate_plugin_root_revision {
        return Ok(PluginConfigurationSnapshotReconciliation::NoRootChange(
            intent,
        ));
    }
    Ok(PluginConfigurationSnapshotReconciliation::Proposed {
        intent,
        proposal: Box::new(proposal),
    })
}

fn current_root_configuration(
    current: &crate::PluginRootAuthoringState,
    plugin_id: &str,
    instance_key: &str,
) -> anyhow::Result<toml::Table> {
    let source = current
        .plugins()
        .iter()
        .find(|plugin| plugin.plugin_id() == plugin_id)
        .and_then(|plugin| {
            plugin.instances().iter().find(|instance| {
                instance.id().plugin_id() == plugin_id
                    && instance.id().instance_key() == instance_key
            })
        })
        .and_then(crate::PluginInstanceAuthoringState::root_configuration_toml);
    source.map_or_else(
        || Ok(toml::Table::new()),
        |source| {
            toml::from_str(source)
                .map_err(|_| anyhow::anyhow!("current Plugin configuration TOML is invalid"))
        },
    )
}

fn sha256(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity("sha256:".len() + 64);
    encoded.push_str("sha256:");
    for byte in Sha256::digest(bytes) {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

fn validate_sha256(value: &str, subject: &str) -> anyhow::Result<()> {
    let digest = value
        .strip_prefix("sha256:")
        .with_context(|| format!("{subject} must use SHA-256"))?;
    ensure!(
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "{subject} is invalid"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::Write as _,
        net::TcpListener,
        sync::{Arc, Mutex},
        thread,
        time::Duration,
    };

    use lenso_app_plan::authoring::{
        HostDefaultPlugin, HostPluginRelease, HostSlot, PluginDescriptor,
    };

    use super::*;
    use crate::{HOST_CATALOG, LocalPluginRootAuthority};

    fn fixture_root() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(".lenso")).unwrap();
        let descriptor = PluginDescriptor::new("example.agent", "1.0.0", "agent")
            .with_configuration_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "greeting": { "type": "string" },
                    "token": { "x-lenso-sensitive": true }
                },
                "additionalProperties": false
            }));
        let host = lenso_app_plan::authoring::HostCatalog::new(
            [HostSlot::one("agent")],
            [HostPluginRelease::new(descriptor)],
            [HostDefaultPlugin::new("example.agent", "default")],
        );
        fs::write(
            root.path().join(HOST_CATALOG),
            serde_json::to_vec(&host).unwrap(),
        )
        .unwrap();
        root
    }

    fn source(path: &Path) -> FilePluginConfigurationSnapshotSource {
        FilePluginConfigurationSnapshotSource::new(path, source_identity())
    }

    fn source_identity() -> PluginConfigurationAuthoritySource {
        PluginConfigurationAuthoritySource::new("file_snapshot", "development").unwrap()
    }

    fn authorization() -> PluginConfigurationSnapshotAuthorization {
        PluginConfigurationSnapshotAuthorization::new(
            source_identity(),
            [PluginConfigurationSnapshotObjectScope::new(
                "example.agent",
                "default",
                ["greeting", "token"],
            )
            .unwrap()],
        )
        .unwrap()
    }

    fn write_snapshot(path: &Path, revision: u64, greeting: &str) {
        fs::write(
            path,
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema": SNAPSHOT_SCHEMA,
                "revision": revision,
                "configurations": [{
                    "plugin_id": "example.agent",
                    "instance_key": "default",
                    "toml": format!("greeting = {greeting:?}\n")
                }]
            }))
            .unwrap(),
        )
        .unwrap();
    }

    struct PollServer {
        url: String,
        agent: ureq::Agent,
        requests: Arc<Mutex<Vec<String>>>,
        worker: thread::JoinHandle<()>,
    }

    fn serve_snapshot_poll(bytes: Vec<u8>) -> PollServer {
        let certificate =
            rcgen::generate_simple_self_signed(vec!["configuration.example".into()]).unwrap();
        let cert = certificate.cert.der().clone();
        let key =
            rustls::pki_types::PrivatePkcs8KeyDer::from(certificate.signing_key.serialize_der());
        let server_config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![cert.clone()], key.into())
        .unwrap();
        let mut roots = rustls::RootCertStore::empty();
        roots.add(cert).unwrap();
        let client_config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let url = format!("https://configuration.example:{}/snapshot", address.port());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&requests);
        let worker = thread::spawn(move || {
            for request_index in 0..2 {
                let (socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                socket
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut stream = rustls::StreamOwned::new(
                    rustls::ServerConnection::new(Arc::new(server_config.clone())).unwrap(),
                    socket,
                );
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") && request.len() < 8_192 {
                    let mut byte = [0];
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                captured
                    .lock()
                    .unwrap()
                    .push(String::from_utf8(request).unwrap());
                if request_index == 0 {
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nETag: \"revision-7\"\r\nConnection: close\r\n\r\n",
                        bytes.len()
                    )
                    .unwrap();
                    stream.write_all(&bytes).unwrap();
                } else {
                    stream
                        .write_all(
                            b"HTTP/1.1 304 Not Modified\r\nETag: \"revision-7\"\r\nConnection: close\r\n\r\n",
                        )
                        .unwrap();
                }
                stream.flush().unwrap();
                stream.conn.send_close_notify();
                let _ = stream.flush();
            }
        });
        let agent = restricted_https_agent_builder()
            .resolver(move |_: &str| Ok(vec![address]))
            .tls_config(Arc::new(client_config))
            .build();
        PollServer {
            url,
            agent,
            requests,
            worker,
        }
    }

    fn publish_snapshot(
        authority: &LocalPluginRootAuthority,
        previous: Option<&PluginConfigurationSnapshotIntent>,
        snapshot: &VersionedPluginConfigurationSnapshot,
    ) -> PluginConfigurationSnapshotIntent {
        let reviewed = propose_versioned_plugin_configuration_snapshot(
            authority,
            &authorization(),
            previous,
            snapshot,
        )
        .unwrap();
        let intent = reviewed.intent().clone();
        assert_eq!(
            intent
                .publication_state(authority.inspect().unwrap().revision())
                .unwrap(),
            PluginConfigurationSnapshotPublicationState::AwaitingPublication
        );
        authority
            .publish_changes(reviewed.proposal().unwrap())
            .unwrap();
        assert_eq!(
            intent
                .publication_state(authority.inspect().unwrap().revision())
                .unwrap(),
            PluginConfigurationSnapshotPublicationState::Published
        );
        intent
    }

    #[test]
    fn file_snapshot_uses_the_existing_proposal_and_publication_path() {
        let root = fixture_root();
        let path = root.path().join("configuration.json");
        write_snapshot(&path, 1, "hello");
        let snapshot = source(&path).read().unwrap();
        let authority = LocalPluginRootAuthority::new(root.path());

        let reviewed = propose_versioned_plugin_configuration_snapshot(
            &authority,
            &authorization(),
            None,
            &snapshot,
        )
        .unwrap();
        let intent = reviewed.intent().clone();

        assert!(reviewed.proposal().is_some());
        assert_eq!(intent.revision(), 1);
        assert_eq!(intent.source().unwrap().kind(), "file_snapshot");
        assert!(
            !root
                .path()
                .join("plugins/example.agent/default.toml")
                .exists()
        );
        assert!(
            propose_versioned_plugin_configuration_snapshot(
                &authority,
                &authorization(),
                Some(&intent),
                &snapshot,
            )
            .unwrap_err()
            .to_string()
            .contains("has not been published")
        );
        authority
            .publish_changes(reviewed.proposal().unwrap())
            .unwrap();
        assert_eq!(
            fs::read_to_string(root.path().join("plugins/example.agent/default.toml")).unwrap(),
            "greeting = \"hello\"\n"
        );
        assert_eq!(
            intent
                .publication_state(authority.inspect().unwrap().revision())
                .unwrap(),
            PluginConfigurationSnapshotPublicationState::Published
        );
        let unchanged = propose_versioned_plugin_configuration_snapshot(
            &authority,
            &authorization(),
            Some(&intent),
            &snapshot,
        )
        .unwrap();
        assert!(unchanged.proposal().is_none());
        assert_eq!(unchanged.intent(), &intent);

        write_snapshot(&path, 2, "hello");
        let same_value = source(&path).read().unwrap();
        let no_root_change = propose_versioned_plugin_configuration_snapshot(
            &authority,
            &authorization(),
            Some(&intent),
            &same_value,
        )
        .unwrap();
        assert!(matches!(
            &no_root_change,
            PluginConfigurationSnapshotReconciliation::NoRootChange(_)
        ));
        assert_eq!(
            no_root_change
                .intent()
                .publication_state(authority.inspect().unwrap().revision())
                .unwrap(),
            PluginConfigurationSnapshotPublicationState::NoRootChange
        );
    }

    #[test]
    fn https_poll_fetches_one_version_and_uses_etag_for_not_modified() {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "schema": SNAPSHOT_SCHEMA,
            "revision": 7,
            "configurations": [{
                "plugin_id": "example.agent",
                "instance_key": "default",
                "toml": "greeting = \"remote\"\n"
            }]
        }))
        .unwrap();
        let server = serve_snapshot_poll(bytes);
        let origin = Url::parse(&server.url)
            .unwrap()
            .origin()
            .ascii_serialization();
        let source = HttpsPluginConfigurationSnapshotSource::new(
            &server.url,
            PluginConfigurationAuthoritySource::new("https_poll", "production").unwrap(),
            &[origin],
        )
        .unwrap();

        let root = fixture_root();
        let authority = LocalPluginRootAuthority::new(root.path());
        let initial_root_revision = authority.inspect().unwrap().revision().as_str().to_owned();
        let closed_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let closed_address = closed_listener.local_addr().unwrap();
        drop(closed_listener);
        let failing_agent = restricted_https_agent_builder()
            .resolver(move |_: &str| Ok(vec![closed_address]))
            .timeout(Duration::from_millis(100))
            .build();
        assert!(source.poll_with_agent(None, &failing_agent).is_err());
        assert_eq!(
            authority.inspect().unwrap().revision().as_str(),
            initial_root_revision
        );

        let updated = source.poll_with_agent(None, &server.agent).unwrap();
        assert_eq!(updated.etag(), Some("\"revision-7\""));
        assert_eq!(updated.snapshot().unwrap().revision(), 7);
        let remote_authorization =
            PluginConfigurationSnapshotAuthorization::new(
                source.source().clone(),
                [PluginConfigurationSnapshotObjectScope::new(
                    "example.agent",
                    "default",
                    ["greeting"],
                )
                .unwrap()],
            )
            .unwrap();
        let reviewed = propose_versioned_plugin_configuration_snapshot(
            &authority,
            &remote_authorization,
            None,
            updated.snapshot().unwrap(),
        )
        .unwrap();
        let intent = reviewed.intent().clone();
        authority
            .publish_changes(reviewed.proposal().unwrap())
            .unwrap();
        assert_eq!(
            intent
                .publication_state(authority.inspect().unwrap().revision())
                .unwrap(),
            PluginConfigurationSnapshotPublicationState::Published
        );
        assert_eq!(
            fs::read_to_string(root.path().join("plugins/example.agent/default.toml")).unwrap(),
            "greeting = \"remote\"\n"
        );
        let published_root_revision = authority.inspect().unwrap().revision().as_str().to_owned();
        assert!(
            source
                .poll_with_agent(updated.cursor(), &failing_agent)
                .is_err()
        );
        assert_eq!(
            authority.inspect().unwrap().revision().as_str(),
            published_root_revision
        );
        let other_source = HttpsPluginConfigurationSnapshotSource::new(
            &server.url,
            PluginConfigurationAuthoritySource::new("https_poll", "other").unwrap(),
            &[Url::parse(&server.url)
                .unwrap()
                .origin()
                .ascii_serialization()],
        )
        .unwrap();
        assert!(
            other_source
                .poll_with_agent(updated.cursor(), &failing_agent)
                .unwrap_err()
                .to_string()
                .contains("different source")
        );
        let unchanged = source
            .poll_with_agent(updated.cursor(), &server.agent)
            .unwrap();
        assert!(unchanged.snapshot().is_none());
        assert_eq!(unchanged.etag(), Some("\"revision-7\""));

        server.worker.join().unwrap();
        let requests = server.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(
            requests[1]
                .to_ascii_lowercase()
                .contains("if-none-match: \"revision-7\"")
        );
    }

    #[test]
    fn stale_or_reused_revisions_never_replace_the_active_configuration() {
        let root = fixture_root();
        let path = root.path().join("configuration.json");
        let authority = LocalPluginRootAuthority::new(root.path());
        write_snapshot(&path, 2, "winner");
        let winner = source(&path).read().unwrap();
        let intent = publish_snapshot(&authority, None, &winner);

        write_snapshot(&path, 1, "stale");
        let stale = source(&path).read().unwrap();
        assert!(
            propose_versioned_plugin_configuration_snapshot(
                &authority,
                &authorization(),
                Some(&intent),
                &stale,
            )
            .unwrap_err()
            .to_string()
            .contains("stale external configuration revision")
        );

        write_snapshot(&path, 2, "changed");
        let changed = source(&path).read().unwrap();
        assert!(
            propose_versioned_plugin_configuration_snapshot(
                &authority,
                &authorization(),
                Some(&intent),
                &changed,
            )
            .unwrap_err()
            .to_string()
            .contains("reused with different content")
        );
        assert_eq!(
            fs::read_to_string(root.path().join("plugins/example.agent/default.toml")).unwrap(),
            "greeting = \"winner\"\n"
        );
    }

    #[test]
    fn invalid_values_and_root_drift_fail_without_false_acknowledgement() {
        let root = fixture_root();
        let path = root.path().join("configuration.json");
        let authority = LocalPluginRootAuthority::new(root.path());
        write_snapshot(&path, 1, "winner");
        let snapshot = source(&path).read().unwrap();
        let intent = publish_snapshot(&authority, None, &snapshot);

        fs::write(
            root.path().join("plugins/example.agent/default.toml"),
            "greeting = \"manual\"\n",
        )
        .unwrap();
        write_snapshot(&path, 2, "newer");
        let newer = source(&path).read().unwrap();
        assert!(
            propose_versioned_plugin_configuration_snapshot(
                &authority,
                &authorization(),
                Some(&intent),
                &newer,
            )
            .unwrap_err()
            .to_string()
            .contains("Plugin Root no longer matches")
        );
        assert_eq!(
            fs::read_to_string(root.path().join("plugins/example.agent/default.toml")).unwrap(),
            "greeting = \"manual\"\n"
        );

        fs::write(
            root.path().join("plugins/example.agent/default.toml"),
            "greeting = \"winner\"\n",
        )
        .unwrap();
        write_snapshot(&path, 2, "valid");
        let mut invalid = source(&path).read().unwrap();
        invalid.configurations[0].toml = "unknown = true\n".to_owned();
        assert!(
            propose_versioned_plugin_configuration_snapshot(
                &authority,
                &authorization(),
                Some(&intent),
                &invalid,
            )
            .is_err()
        );
        assert_eq!(
            fs::read_to_string(root.path().join("plugins/example.agent/default.toml")).unwrap(),
            "greeting = \"winner\"\n"
        );
    }

    #[test]
    fn file_source_rejects_document_owned_identity_and_non_regular_inputs() {
        let root = fixture_root();
        let path = root.path().join("configuration.json");
        fs::write(
            &path,
            br#"{"schema":"lenso.plugin-configuration-snapshot.v1","source":{"kind":"self_approved"},"revision":1,"configurations":[]}"#,
        )
        .unwrap();
        assert!(
            source(&path)
                .read()
                .unwrap_err()
                .to_string()
                .contains("parse")
        );

        assert!(
            source(root.path())
                .read()
                .unwrap_err()
                .to_string()
                .contains("regular file")
        );

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&path, root.path().join("configuration-link.json")).unwrap();
            assert!(
                source(&root.path().join("configuration-link.json"))
                    .read()
                    .is_err()
            );
        }
    }

    #[test]
    fn host_scope_rejects_other_objects_and_fields_before_publication() {
        let allowed =
            PluginConfigurationSnapshotAuthorization::new(
                source_identity(),
                [PluginConfigurationSnapshotObjectScope::new(
                    "example.agent",
                    "default",
                    ["greeting"],
                )
                .unwrap()],
            )
            .unwrap();
        let other_object = VersionedPluginConfigurationSnapshot::new(
            source_identity(),
            1,
            [VersionedPluginConfiguration::new(
                "example.agent",
                "other",
                "greeting = \"no\"\n",
            )],
        )
        .unwrap();
        assert!(allowed.authorize(&other_object).is_err());

        let other_field = VersionedPluginConfigurationSnapshot::new(
            source_identity(),
            1,
            [VersionedPluginConfiguration::new(
                "example.agent",
                "default",
                "token = { secret_ref = \"credential\" }\n",
            )],
        )
        .unwrap();
        assert!(allowed.authorize(&other_field).is_err());
    }

    #[test]
    fn sensitive_schema_rejects_raw_secret_without_echoing_it() {
        let root = fixture_root();
        let authority = LocalPluginRootAuthority::new(root.path());
        let snapshot = VersionedPluginConfigurationSnapshot::new(
            source_identity(),
            1,
            [VersionedPluginConfiguration::new(
                "example.agent",
                "default",
                "token = \"do-not-echo-this-secret\"\n",
            )],
        )
        .unwrap();
        let error = propose_versioned_plugin_configuration_snapshot(
            &authority,
            &authorization(),
            None,
            &snapshot,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("secret_ref"));
        assert!(!error.contains("do-not-echo-this-secret"));
        assert!(
            !root
                .path()
                .join("plugins/example.agent/default.toml")
                .exists()
        );
    }

    #[test]
    fn field_scope_preserves_unowned_existing_fields() {
        let root = fixture_root();
        let configuration_path = root.path().join("plugins/example.agent/default.toml");
        fs::create_dir_all(configuration_path.parent().unwrap()).unwrap();
        fs::write(
            &configuration_path,
            "greeting = \"old\"\ntoken = { secret_ref = \"credential\" }\n",
        )
        .unwrap();
        let authority = LocalPluginRootAuthority::new(root.path());
        let allowed =
            PluginConfigurationSnapshotAuthorization::new(
                source_identity(),
                [PluginConfigurationSnapshotObjectScope::new(
                    "example.agent",
                    "default",
                    ["greeting"],
                )
                .unwrap()],
            )
            .unwrap();
        let snapshot = VersionedPluginConfigurationSnapshot::new(
            source_identity(),
            1,
            [VersionedPluginConfiguration::new(
                "example.agent",
                "default",
                "greeting = \"new\"\n",
            )],
        )
        .unwrap();
        let reviewed =
            propose_versioned_plugin_configuration_snapshot(&authority, &allowed, None, &snapshot)
                .unwrap();
        authority
            .publish_changes(reviewed.proposal().unwrap())
            .unwrap();
        let table: toml::Table =
            toml::from_str(&fs::read_to_string(configuration_path).unwrap()).unwrap();
        assert_eq!(table["greeting"].as_str(), Some("new"));
        assert_eq!(table["token"]["secret_ref"].as_str(), Some("credential"));
    }

    #[test]
    fn malformed_external_toml_does_not_echo_its_source() {
        let root = fixture_root();
        let authority = LocalPluginRootAuthority::new(root.path());
        let snapshot = VersionedPluginConfigurationSnapshot::new(
            source_identity(),
            1,
            [VersionedPluginConfiguration::new(
                "example.agent",
                "default",
                "token = \"do-not-echo-this-secret",
            )],
        )
        .unwrap();
        let error = propose_versioned_plugin_configuration_snapshot(
            &authority,
            &authorization(),
            None,
            &snapshot,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("TOML is invalid"));
        assert!(!error.contains("do-not-echo-this-secret"));
    }

    #[test]
    fn in_memory_snapshots_enforce_configuration_and_aggregate_bounds() {
        assert!(
            VersionedPluginConfigurationSnapshot::new(
                source_identity(),
                1,
                [VersionedPluginConfiguration::new(
                    "example.agent",
                    "default",
                    "x".repeat(usize::try_from(MAX_CONFIGURATION_BYTES).unwrap() + 1),
                )],
            )
            .is_err()
        );
        let configurations = (0..65).map(|index| {
            VersionedPluginConfiguration::new(
                "example.agent",
                format!("instance-{index}"),
                "x".repeat(usize::try_from(MAX_CONFIGURATION_BYTES).unwrap()),
            )
        });
        assert!(
            VersionedPluginConfigurationSnapshot::new(source_identity(), 1, configurations)
                .is_err()
        );
    }
}
