// Embedded by the local Host generator. Generated contract crates retain the
// same Cargo package identities as the native Plugin's normal dependencies.
use anyhow::{Context, bail};
// LENSO_NATIVE_RESOURCES
use lenso_app_plan::ResolvedAppPlan;
#[cfg(generated_native_host)]
use lenso_app_plan::authoring::HostCatalog;
use lenso_kernel::{ExecutionAdapterCatalog, Kernel, ShutdownOutcome};
use lenso_native_adapter::NativePluginRegistry;
use lenso_runtime_codec::{ArtifactCatalog, ArtifactHandle};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf, time::Duration};

#[derive(Deserialize)]
struct Resolution {
    schema: String,
    plugin_root_revision: String,
    plan: ResolvedAppPlan,
}

struct Activation {
    _lock: fs::File,
    path: PathBuf,
    state: serde_json::Value,
}

#[cfg(unix)]
fn begin_activation(
    distribution: &std::path::Path,
    intent: &std::path::Path,
) -> anyhow::Result<Option<Activation>> {
    let built_intent = distribution.join("intent");
    if fs::canonicalize(intent)? != fs::canonicalize(&built_intent)? {
        return Ok(None);
    }
    let control = intent.join(".lenso");
    if !fs::symlink_metadata(&control)?.file_type().is_dir() {
        bail!("configuration control directory must be a real directory");
    }
    let path = control.join("configuration-source-state.json");
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(metadata) if !metadata.file_type().is_file() => {
            bail!("configuration state must be a regular file")
        }
        Ok(_) => {}
    }
    let lock = {
        use rustix::fs::{Mode, OFlags};
        let descriptor = rustix::fs::open(
            control.join("configuration-source.lock"),
            OFlags::RDWR | OFlags::CREATE | OFlags::CLOEXEC | OFlags::NOFOLLOW,
            Mode::RUSR | Mode::WUSR,
        )?;
        let file = fs::File::from(descriptor);
        if !file.metadata()?.file_type().is_file() {
            bail!("configuration lock must be a regular file");
        }
        file
    };
    lock.lock()?;
    use std::io::Read as _;
    let mut bytes = Vec::new();
    let descriptor = rustix::fs::open(
        &path,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::CLOEXEC | rustix::fs::OFlags::NOFOLLOW,
        rustix::fs::Mode::empty(),
    )?;
    let file = fs::File::from(descriptor);
    file.take(64 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 64 * 1024 * 1024 {
        bail!("configuration state exceeds runtime limit");
    }
    let state: serde_json::Value = serde_json::from_slice(&bytes)?;
    if state.get("schema").and_then(|v| v.as_str()) != Some("lenso.configuration-source-state.v2") {
        bail!("unsupported configuration state");
    }
    Ok(Some(Activation {
        _lock: lock,
        path,
        state,
    }))
}

#[cfg(not(unix))]
fn begin_activation(
    distribution: &std::path::Path,
    intent: &std::path::Path,
) -> anyhow::Result<Option<Activation>> {
    if fs::canonicalize(intent)? == fs::canonicalize(distribution.join("intent"))?
        && intent
            .join(".lenso/configuration-source-state.json")
            .exists()
    {
        bail!("external configuration startup is unsupported on this platform");
    }
    Ok(None)
}

fn mark_activated(mut activation: Activation, root_revision: &str) -> anyhow::Result<()> {
    let desired = activation
        .state
        .get("desired")
        .context("missing desired configuration")?;
    let selected = desired
        .get("candidate_plugin_root_revision")
        .and_then(|v| v.as_str())
        .context("missing desired Root revision")?;
    if selected != root_revision {
        bail!("Host resolved a different Root revision than the desired configuration");
    }
    let last_activated = serde_json::json!({
        "revision": desired.get("revision").and_then(|v| v.as_u64()).context("missing desired revision")?,
        "snapshot_digest": desired.get("snapshot_digest").and_then(|v| v.as_str()).context("missing snapshot digest")?,
        "plugin_root_revision": root_revision,
        "policy_digest": activation.state.get("policy_digest").and_then(|v| v.as_str()).context("missing accepted Host policy digest")?,
    });
    activation
        .state
        .as_object_mut()
        .context("configuration state must be an object")?
        .insert("last_activated".into(), last_activated);
    let bytes = serde_json::to_vec(&activation.state)?;
    if bytes.len() > 64 * 1024 * 1024 {
        bail!("configuration state exceeds runtime limit");
    }
    let parent = activation
        .path
        .parent()
        .context("configuration state parent")?;
    let mut stage = tempfile::NamedTempFile::new_in(parent)?;
    std::io::Write::write_all(&mut stage, &bytes)?;
    stage.as_file().sync_all()?;
    stage.persist(&activation.path)?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}
