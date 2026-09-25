//! A deliberately small Workers App target over the ordinary Bundle selector
//! and Plugin Root resolver. No Native artifact can enter this distribution.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, bail, ensure};
use lenso_app_authoring::{
    discovery::{SourceRole, discover},
    host_authoring::{GeneratedHostBuild, LocalPluginInput},
};
use lenso_app_plan::{
    CapabilityOperationKind, ExecutionClassId, ExecutionTargetCapability, PluginCriticality,
    RestartPolicy, TerminalPolicy, authoring::ResolvedApp,
};
use lenso_plugin_bundle::{
    ExecutionTargetCapabilities, ImplementationPolicy, PluginManifest, RuntimeAdmission,
    extract_plugin_descriptor, read_bundle_manifest, verify_bundle_directory,
};
use lenso_process_protocol::ExecutionTargetCapability as HostTargetCapability;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use crate::archive::{archive_bundle, with_bundle_directory};

const HOST_TARGET: &str = "workers";
const RUNTIME_VERSION: &str = "0.1.4";
// Exact module from lenso-js 3f83cde (not a floating package release).
const RUNTIME_MODULE_SHA256: &str =
    "b7f72c8dd0c14cd2a3e63a4ca2c04fddb08deadc76b7b0c238533dddb845576b";
const KNOWLEDGE_SETTINGS_PLUGIN_ID: &str = "lenso.reference.knowledge-settings";
const KNOWLEDGE_SETTINGS_PLAN_KEY: &str = "lenso.reference.knowledge-settings/default";
const KNOWLEDGE_SETTINGS_WORLD: &str = "lenso:knowledge-settings-local@1.0.0/plugin";
const KNOWLEDGE_SETTINGS_RUNTIME_VERSION: &str = "0.1.5";
const KNOWLEDGE_SETTINGS_RUNTIME_FILES: [(&str, &str); 3] = [
    (
        "component-admission.mjs",
        "e06d95bc3fe958f72e4eefd4c97a21f6a81b94a65dd6ca32e0e61bfa4de5b14e",
    ),
    (
        "component-requests.mjs",
        "b5e315b999b2ee6fa6ab374c8238dff4428cfa0b792270d0953e12560cac0b43",
    ),
    (
        "knowledge-settings-local.mjs",
        "210a4f259eae846725995292b69ad731c5a7a5f7478761568c6137efc43aaa92",
    ),
];
const JCO_VERSION: &str = "1.35.0";
const MAX_WORKERS_MODULE_BYTES: u64 = 32 * 1024 * 1024;

pub(super) struct BuildArgs {
    pub(super) root: PathBuf,
    pub(super) out: PathBuf,
    pub(super) workers_runtime: PathBuf,
    pub(super) jco: PathBuf,
}

struct SelectedBundle {
    plugin_id: String,
    component: PathBuf,
    artifact_digest: String,
    bundle_digest: String,
    manifest_digest: String,
    implementation_id: String,
    variant_id: Option<String>,
    descriptor: lenso_app_plan::authoring::PluginDescriptor,
    selection: crate::target_profile::ImplementationSelectionEvidence,
    target_capability_profile: lenso_plugin_bundle::ExecutionTargetCapabilityProfile,
}

struct DescriptorEvidence {
    source_digest: String,
    expected_digests: Option<BTreeMap<String, String>>,
}

