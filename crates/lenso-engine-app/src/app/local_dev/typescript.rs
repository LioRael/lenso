//! Repackage local Bun implementations against the retained Host's exact
//! admission. This does not mutate live factories or widen the Host profile.
use std::{collections::BTreeSet, fs, future::Future, path::Path, pin::Pin};

use anyhow::Context;
use lenso_app_authoring::{discovery::Candidate, host_authoring::GeneratedHostBuild};
use lenso_plugin_bundle::{ImplementationPolicy, read_bundle_manifest, verify_bundle_directory};
use serde_json::Value;

use super::changes::Inputs;

pub(super) enum Outcome {
    Unavailable(&'static str),
    Packaged(Vec<String>),
    Interrupted,
}

pub(super) async fn candidate(
    root: &Path,
    current: &Path,
    output: &Path,
    inputs: &Inputs,
    trust: &[String],
    frontend_enabled: bool,
    mut interrupt: Pin<&mut impl Future<Output = std::io::Result<()>>>,
) -> anyhow::Result<Outcome> {
    let Some(next) = Inputs::capture(root, frontend_enabled)? else {
        return Ok(Outcome::Unavailable(
            "selected convention compiler owns packaging",
        ));
    };
    let Some(changed) = inputs.implementation_edits(&next, current, root)? else {
        return Ok(Outcome::Unavailable(
            "source/config/dependency membership changed or no reproducible implementation diff",
        ));
    };
    let mut provenance: Value =
        serde_json::from_slice(&fs::read(current.join("local-sources.json"))?)?;
    let sources: Vec<Candidate> = serde_json::from_value(provenance["sources"].clone())?;
    let mut inventory: Vec<Value> =
        serde_json::from_slice(&fs::read(current.join("bundles.json"))?)?;
    let selected: Vec<_> = sources
        .iter()
        .filter(|source| {
            source.surface_owner.is_none()
                && source.composite.is_none()
                && source.published_resources.is_empty()
                && inventory.iter().any(|entry| {
                    entry["plugin_id"] == source.plugin_id
                        && entry["execution_class"] == "lenso.bun-process@1"
                })
                && changed.iter().any(|path| path.starts_with(&source.project))
        })
        .collect();
    // Every changed file must have exactly one retained Plugin owner. Shared
    // dependencies, overlapping projects and files outside that source fall back.
    if selected.is_empty()
        || changed.iter().any(|path| {
            selected
                .iter()
                .filter(|source| path.starts_with(&source.project))
                .count()
                != 1
        })
    {
        return Ok(Outcome::Unavailable(
            "implementation edits lack one retained Bun Plugin owner",
        ));
    }
    let temporary = tempfile::tempdir_in(root.join(".lenso"))?;
    let authority_path = ".lenso/host-build.json";
    let mut authority: Value = serde_json::from_slice(&fs::read(current.join(authority_path))?)?;
    let codecs: Value = serde_json::from_slice(&fs::read(current.join("runtime-codecs.json"))?)?;
    super::changes::copy_execution_packaging(current, output)?;
    verify_retained(output)?;
    let mut packaged = Vec::new();
    for source in selected {
        let archive = temporary
            .path()
            .join(format!("{}.lenso-plugin", source.plugin_id));
        let mut command = super::command(std::env::current_exe()?);
        command
            .args(["plugin", "pack", "--repo-root"])
            .arg(&source.project)
            .arg("--output")
            .arg(&archive);
        for declaration in trust {
            command.arg("--trust-adopted-build").arg(declaration);
        }
        eprintln!("Dev packaging Plugin {} (retained Host)", source.plugin_id);
        let mut child = command.spawn().context("start targeted Plugin packaging")?;
        let status = tokio::select! {
            biased;
            signal = &mut interrupt => {
                signal?;
                super::stop(&mut child, true).await?;
                return Ok(Outcome::Interrupted);
            }
            status = child.wait() => status?,
        };
        anyhow::ensure!(
            status.success(),
            "targeted Plugin packaging failed: {}",
            source.plugin_id
        );
        let entry = inventory
            .iter_mut()
            .find(|entry| entry["plugin_id"] == source.plugin_id)
            .context("retained Plugin inventory")?;
        let compatible = crate::archive::with_bundle_directory(&archive, |directory| {
            replace(directory, output, entry, &mut authority, &codecs)
        })?;
        if !compatible {
            fs::remove_dir_all(output)?;
            eprintln!(
                "Dev packaging {} changed its contract/profile; full App build required",
                source.plugin_id
            );
            return Ok(Outcome::Unavailable(
                "changed Plugin contract, codec or target profile",
            ));
        }
        fs::copy(
            &archive,
            output.join(entry["path"].as_str().context("bundle path")?),
        )?;
        entry["archive_digest"] = super::super::local_host::digest(&archive)?.into();
        provenance["source_digests"][&source.plugin_id] =
            super::super::local_host::input_digest(&source.project)?.into();
        packaged.push(source.plugin_id.clone());
    }
    fs::write(
        output.join(authority_path),
        serde_json::to_vec_pretty(&authority)?,
    )?;
    fs::write(
        output.join("intent").join(authority_path),
        serde_json::to_vec_pretty(&authority)?,
    )?;
    fs::write(
        output.join("bundles.json"),
        serde_json::to_vec_pretty(&inventory)?,
    )?;
    fs::write(
        output.join("local-sources.json"),
        serde_json::to_vec_pretty(&provenance)?,
    )?;
    let before = lenso_app_authoring::load_resolved_app(current)?;
    let after = lenso_app_authoring::load_resolved_app(output)?;
    if before.plan() != after.plan() {
        fs::remove_dir_all(output)?;
        return Ok(Outcome::Unavailable("resolved Plan changed"));
    }
    super::super::target_closure::admit(&after, &inventory)?;
    // The fresh source snapshot must remain valid after every child pack.
    anyhow::ensure!(
        next.matches(root, frontend_enabled)?,
        "source changed during targeted packaging; retry"
    );
    for path in &changed {
        anyhow::ensure!(
            path.is_file(),
            "implementation disappeared during packaging"
        );
    }
    // Configuration is intentionally excluded from Inputs.matches for config
    // reuse, so also verify it here before retaining the previous Plan.
    anyhow::ensure!(
        inputs.implementation_edits(&next, current, root)?.is_some(),
        "configuration changed during targeted packaging"
    );
    relock(output, &packaged)?;
    Ok(Outcome::Packaged(packaged))
}

fn replace(
    directory: &Path,
    output: &Path,
    entry: &mut Value,
    authority: &mut Value,
    codecs: &Value,
) -> anyhow::Result<bool> {
    let verified = verify_bundle_directory(directory)?;
    let profile = serde_json::from_value(entry["target_capability_profile"].clone())?;
    let admission = crate::target_profile::admission_from_profile(
        lenso_app_plan::ExecutionClassId::new(
            entry["execution_class"]
                .as_str()
                .context("execution class")?,
        ),
        entry["runtime_profile"]
            .as_str()
            .context("runtime profile")?,
        &profile,
    )?;
    let selected = match crate::target_profile::select_implementation(
        &read_bundle_manifest(directory)?,
        &ImplementationPolicy {
            host_target: entry["target"].as_str().context("retained target")?.into(),
            runtimes: vec![admission],
        },
    ) {
        Ok(selected) => selected,
        Err(error) => {
            eprintln!("Retained Host profile cannot select changed Plugin: {error:#}");
            return Ok(false);
        }
    };
    if verified.plugin_id != entry["plugin_id"]
        || verified.release_version != entry["release_version"]
        || serde_json::to_value(&selected.evidence)? != entry["selection"]
        || serde_json::to_value(&selected.target_capability_profile)?
            != entry["target_capability_profile"]
    {
        return Ok(false);
    }
    let retained: GeneratedHostBuild = serde_json::from_value(authority.clone())?;
    if retained
        .verify_distribution_bundle(
            &selected.implementation.descriptor,
            entry["manifest_digest"]
                .as_str()
                .context("retained bundle digest")?,
        )
        .is_err()
    {
        return Ok(false);
    }
    let artifact = directory.join(&selected.implementation.artifact.path);
    if crate::plugin::local_runtime_descriptor(&artifact, "lenso.bun-process@1")?
        != Some(codecs[&verified.plugin_id].clone())
    {
        return Ok(false);
    }
    if !update_admission(
        authority,
        &selected.implementation.descriptor,
        entry["manifest_digest"]
            .as_str()
            .context("retained bundle digest")?,
        &verified.manifest_digest,
    )? {
        return Ok(false);
    }
    fs::copy(
        artifact,
        output.join("runtime/artifacts").join(&verified.plugin_id),
    )?;
    let artifact = selected.implementation.artifact;
    entry["manifest_digest"] = verified.manifest_digest.into();
    entry["artifact_path"] = artifact.path.into();
    entry["artifact_digest"] = artifact.digest.into();
    entry["artifact_size"] = artifact.size.into();
    entry["artifact_media_type"] = artifact.media_type.into();
    entry["artifact_target"] = artifact.target.into();
    // The caller records the digest of the newly serialized archive.
    Ok(true)
}

fn verify_retained(output: &Path) -> anyhow::Result<()> {
    let lock: Value =
        serde_json::from_slice(&fs::read(output.join(".lenso/distribution.lock.json"))?)?;
    for file in lock["files"]
        .as_array()
        .context("retained distribution files")?
    {
        let path = output.join(
            file["path"]
                .as_str()
                .context("retained distribution path")?,
        );
        anyhow::ensure!(
            fs::metadata(&path)?.len()
                == file["size"].as_u64().context("retained artifact size")?
                && super::super::local_host::digest(&path)? == file["sha256"],
            "retained artifact differs from its execution lock: {}",
            path.display()
        );
    }
    Ok(())
}

fn relock(output: &Path, packaged: &[String]) -> anyhow::Result<()> {
    let path = output.join(".lenso/distribution.lock.json");
    let mut lock: Value = serde_json::from_slice(&fs::read(&path)?)?;
    let mut changed = BTreeSet::from([
        ".lenso/host-build.json".to_owned(),
        "bundles.json".to_owned(),
        "local-sources.json".to_owned(),
    ]);
    for id in packaged {
        changed.insert(format!("runtime/artifacts/{id}"));
        changed.insert(format!("bundles/{id}.lenso-plugin"));
    }
    for file in lock["files"]
        .as_array_mut()
        .context("retained distribution files")?
    {
        let relative = file["path"].as_str().context("retained file path")?;
        if changed.remove(relative) {
            let artifact = output.join(relative);
            file["sha256"] = super::super::local_host::digest(&artifact)?.into();
            file["size"] = fs::metadata(artifact)?.len().into();
        }
    }
    anyhow::ensure!(
        changed.is_empty(),
        "targeted packaging cannot add unlocked artifacts"
    );
    fs::write(path, serde_json::to_vec_pretty(&lock)?)?;
    // Retained runtime, entrypoint mode, provenance and unrelated artifacts keep
    // their exact original proof. No finalize step can switch a portable Host
    // to native mode or accidentally refresh an unrelated execution identity.
    verify_retained(output)
}

fn update_admission(
    authority: &mut Value,
    descriptor: &lenso_app_plan::authoring::PluginDescriptor,
    before: &str,
    after: &str,
) -> anyhow::Result<bool> {
    let descriptor = serde_json::to_value(descriptor)?;
    let mut changed = false;
    for rule in authority["admissions"]
        .as_array_mut()
        .context("retained admissions")?
    {
        for release in rule["releases"]
            .as_array_mut()
            .context("retained releases")?
        {
            if release["descriptor"] == descriptor && release["manifest_digest"] == before {
                release["manifest_digest"] = after.into();
                changed = true;
            }
        }
    }
    if changed {
        let _: GeneratedHostBuild = serde_json::from_value(authority.clone())?;
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repackaging_cannot_relock_a_tampered_retained_host() {
        let output = tempfile::tempdir().unwrap();
        fs::create_dir(output.path().join(".lenso")).unwrap();
        let host = output.path().join(".lenso/host");
        fs::write(&host, "original").unwrap();
        fs::write(output.path().join(".lenso/distribution.lock.json"), serde_json::to_vec(&serde_json::json!({"files":[{
            "path":".lenso/host", "size":8, "sha256":super::super::super::local_host::digest(&host).unwrap(),
        }]})).unwrap()).unwrap();
        verify_retained(output.path()).unwrap();
        fs::write(host, "modified").unwrap();
        assert!(verify_retained(output.path()).is_err());
    }
}
