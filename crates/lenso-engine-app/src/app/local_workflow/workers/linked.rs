//! Static linked Rust source lowering into the existing Workers Kernel Host.
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, ensure};
use lenso_app_plan::{CapabilityOperationKind, ResolvedAppPlan, RestartPolicy};
use serde_json::{Value, json};

const PROFILE: &str = "lenso.linked-rust-workers@2";
const BINDGEN_VERSION: &str = "wasm-bindgen 0.2.127";
const WORKERS_DRIVER_REQUIREMENT: &str = "=0.1.2";
mod js;
mod limits;
const RUNTIME_FILES: &[(&str, &str)] = &[
    (
        "host.mjs",
        "d6315c2105e4f0389789d755a42aa40209e8421d11403b10f79b5a61b754041c",
    ),
    (
        "http.mjs",
        "a234f1bc5b0d3cbfec08809edfa400715c0834b6ce8a3c0c3cc1588bfd9cc72b",
    ),
    (
        "runner.mjs",
        "a2abb3e199c377918f32f026d495b82d919be3eb73e40fdb936baa3854a1dcc6",
    ),
    (
        "scope.mjs",
        "880ac5f58793020ea95d41c72e51057d5d3d31ddcb7d5321aec21ba787271e43",
    ),
    (
        "clock.mjs",
        "f8ddb0befe68c65e91a51421ea1fa40a616a4f1b5617c299966d2e758948ce33",
    ),
];