#[derive(Deserialize)]
struct Artifact {
    plugin_id: String,
    execution_class: String,
    runtime_profile: String,
    artifact_digest: String,
    artifact_size: u64,
    selection: ArtifactSelection,
}
#[derive(Deserialize)]
struct ArtifactSelection {
    selected: SelectedArtifact,
}
#[derive(Deserialize)]
struct SelectedArtifact {
    execution_class: String,
    runtime_profile: String,
    #[serde(default)]
    enforced_wasm_memory_ceiling_bytes: Option<u64>,
}
#[derive(Deserialize)]
struct FileProof {
    path: String,
    sha256: String,
    size: u64,
    role: String,
}
#[derive(Deserialize)]
struct DistributionLock {
    schema: String,
    files: Vec<FileProof>,
}

#[derive(Serialize)]
struct WebRouteFact {
    method: String,
    path: String,
    route_id: String,
}

fn capture_web_routes<'a>(
    routes: impl Iterator<Item = (&'a str, &'a str, &'a str)>,
) -> Option<Vec<WebRouteFact>> {
    let mut captured = Vec::new();
    let mut source_bytes = 0usize;
    for (method, path, route_id) in routes {
        if captured.len() == 256 {
            return None;
        }
        let route_bytes = method.len().checked_add(path.len())?.checked_add(route_id.len())?;
        if route_bytes > 4096 {
            return None;
        }
        source_bytes = source_bytes
            .checked_add(route_bytes)?;
        if source_bytes > 128 * 1024 {
            return None;
        }
        captured.push(WebRouteFact {
            method: method.to_owned(),
            path: path.to_owned(),
            route_id: route_id.to_owned(),
        });
    }
    Some(captured)
}

fn publish_web_route_receipt(
    ready_file: &std::path::Path,
    plugin_root_revision: &str,
    distribution_lock_sha256: &str,
    routes: Option<Vec<WebRouteFact>>,
) -> anyhow::Result<()> {
    let Some(routes) = routes else {
        return Ok(());
    };
    let receipt = serde_json::json!({
        "schema": "lenso.live-web-routes.v1",
        "capture": "ready_gate",
        "plugin_root_revision": plugin_root_revision,
        "distribution_lock_sha256": distribution_lock_sha256,
        "routes": routes,
    });
    let bytes = serde_json::to_vec(&receipt)?;
    if bytes.len() > 128 * 1024 {
        return Ok(());
    }
    let path = ready_file.with_extension("web-routes.json");
    let parent = path
        .parent()
        .context("Web route receipt needs a parent directory")?;
    let mut stage = tempfile::NamedTempFile::new_in(parent)?;
    std::io::Write::write_all(&mut stage, &bytes)?;
    stage.as_file().sync_all()?;
    stage.persist(path)?;
    Ok(())
}

#[cfg(generated_native_host)]
fn main() -> anyhow::Result<()> {
    // LENSO_LINK_PLUGINS
    run(std::env::args().skip(1).collect())
}

#[cfg(not(generated_native_host))]
fn portable_web_proof() -> anyhow::Result<serde_json::Value> {
    use lenso_capability_http_endpoint as endpoint;
    use lenso_app_plan::authoring::{HostCatalog, HostPluginRelease};

    let ingress = lenso_web_ingress_plugin::WebIngressFactory::plugin_descriptor();
    let registry = NativePluginRegistry::new()
        .with_factory(lenso_web_ingress_plugin::WebIngressFactory::new());
    let factories = registry.factories().collect::<Vec<_>>();
    if factories.len() != 1
        || factories[0].package_id() != ingress.plugin_id()
        || factories[0].package_version() != ingress.release_version()
        || factories[0].factory_identity()
            != format!("{}@{}", ingress.plugin_id(), ingress.release_version())
    {
        bail!("precompiled portable Host does not contain the declared Web Ingress factory");
    }
    let catalog = HostCatalog::new([], [HostPluginRelease::new(ingress.clone())], []);

    Ok(serde_json::json!({
        "schema": "lenso.portable-web-host.v1",
        "target": lenso_app_authoring::native_host_target(),
        "catalog": catalog,
        "ingress": ingress,
        "endpoint_codec": {
            "capability_id": endpoint::CAPABILITY_ID,
            "descriptor_version": endpoint::DESCRIPTOR_VERSION,
            "descriptor_digest": endpoint::DESCRIPTOR_DIGEST,
            "request_operations": [endpoint::DESCRIBE_OPERATION, endpoint::HANDLE_OPERATION],
        },
        "execution_classes": ["lenso.process@1", "lenso.wasm-component@1"],
    }))
}

