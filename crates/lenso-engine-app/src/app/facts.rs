use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::bail;
use lenso_app_plan::{
    CapabilityCardinality, CapabilityOperationKind,
    authoring::{PluginInstanceSource, ResolvedApp},
};
use serde::Serialize;

use super::ProjectArgs;
use crate::plugins::project_root;

#[derive(Debug, Serialize)]
pub struct ProjectFacts {
    pub schema_version: u32,
    pub kind: &'static str,
    pub status: &'static str,
    pub root: PathBuf,
    pub host_target: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_root_revision: Option<String>,
    pub runtime: RuntimeFacts,
    pub plugins: Vec<PluginFacts>,
    pub bindings: Vec<BindingFacts>,
    pub discovered_sources: Vec<DiscoveredSourceFacts>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Serialize)]
pub struct RuntimeFacts {
    pub status: &'static str,
    pub detail: &'static str,
}

#[derive(Debug, Serialize)]
pub struct PluginFacts {
    pub plugin_id: String,
    pub release_version: String,
    pub release_source: &'static str,
    pub source_location: SourceLocation,
    pub instances: Vec<InstanceFacts>,
}

#[derive(Debug, Serialize)]
pub struct InstanceFacts {
    pub id: String,
    pub enabled: bool,
    pub source: &'static str,
    pub configuration_source: SourceLocation,
    pub selection_source: SourceLocation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution: Option<ExecutionFacts>,
}

#[derive(Debug, Serialize)]
pub struct ExecutionFacts {
    pub plan_key: String,
    pub package_id: String,
    pub package_revision: String,
    pub execution_class: String,
    pub runtime_profile: String,
    pub entrypoint: String,
    pub provided_capabilities: Vec<ProvidedCapabilityFacts>,
    pub required_capabilities: Vec<RequiredCapabilityFacts>,
}

#[derive(Debug, Serialize)]
pub struct ProvidedCapabilityFacts {
    pub capability_id: String,
    pub descriptor_version: String,
    pub operations: Vec<OperationFacts>,
}

#[derive(Debug, Serialize)]
pub struct OperationFacts {
    pub name: String,
    pub kind: CapabilityOperationKind,
}

#[derive(Debug, Serialize)]
pub struct RequiredCapabilityFacts {
    pub requirement_id: String,
    pub capability_id: String,
    pub descriptor_version: String,
    pub cardinality: CapabilityCardinality,
}

#[derive(Debug, Serialize)]
pub struct BindingFacts {
    pub consumer_instance: String,
    pub requirement_id: String,
    pub capability_id: String,
    pub descriptor_version: String,
    pub provider_instance: String,
}

#[derive(Debug, Serialize)]
pub struct DiscoveredSourceFacts {
    pub plugin_id: String,
    pub release_version: String,
    pub role: lenso_app_authoring::discovery::SourceRole,
    pub format: String,
    pub project: PathBuf,
    pub metadata: PathBuf,
    pub matches_adopted_coordinates: bool,
    pub status: &'static str,
}

#[derive(Debug, Serialize)]
pub struct Diagnostic {
    pub code: &'static str,
    pub severity: &'static str,
    pub message: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceLocation>,
    pub help: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct SourceLocation {
    pub path: PathBuf,
}

pub(super) fn facts(args: ProjectArgs) -> anyhow::Result<()> {
    let report = inspect_project_facts(project_root(args.root)?)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_human(&report);
    }
    if report.status == "invalid" {
        bail!("App facts are invalid; use the reported diagnostic code and help");
    }
    Ok(())
}

