//! Accepted compatibility inputs use Engine's existing atomic publication.
use crate::{ContractInput, digest, discovery::contained, snapshot_contract};
use anyhow::{Context, ensure};
use lenso_engine::{
    Generation, Resource, Snapshot,
    publication::{Publication, publish, verify},
};
use std::{collections::BTreeMap, fs, path::Path};

fn key(snapshot: &Snapshot, input: &ContractInput) -> anyhow::Result<String> {
    let descriptor: serde_json::Value = serde_json::from_slice(
        snapshot
            .files()
            .get(&input.descriptor)
            .context("descriptor input")?,
    )?;
    Ok(digest(
        descriptor["id"]
            .as_str()
            .context("Capability identity")?
            .as_bytes(),
    ))
}

pub(crate) fn attach(
    root: &Path,
    baseline_root: &str,
    snapshot: &mut Snapshot,
    inputs: &mut [ContractInput],
) -> anyhow::Result<()> {
    contained(root, baseline_root)?;
    let directory = root.join(baseline_root);
    let current = directory.join("current.json");
    if !current.exists() {
        return Ok(());
    }
    contained(&directory, "current.json")?;
    ensure!(
        fs::metadata(&current)?.len() <= 4 * 1024 * 1024,
        "baseline publication exceeds byte budget"
    );
    let publication: Publication = serde_json::from_slice(&fs::read(current)?)?;
    contained(
        &directory,
        &format!("generations/{}", publication.generation),
    )?;
    let generation = verify(&directory, &publication)?.join("files");
    for input in inputs {
        if input.baseline.is_some() {
            continue;
        }
        let key = key(snapshot, input)?;
        let descriptor = format!("{key}/capability.json");
        if !generation.join(&descriptor).exists() {
            continue;
        }
        let target = format!("accepted/{descriptor}");
        snapshot_contract(&generation, &descriptor, &target, snapshot)?;
        input.baseline = Some(target);
    }
    Ok(())
}

/// Accept all successfully generated contracts together. Invoke only after run()
/// succeeds. Default CLI generation does this; Check never advances the baseline.
/// Custom policies can omit baseline_root and provide ContractInput.baseline.
pub fn accept(
    root: &Path,
    baseline_root: &str,
    snapshot: &Snapshot,
    inputs: &[ContractInput],
) -> anyhow::Result<Publication> {
    contained(root, baseline_root)?;
    let mut resources = BTreeMap::new();
    for input in inputs {
        let key = key(snapshot, input)?;
        let parent = Path::new(&input.descriptor)
            .parent()
            .context("descriptor parent")?
            .to_str()
            .context("UTF8 input")?;
        ensure!(
            !parent.is_empty(),
            "custom root-level inputs must provide their own baseline policy"
        );
        let prefix = format!("{parent}/");
        for (path, bytes) in snapshot
            .files()
            .iter()
            .filter(|(path, _)| path.starts_with(&prefix))
        {
            let relative = if path == &input.descriptor {
                "capability.json"
            } else {
                &path[prefix.len()..]
            };
            let output = format!("{key}/{relative}");
            let resource = Resource::file(output.clone(), bytes.clone())?;
            if let Some(previous) = resources.insert(output.clone(), resource.clone()) {
                ensure!(
                    previous == resource,
                    "different contracts share Capability identity; select one or supply explicit baselines: {output}"
                );
            }
        }
    }
    let directory = root.join(baseline_root);
    fs::create_dir_all(&directory)?;
    let generation = Generation {
        outputs: BTreeMap::from([("accepted".into(), resources)]),
        ..Default::default()
    };
    publish(&directory, &generation)
}
