//! Review and publish one Plugin Root configuration through its existing authority.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    str::FromStr as _,
    sync::Mutex,
};

use anyhow::{Context as _, ensure};
use lenso_app_authoring::{
    LocalPluginRootAuthority, PluginConfigurationApplication, PluginConfigurationAuthority as _,
    PluginConfigurationProposal, PluginConfigurationProposalStatus,
    PluginConfigurationSourceDigest, PluginRootChangeProposal, PluginRootChangeSet,
    PluginRootRevision, PluginRootSelectionChange,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(super) struct ChangePreview {
    schema_version: u32,
    kind: &'static str,
    proposal_digest: String,
    base_revision: String,
    candidate_revision: String,
    plugin_id: String,
    instance: String,
    status: &'static str,
    application: &'static str,
    changed_fields: Vec<String>,
    diagnostic_codes: Vec<String>,
    values_redacted: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct SelectionPreview {
    schema_version: u32,
    kind: &'static str,
    proposal_digest: String,
    base_revision: String,
    candidate_revision: String,
    plugin_id: String,
    instance: String,
    before_enabled: bool,
    requested_enabled: bool,
    status: &'static str,
    application: &'static str,
    diagnostic_codes: Vec<String>,
}

#[derive(Debug)]
enum StoredProposal {
    Configuration(Box<PluginConfigurationProposal>),
    Selection(Box<PluginRootChangeProposal>),
}

impl StoredProposal {
    fn status(&self) -> PluginConfigurationProposalStatus {
        match self {
            Self::Configuration(proposal) => proposal.status(),
            Self::Selection(proposal) => proposal.status(),
        }
    }

    fn base_revision(&self) -> &PluginRootRevision {
        match self {
            Self::Configuration(proposal) => proposal.base_revision(),
            Self::Selection(proposal) => proposal.base_revision(),
        }
    }

    fn candidate_revision(&self) -> &PluginRootRevision {
        match self {
            Self::Configuration(proposal) => proposal.candidate_revision(),
            Self::Selection(proposal) => proposal.candidate_revision(),
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Configuration(_) => "lenso.mcp-configuration-apply",
            Self::Selection(_) => "lenso.mcp-selection-apply",
        }
    }

    fn publish(&self, authority: &LocalPluginRootAuthority) -> anyhow::Result<serde_json::Value> {
        match self {
            Self::Configuration(proposal) => {
                let publication = authority.publish(proposal)?;
                Ok(serde_json::json!({
                    "base_revision": publication.base_revision().as_str(),
                    "revision": publication.revision().as_str(),
                    "proposal_digest": publication.proposal_digest(),
                }))
            }
            Self::Selection(proposal) => {
                let publication = authority.publish_changes(proposal)?;
                Ok(serde_json::json!({
                    "base_revision": publication.base_revision().as_str(),
                    "revision": publication.revision().as_str(),
                    "proposal_digest": publication.proposal_digest(),
                }))
            }
        }
    }
}

#[derive(Debug)]
struct ApplyRecord {
    proposal_digest: String,
    result: serde_json::Value,
}

#[derive(Debug, Default)]
struct ChangeState {
    proposals: BTreeMap<String, StoredProposal>,
    applies: BTreeMap<String, ApplyRecord>,
}

#[derive(Debug)]
pub(super) struct ChangeController {
    root: PathBuf,
    state: Mutex<ChangeState>,
}

impl ChangeController {
    pub(super) fn new(root: PathBuf) -> Self {
        Self {
            root,
            state: Mutex::new(ChangeState::default()),
        }
    }

    pub(super) fn preview(
        &self,
        base_revision: &str,
        plugin_id: &str,
        instance: &str,
        toml: &str,
    ) -> anyhow::Result<ChangePreview> {
        ensure!(toml.len() <= 256 * 1024, "configuration exceeds 256 KiB");
        let base = PluginRootRevision::from_str(base_revision)?;
        let authority = LocalPluginRootAuthority::new(&self.root);
        let proposal = authority.propose(&base, plugin_id, instance, toml.as_bytes())?;
        let path = self
            .root
            .join("plugins")
            .join(plugin_id)
            .join(format!("{instance}.toml"));
        let previous_bytes = match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_file() => Some(fs::read(&path)?),
            Ok(_) => anyhow::bail!("Plugin configuration source is not a regular file"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error).context("inspect prior Plugin configuration"),
        };
        let observed = PluginConfigurationSourceDigest::for_source(
            plugin_id,
            instance,
            previous_bytes.as_deref(),
        )?;
        ensure!(
            &observed == proposal.base_source_digest(),
            "Plugin configuration changed while preparing the preview"
        );
        let previous = previous_bytes
            .map(String::from_utf8)
            .transpose()?
            .unwrap_or_default();
        let changed_fields = changed_fields(&previous, toml)?;
        ensure!(
            changed_fields.len() <= 1024,
            "configuration diff has too many fields"
        );
        let report = ChangePreview {
            schema_version: 1,
            kind: "lenso.mcp-configuration-preview",
            proposal_digest: proposal.digest().to_owned(),
            base_revision: proposal.base_revision().as_str().to_owned(),
            candidate_revision: proposal.candidate_revision().as_str().to_owned(),
            plugin_id: plugin_id.to_owned(),
            instance: instance.to_owned(),
            status: status(proposal.status()),
            application: application(proposal.application()),
            changed_fields,
            diagnostic_codes: proposal
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().to_owned())
                .collect(),
            values_redacted: true,
        };
        let mut state = self.state.lock().expect("MCP change state lock");
        if !state.proposals.contains_key(proposal.digest()) {
            ensure!(
                state.proposals.len() < 32,
                "MCP proposal history is full; restart the bridge"
            );
            state.proposals.insert(
                proposal.digest().to_owned(),
                StoredProposal::Configuration(Box::new(proposal)),
            );
        }
        Ok(report)
    }

    pub(super) fn preview_selection(
        &self,
        base_revision: &str,
        plugin_id: &str,
        instance: &str,
        enabled: bool,
    ) -> anyhow::Result<SelectionPreview> {
        let base = PluginRootRevision::from_str(base_revision)?;
        let authority = LocalPluginRootAuthority::new(&self.root);
        let proposal = authority.propose_changes(
            &base,
            PluginRootChangeSet::new()
                .with_selection(PluginRootSelectionChange::new(plugin_id, instance, enabled)),
        )?;
        let current = authority.inspect()?;
        ensure!(
            current.revision() == proposal.base_revision(),
            "Plugin Root changed while preparing the selection preview"
        );
        let before_enabled = current
            .plugins()
            .iter()
            .find(|plugin| plugin.plugin_id() == plugin_id)
            .and_then(|plugin| {
                plugin.instances().iter().find(|candidate| {
                    candidate.id().plugin_id() == plugin_id
                        && candidate.id().instance_key() == instance
                })
            })
            .context("Plugin Instance is not in the current Host Catalog or Plugin Root")?
            .is_enabled();
        let report = SelectionPreview {
            schema_version: 1,
            kind: "lenso.mcp-selection-preview",
            proposal_digest: proposal.digest().to_owned(),
            base_revision: proposal.base_revision().as_str().to_owned(),
            candidate_revision: proposal.candidate_revision().as_str().to_owned(),
            plugin_id: plugin_id.to_owned(),
            instance: instance.to_owned(),
            before_enabled,
            requested_enabled: enabled,
            status: status(proposal.status()),
            application: application(proposal.application()),
            diagnostic_codes: proposal
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().to_owned())
                .collect(),
        };
        let mut state = self.state.lock().expect("MCP change state lock");
        if !state.proposals.contains_key(proposal.digest()) {
            ensure!(
                state.proposals.len() < 32,
                "MCP proposal history is full; restart the bridge"
            );
            state.proposals.insert(
                proposal.digest().to_owned(),
                StoredProposal::Selection(Box::new(proposal)),
            );
        }
        Ok(report)
    }

    pub(super) fn apply(
        &self,
        proposal_digest: &str,
        request_id: &str,
    ) -> anyhow::Result<serde_json::Value> {
        ensure!(
            !request_id.is_empty()
                && request_id.len() <= 128
                && request_id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')),
            "request_id must be 1 to 128 ASCII letters, digits, dots, hyphens or underscores"
        );
        let mut state = self.state.lock().expect("MCP change state lock");
        if let Some(record) = state.applies.get(request_id) {
            ensure!(
                record.proposal_digest == proposal_digest,
                "request_id was already used for another proposal"
            );
            return Ok(record.result.clone());
        }
        ensure!(
            state.applies.len() < 32,
            "MCP apply history is full; restart the bridge"
        );
        if let Some(revision) = state
            .applies
            .values()
            .find(|record| {
                record.proposal_digest == proposal_digest && record.result["state"] == "published"
            })
            .and_then(|record| record.result["revision"].as_str())
            .map(str::to_owned)
        {
            let kind = state
                .proposals
                .get(proposal_digest)
                .context("unknown proposal digest")?
                .kind();
            let current = LocalPluginRootAuthority::new(&self.root)
                .inspect()
                .ok()
                .map(|view| view.revision().as_str().to_owned());
            let result = serde_json::json!({
                "schema_version": 1,
                "kind": kind,
                "request_id": request_id,
                "proposal_digest": proposal_digest,
                "revision": revision,
                "current_revision": current,
                "state": "previously_published",
                "activation": "not_observed",
            });
            state.applies.insert(
                request_id.to_owned(),
                ApplyRecord {
                    proposal_digest: proposal_digest.to_owned(),
                    result: result.clone(),
                },
            );
            return Ok(result);
        }
        let proposal = state
            .proposals
            .get(proposal_digest)
            .context("unknown proposal digest; preview the exact change first")?;
        ensure!(
            proposal.status() == PluginConfigurationProposalStatus::Ready,
            "proposal is not ready for publication"
        );
        let result = publication_result(&self.root, proposal, proposal_digest, request_id);
        state.applies.insert(
            request_id.to_owned(),
            ApplyRecord {
                proposal_digest: proposal_digest.to_owned(),
                result: result.clone(),
            },
        );
        Ok(result)
    }
}