pub fn inspect_project_facts(root: impl AsRef<Path>) -> anyhow::Result<ProjectFacts> {
    let root = fs::canonicalize(root)?;
    let discovery = lenso_app_authoring::discovery::discover(&root);
    let state = lenso_app_authoring::inspect_plugin_root(&root);
    let mut diagnostics = Vec::new();

    if discovery.is_err() {
        diagnostics.push(Diagnostic {
            code: "LENSO_PROJECT_DISCOVERY_FAILED",
            severity: "error",
            message: "Local Plugin source discovery failed.",
            source: None,
            help: "Run `lenso app discover` for the bounded discovery error.",
        });
    }
    if state.is_err() {
        diagnostics.push(Diagnostic {
            code: "LENSO_APP_RESOLUTION_FAILED",
            severity: "error",
            message: "The Host Catalog and Plugin Root did not resolve to a valid App.",
            source: None,
            help: "Run `lenso doctor` for resolution checks without exposing configuration values.",
        });
    }

    let mut report = ProjectFacts {
        schema_version: 1,
        kind: "lenso.app-facts",
        status: if diagnostics.is_empty() {
            "resolved"
        } else {
            "invalid"
        },
        root: root.clone(),
        host_target: lenso_app_authoring::native_host_target(),
        plugin_root_revision: None,
        runtime: RuntimeFacts {
            status: "not_observed",
            detail: "This read-only command does not infer a running process from build artifacts.",
        },
        plugins: Vec::new(),
        bindings: Vec::new(),
        discovered_sources: Vec::new(),
        diagnostics,
    };

    if let Ok(state) = state {
        report.plugin_root_revision = Some(state.revision().as_str().to_owned());
        report.bindings = bindings(state.resolved());
        report.plugins = plugins(&root, &state);
    }
    if let Ok(discovery) = discovery {
        let adopted = report
            .plugins
            .iter()
            .map(|plugin| {
                (
                    (plugin.plugin_id.as_str(), plugin.release_version.as_str()),
                    (),
                )
            })
            .collect::<BTreeMap<_, _>>();
        report.discovered_sources = discovery
            .candidates
            .into_iter()
            .map(|candidate| DiscoveredSourceFacts {
                matches_adopted_coordinates: adopted.contains_key(&(
                    candidate.plugin_id.as_str(),
                    candidate.release_version.as_str(),
                )),
                plugin_id: candidate.plugin_id,
                release_version: candidate.release_version,
                role: candidate.role,
                format: candidate.format,
                project: candidate.project,
                metadata: candidate.metadata,
                status: "candidate_only",
            })
            .collect();
    }
    Ok(report)
}

