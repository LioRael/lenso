//! Source-first checks without inventing descriptors or executing export code.
use super::*;
use lenso_app_authoring::discovery::{Candidate, SourceRole, conventions};
use std::collections::BTreeSet;

pub(super) fn inspect(
    root: &Path,
    target: &str,
    facilities: Option<&Path>,
) -> anyhow::Result<Value> {
    ensure!(
        matches!(target, "native" | "workers"),
        "unsupported check target"
    );
    let discovery = lenso_app_authoring::discovery::discover(root)?;
    let grants = facilities
        .map(read_grants)
        .transpose()?
        .unwrap_or_else(|| Grants {
            schema: "lenso.host-facilities.v1".into(),
            instances: BTreeMap::new(),
        });
    let mut selected = BTreeSet::new();
    let mut reports = Vec::new();
    let mut all_inputs_checked = true;
    for candidate in &discovery.candidates {
        let count = conventions::active_instances(root, candidate)?;
        if count == 0 {
            continue;
        }
        let ids = instance_ids(root, candidate, count)?;
        let executions = candidate
            .implementations
            .iter()
            .filter(|implementation| {
                !super::super::local_host::is_native(candidate)
                    || implementation.runtime == "native-linked"
            })
            .filter_map(|implementation| match implementation.runtime.as_str() {
                "native-linked" => Some("lenso.native-rust@1"),
                "bun" if target == "native" => Some("lenso.bun-process@1"),
                "process" if target == "native" => Some("lenso.process@1"),
                "wasm" if target == "native" => Some("lenso.wasm-component@1"),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        ensure!(
            !executions.is_empty(),
            "instance `{}/{}`: required {target} source execution; available: {:?}",
            candidate.plugin_id,
            ids.iter().next().unwrap(),
            candidate
                .implementations
                .iter()
                .map(|implementation| &implementation.runtime)
                .collect::<Vec<_>>()
        );
        let declaration = source::declaration(candidate)?;
        for name in ids {
            let id = format!("{}/{name}", candidate.plugin_id);
            selected.insert(id.clone());
            let resources = resource_types(&id, grants.instances.get(&id), target)?;
            let available = declaration
                .as_ref()
                .map(|declaration| declaration.combinations.clone())
                .unwrap_or_else(|| {
                    executions
                        .iter()
                        .map(|execution| Support {
                            environment: "native".into(),
                            execution: (*execution).into(),
                            resources: BTreeMap::new(),
                        })
                        .collect()
                });
            let matching = available
                .iter()
                .filter(|tuple| {
                    tuple.environment == target
                        && executions.contains(tuple.execution.as_str())
                        && tuple.resources == resources
                })
                .cloned()
                .collect::<Vec<_>>();
            ensure!(
                !matching.is_empty(),
                "instance `{id}`: required exact support environment={target}, execution from {executions:?}, resources={resources:?}; available declared combinations: {}",
                serde_json::to_string(&available)?
            );
            let matching_executions = matching
                .iter()
                .map(|tuple| tuple.execution.as_str())
                .collect::<BTreeSet<_>>();
            let inputs_checked = matching_executions.len() == 1;
            if inputs_checked {
                source::check_inputs(candidate, &id, matching_executions.first().unwrap())?;
            } else {
                all_inputs_checked = false;
            }
            reports.push(json!({"instance":id,"support":"declared",
                "candidate_combinations":matching,"execution_selection":"pending_contract_resolution",
                "source_inputs":if inputs_checked {"checked"} else {"pending_execution_selection"},
                "evidence_references":declaration.as_ref().map(|d| &d.evidence),
                "qualification":"not_assessed"}));
        }
    }
    ensure!(
        !selected.is_empty(),
        "required active source Plugin Instance; available: none"
    );
    for id in grants.instances.keys() {
        ensure!(
            selected.contains(id),
            "instance `{id}`: required active source Instance; available: none"
        );
    }
    let mut verified = vec![
        "source_identity_and_active_instances",
        "declared_exact_support_candidates",
        "resource_reference_shape",
    ];
    let mut deferred = vec![
        "execution_selection",
        "capability_closure",
        "semantic_entry_exports",
        "source_owned_resource_factories",
        "owner_configuration_schema",
        "whole_plan_target_restrictions",
        "registry_dependency_availability",
    ];
    if all_inputs_checked {
        verified.push("selected_source_entry_files");
        verified.push("source_tools_and_local_dependencies");
    } else {
        deferred.push("selected_source_inputs_and_tools");
    }
    Ok(json!({"schema_version":1,"kind":"lenso.app-target-check",
        "status":"static_passed_contract_pending","target":target,"instances":reports,
        "contract_resolution":"pending","requires_follow_up":true,
        "verified":verified,"deferred":deferred,
        "qualification":"not_assessed","resource_readiness":"not_run",
        "build_started":false,"secret_values_read":false}))
}

// The existing convention reader validates filenames, collisions, config and
// disabled markers first. This only collects names, without another TOML schema
// or a fabricated Host/PluginDescriptor to force an early resolution.
fn instance_ids(
    root: &Path,
    candidate: &Candidate,
    expected: usize,
) -> anyhow::Result<BTreeSet<String>> {
    let directory = root.join("plugins").join(&candidate.plugin_id);
    let mut names = BTreeSet::new();
    if candidate.role == SourceRole::AppOwned {
        names.insert("default".into());
    }
    if directory.is_dir() {
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name();
            if let Some(name) = name.to_str().and_then(|name| name.strip_suffix(".toml")) {
                names.insert(name.into());
            }
        }
        names.retain(|name| !directory.join(format!("{name}.disabled")).exists());
    }
    ensure!(
        names.len() == expected,
        "source Instance selection changed during preflight"
    );
    Ok(names)
}