pub(super) fn build(args: BuildArgs) -> anyhow::Result<()> {
    let root = fs::canonicalize(&args.root).context("locate source App")?;
    let source_digest = super::super::local_host::input_digest(&root)?;
    let report = discover(&root)?;
    super::super::convention_authoring::linked_catalog::verify_sources(&root, &report.candidates)?;
    let conventions = lenso_app_authoring::discovery::conventions::plan(&report)?;
    ensure!(
        conventions.compilations.is_empty(),
        "Workers App target cannot run selected convention compilers; remove the convention or build a verified Workers Component Bundle first"
    );

    let destination = std::path::absolute(&args.out)?;
    ensure!(
        !destination
            .components()
            .any(|component| component == std::path::Component::ParentDir),
        "Workers App output path cannot contain `..`"
    );
    if fs::symlink_metadata(&destination).is_ok() {
        bail!(
            "Workers App output already exists: {}",
            destination.display()
        );
    }
    let output_parent = destination
        .parent()
        .context("Workers App output needs a parent")?;
    ensure!(
        !output_parent.starts_with(&root) || output_parent == root,
        "Workers App output inside source App must be a direct child of its root"
    );
    fs::create_dir_all(output_parent)?;
    let parent = fs::canonicalize(output_parent)?;
    ensure!(
        !parent.starts_with(&root) || parent == root,
        "Workers App output parent resolves into authored App sources"
    );
    let destination = parent.join(
        destination
            .file_name()
            .context("Workers App output needs a name")?,
    );
    ensure!(
        destination != root.join(".lenso"),
        "reserved .lenso output directory inside App"
    );
    let stage = super::super::assemble::stage_output(&root, &parent)?;
    fs::create_dir_all(stage.path().join(".lenso"))?;
    fs::create_dir_all(stage.path().join("bundles"))?;
    fs::create_dir_all(stage.path().join("components"))?;
    let plugin_root = root.join("plugins");
    if plugin_root.try_exists()? {
        super::super::assemble::copy_root(&plugin_root, &stage.path().join("plugins"), 0, &mut 0)?;
    }

    let policy = ImplementationPolicy {
        host_target: HOST_TARGET.to_owned(),
        runtimes: vec![RuntimeAdmission::new(
            ExecutionClassId::new("lenso.wasm-component@1"),
            "lenso.wasm-component@1",
            ExecutionTargetCapabilities::new([
                HostTargetCapability::Request,
                HostTargetCapability::WasmComponent,
                HostTargetCapability::Workers,
            ]),
        )],
    };
    let mut inputs = Vec::new();
    let mut selected = Vec::new();
    let mut inventory = Vec::new();
    let candidates = conventions
        .candidates
        .iter()
        .filter(|candidate| {
            candidate.role != SourceRole::Shared
                || candidate.surface_owner.is_some()
                || stage
                    .path()
                    .join("plugins")
                    .join(&candidate.plugin_id)
                    .exists()
        })
        .collect::<Vec<_>>();
    let source_digests = candidates
        .iter()
        .map(|candidate| {
            Ok((
                candidate.plugin_id.clone(),
                super::super::local_host::input_digest(&candidate.project)?,
            ))
        })
        .collect::<anyhow::Result<BTreeMap<_, _>>>()?;
    for candidate in &candidates {
        ensure!(
            candidate.format == "bundle",
            "Workers App target rejects Plugin `{}` from {}: source format `{}` has no verified Workers implementation; package an explicit V4/V6 Workers Component variant",
            candidate.plugin_id,
            candidate.project.display(),
            candidate.format,
        );
        ensure!(
            candidate.published_resources.is_empty(),
            "Workers App target rejects Plugin `{}`: published resources have no Workers loader",
            candidate.plugin_id,
        );
        let archive_relative = format!("bundles/{}.lenso-plugin", candidate.plugin_id);
        let archive = stage.path().join(&archive_relative);
        if candidate
            .project
            .starts_with(root.join("vendor/lenso/portable"))
        {
            crate::plugins::signed_install::stage_source_archive(&root, candidate, &archive)?;
        } else {
            with_bundle_directory(&candidate.project, |directory| {
                archive_bundle(directory, &archive)
            })?;
        }
        let (verified, choice) = with_bundle_directory(&archive, |directory| {
            let verified = verify_bundle_directory(directory)?;
            let manifest = read_bundle_manifest(directory)?;
            ensure!(
                matches!(&manifest, PluginManifest::V4(_) | PluginManifest::V6(_)),
                "Workers App target rejects Plugin `{}`: V2/V3/V5 Bundle lacks the exact Workers variant profile required by this target",
                candidate.plugin_id,
            );
            let choice = crate::target_profile::select_implementation(&manifest, &policy)
                .with_context(|| {
                    format!(
                        "Workers target admission for Plugin `{}`",
                        candidate.plugin_id
                    )
                })?;
            let artifact = &choice.implementation.artifact;
            ensure!(
                artifact.media_type == "application/wasm"
                    && artifact.target == "wasm32-unknown-unknown",
                "Workers App target rejects Plugin `{}`: selected Artifact is not a Wasm Component",
                candidate.plugin_id,
            );
            ensure!(
                choice
                    .implementation
                    .descriptor
                    .required_target_capabilities()
                    == [
                        ExecutionTargetCapability::Request,
                        ExecutionTargetCapability::WasmComponent,
                        ExecutionTargetCapability::Workers,
                    ],
                "Workers App target rejects Plugin `{}`: selected Bundle variant must explicitly require request, wasm-component and workers only",
                candidate.plugin_id,
            );
            let component = stage
                .path()
                .join("components")
                .join(format!("{}.wasm", candidate.plugin_id));
            fs::copy(directory.join(&artifact.path), &component)?;
            ensure!(
                super::super::local_host::digest(&component)? == artifact.digest,
                "Workers Component bytes differ from the selected verified Artifact for Plugin `{}`",
                candidate.plugin_id,
            );
            Ok((verified, (choice, component)))
        })?;
        ensure!(
            verified.plugin_id == candidate.plugin_id
                && verified.release_version == candidate.release_version,
            "Workers App source identity changed for Plugin `{}`",
            candidate.plugin_id,
        );
        let (choice, component) = choice;
        let artifact = &choice.implementation.artifact;
        inventory.push(json!({
            "path": archive_relative,
            "plugin_id": candidate.plugin_id,
            "release_version": verified.release_version,
            "manifest_digest": verified.manifest_digest,
            "execution_class": choice.implementation.descriptor.execution_class().as_str(),
            "runtime_profile": choice.implementation.descriptor.runtime_profile(),
            "target": HOST_TARGET,
            "implementation_id": choice.implementation.implementation_id,
            "artifact_path": artifact.path,
            "artifact_digest": artifact.digest,
            "artifact_size": artifact.size,
            "artifact_media_type": artifact.media_type,
            "artifact_target": artifact.target,
            "target_capability_profile": choice.target_capability_profile,
            "selection": choice.evidence,
        }));
        inputs.push(LocalPluginInput {
            descriptor: choice.implementation.descriptor.clone(),
            manifest_digest: verified.manifest_digest.clone(),
            app_owned: candidate.role == SourceRole::AppOwned || candidate.surface_owner.is_some(),
            source: candidate.project.display().to_string(),
        });
        selected.push(SelectedBundle {
            plugin_id: candidate.plugin_id.clone(),
            component,
            artifact_digest: artifact.digest.clone(),
            bundle_digest: super::super::local_host::digest(&archive)?,
            manifest_digest: verified.manifest_digest,
            implementation_id: choice.implementation.implementation_id,
            variant_id: choice.implementation.variant_id,
            descriptor: choice.implementation.descriptor,
            selection: choice.evidence,
            target_capability_profile: choice.target_capability_profile,
        });
    }
    ensure!(
        !inputs.is_empty(),
        "Workers App target found no selected verified Component Bundle"
    );
    let (authority, resolved) =
        GeneratedHostBuild::lower_local("local.app", inputs)?.with_local_root(stage.path())?;
    let selected = admit_plan(&resolved, &selected)?;
    let knowledge_settings = selected.plugin_id == KNOWLEDGE_SETTINGS_PLUGIN_ID;
    if knowledge_settings {
        let instance = &resolved.plan().plugin_instances()[0];
        ensure!(
            instance.instance_key() == KNOWLEDGE_SETTINGS_PLAN_KEY
                && instance.package_id() == KNOWLEDGE_SETTINGS_PLUGIN_ID
                && instance.authoring_version() == 2,
            "Knowledge settings local Workers bridge requires the exact verified Plugin, package, Plan Instance and authoring V2 identity"
        );
    }
    let descriptor = source_descriptor_evidence(
        &selected.component,
        resolved.plan().plugin_instances()[0].authoring_version(),
    )?;
    fs::write(
        stage.path().join(".lenso/host-build.json"),
        serde_json::to_vec_pretty(&authority)?,
    )?;
    fs::write(
        stage.path().join("bundles.json"),
        serde_json::to_vec_pretty(&inventory)?,
    )?;
    let plan_bytes = serde_json::to_vec(resolved.plan())?;
    fs::write(
        stage.path().join("plan.mjs"),
        format!(
            "export default {};\n",
            String::from_utf8(plan_bytes.clone())?
        ),
    )?;
    let trusted_digests = descriptor
        .expected_digests
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?
        .unwrap_or_else(|| "undefined".to_owned());
    fs::write(
        stage.path().join("descriptor-digests.mjs"),
        format!("export default {trusted_digests};\n"),
    )?;
    fs::copy(
        &selected.component,
        stage.path().join("guest.component.wasm"),
    )?;

    let runtime_evidence = if knowledge_settings {
        copy_pinned_knowledge_settings_runtime(&args.workers_runtime, stage.path())?
    } else {
        json!({
            "package": "@lenso/workers-runtime",
            "version": RUNTIME_VERSION,
            "module_digest": copy_pinned_runtime(&args.workers_runtime, stage.path())?,
        })
    };
    transpile_component(&args.jco, stage.path())?;
    if knowledge_settings {
        fs::write(
            stage.path().join("knowledge-settings-artifact.mjs"),
            format!(
                "export default {};\n",
                serde_json::to_string(&json!({
                    "world": KNOWLEDGE_SETTINGS_WORLD,
                    "digest": selected.artifact_digest,
                }))?
            ),
        )?;
        fs::write(
            stage.path().join("worker.mjs"),
            include_str!("../../../assets/workers-knowledge-settings-app.mjs"),
        )?;
    } else {
        fs::write(
            stage.path().join("workers-http.mjs"),
            include_str!("../../../assets/workers-http.mjs"),
        )?;
        fs::write(
            stage.path().join("worker.mjs"),
            include_str!("../../../assets/workers-app.mjs"),
        )?;
    }
    fs::write(
        stage.path().join("wrangler.jsonc"),
        include_str!("../../../assets/workers-wrangler.jsonc"),
    )?;
    fs::write(
        stage.path().join("README.md"),
        if knowledge_settings {
            include_str!("../../../assets/workers-knowledge-settings-README.md")
        } else {
            include_str!("../../../assets/workers-app-README.md")
        },
    )?;
    let mut receipt = json!({
        "schema": "lenso.workers-app-build.v1",
        "target": HOST_TARGET,
        "environment": "local-workerd",
        "builder_version": env!("CARGO_PKG_VERSION"),
        "source_digest": source_digest,
        "selected_source_digests": source_digests,
        "plugin_id": selected.plugin_id,
        "manifest_digest": selected.manifest_digest,
        "bundle_digest": selected.bundle_digest,
        "implementation_id": selected.implementation_id,
        "variant_id": selected.variant_id,
        "component_digest": selected.artifact_digest,
        "source_descriptor_digest": descriptor.source_digest,
        "expected_descriptor_digests": descriptor.expected_digests,
        "plan_digest": digest_bytes(&plan_bytes),
        "workers_runtime": runtime_evidence,
        "jco_version": JCO_VERSION,
        "jco_core_digest": super::super::local_host::digest(&stage.path().join("guest.core.wasm"))?,
        "jco_bindings_digest": super::super::local_host::digest(&stage.path().join("guest.js"))?,
    });
    if knowledge_settings {
        receipt["private_world"] = json!(KNOWLEDGE_SETTINGS_WORLD);
        receipt["host_bridge"] = json!("local-loopback-knowledge-settings.v1");
    }
    fs::write(
        stage.path().join("workers-build.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    ensure!(
        super::super::local_host::input_digest(&root)? == source_digest,
        "App source changed during Workers build; retry"
    );
    let fresh = lenso_app_authoring::discovery::conventions::plan(&discover(&root)?)?;
    ensure!(
        serde_json::to_vec(&fresh)? == serde_json::to_vec(&conventions)?,
        "App convention selection changed during Workers build; retry"
    );
    for candidate in &candidates {
        ensure!(
            source_digests[&candidate.plugin_id]
                == super::super::local_host::input_digest(&candidate.project)?,
            "Plugin `{}` source changed during Workers build; retry",
            candidate.plugin_id
        );
    }
    super::super::preset::checkpoint()?;
    super::super::build::publish_new_output(stage.path(), &destination)?;
    println!(
        "Built Workers App at {}. Run `wrangler dev --local --config wrangler.jsonc` there.",
        destination.display()
    );
    Ok(())
}

fn source_descriptor_evidence(
    component: &Path,
    authoring_version: u32,
) -> anyhow::Result<DescriptorEvidence> {
    let source = extract_plugin_descriptor(&fs::read(component)?)
        .context("extract verified Component source descriptor")?;
    let mut expected_capability = json!({
        "capability_id": lenso_capability_http_endpoint::CAPABILITY_ID,
        "descriptor_version": lenso_capability_http_endpoint::DESCRIPTOR_VERSION,
        "request_operations": ["describe", "handle"],
    });
    let expected_digests = match authoring_version {
        1 => None,
        2 => {
            expected_capability["descriptor_digest"] =
                json!(lenso_capability_http_endpoint::DESCRIPTOR_DIGEST);
            Some(BTreeMap::from([(
                lenso_capability_http_endpoint::CAPABILITY_ID.to_owned(),
                lenso_capability_http_endpoint::DESCRIPTOR_DIGEST.to_owned(),
            )]))
        }
        other => bail!("Workers App target rejects unsupported authoring version {other}"),
    };
    let expected = json!({
        "abi": "lenso.json-request@1",
        "capabilities": [expected_capability],
    });
    ensure!(
        serde_json::from_slice::<Value>(&source)? == expected,
        "Workers App target rejects Component source descriptor: authoring V{authoring_version} requires the exact HTTP Endpoint request ABI{}",
        if authoring_version == 2 {
            " and trusted Descriptor digest"
        } else {
            " without Descriptor digest"
        },
    );
    Ok(DescriptorEvidence {
        source_digest: digest_bytes(&source),
        expected_digests,
    })
}

fn admit_plan<'a>(
    resolved: &ResolvedApp,
    selected: &'a [SelectedBundle],
) -> anyhow::Result<&'a SelectedBundle> {
    let plan = resolved.plan();
    ensure!(
        plan.schema_version() == lenso_app_plan::PLAN_SCHEMA_VERSION
            && plan.terminal_policy() == &TerminalPolicy::RequiredPath
            && plan.execution_lanes().len() == 1
            && plan.execution_lanes()[0].id().as_str() == "main",
        "Workers App target rejects non-V4, non-required-path or multi-lane Plan semantics"
    );
    let names = resolved
        .instances()
        .iter()
        .map(|instance| (instance.plan_key(), instance.id().to_string()))
        .collect::<BTreeMap<_, _>>();
    if let Some(binding) = plan.capability_bindings().first() {
        let consumer = names
            .get(binding.consumer_instance())
            .map(String::as_str)
            .unwrap_or(binding.consumer_instance());
        let provider = names
            .get(binding.provider_instance())
            .map(String::as_str)
            .unwrap_or(binding.provider_instance());
        bail!(
            "Workers App target rejects dependency path {consumer} --{}--> {provider}: this first Component host has no Capability binding or Host import adapter",
            binding.requirement_id(),
        );
    }
    let [instance] = plan.plugin_instances() else {
        bail!(
            "Workers App target supports one selected HTTP Endpoint Instance; resolved selected closure: {}",
            names.values().cloned().collect::<Vec<_>>().join(", "),
        );
    };
    let id = resolved
        .instances()
        .iter()
        .find(|candidate| candidate.plan_key() == instance.instance_key())
        .context("resolved Workers Plan Instance lacks Plugin identity")?
        .id();
    let bundle = selected
        .iter()
        .find(|bundle| bundle.plugin_id == id.plugin_id())
        .context("resolved Workers Instance has no selected Bundle Artifact")?;
    ensure!(
        instance.package_id() == bundle.descriptor.runtime_package_id()
            && instance.package_revision() == bundle.artifact_digest
            && instance.execution_class() == bundle.descriptor.execution_class()
            && instance.runtime_profile() == bundle.descriptor.runtime_profile()
            && instance.entrypoint() == bundle.descriptor.entrypoint()
            && instance.entrypoint() == "plugin"
            && instance.execution_lane().as_str() == "main",
        "Workers Plan Instance `{id}` differs from its verified selected Component Artifact",
    );
    ensure!(
        instance.required_target_capabilities()
            == [
                ExecutionTargetCapability::Request,
                ExecutionTargetCapability::WasmComponent,
                ExecutionTargetCapability::Workers,
            ]
            && instance.configuration() == "{}"
            && instance.required_capabilities().is_empty()
            && instance.restart_policy() == RestartPolicy::never()
            && instance.criticality() == PluginCriticality::NonCritical,
        "Workers App target rejects Instance `{id}`: configuration, dependencies, supervision or target requirements exceed the request-only Component profile",
    );
    let [endpoint] = instance.provided_capabilities() else {
        bail!("Workers App target rejects Instance `{id}`: expected one HTTP Endpoint Capability");
    };
    ensure!(
        endpoint.capability_id() == lenso_capability_http_endpoint::CAPABILITY_ID
            && endpoint.descriptor_version() == lenso_capability_http_endpoint::DESCRIPTOR_VERSION
            && endpoint.operations() == ["describe", "handle"]
            && endpoint
                .operations()
                .iter()
                .all(|operation| endpoint.operation_kind(operation)
                    == Some(CapabilityOperationKind::Request))
            && endpoint.default_admission().is_none()
            && endpoint.operation_admissions().is_empty()
            && endpoint.event_admission().is_none()
            && !endpoint.supports_cross_lane_transfer(),
        "Workers App target rejects Instance `{id}`: only uncustomized lenso.http.endpoint@1 describe/handle Requests are implemented",
    );
    ensure!(
        bundle
            .target_capability_profile
            .supports(HostTargetCapability::Workers)
            && bundle.selection.selected.execution_class == *instance.execution_class(),
        "Workers App target admission differs from selected Plan Instance `{id}`",
    );
    Ok(bundle)
}

