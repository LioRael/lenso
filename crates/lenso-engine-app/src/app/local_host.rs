//! Generate a native Host using normal Cargo package identities for linked
//! Plugins and their typed contract projections. Never regenerate native types.
use anyhow::{Context, bail};
use lenso_app_authoring::discovery::Candidate;
use lenso_app_plan::authoring::{HostCatalog, PluginDescriptor};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct AdapterSet {
    bun: bool,
    process: bool,
    wasm: bool,
}

impl AdapterSet {
    pub(super) fn from_candidates(candidates: &[Candidate]) -> anyhow::Result<Self> {
        // Discovery describes the Host's closed release set before portable
        // bundles have been built. Include every declared implementation class;
        // runtime admission below still rejects classes absent from this set.
        let mut declared = Self::default();
        for candidate in candidates {
            for implementation in &candidate.implementations {
                declared.include_runtime(&implementation.runtime)?;
            }
        }
        Ok(declared)
    }

    fn include_runtime(&mut self, runtime: &str) -> anyhow::Result<()> {
        match runtime {
            "native-linked" | "lenso.native-rust@1" => {}
            "bun" | "lenso.bun-process@1" => self.bun = true,
            "process" | "lenso.process@1" => self.process = true,
            "wasm" | "lenso.wasm-component@1" => self.wasm = true,
            other => bail!("generated Host cannot assemble unknown execution class `{other}`"),
        }
        Ok(())
    }

    pub(super) fn admits_portable(&self, execution_class: &str) -> bool {
        match execution_class {
            "lenso.bun-process@1" => self.bun,
            "lenso.process@1" => self.process,
            "lenso.wasm-component@1" => self.wasm,
            _ => false,
        }
    }
}