#[cfg(not(generated_native_host))]
pub(super) fn probe_portable_web(executable: &std::path::Path) -> anyhow::Result<lenso_app_plan::authoring::PluginDescriptor> {
    let output = super::build_command(executable)
        .args(["app", "__run-local", "--", "--probe-portable-web"])
        .output()
        .with_context(|| format!("probe precompiled portable Host {}", executable.display()))?;
    if !output.status.success() {
        bail!("precompiled portable Host has no compatible Web Ingress and HTTP Endpoint codec: {}", String::from_utf8_lossy(&output.stderr));
    }
    let actual: serde_json::Value = serde_json::from_slice(&output.stdout)
        .context("decode precompiled portable Host Web probe")?;
    validate_portable_web_proof(actual)
}

#[cfg(not(generated_native_host))]
fn validate_portable_web_proof(actual: serde_json::Value) -> anyhow::Result<lenso_app_plan::authoring::PluginDescriptor> {
    if actual != portable_web_proof()? {
        bail!("precompiled portable Host Web Ingress, Endpoint codec, target, or Adapter identity mismatch");
    }
    let catalog: lenso_app_plan::authoring::HostCatalog = serde_json::from_value(actual["catalog"].clone())
        .context("decode precompiled portable Host catalog")?;
    let [release] = catalog.plugins() else {
        bail!("precompiled portable Host catalog has no sole Web Ingress release");
    };
    let ingress = release.descriptor().clone();
    if ingress.plugin_id() != "lenso.web-ingress"
        || serde_json::to_value(&ingress)? != actual["ingress"]
    {
        bail!("precompiled portable Host catalog does not admit Web Ingress");
    }
    Ok(ingress)
}

#[cfg(all(test, not(generated_native_host)))]
mod portable_web_tests {
    use super::{portable_web_proof, validate, validate_portable_web_proof};
    use lenso_app_plan::{
        AppComposition, CapabilityEndpointPlan, ExecutionClassId, PluginInstancePlan,
        ResolvedAppPlan,
    };
    use std::collections::BTreeMap;

    #[test]
    fn rejects_missing_ingress_catalog_and_codec_drift() {
        let proof = portable_web_proof().unwrap();
        assert_eq!(validate_portable_web_proof(proof.clone()).unwrap().plugin_id(), "lenso.web-ingress");

        let mut missing = proof.clone();
        missing["catalog"]["plugins"] = serde_json::json!([]);
        assert!(validate_portable_web_proof(missing).is_err());

        let mut drifted = proof;
        drifted["endpoint_codec"]["descriptor_digest"] = serde_json::json!("sha256:0000000000000000000000000000000000000000000000000000000000000000");
        assert!(validate_portable_web_proof(drifted).is_err());
    }