fn copy_pinned_runtime(package: &Path, stage: &Path) -> anyhow::Result<String> {
    let package = fs::canonicalize(package).context("locate @lenso/workers-runtime package")?;
    let package_json = package.join("package.json");
    let module = package.join("component-requests.mjs");
    for path in [&package_json, &module] {
        let metadata = fs::symlink_metadata(path)?;
        ensure!(
            metadata.file_type().is_file() && metadata.len() <= MAX_WORKERS_MODULE_BYTES,
            "Workers runtime input must be a bounded regular file: {}",
            path.display()
        );
    }
    let manifest: Value = serde_json::from_slice(&fs::read(&package_json)?)?;
    ensure!(
        manifest["name"] == "@lenso/workers-runtime"
            && manifest["version"] == RUNTIME_VERSION
            && manifest["exports"]["./component-requests"] == "./component-requests.mjs",
        "Workers runtime package must expose @lenso/workers-runtime/component-requests at version {RUNTIME_VERSION}",
    );
    let bytes = fs::read(&module)?;
    let digest = digest_bytes(&bytes);
    ensure!(
        digest == format!("sha256:{RUNTIME_MODULE_SHA256}"),
        "Workers runtime Component adapter differs from pinned lenso-js 3f83cde candidate: {digest}",
    );
    fs::write(stage.join("component-requests.mjs"), bytes)?;
    Ok(digest)
}

