//! An added link edge must name the same target package as the original Plugin graph.
use std::{collections::BTreeSet, path::Path};

use anyhow::{Context as _, ensure};
use serde_json::Value;

pub(super) fn is_proc_macro(package: &Value) -> bool {
    package["targets"].as_array().is_some_and(|targets| {
        targets.iter().any(|target| {
            target["kind"]
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|kind| kind == "proc-macro"))
        })
    })
}

/// Resolve once, check link identities, then let the caller build this exact lock.
pub(super) fn resolve(
    manifest: &Path,
    expected: &std::collections::BTreeMap<String, String>,
) -> anyhow::Result<()> {
    let output = crate::app::cargo_command()
        .args(["metadata", "--format-version=1", "--filter-platform"])
        .arg(lenso_app_authoring::native_host_target())
        .arg("--manifest-path")
        .arg(manifest)
        .output()
        .context("resolve generated Host link identities")?;
    ensure!(
        output.status.success(),
        "resolve generated Host link identities: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata = serde_json::from_slice(&output.stdout)?;
    verify(&metadata, expected)?;
    verify_target_paths(manifest, &metadata, expected)
}

fn verify_target_paths(
    manifest: &Path,
    metadata: &Value,
    expected: &std::collections::BTreeMap<String, String>,
) -> anyhow::Result<()> {
    let root = metadata["resolve"]["nodes"]
        .as_array()
        .context("generated Host Cargo nodes")?
        .iter()
        .find(|node| node["id"] == metadata["resolve"]["root"])
        .context("generated Host Cargo root")?;
    let native_roots = root["deps"]
        .as_array()
        .context("generated Host dependencies")?
        .iter()
        .filter(|dependency| {
            dependency["name"]
                .as_str()
                .is_some_and(|name| name.starts_with("local_plugin_"))
        })
        .map(|dependency| dependency["pkg"].as_str().context("native root Cargo ID"))
        .collect::<anyhow::Result<BTreeSet<_>>>()?;
    for (alias, selected) in expected {
        if native_roots.contains(selected.as_str()) {
            // Cargo does not prune the inversion's starting package itself.
            continue;
        }
        let dependents = |prune_roots: bool| -> anyhow::Result<Vec<u8>> {
            let mut command = crate::app::cargo_command();
            command
                .args([
                    "tree",
                    "--locked",
                    "--edges",
                    "normal,no-proc-macro",
                    "--prefix",
                    "none",
                    "--color",
                    "never",
                    "--format",
                    "{p}",
                    "--invert",
                ])
                .arg(selected)
                .arg("--target")
                .arg(lenso_app_authoring::native_host_target())
                .arg("--manifest-path")
                .arg(manifest);
            if prune_roots {
                for id in &native_roots {
                    command.arg("--prune").arg(id);
                }
            }
            let output = command
                .output()
                .context("inspect generated Host target dependencies")?;
            ensure!(
                output.status.success(),
                "inspect generated Host target dependencies for `{alias}`: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            Ok(output.stdout)
        };
        // Metadata merges feature domains: a shared host/target package can
        // expose an optional normal edge enabled only by its host features.
        // Cargo tree retains those domains. Removing all native roots must
        // change the inverse tree if any truly requests this target package.
        // Compare complete output; never parse Cargo's display package IDs.
        ensure!(
            dependents(false)? != dependents(true)?,
            "generated Host link alias `{alias}` has no original target-normal dependency path after Cargo feature resolution: `{selected}`; request it through a native Plugin's normal dependencies"
        );
    }
    Ok(())
}

fn verify(
    metadata: &Value,
    expected: &std::collections::BTreeMap<String, String>,
) -> anyhow::Result<()> {
    let nodes = metadata["resolve"]["nodes"]
        .as_array()
        .context("generated Host Cargo nodes")?;
    let packages = metadata["packages"]
        .as_array()
        .context("generated Host Cargo packages")?;
    let root = nodes
        .iter()
        .find(|node| node["id"] == metadata["resolve"]["root"])
        .context("generated Host Cargo root")?;
    let dependencies = root["deps"]
        .as_array()
        .context("generated Host dependencies")?;
    // Only original native roots can justify an added alias. Ingress and the
    // generated aliases themselves must not make an otherwise unreachable
    // package appear to retain its original target feature requests.
    let mut pending = dependencies
        .iter()
        .filter(|dependency| {
            dependency["name"]
                .as_str()
                .is_some_and(|name| name.starts_with("local_plugin_"))
        })
        .map(|dependency| dependency["pkg"].as_str().context("native root Cargo ID"))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let mut reachable = BTreeSet::new();
    while let Some(id) = pending.pop() {
        let package = packages
            .iter()
            .find(|package| package["id"] == id)
            .context("reachable generated Cargo package")?;
        if is_proc_macro(package) || !reachable.insert(id) {
            continue;
        }
        let node = nodes
            .iter()
            .find(|node| node["id"] == id)
            .context("reachable generated Cargo node")?;
        for dependency in node["deps"].as_array().context("Cargo dependencies")? {
            if dependency["dep_kinds"]
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|kind| kind["kind"].is_null()))
            {
                pending.push(dependency["pkg"].as_str().context("Cargo dependency ID")?);
            }
        }
    }
    for (alias, selected) in expected {
        let actual = dependencies
            .iter()
            .find(|dependency| dependency["name"] == *alias)
            .and_then(|dependency| dependency["pkg"].as_str())
            .with_context(|| format!("generated Host is missing link alias `{alias}`"))?;
        ensure!(
            actual == selected && reachable.contains(actual),
            "generated Host link alias `{alias}` changed Cargo identity or lost its original target-normal dependency path: selected `{selected}`, resolved `{actual}`; align Git selectors and source overrides in the native Plugin graphs"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;