fn publication_result(
    root: &Path,
    proposal: &StoredProposal,
    proposal_digest: &str,
    request_id: &str,
) -> serde_json::Value {
    let authority = LocalPluginRootAuthority::new(root);
    if let Ok(publication) = proposal.publish(&authority) {
        serde_json::json!({
            "schema_version": 1,
            "kind": proposal.kind(),
            "request_id": request_id,
            "proposal_digest": publication["proposal_digest"],
            "base_revision": publication["base_revision"],
            "revision": publication["revision"],
            "state": "published",
            "activation": "not_observed",
        })
    } else {
        let current = authority
            .inspect()
            .ok()
            .map(|view| view.revision().as_str().to_owned());
        let uncertain =
            current.as_deref() == Some(proposal.candidate_revision().as_str()) || current.is_none();
        serde_json::json!({
            "schema_version": 1,
            "kind": proposal.kind(),
            "request_id": request_id,
            "proposal_digest": proposal_digest,
            "base_revision": proposal.base_revision().as_str(),
            "candidate_revision": proposal.candidate_revision().as_str(),
            "current_revision": current,
            "state": if uncertain { "outcome_uncertain" } else { "rejected" },
            "diagnostic_code": if uncertain { "LENSO_CHANGE_OUTCOME_UNCERTAIN" } else { "LENSO_CHANGE_CONFLICT" },
            "activation": "not_observed",
        })
    }
}

