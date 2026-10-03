//! Reuse generated Host contracts and the source discovery/Root projection path.
use super::*;
use lenso_app_authoring::discovery::Candidate;

pub(super) fn has_declarations(root: &Path) -> anyhow::Result<bool> {
    let discovered = lenso_app_authoring::discovery::discover(root)?;
    for candidate in &discovered.candidates {
        if lenso_app_authoring::discovery::conventions::active_instances(root, candidate)? > 0
            && declaration(candidate)?.is_some()
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn read_json(path: &Path) -> anyhow::Result<Value> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.file_type().is_file() && metadata.len() <= 16 * 1024 * 1024,
        "check input must be a bounded regular file: {}",
        path.display()
    );
    serde_json::from_slice(&fs::read(path)?)
        .with_context(|| format!("invalid check input {}", path.display()))
}

pub(super) fn resolve(
    root: &Path,
    from: Option<&Path>,
) -> anyhow::Result<(ResolvedApp, BTreeMap<String, Declaration>)> {
    let authority_root = from.unwrap_or(root);
    let mut declarations = BTreeMap::new();
    let provenance_path = authority_root.join("local-sources.json");
    let mut candidates = Vec::new();
    if provenance_path.exists() {
        let provenance = read_json(&provenance_path)?;
        ensure!(
            provenance["schema"] == "lenso.local-sources.v2",
            "required generated source provenance v2; available: unsupported source evidence"
        );
        candidates = serde_json::from_value(provenance["sources"].clone())?;
        for candidate in &candidates {
            let candidate: &Candidate = candidate;
            let expected = provenance["source_digests"][&candidate.plugin_id]
                .as_str()
                .context("generated source identity is missing")?;
            ensure!(
                super::super::local_host::input_digest(&candidate.project)? == expected,
                "instance `{}/default`: required unchanged generated contract source; available: stale source; regenerate the owning contract before target check",
                candidate.plugin_id
            );
            if let Some(declaration) = declaration(candidate)? {
                declarations.insert(candidate.plugin_id.clone(), declaration);
            }
        }
    } else if from.is_some() {
        bail!(
            "source check requires existing generated Host contracts and local-sources.json; available: none; no build started"
        );
    }
    let resolved = if from.is_some() {
        let inputs = lenso_engine::discovery::DiscoverySession::new(root)?;
        let discovered = lenso_app_authoring::discovery::discover_in(root, &inputs)?;
        for candidate in &discovered.candidates {
            if lenso_app_authoring::discovery::conventions::active_instances(root, candidate)? == 0
            {
                continue;
            }
            ensure!(
                candidates
                    .iter()
                    .any(|previous| previous.plugin_id == candidate.plugin_id
                        && previous.release_version == candidate.release_version
                        && previous.project == candidate.project),
                "instance `{}/default`: required exact generated contract; available: no matching source Instance",
                candidate.plugin_id
            );
        }
        let temporary = tempfile::tempdir()?;
        fs::create_dir(temporary.path().join(".lenso"))?;
        fs::copy(
            authority_root.join(".lenso/host-build.json"),
            temporary.path().join(".lenso/host-build.json"),
        )
        .context("required existing generated Host authority; no build started")?;
        if root.join("plugins").exists() {
            super::super::source_intent::project(root, &temporary.path().join("plugins"), &inputs)?;
        } else {
            fs::create_dir(temporary.path().join("plugins"))?;
        }
        lenso_app_authoring::load_resolved_app(temporary.path())?
    } else {
        // An initialized custom Host has no source provenance; its Catalog is
        // already the existing authoritative contract. Optional support uses
        // the same package metadata as a local source, not another Plan schema.
        if root.join("Cargo.toml").exists()
            || root.join("package.json").exists()
            || root.join("app").is_dir()
            || root.join("lenso.toml").is_file()
        {
            let discovery = lenso_app_authoring::discovery::discover(root)?;
            for candidate in &discovery.candidates {
                if let Some(declaration) = declaration(candidate)? {
                    declarations.insert(candidate.plugin_id.clone(), declaration);
                }
            }
            candidates.extend(discovery.candidates);
        }
        lenso_app_authoring::load_resolved_app(root)
            .context("required existing generated Host contracts; available: no resolvable Host; no build started")?
    };
    for instance in resolved.instances() {
        if let Some(candidate) = candidates
            .iter()
            .find(|candidate| candidate.plugin_id == instance.id().plugin_id())
        {
            let selected = resolved
                .plan()
                .plugin_instance(instance.plan_key())
                .context("selected source Instance missing from Plan")?;
            check_inputs(
                candidate,
                &instance.id().to_string(),
                selected.execution_class().as_str(),
            )?;
        }
    }
    Ok((resolved, declarations))
}

