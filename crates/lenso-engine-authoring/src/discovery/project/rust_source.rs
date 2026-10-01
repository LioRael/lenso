//! Bounded source-first discovery of independently identified native Plugins.
use super::*;
use std::path::PathBuf;
use syn::{Item, LitStr, Visibility};

pub(super) fn read(
    root: &Path,
    inputs: &lenso_engine::discovery::DiscoverySession,
) -> anyhow::Result<Vec<Candidate>> {
    let manifest = root.join("Cargo.toml");
    if !manifest.is_file() {
        return Ok(Vec::new());
    }
    let value = document_in(&manifest, inputs)?;
    if value.get("package").is_none() {
        return Ok(Vec::new());
    }
    let entry = root.join(
        value
            .pointer("/lib/path")
            .and_then(Value::as_str)
            .unwrap_or("src/lib.rs"),
    );
    if !entry.is_file() {
        return Ok(Vec::new());
    }
    let mut declarations = Vec::new();
    let mut visited = Traversal {
        active: BTreeSet::new(),
        visits: 0,
        inputs: inputs.scope(root)?,
    };
    walk(root, &entry, &[], &mut visited, &mut declarations)?;
    if declarations.is_empty() {
        return Ok(Vec::new());
    }
    let version = cargo_version_in(root, &value, inputs)?;
    validate_release_version(&version)?;
    declarations
        .into_iter()
        .map(|(id, link)| {
            classify_existing_plugin_id(&id)?;
            Ok(Candidate {
                native_link: Some(link),
                composite: None,
                surface_owner: None,
                plugin_id: id,
                release_version: version.clone(),
                project: root.to_path_buf(),
                metadata: manifest.clone(),
                format: "cargo".into(),
                // Discovery offers availability. Even an App-owned module requires
                // explicit Plugin Root selection, unlike the single-package fallback.
                role: SourceRole::Shared,
                implementations: vec![implementation("native-linked", "native-linked", root)],
                published_resources: Vec::new(),
                evidence: "rust_source_identity".into(),
            })
        })
        .collect()
}

struct Traversal {
    active: BTreeSet<PathBuf>,
    visits: usize,
    inputs: lenso_engine::discovery::DiscoverySession,
}

fn walk(
    root: &Path,
    file: &Path,
    modules: &[String],
    visited: &mut Traversal,
    found: &mut Vec<(String, String)>,
) -> anyhow::Result<()> {
    let file = fs::canonicalize(file)?;
    if !file.starts_with(fs::canonicalize(root)?) {
        bail!("Plugin source escapes its Cargo package");
    }
    if modules.len() > 32 || visited.visits >= 1024 {
        bail!("Rust Plugin source discovery limit exceeded");
    }
    if !visited.active.insert(file.clone()) {
        bail!("recursive Rust module source: {}", file.display());
    }
    visited.visits += 1;
    let source = super::super::read_metadata_in(&file, &visited.inputs)?;
    let parsed = syn::parse_file(&source)
        .with_context(|| format!("parse Rust Plugin source {}", file.display()))?;
    let directory = if modules.is_empty()
        || file
            .file_name()
            .is_some_and(|n| n == "lib.rs" || n == "mod.rs")
    {
        file.parent().unwrap().to_path_buf()
    } else {
        file.with_extension("")
    };
    let result = items(root, &parsed.items, &directory, modules, visited, found);
    visited.active.remove(&file);
    result
}

fn items(
    root: &Path,
    declarations: &[Item],
    directory: &Path,
    modules: &[String],
    visited: &mut Traversal,
    found: &mut Vec<(String, String)>,
) -> anyhow::Result<()> {
    let mut module_identity = None;
    for item in declarations {
        if let Item::Struct(plugin) = item {
            for attr in &plugin.attrs {
                let path = attr.path();
                if path.segments.last().is_none_or(|s| s.ident != "plugin") {
                    continue;
                }
                if path.segments.len() > 1
                    && path
                        .segments
                        .first()
                        .is_some_and(|s| s.ident != "lenso" && s.ident != "lenso_native_adapter")
                {
                    continue;
                }
                if matches!(attr.meta, syn::Meta::Path(_)) {
                    continue;
                }
                let mut id = None;
                let mut slot = None;
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("id") {
                        if id.is_some() {
                            return Err(meta.error("duplicate Plugin id"));
                        }
                        id = Some(meta.value()?.parse::<LitStr>()?.value());
                    } else if meta.path.is_ident("root_slot") {
                        if slot.is_some() {
                            return Err(meta.error("duplicate Plugin root_slot"));
                        }
                        slot = Some(meta.value()?.parse::<LitStr>()?.value());
                    } else if meta.input.peek(syn::Token![=]) {
                        let _: syn::Expr = meta.value()?.parse()?;
                    }
                    Ok(())
                })?;
                if id.is_none() && slot.is_none() {
                    continue;
                }
                let id = id.context("source Plugin id and root_slot must be declared together")?;
                let slot =
                    slot.context("source Plugin id and root_slot must be declared together")?;
                if slot.is_empty() {
                    bail!("Plugin root_slot must be non-empty");
                }
                if !matches!(plugin.vis, Visibility::Public(_)) {
                    bail!("source-discovered Plugin {id} must be public");
                }
                if plugin
                    .attrs
                    .iter()
                    .any(|a| a.path().is_ident("cfg") || a.path().is_ident("cfg_attr"))
                {
                    bail!(
                        "conditional Plugin {id} requires an explicit custom Host; source discovery cannot select Cargo features"
                    );
                }
                if module_identity.replace(id.clone()).is_some() {
                    bail!("put each independently identified Plugin in its own public Rust module");
                }
                let link = modules
                    .iter()
                    .cloned()
                    .chain(["link_plugin".into()])
                    .collect::<Vec<_>>()
                    .join("::");
                found.push((id, link));
            }
        }
        if let Item::Mod(module) = item {
            // Private and conditional modules are not public linkage surfaces.
            if !matches!(module.vis, Visibility::Public(_))
                || module
                    .attrs
                    .iter()
                    .any(|a| a.path().is_ident("cfg") || a.path().is_ident("cfg_attr"))
            {
                continue;
            }
            let mut path = modules.to_vec();
            path.push(module.ident.to_string());
            let child = directory.join(module.ident.to_string());
            if let Some((_, content)) = &module.content {
                items(root, content, &child, &path, visited, found)?;
            } else {
                let explicit = module
                    .attrs
                    .iter()
                    .find(|a| a.path().is_ident("path"))
                    .map(|a| {
                        let syn::Meta::NameValue(meta) = &a.meta else {
                            bail!("invalid Rust module path");
                        };
                        let syn::Expr::Lit(lit) = &meta.value else {
                            bail!("invalid Rust module path");
                        };
                        let syn::Lit::Str(path) = &lit.lit else {
                            bail!("invalid Rust module path");
                        };
                        Ok(directory.join(path.value()))
                    })
                    .transpose()?;
                let source = explicit.unwrap_or_else(|| {
                    let flat = child.with_extension("rs");
                    if flat.is_file() {
                        flat
                    } else {
                        child.join("mod.rs")
                    }
                });
                walk(root, &source, &path, visited, found)?;
            }
        }
    }
    Ok(())
}