#[allow(clippy::too_many_lines)]
pub(super) fn build(args: super::BuildArgs) -> anyhow::Result<()> {
    ensure!(
        args.jco.is_none() && args.integration.is_none() && args.trust_integration.is_none(),
        "the linked-Rust Workers profile does not admit Component or arbitrary Host integration inputs"
    );
    let bindgen = args
        .wasm_bindgen
        .as_deref()
        .context("linked Workers build needs wasm-bindgen")?;
    let version = crate::app::build_command(bindgen)
        .arg("--version")
        .output()?;
    ensure!(
        version.status.success()
            && String::from_utf8_lossy(&version.stdout).trim() == BINDGEN_VERSION,
        "linked Workers build requires {BINDGEN_VERSION}"
    );
    let runtime = load_runtime(&args.workers_runtime, args.facilities.is_some())?;
    let host_limits = limits::load(args.host_limits.as_deref())?;
    let (event_limits, scope_limits) = limits::split(&host_limits, args.facilities.is_some());
    let root = args.root.canonicalize()?;
    let source_digest = crate::app::local_host::input_digest(&root)?;
    let report = lenso_app_authoring::discovery::discover(&root)?;
    let conventions = lenso_app_authoring::discovery::conventions::plan(&report)?;
    ensure!(
        conventions.compilations.is_empty(),
        "linked Workers does not run convention compilers"
    );
    let selected = conventions
        .candidates
        .iter()
        .filter(|candidate| {
            candidate.role != lenso_app_authoring::discovery::SourceRole::Shared
                || candidate.surface_owner.is_some()
                || root.join("plugins").join(&candidate.plugin_id).exists()
        })
        .collect::<Vec<_>>();
    ensure!(
        !selected.is_empty(),
        "linked Workers found no selected Rust source Plugins"
    );
    for candidate in &selected {
        ensure!(
            (candidate.format == "cargo" && crate::app::local_host::is_native(candidate))
                || candidate.format == "bun",
            "linked Workers rejects Plugin {}: select a linked Cargo implementation, not {}",
            candidate.plugin_id,
            candidate.format
        );
        ensure!(
            candidate.published_resources.is_empty(),
            "linked Workers rejects published resources for Plugin {}; this profile has no resource loader",
            candidate.plugin_id
        );
    }
    let destination = std::path::absolute(&args.out)?;
    ensure!(
        !destination
            .components()
            .any(|part| part == std::path::Component::ParentDir),
        "Workers output cannot contain .."
    );
    ensure!(
        !destination.exists(),
        "Workers output already exists: {}",
        destination.display()
    );
    let parent = destination
        .parent()
        .context("Workers output needs a parent")?;
    fs::create_dir_all(parent)?;
    let parent = parent.canonicalize()?;
    ensure!(
        !parent.starts_with(&root) || parent == root,
        "Workers output inside the source App must be a direct child"
    );
    let destination = parent.join(
        destination
            .file_name()
            .context("Workers output needs a name")?,
    );
    ensure!(
        destination != root.join(".lenso"),
        "reserved .lenso output directory"
    );
    let stage = crate::app::assemble::stage_output(&root, &parent)?;
    let js_artifacts = js::compile(&selected, stage.path(), &args.workers_runtime)?;
    let native_stage = tempfile::tempdir()?;
    let native = native_stage.path().join("native");
    crate::app::assemble::assemble(crate::app::assemble::AssembleArgs {
        root: Some(root.clone()),
        id: "local.app".into(),
        out: native.clone(),
        json: false,
        executable: true,
        trust_linked_build: args.trust_linked_build,
        portable_implementations: Vec::new(),
        host_many_slots: args.host_many_slots,
    })?;
    let resolved = lenso_app_authoring::load_resolved_app(&native)?;
    let target_plan = js::lower(resolved.plan(), &js_artifacts)?;
    admit(&target_plan)?;
    let generated = stage.path().join(".lenso/generated-host");
    fs::create_dir_all(generated.join("src"))?;
    let native_source = native.join(".lenso/generated-host");
    let manifest: Value = toml::from_str(&fs::read_to_string(native_source.join("Cargo.toml"))?)?;
    let mut manifest = wasm_manifest(manifest)?;
    fs::copy(
        native_source.join("src/plugin_links.rs"),
        generated.join("src/plugin_links.rs"),
    )
    .context("retain selected source Plugin linkage for Workers")?;
    let js_module = if !js_artifacts.is_empty() {
        Some(js::prepare(&native, &generated, &mut manifest)?)
    } else {
        None
    };
    let mut worker_scope = String::new();
    let mut facility_evidence = Vec::new();
    let mut grants_digest = None;
    let mut host_source = include_str!("linked_host.rs")
        .replace("// LENSO_LINK_PLUGINS", "include!(\"plugin_links.rs\");");
    host_source = host_source.replace(
        "// LENSO_WORKERS_JS_IMPORT",
        js_module.as_deref().unwrap_or(""),
    );
    host_source = host_source.replace("// LENSO_WORKERS_JS_ADAPTER", if js_module.is_some() {
        "let adapters = lenso_kernel::ExecutionAdapterCatalog::single(registry).with_adapter(js_host::WorkersJsAdapter::new(scope.clone())).map_err(error)?;"
    } else { "let adapters = lenso_kernel::ExecutionAdapterCatalog::single(registry);" });
    if let Some(path) = &args.facilities {
        let bytes = super::runtime::read_file(path)?;
        ensure!(
            bytes.len() <= 1_048_576,
            "Workers facility grant size limit"
        );
        let grants: Value =
            serde_json::from_slice(&bytes).context("invalid Workers facility grants")?;
        let sources = crate::app::local_host::facilities::read_sources(&native)?;
        let (factories, scope, evidence) = crate::app::local_host::facilities::render_workers(
            &sources,
            &grants,
            resolved.plan(),
            stage.path(),
            &scope_limits,
        )?;
        worker_scope = scope;
        facility_evidence = evidence;
        grants_digest = Some(super::digest_bytes(&serde_json::to_vec(&grants)?));
        fs::write(
            stage.path().join(".lenso/host-facility-grants.json"),
            serde_json::to_vec_pretty(&grants)?,
        )?;
        let public_sources = sources
            .iter()
            .cloned()
            .map(|mut source| {
                source.workers_adapter = None;
                source
            })
            .collect::<Vec<_>>();
        fs::write(
            stage.path().join(".lenso/host-facility-sources.json"),
            serde_json::to_vec_pretty(&public_sources)?,
        )?;
        host_source = host_source
            .replace("// LENSO_WORKERS_FACILITY_IMPORT", "#[wasm_bindgen(raw_module = \"@lenso/workers-runtime/facilities\")] extern \"C\" { #[wasm_bindgen(catch, js_name = facility)] fn instance_facility(scope: &JsValue, instance: &str, slot: &str) -> Result<JsValue, JsValue>; }")
            .replace("// LENSO_WORKERS_FACILITY_PREPARE", &format!("let mut facilities = lenso_native_adapter::NativeInstanceFacilities::new();\n{factories}"))
            .replace("// LENSO_WORKERS_FACILITY_BIND", "let registry = registry.with_facilities(facilities);");
    } else {
        for marker in [
            "// LENSO_WORKERS_FACILITY_IMPORT",
            "// LENSO_WORKERS_FACILITY_PREPARE",
            "// LENSO_WORKERS_FACILITY_BIND",
        ] {
            host_source = host_source.replace(marker, "");
        }
    }
    fs::write(
        generated.join("Cargo.toml"),
        toml::to_string_pretty(&manifest)?,
    )?;
    fs::write(generated.join("src/lib.rs"), host_source)?;
    fs::write(
        generated.join("src/response_session.rs"),
        include_str!("linked_response_session.rs"),
    )?;
    let plan = serde_json::to_vec(&target_plan)?;
    fs::write(generated.join("src/plan.json"), &plan)?;
    fs::write(
        stage.path().join(".lenso/resolved-plan.json"),
        serde_json::to_vec(resolved.plan())?,
    )?;
    let status = crate::app::cargo_command()
        .args(["generate-lockfile", "--manifest-path"])
        .arg(generated.join("Cargo.toml"))
        .status()?;
    ensure!(status.success(), "resolve linked Workers Host Cargo lock");
    let output = crate::app::cargo_command()
        .args([
            "rustc",
            "--locked",
            "--release",
            "--lib",
            "--target",
            "wasm32-unknown-unknown",
            "--message-format=json-render-diagnostics",
            "--manifest-path",
        ])
        .arg(generated.join("Cargo.toml"))
        .args(["--", "-C", "link-arg=--export=__wasm_call_ctors"])
        .stderr(std::process::Stdio::inherit())
        .output()
        .context("compile linked Workers Host")?;
    ensure!(
        output.status.success(),
        "linked Workers Host does not compile for wasm32-unknown-unknown"
    );
    let artifact = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|message| {
            message["reason"] == "compiler-artifact"
                && message["target"]["name"] == "lenso_generated_workers_host"
        })
        .flat_map(|message| message["filenames"].as_array().cloned().unwrap_or_default())
        .filter_map(|name| name.as_str().map(PathBuf::from))
        .find(|name| {
            name.extension()
                .is_some_and(|extension| extension == "wasm")
        })
        .context("Cargo did not report the linked Workers Wasm artifact")?;
    let output = crate::app::build_command(bindgen)
        .arg(artifact)
        .args([
            "--target",
            "web",
            "--experimental-reset-state-function",
            "--out-name",
            "host",
            "--out-dir",
        ])
        .arg(stage.path())
        .output()?;
    ensure!(
        output.status.success(),
        "wasm-bindgen failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bindings = rewrite_bindings(&fs::read_to_string(stage.path().join("host.js"))?)?;
    fs::write(stage.path().join("host.js"), bindings)?;
    fs::create_dir_all(stage.path().join("runtime"))?;
    for module in &runtime.modules {
        fs::write(
            stage.path().join("runtime").join(module.name),
            &module.bytes,
        )?;
    }
    fs::write(
        stage.path().join("worker.mjs"),
        include_str!("../../../../assets/workers-linked-stream.mjs")
            .replace("// LENSO_SCOPE", &worker_scope)
            .replace(
                "/* LENSO_LIMITS */ {}",
                &serde_json::to_string(&event_limits)?,
            ),
    )?;
    fs::write(
        stage.path().join("wrangler.jsonc"),
        include_str!("../../../../assets/workers-linked-wrangler.jsonc"),
    )?;
    fs::write(
        stage.path().join("README.md"),
        "Static linked Rust Workers candidate. Run `wrangler dev --local --config wrangler.jsonc`. The exact resolved graph uses the Workers Driver and Kernel for every HTTP event. Request and Stream Capabilities and one main lane are admitted. HTTP response chunks are pulled incrementally. Internally, the JavaScript response reader reports clean EOF only after the provider's successful terminal and clean request-App shutdown. External HTTP completion is a separate, unqualified boundary; the strict local workerd abnormal-EOF check is currently failing. Cancellation and session limits retain the event generation until cleanup completes or bounded generation abandonment. Plugin memory is recreated per event; this does not prove durable state, D1, PostgreSQL, deployed Workers, dynamic loading or Event Capability support.\n",
    )?;
    for name in [".lenso/host-build.json", "local-sources.json"] {
        fs::copy(native.join(name), stage.path().join(name))
            .with_context(|| format!("copy linked Workers provenance {name}"))?;
    }
    if native.join("plugins").exists() {
        crate::app::assemble::copy_root(
            &native.join("plugins"),
            &stage.path().join("plugins"),
            0,
            &mut 0,
        )?;
    }
    let receipt = json!({
        "schema": "lenso.workers-app-build.v1", "target": "workers", "environment": "local-workerd",
        "profile": PROFILE, "execution_lowering": if js_module.is_some() { "native-bun-v2-to-workers-js-v2" } else { "linked-rust" },
        "resolved_plan_digest": super::digest_bytes(&serde_json::to_vec(resolved.plan())?),
        "js_implementation_digests": js_artifacts, "builder_version": env!("CARGO_PKG_VERSION"),
        "compatibility_date": "2026-09-26",
        "source_digest": source_digest,
        "plan_digest": super::digest_bytes(&plan), "plugin_instances": resolved.plan().plugin_instances().len(),
        "capability_bindings": resolved.plan().capability_bindings().len(),
        "host_wasm_digest": crate::app::local_host::digest(&stage.path().join("host_bg.wasm"))?,
        "host_bindings_digest": crate::app::local_host::digest(&stage.path().join("host.js"))?,
        "worker_entry_digest": crate::app::local_host::digest(&stage.path().join("worker.mjs"))?,
        "workers_runtime": {"package":"@lenso/workers-runtime", "version":runtime.package_version, "module_digests":runtime.modules.iter().map(|module| (module.name,super::digest_bytes(&module.bytes))).collect::<BTreeMap<_,_>>()},
        "host_facilities": {"grants_digest":grants_digest,"owner_modules":facility_evidence},
        "host_limits":host_limits,
        "wasm_bindgen": BINDGEN_VERSION, "state_lifetime": "event", "remote_deployment": "not_run",
    });
    fs::write(
        stage.path().join("workers-build.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    ensure!(
        crate::app::local_host::input_digest(&root)? == source_digest,
        "App source changed during linked Workers build; retry"
    );
    let local_inputs: BTreeMap<PathBuf, String> =
        serde_json::from_slice(&fs::read(native_source.join("local-inputs.json"))?)?;
    for (path, expected) in local_inputs {
        ensure!(
            crate::app::local_host::input_digest(&path)? == expected,
            "linked source dependency changed during Workers build: {}",
            path.display()
        );
    }
    ensure!(
        limits::load(args.host_limits.as_deref())? == host_limits,
        "Workers Host limits changed during build; retry"
    );
    if let Some(path) = &args.facilities {
        let current: Value = serde_json::from_slice(&super::runtime::read_file(path)?)?;
        ensure!(
            Some(super::digest_bytes(&serde_json::to_vec(&current)?)) == grants_digest,
            "Workers facility grants changed during build; retry"
        );
    }
    crate::app::preset::checkpoint()?;
    crate::app::build::publish_new_output(stage.path(), &destination)?;
    println!(
        "Built static linked Workers App at {}",
        destination.display()
    );
    Ok(())
}

fn admit(plan: &ResolvedAppPlan) -> anyhow::Result<()> {
    ensure!(
        plan.execution_lanes().len() == 1,
        "linked Workers supports one main execution lane"
    );
    ensure!(
        plan.plugin_instances()
            .iter()
            .any(|instance| instance.package_id() == "lenso.web-ingress"),
        "linked Workers requires the selected HTTP Ingress"
    );
    for instance in plan.plugin_instances() {
        ensure!(
            matches!(
                instance.execution_class().as_str(),
                "lenso.native-rust@1" | "lenso.workers-js@1"
            ) && instance.execution_lane().as_str() == "main",
            "linked Workers rejects unsupported execution for {}",
            instance.instance_key()
        );
        ensure!(
            instance.restart_policy() == RestartPolicy::never(),
            "linked Workers cannot preserve Plugin supervision across events: {}",
            instance.instance_key()
        );
        for capability in instance.provided_capabilities() {
            ensure!(
                capability.operations().iter().all(|operation| matches!(
                    capability.operation_kind(operation),
                    Some(CapabilityOperationKind::Request | CapabilityOperationKind::Stream)
                )),
                "linked Workers rejects Event Capability {} on {}",
                capability.capability_id(),
                instance.instance_key()
            );
        }
    }
    ensure!(
        !plan.plugin_instances().iter().any(|instance| instance
            .provided_capabilities()
            .iter()
            .any(|capability| capability.capability_id() == "lenso.websocket.endpoint@1")),
        "linked Workers does not admit WebSocket Endpoint providers"
    );
    Ok(())
}

fn rewrite_bindings(source: &str) -> anyhow::Result<String> {
    let mut bindings = source.to_owned();
    for name in ["clock", "http", "facilities"] {
        for quote in ["'", "\""] {
            bindings = bindings.replace(
                &format!("from {quote}@lenso/workers-runtime/{name}{quote}"),
                &format!("from {quote}./runtime/{name}.mjs{quote}"),
            );
        }
    }
    ensure!(
        !bindings
            .lines()
            .any(|line| line.trim_start().starts_with("import ") && line.contains("@lenso/")),
        "linked Workers Host requires an undeclared JavaScript integration"
    );
    Ok(bindings)
}

fn wasm_manifest(mut manifest: Value) -> anyhow::Result<Value> {
    manifest["package"]["name"] = json!("lenso-generated-workers-host");
    manifest["lib"] = json!({"crate-type":["cdylib"]});
    let patched_kernel = manifest.pointer("/patch/crates-io/lenso-kernel").cloned();
    let dependencies = manifest["dependencies"]
        .as_object_mut()
        .context("generated Host dependencies")?;
    for name in [
        "anyhow",
        "sha2",
        "tempfile",
        "rustix",
        "tokio",
        "lenso-runner",
    ] {
        dependencies.remove(name);
    }
    let framework = patched_kernel.unwrap_or_else(|| dependencies["lenso-kernel"].clone());
    let workers = if let Some(path) = framework["path"].as_str() {
        json!({"path":Path::new(path).parent().context("framework crates directory")?.join("lenso-workers-driver"),"version":WORKERS_DRIVER_REQUIREMENT})
    } else if let Some(git) = framework["git"].as_str() {
        json!({"version":WORKERS_DRIVER_REQUIREMENT,"git":git,"rev":framework["rev"]})
    } else {
        json!(WORKERS_DRIVER_REQUIREMENT)
    };
    dependencies.insert("lenso-workers-driver".into(), workers);
    for (name, version) in [
        ("bytes", "1"),
        ("http", "1"),
        ("wasm-bindgen", "=0.2.127"),
        ("wasm-bindgen-futures", "=0.4.77"),
        ("js-sys", "=0.3.104"),
    ] {
        dependencies.insert(name.into(), json!(version));
    }
    let ingress = dependencies
        .get_mut("lenso-web-ingress-plugin")
        .context("linked HTTP Ingress dependency")?;
    if let Value::String(version) = ingress {
        *ingress = json!({"version":version});
    }
    ingress
        .as_object_mut()
        .context("Ingress Cargo identity")?
        .insert("default-features".into(), json!(false));
    Ok(manifest)
}

struct LoadedRuntime {
    package_version: String,
    modules: Vec<RuntimeModule>,
}

struct RuntimeModule {
    name: &'static str,
    bytes: Vec<u8>,
}

fn load_runtime(package: &Path, facilities: bool) -> anyhow::Result<LoadedRuntime> {
    let manifest: Value =
        serde_json::from_slice(&super::runtime::read_file(&package.join("package.json"))?)?;
    ensure!(
        manifest["name"] == "@lenso/workers-runtime" && manifest["version"] == "0.1.6",
        "linked Workers requires the qualified @lenso/workers-runtime 0.1.6 candidate with pinned module bytes"
    );
    // The repaired source candidate retains 0.1.6. Version alone cannot
    // distinguish it from the old cleanup implementation; admit exact bytes.
    let mut files = RUNTIME_FILES.to_vec();
    if facilities {
        files.push((
            "facilities.mjs",
            "b096e80dbee638204c5f57f88530a765e2654c5424c9f045b0ae5df62e605f1b",
        ));
    }
    let modules = files
        .iter()
        .map(|&(name, expected)| {
            let bytes = super::runtime::read_file(&package.join(name))?;
            ensure!(
                super::digest_bytes(&bytes) == format!("sha256:{expected}"),
                "linked Workers runtime module {name} differs from the pinned candidate"
            );
            Ok(RuntimeModule { name, bytes })
        })
        .collect::<anyhow::Result<_>>()?;
    Ok(LoadedRuntime {
        package_version: manifest["version"]
            .as_str()
            .context("Workers runtime version")?
            .into(),
        modules,
    })
}

#[cfg(test)]
mod runtime_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use lenso_app_plan::{AppComposition, CapabilityEndpointPlan, PluginInstancePlan};

    #[test]
    fn staging_imports_preserves_the_original_wasm_import_names() {
        let source = "import * as import0 from '@lenso/workers-runtime/http';\nconst imports = {'@lenso/workers-runtime/http': import0};";
        let staged = rewrite_bindings(source).unwrap();
        assert!(staged.contains("from './runtime/http.mjs'"));
        assert!(staged.contains("'@lenso/workers-runtime/http': import0"));
        assert!(rewrite_bindings("import * as resource from '@lenso/undeclared';").is_err());
    }

    #[test]
    fn wasm_host_preserves_pinned_sources_and_disables_native_transport() {
        let manifest = json!({"package":{"name":"native"},"workspace":{},"dependencies":{
            "lenso-kernel":"=0.3.12", "lenso-runner":"=0.2.20", "tokio":"1.52",
            "lenso-web-ingress-plugin":{"version":"=0.4.10","git":"https://example.test/framework","rev":"exact"},
            "local_plugin_1":{"path":"/owner/plugin"},
            "root_plugin_2":{"path":"/owner/auth","default-features":false,"features":["workers"]}
        },"patch":{"crates-io":{"lenso-kernel":{"path":"/owner/framework/crates/lenso-kernel"}}}});
        let wasm = wasm_manifest(manifest).unwrap();
        assert_eq!(
            wasm["dependencies"]["lenso-workers-driver"]["path"],
            "/owner/framework/crates/lenso-workers-driver"
        );
        assert_eq!(
            wasm["dependencies"]["lenso-web-ingress-plugin"]["rev"],
            "exact"
        );
        assert_eq!(
            wasm["dependencies"]["lenso-web-ingress-plugin"]["default-features"],
            false
        );
        assert!(wasm["dependencies"].get("tokio").is_none());
        assert_eq!(
            wasm["dependencies"]["root_plugin_2"]["default-features"],
            false
        );
        assert_eq!(
            wasm["dependencies"]["root_plugin_2"]["features"],
            json!(["workers"])
        );
        assert!(wasm["dependencies"].get("local_plugin_1").is_some());
    }

    #[test]
    fn wasm_host_pins_the_workers_driver_for_each_framework_source() {
        for (kernel, expected_driver) in [
            (
                json!({"path":"/owner/framework/crates/lenso-kernel","version":"=0.3.12"}),
                json!({"path":"/owner/framework/crates/lenso-workers-driver","version":"=0.1.2"}),
            ),
            (
                json!({"git":"https://example.test/framework","rev":"selected-source","version":"=0.3.12"}),
                json!({"git":"https://example.test/framework","rev":"selected-source","version":"=0.1.2"}),
            ),
            (json!("=0.3.12"), json!("=0.1.2")),
        ] {
            let manifest = json!({"package":{"name":"native"},"dependencies":{
                "lenso-kernel":kernel,
                "lenso-web-ingress-plugin":"=0.4.10"
            }});
            let wasm = wasm_manifest(manifest).unwrap();
            assert_eq!(
                wasm["dependencies"]["lenso-workers-driver"],
                expected_driver
            );
        }
    }

    #[test]
    fn linked_profile_admits_streams_but_rejects_events_and_non_http_graphs() {
        let endpoint = PluginInstancePlan::new("endpoint", "example.http")
            .with_capability(CapabilityEndpointPlan::new("example.http@1", "1", ["read"]));
        let no_ingress = AppComposition::new(vec![endpoint.clone()], vec![])
            .resolve()
            .unwrap();
        assert!(
            admit(&no_ingress)
                .unwrap_err()
                .to_string()
                .contains("HTTP Ingress")
        );
        let ingress = PluginInstancePlan::new("ingress", "lenso.web-ingress");
        let request = AppComposition::new(vec![endpoint, ingress.clone()], vec![])
            .resolve()
            .unwrap();
        assert!(admit(&request).is_ok());
        let stream = PluginInstancePlan::new("stream", "example.stream").with_capability(
            CapabilityEndpointPlan::new("example.stream@1", "1", ["watch"])
                .with_operation_kind("watch", CapabilityOperationKind::Stream),
        );
        let plan = AppComposition::new(vec![stream, ingress], vec![])
            .resolve()
            .unwrap();
        assert!(admit(&plan).is_ok());
        let event = PluginInstancePlan::new("event", "example.event").with_capability(
            CapabilityEndpointPlan::new("example.event@1", "1", ["publish"])
                .with_operation_kind("publish", CapabilityOperationKind::Event),
        );
        let ingress = PluginInstancePlan::new("ingress", "lenso.web-ingress");
        let plan = AppComposition::new(vec![event, ingress], vec![])
            .resolve()
            .unwrap();
        assert!(
            admit(&plan)
                .unwrap_err()
                .to_string()
                .contains("Event Capability")
        );
        let websocket = PluginInstancePlan::new("websocket", "example.websocket").with_capability(
            CapabilityEndpointPlan::new("lenso.websocket.endpoint@1", "1", ["connect"])
                .with_operation_kind("connect", CapabilityOperationKind::Stream),
        );
        let ingress = PluginInstancePlan::new("ingress", "lenso.web-ingress");
        let plan = AppComposition::new(vec![websocket, ingress], vec![])
            .resolve()
            .unwrap();
        assert!(admit(&plan).unwrap_err().to_string().contains("WebSocket"));
    }
}