pub(super) fn declaration(candidate: &Candidate) -> anyhow::Result<Option<Declaration>> {
    if !matches!(candidate.format.as_str(), "cargo" | "bun") {
        return Ok(None);
    }
    let value: Value = if candidate.format == "cargo" {
        let metadata = fs::symlink_metadata(&candidate.metadata)?;
        ensure!(
            metadata.file_type().is_file() && metadata.len() <= 1024 * 1024,
            "invalid Cargo support metadata"
        );
        toml::from_str(&fs::read_to_string(&candidate.metadata)?)?
    } else {
        read_json(&candidate.metadata)?
    };
    let metadata = if candidate.format == "cargo" {
        value.pointer("/package/metadata/lenso-cli")
    } else {
        value.get("lenso")
    };
    let Some(metadata) = metadata else {
        return Ok(None);
    };
    // Explicit source identities can share one package without becoming
    // separate implementation packages. Package-wide support is the default.
    let support = metadata
        .get("plugins")
        .and_then(|plugins| plugins.get(&candidate.plugin_id))
        .and_then(|plugin| plugin.get("support"))
        .or_else(|| metadata.get("support"));
    support.map(|value| {
        let declaration = serde_json::from_value(value.clone())
            .with_context(|| format!("Plugin `{}` support: use exact combinations, not separate environment/storage axis lists", candidate.plugin_id))?;
        validate_declaration(&declaration)?;
        Ok(declaration)
    }).transpose()
}

pub(super) fn check_inputs(
    candidate: &Candidate,
    instance: &str,
    execution: &str,
) -> anyhow::Result<()> {
    if candidate.format == "bundle" {
        return Ok(());
    }
    // Only inspect selected source inputs; no Cargo metadata, package hooks,
    // imports, --version probes or business code run during this command.
    let mut matched = false;
    for implementation in &candidate.implementations {
        let matching = matches!(
            (implementation.runtime.as_str(), execution),
            ("bun", "lenso.bun-process@1")
                | ("native-linked", "lenso.native-rust@1")
                | ("process", "lenso.process@1")
                | ("wasm", "lenso.wasm-component@1")
        );
        if !matching {
            continue;
        }
        matched = true;
        let root = &implementation.project;
        match implementation.runtime.as_str() {
            "bun" => {
                tool("bun", instance)?;
                let value = read_json(&root.join("package.json"))?;
                let entry = value
                    .pointer("/lenso/source")
                    .and_then(Value::as_str)
                    .unwrap_or("src/plugin.ts");
                regular_inside(root, entry).with_context(|| format!("instance `{instance}`: required source entry/export `{entry}`; available: missing or invalid"))?;
                if value
                    .get("dependencies")
                    .is_some_and(|v| v.as_object().is_some_and(|v| !v.is_empty()))
                {
                    ensure!(
                        root.join("node_modules").is_dir(),
                        "instance `{instance}`: required installed dependencies; available: node_modules missing"
                    );
                }
            }
            "native-linked" | "process" | "wasm" => {
                tool("cargo", instance)?;
                tool("rustc", instance)?;
                let manifest: Value =
                    toml::from_str(&fs::read_to_string(root.join("Cargo.toml"))?)?;
                let entry = manifest
                    .pointer("/lib/path")
                    .and_then(Value::as_str)
                    .unwrap_or(if implementation.runtime == "process" {
                        "src/main.rs"
                    } else {
                        "src/lib.rs"
                    });
                regular_inside(root, entry).with_context(|| format!("instance `{instance}`: required source entry/export `{entry}`; available: missing or invalid"))?;
                for section in ["dependencies", "build-dependencies"] {
                    for dependency in manifest[section]
                        .as_object()
                        .into_iter()
                        .flat_map(|m| m.values())
                    {
                        if let Some(path) = dependency.get("path").and_then(Value::as_str) {
                            ensure!(
                                root.join(path).join("Cargo.toml").is_file(),
                                "instance `{instance}`: required local dependency manifest `{path}`; available: missing"
                            );
                        }
                    }
                }
            }
            other => bail!(
                "instance `{instance}`: required execution `{other}`; available: no offline source checker"
            ),
        }
    }
    ensure!(
        matched,
        "instance `{instance}`: required execution `{execution}`; available source implementations: {:?}",
        candidate
            .implementations
            .iter()
            .map(|implementation| &implementation.runtime)
            .collect::<Vec<_>>()
    );
    Ok(())
}

fn regular_inside(root: &Path, relative: &str) -> anyhow::Result<()> {
    let relative = Path::new(relative);
    ensure!(
        !relative.as_os_str().is_empty()
            && relative
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_))),
        "entry must be a contained relative export"
    );
    let path = root
        .join(relative)
        .canonicalize()
        .context("required source entry/export is missing")?;
    ensure!(
        path.starts_with(root.canonicalize()?) && fs::symlink_metadata(path)?.file_type().is_file(),
        "source entry/export escapes its package"
    );
    Ok(())
}

fn tool(name: &str, instance: &str) -> anyhow::Result<()> {
    let available = std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|directory| {
            let path = directory.join(name);
            fs::metadata(path).is_ok_and(|metadata| {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
                }
                #[cfg(not(unix))]
                {
                    metadata.is_file()
                }
            })
        })
    });
    ensure!(
        available,
        "instance `{instance}`: required tool `{name}`; available: absent from PATH"
    );
    Ok(())
}