    #[test]
    fn precompiled_host_admits_exact_wasm_endpoint_contract() {
        let proof = portable_web_proof().unwrap();
        assert_eq!(
            proof["execution_classes"],
            serde_json::json!(["lenso.process@1", "lenso.wasm-component@1"]),
        );

        let endpoint = PluginInstancePlan::new("endpoint", "local.portable-http")
            .with_entrypoint("plugin")
            .with_execution_class(ExecutionClassId::new("lenso.wasm-component@1"))
            .with_capability(CapabilityEndpointPlan::new(
                lenso_capability_http_endpoint::CAPABILITY_ID,
                lenso_capability_http_endpoint::DESCRIPTOR_VERSION,
                [
                    lenso_capability_http_endpoint::DESCRIBE_OPERATION,
                    lenso_capability_http_endpoint::HANDLE_OPERATION,
                ],
            ));
        let plan = AppComposition::new(vec![endpoint], vec![]).resolve().unwrap();
        validate(&plan, &BTreeMap::new()).unwrap();

        let v2 = ResolvedAppPlan::new(
            vec![plan.plugin_instances()[0]
                .clone()
                .with_authoring(2, "lenso.wasm-component@1")],
            vec![],
        );
        assert!(validate(&v2, &BTreeMap::new()).is_err());
        let exact = BTreeMap::from([(
            "local.portable-http".to_owned(),
            serde_json::json!({"capabilities": [{
                "capability_id": lenso_capability_http_endpoint::CAPABILITY_ID,
                "descriptor_digest": lenso_capability_http_endpoint::DESCRIPTOR_DIGEST,
            }]}),
        )]);
        validate(&v2, &exact).unwrap();
        let wrong = BTreeMap::from([(
            "local.portable-http".to_owned(),
            serde_json::json!({"capabilities": [{
                "capability_id": lenso_capability_http_endpoint::CAPABILITY_ID,
                "descriptor_digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            }]}),
        )]);
        assert!(validate(&v2, &wrong).is_err());
    }
}

pub fn run(args: Vec<String>) -> anyhow::Result<()> {
    #[cfg(not(generated_native_host))]
    if args == ["--probe-portable-web"] {
        println!("{}", portable_web_proof()?);
        return Ok(());
    }
    #[cfg(generated_native_host)]
    if args == ["--describe"] {
        let catalog: HostCatalog =
            NativePluginRegistry::host_catalog([], []).map_err(|e| anyhow::anyhow!("{e:?}"))?;
        // LENSO_DESCRIBE_WEB
        println!("{}", serde_json::to_string(&catalog)?);
        return Ok(());
    }
    let executable = fs::canonicalize(std::env::current_exe()?)?;
    let root = executable
        .parent()
        .and_then(|p| p.parent())
        .context("Host location")?;
    let mut intent = root.join("intent");
    let mut check = false;
    let mut ready_file = None;
    let mut web_address_file = None;
    let mut defer_activation = false;
    let mut command_args = None;
    #[cfg(generated_native_host)]
    let mut business_snapshot_policy = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                index += 1;
                intent = PathBuf::from(args.get(index).context("--root needs a directory")?);
            }
            #[cfg(generated_native_host)]
            "--business-snapshot-policy" => {
                index += 1;
                business_snapshot_policy = Some(PathBuf::from(
                    args.get(index).context("--business-snapshot-policy needs a file")?,
                ));
            }
            "--check" => check = true,
            "--defer-activation" => defer_activation = true,
            "--ready-file" => {
                index += 1;
                ready_file = Some(PathBuf::from(
                    args.get(index).context("--ready-file needs a path")?,
                ));
            }
            "--web-address-file" => {
                index += 1;
                web_address_file = Some(PathBuf::from(
                    args.get(index).context("--web-address-file needs a path")?,
                ));
            }
            "--" => {
                command_args = Some(args[index + 1..].to_vec());
                break;
            }
            other => bail!("unknown Host argument: {other}"),
        }
        index += 1;
    }
    anyhow::ensure!(
        !check || ready_file.is_none(),
        "--ready-file cannot be combined with --check"
    );
    anyhow::ensure!(
        !defer_activation || (!check && ready_file.is_some()),
        "--defer-activation requires --ready-file without --check"
    );
    anyhow::ensure!(
        web_address_file.is_none() || (!check && ready_file.is_some()),
        "--web-address-file requires --ready-file without --check"
    );
    let distribution_lock_bytes = fs::read(root.join(".lenso/distribution.lock.json"))?;
    let distribution_lock_sha256 = format!(
        "sha256:{}",
        Sha256::digest(&distribution_lock_bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    let lock: DistributionLock = serde_json::from_slice(&distribution_lock_bytes)?;
    if lock.schema != "lenso.local-host-distribution.v1" || lock.files.len() > 2048 {
        bail!("unsupported local Host distribution lock");
    }
    let mut locked = std::collections::BTreeSet::new();
    for file in lock.files {
        if file.path.contains('\\')
            || !std::path::Path::new(&file.path)
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
            || !locked.insert(file.path.clone())
            || file.role.is_empty()
        {
            bail!("invalid distribution file: {}", file.path);
        }
        let path = root.join(&file.path);
        if !fs::symlink_metadata(&path)?.file_type().is_file() {
            bail!("runtime file is not regular: {}", file.path);
        }
        let bytes = fs::read(path)?;
        if bytes.len() as u64 != file.size
            || format!(
                "sha256:{}",
                Sha256::digest(&bytes)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            ) != file.sha256
        {
            bail!("runtime file changed: {}", file.path);
        }
    }
    for required in [
        ".lenso/host",
        ".lenso/host-build.json",
        "runtime/lenso-resolver",
        "bundles.json",
        "runtime-codecs.json",
        ".lenso/host-mode",
    ] {
        if !locked.contains(required) {
            bail!("distribution missing locked file: {required}");
        }
    }
    let mut activation = begin_activation(root, &intent)?;
    let output = std::process::Command::new(root.join("runtime/lenso-resolver"))
        .args(["app", "show", "--runtime-json", "--host-build"])
        .arg(root.join(".lenso/host-build.json"))
        .arg("--root")
        .arg(&intent)
        .output()?;
    if !output.status.success() {
        bail!("resolve App: {}", String::from_utf8_lossy(&output.stderr));
    }
    let resolution: Resolution = serde_json::from_slice(&output.stdout)?;
    if resolution.schema != "lenso.runtime-app-resolution.v1" {
        bail!("unsupported resolver schema");
    }
    #[cfg(generated_native_host)]
    for instance in resolution.plan.plugin_instances() {
        let supported = match instance.execution_class().as_str() {
            "lenso.native-rust@1" => true,
            "lenso.bun-process@1" => cfg!(generated_bun_adapter),
            "lenso.process@1" => cfg!(generated_process_adapter),
            "lenso.wasm-component@1" => cfg!(generated_wasm_adapter),
            _ => false,
        };
        if !supported {
            bail!(
                "Host has no built Execution Adapter for {}",
                instance.execution_class().as_str()
            );
        }
    }
    if let Some(active) = &activation {
        let selected = active
            .state
            .pointer("/desired/candidate_plugin_root_revision")
            .and_then(|v| v.as_str());
        if selected != Some(&resolution.plugin_root_revision) {
            bail!("configuration changed during Host resolution");
        }
    }
    // A check-only or supervised candidate has no Host-side activation write.
    // Release the configuration lock before Plugin startup and the long-lived
    // serving loop; the supervisor fences its later receipt against the exact
    // resolved Root revision after the Ready Gate.
    if check || defer_activation {
        activation = None;
    }
    let inventory: Vec<Artifact> = serde_json::from_slice(&fs::read(root.join("bundles.json"))?)?;
    let mut artifacts = ArtifactCatalog::new();
    #[cfg(any(not(generated_native_host), generated_wasm_adapter))]
    let mut wasm_limits = std::collections::BTreeMap::new();
    for instance in resolution.plan.plugin_instances() {
        if instance.execution_class().as_str() == "lenso.native-rust@1" {
            continue;
        }
        let artifact = inventory
            .iter()
            .find(|a| a.plugin_id == instance.package_id())
            .with_context(|| format!("missing built artifact for {}", instance.package_id()))?;
        if artifact.execution_class != instance.execution_class().as_str()
            || artifact.runtime_profile != instance.runtime_profile()
            || artifact.selection.selected.execution_class != artifact.execution_class
            || artifact.selection.selected.runtime_profile != artifact.runtime_profile
        {
            bail!(
                "selected runtime identity differs from locked artifact inventory for {}",
                instance.package_id()
            );
        }
        if instance.execution_class().as_str() == "lenso.wasm-component@1" {
            #[cfg(any(not(generated_native_host), generated_wasm_adapter))]
            {
                let ceiling = artifact
                    .selection
                    .selected
                    .enforced_wasm_memory_ceiling_bytes
                    .context("Wasm Component memory ceiling is absent from locked selection")?;
                let ceiling = usize::try_from(ceiling)
                    .context("Wasm Component memory ceiling exceeds Host address space")?;
                if ceiling == 0 {
                    bail!("Wasm Component memory ceiling must be positive");
                }
                wasm_limits.insert(
                    instance.instance_key().to_owned(),
                    lenso_wasm_component_adapter::WasmComponentLimits {
                        max_memory_bytes: ceiling,
                        ..lenso_wasm_component_adapter::WasmComponentLimits::default()
                    },
                );
            }
            #[cfg(all(generated_native_host, not(generated_wasm_adapter)))]
            bail!("Host has no built Wasm Component Adapter");
        } else if artifact
            .selection
            .selected
            .enforced_wasm_memory_ceiling_bytes
            .is_some()
        {
            bail!("non-Wasm artifact advertises a Wasm Component memory ceiling");
        }
        let path = format!("runtime/artifacts/{}", artifact.plugin_id);
        if !locked.contains(&path) {
            bail!("selected artifact is not locked: {path}");
        }
        if instance.execution_class().as_str() == "lenso.bun-process@1"
            && !locked.contains("runtime/bun")
        {
            bail!("Bun runtime is not locked");
        }
        artifacts = artifacts
            .with_artifact(
                instance.instance_key(),
                ArtifactHandle::open(
                    root.join(&path),
                    &artifact.artifact_digest,
                    artifact.artifact_size,
                )
                .map_err(|e| anyhow::anyhow!("{e:?}"))?,
            )
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    }
    // LENSO_BUSINESS_SNAPSHOT_DECL
    #[cfg(generated_native_host)]
    let native = {
        let mut resources = native_resources::InstanceResourceCatalog::new();
        for instance in resolution.plan.plugin_instances() {
            if instance.execution_class().as_str() != "lenso.native-rust@1" {
                continue;
            }
            let directory = intent.join("plugins").join(instance.instance_key());
            if directory.try_exists()? {
                let mut files = Vec::new();
                read_resources(&directory, &directory, &mut files, &mut 0, 0)?;
                let snapshot = native_resources::InstanceResources::from_files(files)
                    .map_err(|e| anyhow::anyhow!("{e:?}"))?;
                resources = resources
                    .with_resources(instance.instance_key(), snapshot)
                    .map_err(|e| anyhow::anyhow!("{e:?}"))?;
            }
        }
        let registry = NativePluginRegistry::new()
            .with_linked_factories()
            .with_resources(resources);
        // LENSO_BUSINESS_SNAPSHOT_BIND
    };
    #[cfg(not(generated_native_host))]
    let ingress = lenso_web_ingress_plugin::WebIngressFactory::new();
    #[cfg(not(generated_native_host))]
    let native = NativePluginRegistry::new().with_factory(ingress.clone());
    // LENSO_RUNTIME_WEB
    let _ = &artifacts;
    #[cfg(any(not(generated_native_host), generated_bun_adapter))]
    let bun = lenso_bun_adapter::BunAdapter::production(root.join("runtime/bun"))
        .with_artifacts(artifacts.clone());
    #[cfg(any(not(generated_native_host), generated_process_adapter))]
    let process = lenso_process_adapter::ProcessAdapter::new(artifacts.clone());
    #[cfg(any(not(generated_native_host), generated_wasm_adapter))]
    let mut wasm = lenso_wasm_component_adapter::WasmComponentAdapter::new(artifacts)
        .require_exact_instance_limits();
    #[cfg(any(not(generated_native_host), generated_wasm_adapter))]
    for (instance_key, limits) in wasm_limits {
        wasm = wasm.with_instance_limits(instance_key, limits);
    }
    #[cfg(not(generated_native_host))]
    let typed = std::collections::BTreeSet::from([
        super::terminal::command::CAPABILITY_ID,
        super::terminal::provider::CAPABILITY_ID,
        lenso_capability_http_endpoint::CAPABILITY_ID,
    ]);
    #[cfg(not(generated_native_host))]
    let bun = bun
        .with_authoring_codec(super::terminal::command::CommandJsonCodec)
        .with_authoring_codec(super::terminal::provider::CommandProviderJsonCodec);
    #[cfg(not(generated_native_host))]
    let process = process
        .with_codec(super::terminal::command::CommandJsonCodec)
        .with_codec(super::terminal::provider::CommandProviderJsonCodec)
        .with_codec(lenso_capability_http_endpoint::EndpointJsonCodec);
    #[cfg(not(generated_native_host))]
    let wasm = wasm
        .with_codec(super::terminal::command::CommandJsonCodec)
        .with_codec(super::terminal::provider::CommandProviderJsonCodec)
        .with_codec(lenso_capability_http_endpoint::EndpointJsonCodec)
        .require_v2_descriptor_digest_for(lenso_capability_http_endpoint::CAPABILITY_ID);
    // LENSO_REGISTER_CODECS
    let evidence = serde_json::from_slice(&fs::read(root.join("runtime-codecs.json"))?)?;
    #[cfg(any(not(generated_native_host), generated_bun_adapter))]
    let mut bun = bun;
    #[cfg(any(not(generated_native_host), generated_process_adapter))]
    let mut process = process;
    #[cfg(any(not(generated_native_host), generated_wasm_adapter))]
    let mut wasm = wasm;
    for codec in portable_codecs(&resolution.plan, &typed, &evidence)? {
        #[cfg(any(not(generated_native_host), generated_bun_adapter))]
        {
            bun = bun
                .with_codec(LegacyBunCodec(codec.clone()))
                .with_authoring_codec(codec.clone());
        }
        #[cfg(any(not(generated_native_host), generated_process_adapter))]
        {
            process = process.with_codec(codec.clone());
        }
        #[cfg(any(not(generated_native_host), generated_wasm_adapter))]
        {
            wasm = wasm.with_codec(codec);
        }
        #[cfg(all(
            generated_native_host,
            not(any(
                generated_bun_adapter,
                generated_process_adapter,
                generated_wasm_adapter
            ))
        ))]
        let _ = codec;
    }
    let catalog = ExecutionAdapterCatalog::new();
    let catalog = catalog
        .with_adapter(native)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    #[cfg(any(not(generated_native_host), generated_bun_adapter))]
    let catalog = catalog
        .with_adapter(bun)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    #[cfg(any(not(generated_native_host), generated_process_adapter))]
    let catalog = catalog
        .with_adapter(process)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    #[cfg(any(not(generated_native_host), generated_wasm_adapter))]
    let catalog = catalog
        .with_adapter(wasm)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    // Drive the local Kernel outside Tokio's block_on execution context: Bun's
    // synchronous startup handshake owns a separate RPC runtime. Tokio workers
    // still service I/O/timers, and LocalSet retains thread-local Plugin state.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let _entered = runtime.enter();
    futures::executor::block_on(
        tokio::task::LocalSet::new().run_until(async move {
            let app = Kernel::start(resolution.plan, lenso_runner::TokioDriver::new(), catalog)
                .await.map_err(|e| anyhow::anyhow!("Host startup failed: {e:?}"))?;
            // LENSO_BUSINESS_SNAPSHOT_READY
            if !check
                && !defer_activation
                && let Some(activation) = activation
                && let Err(error) = mark_activated(activation, &resolution.plugin_root_revision)
            {
                let outcome = app.shutdown(Duration::from_secs(10)).await;
                bail!("record Host activation: {error}; shutdown: {outcome:?}");
            }
            #[cfg(generated_native_host)]
            let local_web_url: Option<String> = None;
            #[cfg(not(generated_native_host))]
            let local_web_url = ingress.local_address().map(|address| format!("http://{address}/"));
            #[cfg(generated_native_host)]
            let local_web_routes: Option<Vec<WebRouteFact>> = None;
            #[cfg(not(generated_native_host))]
            let local_web_routes = ingress.route_manifest().and_then(|manifest| {
                capture_web_routes(manifest.routes().iter().map(|route| {
                    (route.method.as_str(), route.path.as_str(), route.route_id.as_str())
                }))
            });
            #[cfg(not(generated_native_host))]
            if let Some(address) = &local_web_url { eprintln!("Listening on {address}"); }
            // LENSO_WEB_READY
            // LENSO_WEB_ROUTE_FACTS
            if let Some(path) = web_address_file {
                let address = local_web_url.context("frontend dev requires a ready Web Ingress")?;
                let stage = path.with_extension("stage");
                fs::write(&stage, format!("{address}\n"))?;
                fs::rename(stage, path)?;
            }
            if let Some(path) = ready_file {
                if std::env::var("LENSO_MCP_WEB_ROUTES").ok().as_deref() == Some("1") {
                    if let Err(error) = publish_web_route_receipt(
                        &path,
                        &resolution.plugin_root_revision,
                        &distribution_lock_sha256,
                        local_web_routes,
                    ) {
                        eprintln!("Web route observation unavailable: {error}");
                    }
                }
                let stage = path.with_extension("stage");
                fs::write(&stage, b"lenso.local-host-ready.v1\n")?;
                fs::rename(stage, path)?;
            }
            eprintln!("Local App ready");
            let mut command_result: anyhow::Result<()> = Ok(());
            #[cfg(not(generated_native_host))]
            if let Some(args) = &command_args { command_result = super::terminal::run(&app, args).await; }
            // LENSO_TERMINAL_RUN
            if !check && command_args.is_none() {
                #[cfg(unix)]
                let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
                loop {
                    tokio::select! {
                        signal = tokio::signal::ctrl_c() => { signal?; break; }
                        _ = async { #[cfg(unix)] { terminate.recv().await; }
                            #[cfg(not(unix))] { std::future::pending::<()>().await; } } => break,
                        () = tokio::time::sleep(Duration::from_millis(50)) => { if app.is_failed() { break; } }
                    }
                }
            }
            let failure = app.terminal_failure();
            let outcome = app.shutdown(Duration::from_secs(10)).await;
            if let Some(error) = failure { bail!("App failed: {error:?}; shutdown: {outcome:?}"); }
            if outcome != ShutdownOutcome::Clean { bail!("App shutdown failed: {outcome:?}"); }
            eprintln!("Local App stopped cleanly");
            command_result
        })
    )
}

#[cfg(generated_native_host)]
fn read_resources(
    root: &std::path::Path,
    directory: &std::path::Path,
    files: &mut Vec<(String, Vec<u8>)>,
    total: &mut usize,
    depth: usize,
) -> anyhow::Result<()> {
    if depth > 32 || !fs::symlink_metadata(directory)?.is_dir() {
        bail!("invalid Plugin resource directory");
    }
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            read_resources(root, &path, files, total, depth + 1)?;
        } else if metadata.is_file() {
            if files.len() >= 4096 || metadata.len() > 1024 * 1024 {
                bail!("Plugin resource exceeds snapshot limits");
            }
            let bytes = fs::read(&path)?;
            *total += bytes.len();
            if *total > 16 * 1024 * 1024 {
                bail!("Plugin resources exceed 16 MiB");
            }
            files.push((
                path.strip_prefix(root)?
                    .to_str()
                    .context("resource path must be UTF-8")?
                    .replace('\\', "/"),
                bytes,
            ));
        } else {
            bail!("Plugin resource must be a regular file or directory");
        }
    }
    Ok(())
}

