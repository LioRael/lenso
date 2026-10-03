//! Lower the same resolved graph into request-only Workers JS generations.
use super::*;
use lenso_app_plan::{ExecutionClassId, ExecutionTargetCapability};

const CLASS: &str = "lenso.workers-js@1";
const PROFILE: &str = "lenso.workers-js-authoring@2";
const PLUGIN_RUNTIME_DIGEST: &str =
    "sha256:8cfddeff11674380d6a635859370942f9ff55deee1e8e427dba020752cc173e3";

pub(super) fn lower(
    plan: &ResolvedAppPlan,
    artifacts: &BTreeMap<String, String>,
) -> anyhow::Result<ResolvedAppPlan> {
    let instances = plan
        .plugin_instances()
        .iter()
        .map(|instance| {
            if instance.execution_class().as_str() != "lenso.bun-process@1" {
                return Ok(instance.clone());
            }
            ensure!(
                instance.authoring_version() == 2
                    && instance.runtime_profile() == "lenso.bun-authoring@2",
                "Workers JS requires complete-object Bun authoring v2: {}",
                instance.instance_key()
            );
            ensure!(
                instance
                    .required_target_capabilities()
                    .iter()
                    .all(|capability| *capability == ExecutionTargetCapability::NativeProcess),
                "Workers JS does not replace target facilities required by {}",
                instance.instance_key()
            );
            for capability in instance.provided_capabilities() {
                ensure!(
                    capability.stream_operations().is_empty()
                        && capability.event_operations().is_empty(),
                    "Workers JS Request adapter rejects Stream/Event on {}: {}",
                    instance.instance_key(),
                    capability.capability_id()
                );
            }
            Ok(instance
                .clone()
                .with_execution_class(ExecutionClassId::new(CLASS))
                .with_authoring(2, PROFILE)
                .with_package_revision(
                    artifacts
                        .get(instance.package_id())
                        .context("selected Workers JS artifact")?,
                )
                .with_required_target_capabilities([]))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let lowered =
        lenso_app_plan::AppComposition::new(instances, plan.capability_bindings().to_vec())
            .with_execution_lanes(plan.execution_lanes().to_vec())
            .resolve()?
            .with_terminal_policy(plan.terminal_policy().clone());
    lowered.validate()?;
    Ok(lowered)
}

pub(super) fn compile(
    selected: &[&lenso_app_authoring::discovery::Candidate],
    stage: &Path,
    runtime: &Path,
) -> anyhow::Result<BTreeMap<String, String>> {
    let plugins = selected
        .iter()
        .filter(|candidate| candidate.format == "bun")
        .collect::<Vec<_>>();
    if plugins.is_empty() {
        return Ok(BTreeMap::new());
    }
    let bytes = super::super::runtime::read_file(&runtime.join("plugin.mjs"))?;
    ensure!(
        super::super::digest_bytes(&bytes) == PLUGIN_RUNTIME_DIGEST,
        "Workers JS requires the qualified plugin.mjs candidate bytes"
    );
    fs::create_dir_all(stage.join("runtime"))?;
    fs::write(stage.join("runtime/plugin.mjs"), bytes)?;
    let stream = super::super::runtime::read_file(&runtime.join("plugin-stream.mjs"))?;
    ensure!(
        super::super::digest_bytes(&stream)
            == "sha256:75787e1fa26c247bdb9367c1c2d41db08dda3dce9d59ce78e71ad82da7d9b9cf",
        "Workers JS requires the qualified plugin-stream.mjs candidate bytes"
    );
    fs::write(stage.join("runtime/plugin-stream.mjs"), stream)?;
    fs::create_dir_all(stage.join("js-plugins"))?;
    let mut artifacts = BTreeMap::new();
    let mut imports = Vec::new();
    let mut definitions = Vec::new();
    for (index, candidate) in plugins.into_iter().enumerate() {
        let package: Value =
            serde_json::from_slice(&fs::read(candidate.project.join("package.json"))?)?;
        let entry = package
            .pointer("/lenso/source")
            .and_then(Value::as_str)
            .unwrap_or("src/plugin.ts");
        let output = stage.join(format!("js-plugins/plugin-{index}.mjs"));
        let compiler = serde_json::to_string(&json!({
            "entrypoint": candidate.project.join(entry), "outfile": output, "target": "workers-js"
        }))?;
        // Resolve from the declared source package, not the CLI's dependencies.
        let script = format!(
            "import {{ createRequire }} from 'node:module'; import {{ pathToFileURL }} from 'node:url'; const require = createRequire(process.cwd() + '/package.json'); const api = await import(pathToFileURL(require.resolve('@lenso/bun-plugin/targets')).href); await api.buildPluginTarget({compiler});"
        );
        let status = crate::app::build_command("bun")
            .current_dir(&candidate.project)
            .args(["-e", &script])
            .status()
            .context("compile declared TS source for Workers JS")?;
        ensure!(
            status.success(),
            "Plugin {} is incompatible with Workers JS",
            candidate.plugin_id
        );
        artifacts.insert(
            candidate.plugin_id.clone(),
            crate::app::local_host::digest(&output)?,
        );
        imports.push(format!(
            "import definition{index} from './js-plugins/plugin-{index}.mjs';"
        ));
        definitions.push(format!(
            "{}: definition{index}",
            serde_json::to_string(&candidate.plugin_id)?
        ));
    }
    let bridge = include_str!("../../../../../assets/workers-plugin-host.mjs").replace(
        "// LENSO_JS_DEFINITIONS",
        &format!(
            "{}\nconst definitions = {{{}}};",
            imports.join("\n"),
            definitions.join(",")
        ),
    );
    fs::write(stage.join("plugin-host.mjs"), bridge)?;
    Ok(artifacts)
}

pub(super) fn prepare(
    native: &Path,
    generated: &Path,
    manifest: &mut Value,
) -> anyhow::Result<String> {
    // Exact typed codecs were already selected from the source Cargo graph.
    // Retain those expressions rather than discovering a second contract graph.
    fs::copy(
        native.join(".lenso/generated-host/src/codec_links.rs"),
        generated.join("src/codec_links.rs"),
    )?;
    let mut registration = "for codec in include!(\"codec_links.rs\") { codecs.insert(codec.capability_id().to_owned(), codec); }\n".to_owned();
    let evidence: BTreeMap<String, Value> =
        serde_json::from_slice(&fs::read(native.join("runtime-codecs.json"))?)?;
    for descriptor in evidence.values() {
        for endpoint in descriptor["capabilities"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(
                descriptor["required_capabilities"]
                    .as_array()
                    .into_iter()
                    .flatten(),
            )
        {
            ensure!(
                endpoint["stream_operations"]
                    .as_array()
                    .is_none_or(Vec::is_empty)
                    && endpoint["event_operations"]
                        .as_array()
                        .is_none_or(Vec::is_empty),
                "Workers JS Request adapter rejects Stream/Event dependency evidence"
            );
            let id = endpoint["capability_id"]
                .as_str()
                .context("generated Capability identity")?;
            let version = endpoint["descriptor_version"]
                .as_str()
                .context("generated Capability version")?;
            let digest = endpoint["descriptor_digest"]
                .as_str()
                .context("generated Capability digest")?;
            let operations = endpoint["request_operations"]
                .as_array()
                .context("generated Request operations")?;
            registration.push_str(&format!("codecs.entry({id:?}.to_owned()).or_insert_with(|| Rc::new(PortableCodec {{ id: {id:?}, version: {version:?}, digest: {digest:?}, operations: &{operations:?} }}));\n", operations=operations.iter().map(|v| v.as_str().context("operation")).collect::<anyhow::Result<Vec<_>>>()?));
        }
    }
    let source = include_str!("../linked_js_host.rs")
        .replace("// LENSO_WORKERS_JS_CODECS", &registration)
        + include_str!("../../../local_json_template.rs")
            .split("fn portable_codecs(")
            .next()
            .context("portable JSON codec template")?;
    fs::write(generated.join("src/js_host.rs"), source)?;
    // Process adapters do not enter a Workers distribution.
    let dependencies = manifest["dependencies"]
        .as_object_mut()
        .context("Workers dependencies")?;
    for name in [
        "lenso-bun-adapter",
        "lenso-process-adapter",
        "lenso-wasm-adapter",
        "runtime-codec-legacy",
    ] {
        dependencies.remove(name);
    }
    Ok("mod js_host;".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lenso_app_plan::{CapabilityEndpointPlan, PluginInstancePlan};
    #[test]
    fn target_lowering_preserves_logical_instances_config_and_bindings() {
        let original = ResolvedAppPlan::new(
            vec![
                PluginInstancePlan::new("label/left", "label")
                    .with_authoring(2, "lenso.bun-authoring@2")
                    .with_execution_class(ExecutionClassId::new("lenso.bun-process@1"))
                    .with_configuration("{\"label\":\"left\"}")
                    .with_required_target_capabilities([ExecutionTargetCapability::NativeProcess]),
            ],
            vec![],
        );
        let lowered = lower(
            &original,
            &BTreeMap::from([("label".into(), "sha256:workers-artifact".into())]),
        )
        .unwrap();
        assert_eq!(
            lowered.plugin_instances()[0].instance_key(),
            original.plugin_instances()[0].instance_key()
        );
        assert_eq!(
            lowered.plugin_instances()[0].configuration(),
            original.plugin_instances()[0].configuration()
        );
        assert_eq!(
            lowered.capability_bindings(),
            original.capability_bindings()
        );
        assert_eq!(
            lowered.plugin_instances()[0].execution_class().as_str(),
            CLASS
        );
        assert!(
            lowered.plugin_instances()[0]
                .required_target_capabilities()
                .is_empty()
        );
    }
    #[test]
    fn js_stream_fails_before_compilation_or_execution() {
        let plan = ResolvedAppPlan::new(
            vec![
                PluginInstancePlan::new("label/left", "label")
                    .with_authoring(2, "lenso.bun-authoring@2")
                    .with_execution_class(ExecutionClassId::new("lenso.bun-process@1"))
                    .with_capability(
                        CapabilityEndpointPlan::new("test@1", "1.0.0", ["watch"])
                            .with_stream_operation("watch"),
                    ),
            ],
            vec![],
        );
        assert!(
            lower(&plan, &BTreeMap::new())
                .unwrap_err()
                .to_string()
                .contains("rejects Stream/Event")
        );
    }
}