fn status(status: PluginConfigurationProposalStatus) -> &'static str {
    match status {
        PluginConfigurationProposalStatus::Ready => "ready",
        PluginConfigurationProposalStatus::NeedsDecision => "needs_decision",
        PluginConfigurationProposalStatus::Rejected => "rejected",
    }
}

fn application(application: PluginConfigurationApplication) -> &'static str {
    match application {
        PluginConfigurationApplication::Noop => "noop",
        PluginConfigurationApplication::AppGeneration => "app_generation_required",
        PluginConfigurationApplication::Blocked => "blocked",
    }
}

fn changed_fields(before: &str, after: &str) -> anyhow::Result<Vec<String>> {
    let previous: toml::Value = if before.trim().is_empty() {
        toml::Value::Table(toml::map::Map::new())
    } else {
        toml::from_str(before)?
    };
    let candidate: toml::Value = if after.trim().is_empty() {
        toml::Value::Table(toml::map::Map::new())
    } else {
        toml::from_str(after)?
    };
    let mut old = BTreeMap::new();
    let mut new = BTreeMap::new();
    flatten("", &previous, &mut old);
    flatten("", &candidate, &mut new);
    Ok(old
        .keys()
        .chain(new.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|key| old.get(*key) != new.get(*key))
        .cloned()
        .collect())
}