#[cfg(not(generated_native_host))]
pub fn validate(
    plan: &ResolvedAppPlan,
    evidence: &std::collections::BTreeMap<String, serde_json::Value>,
) -> anyhow::Result<()> {
    for instance in plan.plugin_instances() {
        for capability in instance.provided_capabilities() {
            if capability.capability_id() != lenso_capability_http_endpoint::CAPABILITY_ID {
                continue;
            }
            if !matches!(
                instance.execution_class().as_str(),
                "lenso.process@1" | "lenso.wasm-component@1"
            ) {
                bail!("precompiled portable Web Host admits HTTP Endpoint only from Process or Wasm Component Plugins");
            }
            let operations = capability.request_operations().into_iter().collect::<std::collections::BTreeSet<_>>();
            if capability.descriptor_version() != lenso_capability_http_endpoint::DESCRIPTOR_VERSION
                || operations != std::collections::BTreeSet::from([
                    lenso_capability_http_endpoint::DESCRIBE_OPERATION,
                    lenso_capability_http_endpoint::HANDLE_OPERATION,
                ])
                || !capability.stream_operations().is_empty()
                || !capability.event_operations().is_empty()
            {
                bail!("portable HTTP Endpoint does not match the precompiled Host contract");
            }
            let declared_digest = evidence
                .get(instance.package_id())
                .and_then(|descriptor| descriptor["capabilities"].as_array())
                .and_then(|capabilities| capabilities.iter().find(|provided| {
                    provided["capability_id"] == lenso_capability_http_endpoint::CAPABILITY_ID
                }))
                .and_then(|provided| provided["descriptor_digest"].as_str());
            if instance.authoring_version() == 2
                && declared_digest != Some(lenso_capability_http_endpoint::DESCRIPTOR_DIGEST)
            {
                bail!("portable HTTP Endpoint Descriptor digest differs from the precompiled Host codec");
            }
        }
    }
    portable_codecs(
        plan,
        &std::collections::BTreeSet::from([
            super::terminal::command::CAPABILITY_ID,
            super::terminal::provider::CAPABILITY_ID,
            lenso_capability_http_endpoint::CAPABILITY_ID,
        ]),
        evidence,
    )
    .map(|_| ())
}