pub(super) fn generate(
    stage: &Path,
    cache: &Path,
    candidates: &[Candidate],
    adapters: AdapterSet,
) -> anyhow::Result<Vec<PluginDescriptor>> {
    fs::create_dir_all(cache)?;
    let lock = fs::File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(cache.join("build.lock"))?;
    lock.lock().context("lock generated Host build cache")?;
    let generated = cache.join("source");
    fs::create_dir_all(generated.join("src"))?;
    let mut dependencies = BTreeMap::<String, Value>::new();
    // Native Plugins can be authored against an in-progress local Lenso
    // checkout. The generated Host must use the same package identities for
    // Kernel, Adapter, and codec types; a Cargo version string alone would
    // otherwise admit a registry copy alongside a path copy and split the
    // native Plugin registration inventory.
    let mut local_lenso_patches = BTreeMap::<String, (String, Value)>::new();
    let mut git_lenso_source = None;
    let mut host_framework_dependencies = BTreeSet::<String>::new();
    for (name, version) in [
        ("anyhow", "1"),
        ("futures", "0.3"),
        ("serde_json", "1"),
        ("sha2", "0.10"),
        ("tempfile", "3"),
        ("lenso-app-plan", "=0.4.5"),
        ("lenso-kernel", "=0.3.11"),
        ("lenso-native-adapter", "=0.3.15"),
        ("lenso-runner", "=0.2.17"),
    ] {
        dependencies.insert(name.into(), json!(version));
        if name.starts_with("lenso-") {
            host_framework_dependencies.insert(name.into());
        }
    }
    dependencies.insert(
        "serde".into(),
        json!({"version":"1", "features":["derive"]}),
    );
    dependencies.insert("rustix".into(), json!({"version":"1.1", "features":["fs"]}));
    dependencies.insert(
        "tokio".into(),
        json!({"version":"1.52", "features":["rt-multi-thread","macros","signal","time","net"]}),
    );
    let mut linked = String::new();
    let mut codecs = BTreeMap::<String, (String, Value)>::new();
    let mut seen_packages = BTreeSet::new();
    let mut codec_cohorts = BTreeSet::new();
    let mut web_contract = None;
    let mut watch_roots = BTreeSet::new();
    for (index, candidate) in candidates.iter().enumerate() {
        // A portable Cargo Guest can depend on a rust-runtime projection for
        // its own SDK without making that projection a Host-linked contract.
        // Portable-only Capabilities use the Host's erased JSON codec instead.
        if candidate.format != "cargo" || !is_native(candidate) {
            continue;
        }
        let output = super::cargo_command()
            .args(["metadata", "--format-version=1", "--filter-platform"])
            .arg(lenso_app_authoring::native_host_target())
            .arg("--manifest-path")
            .arg(candidate.project.join("Cargo.toml"))
            .output()
            .context("read native Cargo graph")?;
        if !output.status.success() {
            bail!(
                "Cargo metadata for {}: {}",
                candidate.project.display(),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let metadata: Value = serde_json::from_slice(&output.stdout)?;
        let packages = metadata["packages"].as_array().context("Cargo packages")?;
        for package in packages
            .iter()
            .filter(|package| package["source"].is_null())
        {
            if let Some(path) = package["manifest_path"]
                .as_str()
                .and_then(|path| Path::new(path).parent())
            {
                watch_roots.insert(path.to_path_buf());
            }
        }

        let manifest = fs::canonicalize(candidate.project.join("Cargo.toml"))?;
        let package = packages
            .iter()
            .find(|p| {
                p["manifest_path"]
                    .as_str()
                    .is_some_and(|p| Path::new(p) == manifest)
            })
            .context("selected Cargo package is missing")?;
        let native = is_native(candidate);
        if native {
            if !package["targets"]
                .as_array()
                .context("Cargo targets")?
                .iter()
                .any(|t| {
                    t["kind"]
                        .as_array()
                        .is_some_and(|k| k.iter().any(|k| k == "lib" || k == "rlib"))
                })
            {
                bail!(
                    "native Plugin {} needs a Rust library target",
                    candidate.plugin_id
                );
            }
            let alias = format!("local_plugin_{index}");
            dependencies.insert(alias.clone(), dependency(package)?);
            linked.push_str(&format!("{alias}::link_plugin();\n"));
        }
        // Only normal reachable dependencies are eligible: test/build helper
        // contracts must not introduce runtime identities or competing versions.
        let nodes = metadata
            .pointer("/resolve/nodes")
            .and_then(Value::as_array)
            .context("resolved Cargo graph")?;
        let mut pending = vec![
            package["id"]
                .as_str()
                .context("Cargo package ID")?
                .to_owned(),
        ];
        while let Some(id) = pending.pop() {
            if native {
                let package = packages
                    .iter()
                    .find(|p| p["id"] == id)
                    .context("reachable Cargo package")?;
                collect_local_lenso_patch(&mut local_lenso_patches, package)?;
                collect_git_lenso_source(&mut git_lenso_source, package)?;
            }
            if !seen_packages.insert(id.clone()) {
                continue;
            }
            if let Some(node) = nodes.iter().find(|n| n["id"] == id) {
                for dep in node["deps"].as_array().context("Cargo dependencies")? {
                    if dep["dep_kinds"]
                        .as_array()
                        .is_some_and(|k| k.iter().any(|k| k["kind"].is_null()))
                    {
                        pending.push(dep["pkg"].as_str().context("Cargo dependency ID")?.into());
                    }
                }
            }
            let package = packages
                .iter()
                .find(|p| p["id"] == id)
                .context("reachable Cargo package")?;
            if native && package["name"] == "lenso-capability-http-endpoint" {
                let dependency = dependency(package)?;
                if web_contract
                    .as_ref()
                    .is_some_and(|previous| previous != &dependency)
                {
                    bail!("Web endpoints use different Cargo contract identities");
                }
                web_contract = Some(dependency);
            }
            let Some(contract) = package.pointer("/metadata/lenso/contract") else {
                continue;
            };
            if contract["projection"] != "rust-runtime" {
                continue;
            }
            let node = nodes
                .iter()
                .find(|n| n["id"] == id)
                .context("contract Cargo node")?;
            let codec_id = node["deps"]
                .as_array()
                .context("contract dependencies")?
                .iter()
                .find(|d| d["name"] == "lenso_runtime_codec")
                .context("rust-runtime projection must depend on lenso-runtime-codec")?["pkg"]
                .clone();
            let codec = packages
                .iter()
                .find(|p| p["id"] == codec_id)
                .context("contract Codec package")?;
            let version = codec["version"].as_str().context("Codec version")?;
            codec_cohorts.insert(version.split('.').take(2).collect::<Vec<_>>().join("."));
            let manifest = Path::new(
                package["manifest_path"]
                    .as_str()
                    .context("contract manifest")?,
            );
            let descriptor: Value = serde_json::from_slice(&fs::read(
                manifest.parent().context("contract directory")?.join(
                    contract["descriptor"]
                        .as_str()
                        .context("contract descriptor")?,
                ),
            )?)?;
            let capability = descriptor["id"].as_str().context("contract id")?.to_owned();
            let dep = dependency(package)?;
            if let Some((previous, _)) = codecs.get(&capability) {
                if previous != &id {
                    bail!(
                        "Capability {capability} has competing Rust package identities; align native contract dependencies"
                    );
                }
            } else {
                codecs.insert(capability, (id, dep));
            }
        }
    }
    if codec_cohorts.len() > 1 {
        bail!("native contracts use incompatible Codec cohorts: {codec_cohorts:?}");
    }
    // Adapters have historically changed the Codec cohort in patch releases.
    // Pin a tested set instead of allowing Cargo to mix distinct traits.
    let cohort = codec_cohorts.first().map_or("0.4", String::as_str);
    let local_crates = local_framework_crates_dir(&local_lenso_patches)?;
    let versions = match cohort {
        "0.3" => [
            ("lenso-bun-adapter", "=0.1.8"),
            ("lenso-process-adapter", "=0.3.5"),
            ("lenso-wasm-component-adapter", "=0.2.8"),
            ("lenso-runtime-codec", "=0.3.4"),
        ],
        "0.4" => [
            ("lenso-bun-adapter", "=0.1.14"),
            ("lenso-process-adapter", "=0.3.12"),
            ("lenso-wasm-component-adapter", "=0.2.16"),
            ("lenso-runtime-codec", "=0.4.2"),
        ],
        other => bail!("unsupported typed Codec cohort {other}; use a custom Host"),
    };
    for (name, version) in versions {
        let enabled = match name {
            "lenso-bun-adapter" => adapters.bun,
            "lenso-process-adapter" => adapters.process,
            "lenso-wasm-component-adapter" => adapters.wasm,
            _ => true,
        };
        if enabled {
            let dependency = if name.ends_with("-adapter") {
                if let Some(crates) = &local_crates {
                    let (dependency, path) = local_framework_dependency(crates, name, version)?;
                    watch_roots.insert(path);
                    dependency
                } else {
                    json!(version)
                }
            } else {
                json!(version)
            };
            dependencies.insert(name.into(), dependency);
            host_framework_dependencies.insert(name.into());
        }
    }
    if cohort == "0.3" {
        dependencies.insert(
            "native-resources".into(),
            json!({"package":"lenso-runtime-codec","version":"=0.4.2"}),
        );
        host_framework_dependencies.insert("native-resources".into());
    }
    let web_ingress = web_contract
        .as_ref()
        .map(web_ingress_dependency)
        .transpose()?;
    if let Some(path) = web_ingress
        .as_ref()
        .and_then(|ingress| ingress["path"].as_str())
    {
        watch_roots.insert(PathBuf::from(path));
    }
    fs::write(
        cache.join("watch-roots.json"),
        serde_json::to_vec_pretty(&watch_roots)?,
    )?;
    let local_inputs = watch_roots
        .iter()
        .map(|path| Ok((path.clone(), input_digest(path)?)))
        .collect::<anyhow::Result<BTreeMap<_, _>>>()?;
    let ids = codecs
        .keys()
        .map(|id| format!("{id:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    let terminal_enabled = candidates
        .iter()
        .any(|c| c.plugin_id == "lenso.terminal.cli");
    if terminal_enabled && cohort != "0.4" {
        bail!("bundled terminal support requires runtime-codec 0.4 contracts");
    }
    let mut terminal_aliases = BTreeMap::new();
    let mut register = format!("let typed = std::collections::BTreeSet::<&str>::from([{ids}]);\n");
    for (index, (capability, (_, dependency))) in codecs.into_iter().enumerate() {
        let alias = format!("local_contract_{index}");
        dependencies.insert(alias.clone(), dependency);
        if capability == "lenso.terminal.command@1"
            || capability == "lenso.terminal.command-provider@1"
        {
            terminal_aliases.insert(capability.clone(), alias.clone());
        }
        let name = codec_name(&capability)?;
        if adapters.bun {
            register.push_str(&format!("let bun = bun.with_codec(LegacyBunCodec({alias}::{name})).with_authoring_codec({alias}::{name});\n"));
        }
        if adapters.process {
            register.push_str(&format!(
                "let process = process.with_codec({alias}::{name});\n"
            ));
        }
        if adapters.wasm {
            register.push_str(&format!("let wasm = wasm.with_codec({alias}::{name});\n"));
        }
    }
    if terminal_enabled {
        for (name, version) in [
            ("lenso-contract-runtime", "0.2.0"),
            ("lenso-plugin-authoring", "0.2.0"),
            ("lenso-guest-sdk", "0.5.0"),
            ("shell-words", "1.1"),
        ] {
            dependencies.insert(name.into(), json!(version));
            if name.starts_with("lenso-") {
                host_framework_dependencies.insert(name.into());
            }
        }
        dependencies.insert("clap".into(), json!({"version":"4", "features":["string"]}));
        fs::create_dir_all(generated.join("src/terminal"))?;
        let mut module = include_str!("terminal/mod.rs").to_owned();
        for (id, name, codec, body) in [
            (
                "lenso.terminal.command@1",
                "command",
                "CommandJsonCodec",
                include_str!("terminal/command.rs"),
            ),
            (
                "lenso.terminal.command-provider@1",
                "provider",
                "CommandProviderJsonCodec",
                include_str!("terminal/provider.rs"),
            ),
        ] {
            if let Some(alias) = terminal_aliases.get(id) {
                module = module.replace(
                    &format!("pub mod {name};"),
                    &format!("pub use {alias} as {name};"),
                );
            } else {
                fs::write(generated.join(format!("src/terminal/{name}.rs")), body)?;
                if adapters.bun {
                    register.push_str(&format!(
                        "let bun = bun.with_authoring_codec(terminal::{name}::{codec});\n"
                    ));
                }
                if adapters.process {
                    register.push_str(&format!(
                        "let process = process.with_codec(terminal::{name}::{codec});\n"
                    ));
                }
                if adapters.wasm {
                    register.push_str(&format!(
                        "let wasm = wasm.with_codec(terminal::{name}::{codec});\n"
                    ));
                }
            }
        }
        register.push_str("let mut typed = typed; typed.insert(terminal::command::CAPABILITY_ID); typed.insert(terminal::provider::CAPABILITY_ID);\n");
        fs::write(generated.join("src/terminal/mod.rs"), module)?;
        fs::write(
            generated.join("src/terminal/parser.rs"),
            include_str!("terminal/parser.rs"),
        )?;
    }
    let web = web_contract.is_some();
    if let Some(contract) = web_contract {
        dependencies.insert("local-web-contract".into(), contract.clone());
        // A Git-pinned Endpoint contract must bring the matching Ingress from
        // that exact Web source too. Otherwise Cargo may select a registry
        // Ingress with incompatible Host/Kernel identities.
        dependencies.insert(
            "lenso-web-ingress-plugin".into(),
            web_ingress.context("Web Endpoint needs a matching Ingress")?,
        );
    }
    let mut source = (include_str!("local_runtime_template.rs").to_owned()
        + include_str!("local_json_template.rs"))
    .replace("// LENSO_REGISTER_CODECS", &register)
    .replace("// LENSO_LINK_PLUGINS", &linked);
    source = source.replace("// LENSO_TERMINAL_RUN", if terminal_enabled { "if let Some(args) = &command_args { command_result = terminal::run(&app, args).await; }" } else { "if command_args.is_some() { command_result = Err(anyhow::anyhow!(\"CLI support is not adopted\")); }" });
    if terminal_enabled {
        source.push_str("\n#[allow(dead_code)] mod terminal;\n");
    }
    source = source.replace(
        "// LENSO_NATIVE_RESOURCES",
        if cohort == "0.3" {
            ""
        } else {
            "use lenso_runtime_codec as native_resources;"
        },
    );
    source = source.replace("// LENSO_DESCRIBE_WEB", if web { r#"
        let mut releases = catalog.plugins().to_vec();
        if releases.iter().any(|r| r.descriptor().provided_capabilities().iter().any(|c| c.capability_id() == local_web_contract::CAPABILITY_ID)) {
            releases.push(lenso_app_plan::authoring::HostPluginRelease::new(lenso_web_ingress_plugin::WebIngressFactory::plugin_descriptor()));
        }
        let catalog = HostCatalog::new([], releases, []);
"# } else { "" });
    source = source.replace(
        "// LENSO_RUNTIME_WEB",
        if web {
            r#"
    let ingress = lenso_web_ingress_plugin::WebIngressFactory::new();
    let native = native.with_factory(ingress.clone());
"#
        } else {
            ""
        },
    );
    source = source.replace("// LENSO_WEB_READY", if web { r#"
            if let Some(address) = ingress.local_address() { eprintln!("Listening on http://{address}"); }
"# } else { "" });
    if let Some((git, rev)) = git_lenso_source {
        pin_host_framework_versions(
            &mut dependencies,
            &host_framework_dependencies,
            &git,
            &rev,
        );
    }
    let patches = local_lenso_patches
        .into_iter()
        .map(|(name, (_, dependency))| (name, dependency))
        .collect::<BTreeMap<_, _>>();
    let manifest = json!({"package":{"name":"lenso-generated-local-host", "version":"0.0.0", "edition":"2024"}, "workspace":{}, "dependencies": dependencies, "patch":{"crates-io":patches}});
    fs::write(
        generated.join("Cargo.toml"),
        toml::to_string_pretty(&manifest)?,
    )?;
    fs::write(generated.join("src/main.rs"), source)?;
    let mut build_script = "fn main() { println!(\"cargo:rustc-check-cfg=cfg(generated_native_host)\"); println!(\"cargo:rustc-cfg=generated_native_host\");".to_owned();
    for (enabled, name) in [
        (adapters.bun, "generated_bun_adapter"),
        (adapters.process, "generated_process_adapter"),
        (adapters.wasm, "generated_wasm_adapter"),
    ] {
        build_script.push_str(&format!(
            " println!(\"cargo:rustc-check-cfg=cfg({name})\");"
        ));
        if enabled {
            build_script.push_str(&format!(" println!(\"cargo:rustc-cfg={name}\");"));
        }
    }
    build_script.push_str(" }\n");
    fs::write(generated.join("build.rs"), build_script)?;

    let output = super::cargo_command()
        .args([
            "build",
            "--release",
            "--message-format=json-render-diagnostics",
            "--manifest-path",
        ])
        .arg(generated.join("Cargo.toml"))
        .stderr(std::process::Stdio::inherit())
        .output()
        .context("build generated local Host")?;
    if !output.status.success() {
        bail!("generated local Host build failed");
    }
    for (path, before) in &local_inputs {
        if &input_digest(path)? != before {
            bail!(
                "native path dependency changed during build: {}; retry after edits settle",
                path.display()
            );
        }
    }
    let binary = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|message| {
            message["reason"] == "compiler-artifact"
                && message["target"]["name"] == "lenso-generated-local-host"
        })
        .find_map(|message| message["executable"].as_str().map(PathBuf::from))
        .context("Cargo did not report the generated Host executable")?;
    let output = super::build_command(&binary).arg("--describe").output()?;
    if !output.status.success() {
        bail!(
            "linked Host Descriptor failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let catalog: HostCatalog = serde_json::from_slice(&output.stdout)?;
    fs::copy(binary, stage.join(".lenso/host"))?;
    let provenance = stage.join(".lenso/generated-host");
    fs::create_dir_all(provenance.join("src"))?;
    fs::write(
        provenance.join("local-inputs.json"),
        serde_json::to_vec_pretty(&local_inputs)?,
    )?;
    for file in ["Cargo.toml", "Cargo.lock", "src/main.rs", "build.rs"] {
        fs::copy(generated.join(file), provenance.join(file))?;
    }

    let descriptors = catalog
        .plugins()
        .iter()
        .map(|r| r.descriptor().clone())
        .collect::<Vec<_>>();
    let mut expected = candidates
        .iter()
        .filter(|c| is_native(c))
        .map(|c| c.plugin_id.as_str())
        .collect::<BTreeSet<_>>();
    if web
        && descriptors
            .iter()
            .any(|d| d.plugin_id() == "lenso.web-ingress")
    {
        expected.insert("lenso.web-ingress");
    }
    let actual = descriptors
        .iter()
        .map(|d| d.plugin_id())
        .collect::<BTreeSet<_>>();
    if expected != actual || descriptors.len() != actual.len() {
        bail!(
            "linked Plugin registry differs from selected local sources: expected {expected:?}, linked {actual:?}"
        );
    }
    Ok(descriptors)
}

pub(super) fn is_native(candidate: &Candidate) -> bool {
    candidate
        .implementations
        .iter()
        .any(|i| i.runtime == "native-linked")
}

pub(super) fn dependency(package: &Value) -> anyhow::Result<Value> {
    let name = package["name"].as_str().context("Cargo name")?;
    let version = package["version"].as_str().context("Cargo version")?;
    match package["source"].as_str() {
        None => Ok(
            json!({"package":name,"path":Path::new(package["manifest_path"].as_str().context("Cargo manifest")?).parent().context("Cargo directory")?}),
        ),
        Some("registry+https://github.com/rust-lang/crates.io-index") => {
            Ok(json!({"package":name,"version":format!("={version}")}))
        }
        Some(source) if source.starts_with("git+") => {
            let (url, rev) = source[4..]
                .rsplit_once('#')
                .context("Cargo git source needs exact commit")?;
            let url = url.split('?').next().context("Cargo git URL")?;
            Ok(json!({"package":name,"git":url,"rev":rev}))
        }
        Some(source) => bail!(
            "unsupported contract registry {source}; use a custom Host for alternate registry dependencies"
        ),
    }
}

fn pin_host_framework_versions(
    dependencies: &mut BTreeMap<String, Value>,
    host_framework_dependencies: &BTreeSet<String>,
    git: &str,
    rev: &str,
) {
    for name in host_framework_dependencies {
        let Some(dependency) = dependencies.get_mut(name) else {
            continue;
        };
        let expected_package = if name == "native-resources" {
            "lenso-runtime-codec"
        } else {
            name.as_str()
        };
        let actual_package = match dependency.get("package") {
            Some(Value::String(package)) => package.as_str(),
            None => name.as_str(),
            _ => continue,
        };
        if actual_package != expected_package {
            continue;
        }
        match dependency {
            Value::String(version) => {
                let version = version.clone();
                *dependency = json!({"version":version,"git":git,"rev":rev});
            }
            Value::Object(table)
                if table.get("version").and_then(Value::as_str).is_some()
                    && ["path", "git", "registry", "branch", "tag"]
                        .iter()
                        .all(|key| !table.contains_key(*key)) =>
            {
                table.insert("git".into(), json!(git));
                table.insert("rev".into(), json!(rev));
            }
            _ => {}
        }
    }
}

fn collect_local_lenso_patch(
    patches: &mut BTreeMap<String, (String, Value)>,
    package: &Value,
) -> anyhow::Result<()> {
    if !package["source"].is_null() {
        return Ok(());
    }
    let name = package["name"].as_str().context("Cargo package name")?;
    if name != "lenso" && !name.starts_with("lenso-") {
        return Ok(());
    }
    let id = package["id"].as_str().context("Cargo package ID")?;
    let dependency = dependency(package)?;
    if let Some((previous_id, previous_dependency)) = patches.get(name) {
        if previous_id != id && previous_dependency != &dependency {
            bail!(
                "native Plugins use incompatible local {name} package identities; align their Lenso dependency sources before generating one Host"
            );
        }
        return Ok(());
    }
    patches.insert(name.to_owned(), (id.to_owned(), dependency));
    Ok(())
}

fn local_framework_crates_dir(
    patches: &BTreeMap<String, (String, Value)>,
) -> anyhow::Result<Option<PathBuf>> {
    let mut sources = Vec::new();
    for name in [
        "lenso",
        "lenso-app-plan",
        "lenso-kernel",
        "lenso-native-adapter",
        "lenso-runtime-codec",
        "lenso-plugin-authoring",
        "lenso-contract-runtime",
        "lenso-capability-http-endpoint",
    ] {
        let Some((_, dependency)) = patches.get(name) else {
            continue;
        };
        let path = Path::new(dependency["path"].as_str().context("local Lenso path")?);
        let crates = path.parent().context("local Lenso package parent")?;
        let root = if path.file_name().is_some_and(|part| part == name)
            && crates.file_name().is_some_and(|part| part == "crates")
        {
            Some(fs::canonicalize(crates)?)
        } else {
            None
        };
        sources.push((name, root));
    }
    let Some(selected) = sources.iter().find_map(|(_, root)| root.clone()) else {
        return Ok(None);
    };
    for (name, root) in sources {
        if root.as_ref() != Some(&selected) {
            bail!(
                "native Plugin uses {name} outside the selected local Lenso workspace {}; align framework dependency sources",
                selected.display()
            );
        }
    }
    Ok(Some(selected))
}

fn local_framework_dependency(
    crates: &Path,
    name: &str,
    version: &str,
) -> anyhow::Result<(Value, PathBuf)> {
    let path = crates.join(name);
    let manifest: toml::Value = toml::from_str(
        &fs::read_to_string(path.join("Cargo.toml"))
            .with_context(|| format!("local Lenso workspace is missing {name}"))?,
    )?;
    if manifest["package"]["name"].as_str() != Some(name) {
        bail!("local Lenso workspace package {name} has a different Cargo identity");
    }
    let actual = manifest["package"]["version"]
        .as_str()
        .with_context(|| format!("local Lenso {name} has no package version"))?;
    if actual != version.trim_start_matches('=') {
        bail!("local Lenso {name} version {actual} does not match generated Host cohort {version}");
    }
    Ok((json!({"package":name,"path":path,"version":version}), path))
}

fn collect_git_lenso_source(
    selected: &mut Option<(String, String)>,
    package: &Value,
) -> anyhow::Result<()> {
    let name = package["name"].as_str().context("Cargo package name")?;
    if name != "lenso" && !name.starts_with("lenso-") {
        return Ok(());
    }
    let dependency = dependency(package)?;
    let (Some(git), Some(rev)) = (dependency["git"].as_str(), dependency["rev"].as_str()) else {
        return Ok(());
    };
    let source = (git.to_owned(), rev.to_owned());
    if selected
        .as_ref()
        .is_some_and(|previous| previous != &source)
    {
        bail!(
            "native Plugins use incompatible Lenso Git source revisions; align their dependencies before generating one Host"
        );
    }
    *selected = Some(source);
    Ok(())
}

fn web_ingress_dependency(contract: &Value) -> anyhow::Result<Value> {
    let mut ingress = contract.clone();
    let fields = ingress
        .as_object_mut()
        .context("Web Endpoint dependency must be a Cargo table")?;
    if fields.contains_key("git") {
        fields.insert(
            "package".into(),
            Value::String("lenso-web-ingress-plugin".into()),
        );
        return Ok(ingress);
    }
    if let Some(path) = fields.get("path").and_then(Value::as_str) {
        let path = Path::new(path);
        if path
            .file_name()
            .is_some_and(|name| name == "lenso-capability-http-endpoint")
            && path
                .parent()
                .is_some_and(|parent| parent.file_name().is_some_and(|name| name == "crates"))
        {
            let (dependency, _) = local_framework_dependency(
                path.parent().context("Endpoint crates directory")?,
                "lenso-web-ingress-plugin",
                "=0.4.7",
            )?;
            return Ok(dependency);
        }
    }
    // A standalone local Endpoint package still uses the matching published
    // Ingress dependency, recorded exactly in the generated Cargo lock.
    Ok(json!("=0.4.6"))
}

fn codec_name(capability: &str) -> anyhow::Result<String> {
    let name = capability
        .split('@')
        .next()
        .and_then(|s| s.rsplit('.').next())
        .context("Capability name")?;
    let mut output = String::new();
    for part in name.split(|c: char| !c.is_ascii_alphanumeric()) {
        if part.is_empty() || part.chars().all(char::is_numeric) {
            continue;
        }
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            output.extend(first.to_uppercase());
            output.push_str(chars.as_str());
        }
    }
    if output.is_empty() {
        output.push_str("Value");
    }
    Ok(format!("{output}JsonCodec"))
}

pub(super) fn digest(path: &Path) -> anyhow::Result<String> {
    Ok(format!(
        "sha256:{}",
        Sha256::digest(fs::read(path)?)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ))
}

pub(super) fn finalize(stage: &Path, runtime_artifacts: Vec<Value>) -> anyhow::Result<()> {
    fs::create_dir_all(stage.join("runtime"))?;
    let native = stage.join(".lenso/host").is_file();
    if !native {
        fs::copy(
            super::preset::runtime_executable()?,
            stage.join(".lenso/host"),
        )?;
    }
    fs::write(
        stage.join(".lenso/host-mode"),
        if native { "native" } else { "portable" },
    )?;

    fs::copy(
        super::preset::runtime_executable()?,
        stage.join("runtime/lenso-resolver"),
    )?;
    if runtime_artifacts
        .iter()
        .any(|a| a["execution_class"] == "lenso.bun-process@1")
    {
        let bun = executable_on_path("bun")?;
        fs::copy(bun, stage.join("runtime/bun"))?;
    }
    fs::create_dir_all(stage.join("intent/.lenso"))?;
    fs::write(stage.join("intent/.lenso/plugin-root-authoring.lock"), [])?;
    fs::copy(
        stage.join(".lenso/host-build.json"),
        stage.join("intent/.lenso/host-build.json"),
    )?;
    if stage.join("plugins").exists() {
        super::assemble::copy_root(
            &stage.join("plugins"),
            &stage.join("intent/plugins"),
            0,
            &mut 0,
        )?;
    }
    let mut files = vec![
        ".lenso/host",
        ".lenso/host-mode",
        ".lenso/host-build.json",
        "runtime/lenso-resolver",
        "bundles.json",
        "runtime-codecs.json",
        "local-sources.json",
        ".lenso/precompiled-host.json",
        ".lenso/generated-host/Cargo.lock",
        ".lenso/generated-host/Cargo.toml",
        ".lenso/generated-host/src/main.rs",
        ".lenso/generated-host/build.rs",
        ".lenso/generated-host/local-inputs.json",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    if stage.join("runtime/bun").exists() {
        files.push("runtime/bun".into());
    }
    files.extend(
        runtime_artifacts
            .iter()
            .filter_map(|a| a["path"].as_str().map(str::to_owned)),
    );
    let mut proofs = files
        .into_iter()
        .filter(|path| stage.join(path).is_file())
        .map(|path| {
            let role = match path.as_str() {
                ".lenso/host" => "host_runtime",
                ".lenso/host-build.json" => "host_authority",
                ".lenso/host-mode" => "host_entrypoint",
                "runtime/lenso-resolver" => "runtime_resolver",
                "runtime/bun" => "javascript_runtime",
                "bundles.json" => "bundle_inventory",
                "local-sources.json" => "source_provenance",
                _ if path.starts_with("runtime/artifacts/") => "plugin_artifact",
                _ => "build_provenance",
            };
            Ok(super::prepare::DistributionFile {
                sha256: digest(&stage.join(&path))?,
                size: fs::metadata(stage.join(&path))?.len(),
                executable: matches!(
                    role,
                    "host_runtime" | "runtime_resolver" | "javascript_runtime"
                ),
                role: role.into(),
                path,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let inventory: Vec<Value> = serde_json::from_slice(&fs::read(stage.join("bundles.json"))?)?;
    for bundle in inventory {
        let path = bundle["path"].as_str().context("bundle inventory path")?;
        proofs.push(super::prepare::DistributionFile {
            path: path.into(),
            role: "plugin_bundle".into(),
            sha256: digest(&stage.join(path))?,
            size: fs::metadata(stage.join(path))?.len(),
            executable: false,
        });
    }
    let authority: lenso_app_authoring::host_authoring::GeneratedHostBuild =
        serde_json::from_slice(&fs::read(stage.join(".lenso/host-build.json"))?)?;
    let target = lenso_app_authoring::native_host_target();
    let (platform, arch) = super::prepare::target_platform(target)?;
    let lock = super::prepare::DistributionLock {
        schema: "lenso.local-host-distribution.v1",
        app_id: authority.host_id().into(),
        target: target.into(),
        platform,
        arch,
        files: proofs,
    };
    fs::write(
        stage.join(".lenso/distribution.lock.json"),
        serde_json::to_vec_pretty(&lock)?,
    )?;
    Ok(())
}

fn executable_on_path(name: &str) -> anyhow::Result<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|p| p.join(format!("{name}{}", std::env::consts::EXE_SUFFIX)))
        .find(|p| p.is_file())
        .with_context(|| format!("{name} executable is required to prepare an offline local Host"))
}

/// Content evidence for authored inputs; generated dependency lockfiles are
/// captured separately by Cargo/the package builder and may be created on first build.
pub(super) fn input_digest(root: &Path) -> anyhow::Result<String> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    let mut visited = 0;
    while let Some(path) = pending.pop() {
        visited += 1;
        if visited > 50_000 {
            bail!("source input exceeds 50,000 entries");
        }
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                let name = entry.file_name();
                if name.to_str().is_some_and(|name| {
                    [
                        ".git",
                        ".lenso",
                        "target",
                        "node_modules",
                        "dist",
                        "build",
                        ".next",
                        ".venv",
                        "__pycache__",
                        "Cargo.lock",
                        "bun.lock",
                        "bun.lockb",
                        "package-lock.json",
                        "pnpm-lock.yaml",
                    ]
                    .contains(&name)
                }) {
                    continue;
                }
                pending.push(entry.path());
            }
        } else if metadata.is_file() {
            files.push(path);
        } else {
            bail!(
                "source input contains a symlink or special file: {}",
                path.display()
            );
        }
        if pending.len() + files.len() > 50_000 {
            bail!("source input exceeds 50,000 entries");
        }
    }
    files.sort();
    let mut hasher = Sha256::new();
    let mut size = 0;
    for path in files {
        let relative = path.strip_prefix(root)?.to_string_lossy();
        size += fs::metadata(&path)?.len();
        if size > 256 * 1024 * 1024 {
            bail!("source input exceeds 256 MiB");
        }
        let bytes = fs::read(&path)?;
        hasher.update((relative.len() as u64).to_be_bytes());
        hasher.update(relative.as_bytes());
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    }
    Ok(format!(
        "sha256:{}",
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    ))
}

/// Existing dependency locks are authoritative build inputs. A first build may
/// create an absent lock, but it may not silently replace one seen at planning.
pub(super) fn dependency_lock_digests<'a>(
    roots: impl IntoIterator<Item = &'a Path>,
) -> anyhow::Result<BTreeMap<PathBuf, String>> {
    let mut locks = BTreeMap::new();
    for root in roots {
        for name in [
            "Cargo.lock",
            "bun.lock",
            "bun.lockb",
            "package-lock.json",
            "pnpm-lock.yaml",
            "yarn.lock",
            "npm-shrinkwrap.json",
        ] {
            let path = root.join(name);
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                bail!("dependency lock is not a regular file: {}", path.display());
            }
            if metadata.len() > 32 * 1024 * 1024 {
                bail!("dependency lock exceeds 32 MiB: {}", path.display());
            }
            locks.insert(path.clone(), digest(&path)?);
        }
    }
    Ok(locks)
}

pub(super) fn verify_dependency_lock_digests(
    expected: &BTreeMap<PathBuf, String>,
) -> anyhow::Result<()> {
    for (path, digest_before) in expected {
        let metadata = fs::symlink_metadata(path).with_context(|| {
            format!(
                "dependency lock disappeared during build: {}",
                path.display()
            )
        })?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() > 32 * 1024 * 1024
            || digest(path)? != *digest_before
        {
            bail!(
                "dependency lock changed during build: {}; update the lock and retry",
                path.display()
            );
        }
    }
    Ok(())
}

pub(super) fn host_arguments(root: &Path) -> anyhow::Result<Vec<&'static str>> {
    match fs::read_to_string(root.join(".lenso/host-mode"))?.as_str() {
        "native" => Ok(Vec::new()),
        "portable" => Ok(vec!["app", "__run-local", "--"]),
        _ => bail!("unsupported local Host entrypoint"),
    }
}

pub(super) fn digest_text(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use serde_json::json;

    use super::{
        AdapterSet, collect_local_lenso_patch, dependency, dependency_lock_digests,
        local_framework_crates_dir, local_framework_dependency, pin_host_framework_versions,
        verify_dependency_lock_digests, web_ingress_dependency,
    };

    #[test]
    fn git_framework_source_does_not_rewrite_signed_vendor_paths() {
        let git = "https://github.com/LioRael/lenso";
        let rev = "8e6eb5eb9f468959eea713eab5f20592dfe65a71";
        let vendor = dependency(&json!({
            "name": "lenso-secrets-env-plugin",
            "version": "0.1.8",
            "source": null,
            "manifest_path": "/app/vendor/lenso/lenso.secrets.env/0.1.8/Cargo.toml"
        }))
        .unwrap();
        let local_adapter = json!({
            "package": "lenso-wasm-component-adapter",
            "path": "/work/lenso/crates/lenso-wasm-component-adapter",
            "version": "=0.2.16"
        });
        let mut dependencies = BTreeMap::from([
            ("lenso-app-plan".into(), json!("=0.4.5")),
            (
                "lenso-native-adapter".into(),
                json!({"version":"=0.3.15", "features":["test-support"]}),
            ),
            ("lenso-wasm-component-adapter".into(), local_adapter.clone()),
            ("local_plugin_0".into(), vendor.clone()),
            (
                "local_plugin_1".into(),
                json!({"package":"lenso-kernel","version":"=0.3.11"}),
            ),
        ]);
        let framework = BTreeSet::from([
            "lenso-app-plan".into(),
            "lenso-native-adapter".into(),
            "lenso-wasm-component-adapter".into(),
        ]);

        pin_host_framework_versions(&mut dependencies, &framework, git, rev);

        assert_eq!(dependencies["local_plugin_0"], vendor);
        assert_eq!(dependencies["lenso-wasm-component-adapter"], local_adapter);
        assert!(dependencies["local_plugin_1"].get("git").is_none());
        assert_eq!(dependencies["lenso-app-plan"]["version"], "=0.4.5");
        assert_eq!(dependencies["lenso-app-plan"]["git"], git);
        assert_eq!(dependencies["lenso-app-plan"]["rev"], rev);
        assert_eq!(
            dependencies["lenso-native-adapter"]["features"],
            json!(["test-support"])
        );
        assert_eq!(dependencies["lenso-native-adapter"]["git"], git);

        let manifest = json!({
            "package": {"name":"lenso-generated-local-host", "version":"0.0.0", "edition":"2024"},
            "dependencies": dependencies
        });
        let rendered = toml::to_string_pretty(&manifest).unwrap();
        let parsed: toml::Value = toml::from_str(&rendered).unwrap();
        assert_eq!(
            parsed["dependencies"]["local_plugin_0"]["path"].as_str(),
            Some("/app/vendor/lenso/lenso.secrets.env/0.1.8")
        );
        assert_eq!(
            parsed["dependencies"]["lenso-app-plan"]["version"].as_str(),
            Some("=0.4.5")
        );
    }

    #[test]
    fn git_framework_source_rejects_renamed_and_alternate_registry_dependencies() {
        let renamed = json!({"package":"unrelated-package", "version":"=0.1.0"});
        let alternate_registry = json!({"version":"=0.5.0", "registry":"private"});
        let mut dependencies = BTreeMap::from([
            ("lenso-kernel".into(), renamed.clone()),
            ("lenso-guest-sdk".into(), alternate_registry.clone()),
        ]);
        let framework = BTreeSet::from(["lenso-kernel".into(), "lenso-guest-sdk".into()]);

        pin_host_framework_versions(
            &mut dependencies,
            &framework,
            "https://example.invalid/lenso",
            "abc123",
        );

        assert_eq!(dependencies["lenso-kernel"], renamed);
        assert_eq!(dependencies["lenso-guest-sdk"], alternate_registry);
    }

    #[test]
    fn generated_host_only_links_declared_execution_classes() {
        let mut adapters = AdapterSet::default();
        adapters.include_runtime("native-linked").unwrap();
        assert_eq!(adapters, AdapterSet::default());
        adapters.include_runtime("lenso.process@1").unwrap();
        adapters.include_runtime("wasm").unwrap();
        assert_eq!(
            adapters,
            AdapterSet {
                bun: false,
                process: true,
                wasm: true,
            }
        );
        assert!(adapters.admits_portable("lenso.process@1"));
        assert!(adapters.admits_portable("lenso.wasm-component@1"));
        assert!(!adapters.admits_portable("lenso.bun-process@1"));
        assert!(!adapters.admits_portable("lenso.native-rust@1"));
        adapters.include_runtime("bun").unwrap();
        assert!(adapters.bun);
        assert!(adapters.include_runtime("unrecognized-runtime").is_err());
    }

    #[test]
    fn existing_dependency_lock_change_is_rejected_but_first_lock_is_allowed() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("Cargo.lock");
        let absent = dependency_lock_digests([root.path()]).unwrap();
        assert!(absent.is_empty());
        std::fs::write(&path, b"first lock").unwrap();
        verify_dependency_lock_digests(&absent).unwrap();

        let pinned = dependency_lock_digests([root.path()]).unwrap();
        verify_dependency_lock_digests(&pinned).unwrap();
        std::fs::write(&path, b"changed lock").unwrap();
        assert!(verify_dependency_lock_digests(&pinned).is_err());
        std::fs::remove_file(&path).unwrap();
        assert!(verify_dependency_lock_digests(&pinned).is_err());

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.path().join("other.lock"), &path).unwrap();
            assert!(dependency_lock_digests([root.path()]).is_err());
            assert!(verify_dependency_lock_digests(&pinned).is_err());
        }
    }

    fn local_package(name: &str, id: &str, manifest: &str) -> serde_json::Value {
        json!({
            "name": name,
            "id": id,
            "source": null,
            "version": "0.3.9",
            "manifest_path": manifest,
        })
    }

    #[test]
    fn local_lenso_runtime_packages_become_generated_host_patches() {
        let mut patches = BTreeMap::new();
        collect_local_lenso_patch(
            &mut patches,
            &local_package(
                "lenso-native-adapter",
                "path+file:///work/lenso/crates/lenso-native-adapter#0.3.14",
                "/work/lenso/crates/lenso-native-adapter/Cargo.toml",
            ),
        )
        .expect("local Lenso package should be admitted");
        collect_local_lenso_patch(
            &mut patches,
            &local_package(
                "lenso-kernel",
                "path+file:///work/lenso/crates/lenso-kernel#0.3.9",
                "/work/lenso/crates/lenso-kernel/Cargo.toml",
            ),
        )
        .expect("local Kernel should be admitted");

        assert_eq!(patches.len(), 2);
        assert_eq!(
            patches["lenso-native-adapter"].1,
            json!({
                "package": "lenso-native-adapter",
                "path": "/work/lenso/crates/lenso-native-adapter",
            })
        );
    }

    #[test]
    fn local_lenso_workspace_supplies_the_matching_unpublished_adapter() {
        let root = tempfile::tempdir().unwrap();
        let crates = root.path().join("crates");
        let adapter = crates.join("lenso-wasm-component-adapter");
        std::fs::create_dir_all(&adapter).unwrap();
        std::fs::write(
            adapter.join("Cargo.toml"),
            "[package]\nname = \"lenso-wasm-component-adapter\"\nversion = \"0.2.16\"\n",
        )
        .unwrap();
        let mut patches = BTreeMap::new();
        collect_local_lenso_patch(
            &mut patches,
            &local_package(
                "lenso",
                "path+file:///work/lenso/crates/lenso#0.5.25",
                &crates.join("lenso/Cargo.toml").to_string_lossy(),
            ),
        )
        .unwrap();
        let found = local_framework_crates_dir(&patches).unwrap().unwrap();
        assert_eq!(found, std::fs::canonicalize(crates).unwrap());
        let (dependency, watched_path) =
            local_framework_dependency(&found, "lenso-wasm-component-adapter", "=0.2.16").unwrap();
        let adapter = std::fs::canonicalize(adapter).unwrap();
        assert_eq!(dependency["path"], adapter.to_string_lossy().as_ref());
        assert_eq!(watched_path, adapter);
        assert!(
            local_framework_dependency(&found, "lenso-wasm-component-adapter", "=0.2.15")
                .unwrap_err()
                .to_string()
                .contains("does not match generated Host cohort")
        );
        std::fs::write(
            adapter.join("Cargo.toml"),
            "[package]\nname = \"other-adapter\"\nversion = \"0.2.16\"\n",
        )
        .unwrap();
        assert!(
            local_framework_dependency(&found, "lenso-wasm-component-adapter", "=0.2.16")
                .unwrap_err()
                .to_string()
                .contains("different Cargo identity")
        );
    }

    #[test]
    fn local_framework_runtime_packages_cannot_mix_workspaces() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        std::fs::create_dir(first.path().join("crates")).unwrap();
        std::fs::create_dir(second.path().join("crates")).unwrap();
        let mut patches = BTreeMap::new();
        collect_local_lenso_patch(
            &mut patches,
            &local_package(
                "lenso",
                "path+file:///first/crates/lenso#0.5.25",
                &first
                    .path()
                    .join("crates/lenso/Cargo.toml")
                    .to_string_lossy(),
            ),
        )
        .unwrap();
        collect_local_lenso_patch(
            &mut patches,
            &local_package(
                "lenso-native-adapter",
                "path+file:///second/crates/lenso-native-adapter#0.3.15",
                &second
                    .path()
                    .join("crates/lenso-native-adapter/Cargo.toml")
                    .to_string_lossy(),
            ),
        )
        .unwrap();
        assert!(
            local_framework_crates_dir(&patches)
                .unwrap_err()
                .to_string()
                .contains("align framework dependency sources")
        );
    }

    #[test]
    fn conflicting_local_lenso_sources_are_rejected_before_host_build() {
        let mut patches = BTreeMap::new();
        collect_local_lenso_patch(
            &mut patches,
            &local_package(
                "lenso-kernel",
                "path+file:///first/lenso-kernel#0.3.9",
                "/first/lenso-kernel/Cargo.toml",
            ),
        )
        .expect("first local Kernel source");
        let error = collect_local_lenso_patch(
            &mut patches,
            &local_package(
                "lenso-kernel",
                "path+file:///second/lenso-kernel#0.3.9",
                "/second/lenso-kernel/Cargo.toml",
            ),
        )
        .expect_err("different local Kernel sources cannot share one generated Host");
        assert!(
            error
                .to_string()
                .contains("incompatible local lenso-kernel package identities")
        );
    }

    #[test]
    fn git_pinned_endpoint_keeps_the_ingress_on_the_same_web_source() {
        let dependency = web_ingress_dependency(&json!({
            "package": "lenso-capability-http-endpoint",
            "git": "https://github.com/LioRael/lenso",
            "rev": "c9cd15629b7d65d6f6cdc12113234acd85c89a89",
        }))
        .unwrap();
        assert_eq!(dependency["package"], "lenso-web-ingress-plugin");
        assert_eq!(dependency["git"], "https://github.com/LioRael/lenso");
        assert_eq!(
            dependency["rev"],
            "c9cd15629b7d65d6f6cdc12113234acd85c89a89"
        );
    }

    #[test]
    fn local_endpoint_uses_sibling_ingress_from_the_same_workspace() {
        let root = tempfile::tempdir().unwrap();
        let endpoint = root.path().join("crates/lenso-capability-http-endpoint");
        let ingress = root.path().join("crates/lenso-web-ingress-plugin");
        std::fs::create_dir_all(&endpoint).unwrap();
        std::fs::create_dir_all(&ingress).unwrap();
        std::fs::write(
            ingress.join("Cargo.toml"),
            "[package]\nname = \"lenso-web-ingress-plugin\"\nversion = \"0.4.7\"\n",
        )
        .unwrap();
        let dependency = web_ingress_dependency(&json!({
            "package": "lenso-capability-http-endpoint",
            "path": endpoint,
        }))
        .unwrap();
        assert_eq!(dependency["path"], ingress.to_string_lossy().as_ref());
        assert_eq!(dependency["version"], "=0.4.7");
    }
}