fn copy_pinned_knowledge_settings_runtime(package: &Path, stage: &Path) -> anyhow::Result<Value> {
    let package = fs::canonicalize(package).context("locate @lenso/workers-runtime package")?;
    let package_json = package.join("package.json");
    let metadata = fs::symlink_metadata(&package_json)?;
    ensure!(
        metadata.file_type().is_file() && metadata.len() <= MAX_WORKERS_MODULE_BYTES,
        "Workers runtime input must be a bounded regular file: {}",
        package_json.display()
    );
    let manifest: Value = serde_json::from_slice(&fs::read(&package_json)?)?;
    ensure!(
        manifest["name"] == "@lenso/workers-runtime"
            && manifest["version"] == KNOWLEDGE_SETTINGS_RUNTIME_VERSION
            && manifest["exports"]["./component-requests"] == "./component-requests.mjs"
            && manifest["exports"]["./knowledge-settings-local"]
                == "./knowledge-settings-local.mjs",
        "Knowledge settings local Workers App requires the exact @lenso/workers-runtime {} package exports",
        KNOWLEDGE_SETTINGS_RUNTIME_VERSION
    );
    let mut copies = Vec::new();
    let mut digests = BTreeMap::new();
    for (name, expected_sha256) in KNOWLEDGE_SETTINGS_RUNTIME_FILES {
        let path = package.join(name);
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            metadata.file_type().is_file() && metadata.len() <= MAX_WORKERS_MODULE_BYTES,
            "Workers runtime input must be a bounded regular file: {}",
            path.display()
        );
        let bytes = fs::read(&path)?;
        let digest = digest_bytes(&bytes);
        ensure!(
            digest == format!("sha256:{expected_sha256}"),
            "Knowledge settings Workers runtime module {name} differs from pinned local candidate: {digest}"
        );
        copies.push((name, bytes));
        digests.insert(name.to_owned(), digest);
    }
    for (name, bytes) in copies {
        fs::write(stage.join(name), bytes)?;
    }
    Ok(json!({
        "package": "@lenso/workers-runtime",
        "version": KNOWLEDGE_SETTINGS_RUNTIME_VERSION,
        "module_digests": digests,
    }))
}