fn plugins(
    root: &std::path::Path,
    state: &lenso_app_authoring::PluginRootAuthoringState,
) -> Vec<PluginFacts> {
    let resolved = state.resolved();
    let enabled = resolved
        .instances()
        .iter()
        .map(|instance| (instance.id().to_string(), instance))
        .collect::<BTreeMap<_, _>>();
    state
        .plugins()
        .iter()
        .filter(|plugin| !plugin.instances().is_empty())
        .map(|plugin| {
            let release_location = if plugin.is_root_supplied() {
                root.join("plugins")
                    .join(plugin.plugin_id())
                    .join("plugin.lenso-plugin")
            } else {
                root.join(".lenso/host-catalog.json")
            };
            PluginFacts {
                plugin_id: plugin.plugin_id().to_owned(),
                release_version: plugin.release_version().to_owned(),
                release_source: if plugin.is_root_supplied() {
                    "plugin_root"
                } else {
                    "host_catalog"
                },
                source_location: SourceLocation {
                    path: release_location,
                },
                instances: plugin
                    .instances()
                    .iter()
                    .map(|instance| {
                        let id = instance.id().to_string();
                        let resolved_instance = enabled.get(&id).copied();
                        let configuration_source = if instance.root_configuration_toml().is_some() {
                            root.join("plugins")
                                .join(instance.id().plugin_id())
                                .join(format!("{}.toml", instance.id().instance_key()))
                        } else {
                            root.join(".lenso/host-catalog.json")
                        };
                        let selection_source = if instance.is_disabled_by_root() {
                            root.join("plugins")
                                .join(instance.id().plugin_id())
                                .join(format!("{}.disabled", instance.id().instance_key()))
                        } else if instance.is_host_default() {
                            root.join(".lenso/host-catalog.json")
                        } else {
                            root.join("plugins")
                                .join(instance.id().plugin_id())
                                .join(format!("{}.toml", instance.id().instance_key()))
                        };
                        InstanceFacts {
                            id,
                            enabled: instance.is_enabled(),
                            source: resolved_instance
                                .map_or("disabled", |entry| source_name(entry.source())),
                            configuration_source: SourceLocation {
                                path: configuration_source,
                            },
                            selection_source: SourceLocation {
                                path: selection_source,
                            },
                            execution: resolved_instance.and_then(|entry| {
                                resolved
                                    .plan()
                                    .plugin_instances()
                                    .iter()
                                    .find(|plan| plan.instance_key() == entry.plan_key())
                                    .map(|plan| ExecutionFacts {
                                        plan_key: entry.plan_key().to_owned(),
                                        package_id: plan.package_id().to_owned(),
                                        package_revision: plan.package_revision().to_owned(),
                                        execution_class: plan.execution_class().as_str().to_owned(),
                                        runtime_profile: plan.runtime_profile().to_owned(),
                                        entrypoint: plan.entrypoint().to_owned(),
                                        provided_capabilities: plan
                                            .provided_capabilities()
                                            .iter()
                                            .map(|capability| ProvidedCapabilityFacts {
                                                capability_id: capability
                                                    .capability_id()
                                                    .to_owned(),
                                                descriptor_version: capability
                                                    .descriptor_version()
                                                    .to_owned(),
                                                operations: capability
                                                    .operations()
                                                    .iter()
                                                    .map(|operation| OperationFacts {
                                                        name: operation.clone(),
                                                        kind: capability
                                                            .operation_kind(operation)
                                                            .unwrap_or(
                                                                CapabilityOperationKind::Request,
                                                            ),
                                                    })
                                                    .collect(),
                                            })
                                            .collect(),
                                        required_capabilities: plan
                                            .required_capabilities()
                                            .iter()
                                            .map(|requirement| RequiredCapabilityFacts {
                                                requirement_id: requirement
                                                    .requirement_id()
                                                    .to_owned(),
                                                capability_id: requirement
                                                    .capability_id()
                                                    .to_owned(),
                                                descriptor_version: requirement
                                                    .descriptor_version()
                                                    .to_owned(),
                                                cardinality: requirement.cardinality(),
                                            })
                                            .collect(),
                                    })
                            }),
                        }
                    })
                    .collect(),
            }
        })
        .collect()
}

fn bindings(resolved: &ResolvedApp) -> Vec<BindingFacts> {
    resolved
        .plan()
        .capability_bindings()
        .iter()
        .map(|binding| BindingFacts {
            consumer_instance: binding.consumer_instance().to_owned(),
            requirement_id: binding.requirement_id().to_owned(),
            capability_id: binding.capability_id().to_owned(),
            descriptor_version: binding.descriptor_version().to_owned(),
            provider_instance: binding.provider_instance().to_owned(),
        })
        .collect()
}

const fn source_name(source: PluginInstanceSource) -> &'static str {
    match source {
        PluginInstanceSource::HostDefault => "host_default",
        PluginInstanceSource::HostDefaultConfiguredByRoot => "host_default_configured_by_root",
        PluginInstanceSource::PluginRoot => "plugin_root",
    }
}