fn flatten(prefix: &str, value: &toml::Value, out: &mut BTreeMap<String, toml::Value>) {
    if let Some(table) = value.as_table() {
        for (name, child) in table {
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}.{name}")
            };
            flatten(&path, child, out);
        }
    } else {
        out.insert(prefix.to_owned(), value.clone());
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use lenso_app_plan::authoring::{
        HostCatalog, HostDefaultPlugin, HostPluginRelease, HostSlot, PluginDescriptor,
    };

    use super::ChangeController;

    #[test]
    fn preview_redacts_values_and_publish_is_idempotent_and_fenced() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join(".lenso")).unwrap();
        let host = HostCatalog::new(
            [HostSlot::one("agent")],
            [HostPluginRelease::new(
                PluginDescriptor::new("example.agent", "1.0.0", "agent").with_configuration_schema(
                    serde_json::json!({
                        "type": "object",
                        "properties": {"greeting": {"type": "string"}},
                        "additionalProperties": false
                    }),
                ),
            )],
            [HostDefaultPlugin::new("example.agent", "default")],
        );
        let host_path = temp.path().join(".lenso/host-catalog.json");
        fs::write(&host_path, serde_json::to_vec(&host).unwrap()).unwrap();
        let initial = lenso_app_authoring::inspect_plugin_root(temp.path())
            .unwrap()
            .revision()
            .as_str()
            .to_owned();
        let controller = ChangeController::new(temp.path().to_path_buf());
        let preview = controller
            .preview(
                &initial,
                "example.agent",
                "default",
                "greeting = 'private-value'\n",
            )
            .unwrap();
        assert_eq!(preview.status, "ready");
        assert_eq!(preview.changed_fields, ["greeting"]);
        assert!(
            !serde_json::to_string(&preview)
                .unwrap()
                .contains("private-value")
        );
        let path = temp.path().join("plugins/example.agent/default.toml");
        assert!(!path.exists());
        let published = controller
            .apply(&preview.proposal_digest, "apply-1")
            .unwrap();
        assert_eq!(published["state"], "published");
        assert_eq!(published["activation"], "not_observed");
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "greeting = 'private-value'\n"
        );
        assert_eq!(
            controller
                .apply(&preview.proposal_digest, "apply-1")
                .unwrap(),
            published
        );
        assert_eq!(
            controller
                .apply(&preview.proposal_digest, "apply-2")
                .unwrap()["state"],
            "previously_published"
        );

        let next = controller
            .preview(
                published["revision"].as_str().unwrap(),
                "example.agent",
                "default",
                "greeting = 'next-value'\n",
            )
            .unwrap();
        let host_value: serde_json::Value =
            serde_json::from_slice(&fs::read(&host_path).unwrap()).unwrap();
        fs::write(&host_path, serde_json::to_vec_pretty(&host_value).unwrap()).unwrap();
        let rejected = controller.apply(&next.proposal_digest, "apply-3").unwrap();
        assert_eq!(rejected["state"], "rejected");
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "greeting = 'private-value'\n"
        );
    }

    #[test]
    fn selection_preview_and_apply_disable_then_enable_without_a_second_resolver() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join(".lenso")).unwrap();
        let host = HostCatalog::new(
            [HostSlot::many("agent")],
            [HostPluginRelease::new(PluginDescriptor::new(
                "example.agent",
                "1.0.0",
                "agent",
            ))],
            [],
        );
        fs::write(
            temp.path().join(".lenso/host-catalog.json"),
            serde_json::to_vec(&host).unwrap(),
        )
        .unwrap();
        fs::create_dir_all(temp.path().join("plugins/example.agent")).unwrap();
        fs::write(temp.path().join("plugins/example.agent/default.toml"), "").unwrap();
        let base = lenso_app_authoring::inspect_plugin_root(temp.path())
            .unwrap()
            .revision()
            .as_str()
            .to_owned();
        let controller = ChangeController::new(temp.path().to_path_buf());
        let disable = controller
            .preview_selection(&base, "example.agent", "default", false)
            .unwrap();
        assert_eq!(disable.status, "ready");
        assert!(disable.before_enabled);
        assert!(!disable.requested_enabled);
        let marker = temp.path().join("plugins/example.agent/default.disabled");
        assert!(!marker.exists());
        let disabled = controller
            .apply(&disable.proposal_digest, "disable-1")
            .unwrap();
        assert_eq!(disabled["state"], "published");
        assert_eq!(disabled["kind"], "lenso.mcp-selection-apply");
        assert!(marker.is_file());
        let enable = controller
            .preview_selection(
                disabled["revision"].as_str().unwrap(),
                "example.agent",
                "default",
                true,
            )
            .unwrap();
        assert_eq!(enable.status, "ready");
        assert!(!enable.before_enabled);
        let enabled = controller
            .apply(&enable.proposal_digest, "enable-1")
            .unwrap();
        assert_eq!(enabled["state"], "published");
        assert!(!marker.exists());
        assert_eq!(enabled["revision"], base);
    }
}