fn transpile_component(jco: &Path, stage: &Path) -> anyhow::Result<()> {
    let component = stage.join("guest.component.wasm");
    let component_digest = super::super::local_host::digest(&component)?;
    for payload in wasmparser::Parser::new(0).parse_all(&fs::read(&component)?) {
        if let wasmparser::Payload::ComponentImportSection(imports) =
            payload.context("inspect selected Component imports")?
        {
            ensure!(
                imports.count() == 0,
                "Workers App target rejects Component Host imports"
            );
        }
    }
    let before = fs::read_dir(stage)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<Result<BTreeSet<_>, _>>()?;
    let version = super::super::build_command(jco)
        .arg("--version")
        .output()
        .context("probe Jco version")?;
    ensure!(
        version.status.success() && String::from_utf8_lossy(&version.stdout).trim() == JCO_VERSION,
        "Workers App build requires Jco {JCO_VERSION}"
    );
    let result = super::super::build_command(jco)
        .arg("transpile")
        .arg(stage.join("guest.component.wasm"))
        .args([
            "--instantiation",
            "sync",
            "--no-wasi-shim",
            "--name",
            "guest",
            "-o",
        ])
        .arg(stage)
        .output()
        .context("transpile selected Component for workerd")?;
    ensure!(
        result.status.success(),
        "Jco could not transpile the selected Workers Component: {}",
        String::from_utf8_lossy(&result.stderr).trim(),
    );
    let after = fs::read_dir(stage)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<Result<BTreeSet<_>, _>>()?;
    let created = after.difference(&before).cloned().collect::<BTreeSet<_>>();
    ensure!(
        created
            == ["guest.core.wasm", "guest.d.ts", "guest.js"]
                .into_iter()
                .map(std::ffi::OsString::from)
                .collect(),
        "Jco output closure differs from the pinned single-core Component profile"
    );
    ensure!(
        super::super::local_host::digest(&component)? == component_digest,
        "Jco changed the selected verified Component input"
    );
    for path in ["guest.js", "guest.core.wasm", "guest.d.ts"] {
        let metadata = fs::symlink_metadata(stage.join(path))
            .with_context(|| format!("Jco did not create {path}"))?;
        ensure!(
            metadata.file_type().is_file() && metadata.len() <= MAX_WORKERS_MODULE_BYTES,
            "Jco produced an unsupported or oversized {path}"
        );
    }
    let core = fs::read(stage.join("guest.core.wasm"))?;
    let mut module = false;
    for payload in wasmparser::Parser::new(0).parse_all(&core) {
        match payload.context("inspect Jco core Wasm")? {
            wasmparser::Payload::Version { encoding, .. } => {
                ensure!(
                    encoding == wasmparser::Encoding::Module,
                    "Jco output is not core Wasm"
                );
                module = true;
            }
            wasmparser::Payload::ImportSection(imports) => {
                ensure!(
                    imports.count() == 0,
                    "Workers App target rejects Component core Host imports"
                );
            }
            _ => {}
        }
    }
    ensure!(module, "Jco did not emit a core Wasm module");
    Ok(())
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

#[cfg(test)]
mod tests;