fn print_human(report: &ProjectFacts) {
    println!("App facts: {}", report.status);
    println!("Root: {}", report.root.display());
    if let Some(revision) = &report.plugin_root_revision {
        println!("Plugin Root revision: {revision}");
    }
    println!("Host target: {}", report.host_target);
    println!("Runtime: {}", report.runtime.status);
    for plugin in &report.plugins {
        println!(
            "{}@{}\t{}\t{} instance(s)",
            plugin.plugin_id,
            plugin.release_version,
            plugin.release_source,
            plugin.instances.len()
        );
    }
    for diagnostic in &report.diagnostics {
        println!(
            "{}\t{}\t{}\t{}",
            diagnostic.severity,
            diagnostic.code,
            diagnostic.message,
            diagnostic.source.as_ref().map_or_else(
                || "-".to_owned(),
                |source| source.path.display().to_string()
            )
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lenso_app_plan::authoring::{
        HostCatalog, HostDefaultPlugin, HostPluginRelease, HostSlot, PluginDescriptor,
    };

    fn app_root() -> tempfile::TempDir {
        let temporary = tempfile::tempdir().unwrap();
        let control = temporary.path().join(".lenso");
        fs::create_dir(&control).unwrap();
        fs::create_dir(temporary.path().join("plugins")).unwrap();
        let catalog = HostCatalog::new(
            [HostSlot::optional("agent")],
            [
                HostPluginRelease::new(
                    PluginDescriptor::new("example.agent", "1.2.3", "agent")
                        .with_configuration_defaults(serde_json::json!({
                            "credential": "must-not-enter-project-facts"
                        }))
                        .with_configuration_schema(serde_json::json!({
                            "type": "object",
                            "properties": {"credential": {"type": "string"}},
                            "additionalProperties": false
                        })),
                ),
                HostPluginRelease::new(PluginDescriptor::new(
                    "example.available-only",
                    "9.9.9",
                    "agent",
                )),
            ],
            [HostDefaultPlugin::new("example.agent", "default").disableable()],
        );
        fs::write(
            control.join("host-catalog.json"),
            serde_json::to_vec(&catalog).unwrap(),
        )
        .unwrap();
        let source = temporary.path().join("app/unrelated-source");
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join("Cargo.toml"),
            r#"
[package]
name = "unrelated-source"
version = "1.2.3"

[package.metadata.lenso]
plugin-id = "example.agent"
root-slot = "agent"
"#,
        )
        .unwrap();
        temporary
    }

    #[test]
    fn reports_exact_adopted_release_without_claiming_runtime_state() {
        let temporary = app_root();

        let report = inspect_project_facts(temporary.path()).unwrap();

        assert_eq!(report.status, "resolved");
        assert_eq!(report.plugins.len(), 1);
        assert_eq!(report.plugins[0].plugin_id, "example.agent");
        assert_eq!(report.plugins[0].release_version, "1.2.3");
        assert!(
            report
                .plugins
                .iter()
                .all(|plugin| plugin.plugin_id != "example.available-only")
        );
        assert_eq!(report.plugins[0].instances[0].source, "host_default");
        assert_eq!(report.runtime.status, "not_observed");
        assert!(report.diagnostics.is_empty());
        assert!(report.plugin_root_revision.is_some());
        assert_eq!(report.discovered_sources.len(), 1);
        assert!(report.discovered_sources[0].matches_adopted_coordinates);
        let serialized = serde_json::to_string(&report).unwrap();
        assert!(serialized.contains("matches_adopted_coordinates"));
        assert!(!serialized.contains("matches_adopted_release"));
        assert!(!serialized.contains("must-not-enter-project-facts"));
    }

    #[test]
    fn returns_stable_diagnostic_when_resolution_is_invalid() {
        let temporary = tempfile::tempdir().unwrap();

        let report = inspect_project_facts(temporary.path()).unwrap();

        assert_eq!(report.status, "invalid");
        assert!(report.plugins.is_empty());
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].code, "LENSO_APP_RESOLUTION_FAILED");
        assert!(report.diagnostics[0].source.is_none());
    }

    #[test]
    fn disabled_selection_does_not_masquerade_as_configuration_source() {
        let temporary = app_root();
        let plugin = temporary.path().join("plugins/example.agent");
        fs::create_dir(&plugin).unwrap();
        fs::write(plugin.join("default.disabled"), []).unwrap();

        let report = inspect_project_facts(temporary.path()).unwrap();

        let instance = &report.plugins[0].instances[0];
        assert!(!instance.enabled);
        assert!(
            instance
                .configuration_source
                .path
                .ends_with(".lenso/host-catalog.json")
        );
        assert!(
            instance
                .selection_source
                .path
                .ends_with("plugins/example.agent/default.disabled")
        );
    }
}
