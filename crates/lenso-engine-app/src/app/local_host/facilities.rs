//! Source-owned facility factories admitted into the generated Host.
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct FacilitySource {
    pub(crate) alias: String,
    pub(crate) package_id: String,
    pub(crate) slot: String,
    pub(crate) native: Option<String>,
    #[serde(default)]
    pub(crate) native_clock: bool,
    pub(crate) workers: Option<String>,
    pub(crate) workers_adapter: Option<PathBuf>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SlotMetadata {
    native: Option<String>,
    #[serde(default, rename = "native-clock")]
    native_clock: bool,
    workers: Option<String>,
    #[serde(rename = "workers-adapter")]
    workers_adapter: Option<PathBuf>,
}

#[derive(Debug, Default)]
pub(super) struct Sources(Vec<FacilitySource>);

impl Sources {
    pub(super) fn select(
        &mut self,
        alias: &str,
        package_id: &str,
        package: &Value,
        root: &Path,
    ) -> anyhow::Result<()> {
        let Some(metadata) = package.pointer("/metadata/lenso/host-facilities") else {
            return Ok(());
        };
        let slots = metadata
            .as_object()
            .context("host-facilities must be an object")?;
        ensure!(slots.len() <= 16, "source facility slot count limit");
        for (slot, value) in slots {
            ensure!(valid_slot(slot), "invalid source Host facility slot");
            ensure!(
                !self
                    .0
                    .iter()
                    .any(|source| source.package_id == package_id && source.slot == *slot),
                "duplicate source Host facility slot"
            );
            let metadata: SlotMetadata =
                serde_json::from_value(value.clone()).context("invalid Host facility metadata")?;
            ensure!(
                metadata.native.is_some() || metadata.workers.is_some(),
                "Host facility needs a target factory"
            );
            ensure!(
                !metadata.native_clock || metadata.native.is_some(),
                "native-clock needs a Native owner factory"
            );
            for factory in [&metadata.native, &metadata.workers].into_iter().flatten() {
                ensure!(
                    valid_factory(factory),
                    "Host facility factory must be a relative Rust module path"
                );
            }
            ensure!(
                metadata.workers.is_some() == metadata.workers_adapter.is_some(),
                "Workers facility needs both its Rust factory and private JavaScript adapter"
            );
            let workers_adapter = metadata
                .workers_adapter
                .map(|path| -> anyhow::Result<PathBuf> {
                    ensure!(
                        !path.is_absolute()
                            && path.components().all(|component| matches!(
                                component,
                                std::path::Component::Normal(_)
                            )),
                        "Workers facility adapter must be a relative file path"
                    );
                    ensure!(
                        path.extension().is_some_and(|extension| extension == "mjs"),
                        "Workers facility adapter must be an ES module"
                    );
                    let path = root.join(path);
                    let canonical = path
                        .canonicalize()
                        .context("locate Workers facility adapter")?;
                    ensure!(
                        canonical.starts_with(root.canonicalize()?)
                            && fs::metadata(&canonical)?.is_file(),
                        "Workers facility adapter escapes its owning source"
                    );
                    ensure!(
                        fs::metadata(&canonical)?.len() <= 131_072,
                        "Workers facility adapter size limit"
                    );
                    Ok(canonical)
                })
                .transpose()?;
            self.0.push(FacilitySource {
                alias: alias.into(),
                package_id: package_id.into(),
                slot: slot.clone(),
                native: metadata.native,
                native_clock: metadata.native_clock,
                workers: metadata.workers,
                workers_adapter,
            });
        }
        Ok(())
    }

    pub(super) fn write(&self, stage: &Path) -> anyhow::Result<()> {
        fs::write(
            stage.join(".lenso/host-facility-sources.json"),
            serde_json::to_vec_pretty(&self.0)?,
        )?;
        Ok(())
    }

    pub(super) fn render_native(&self, source: &str) -> String {
        if self.0.is_empty() {
            return source.replace("// LENSO_NATIVE_FACILITY_BIND", "if host_facilities.is_some() { bail!(\"this Host has no selected facility factories\"); }");
        }
        let arms = self.0.iter().filter_map(|facility| facility.native.as_ref().map(|factory| {
            let clock = if facility.native_clock { "let clock = clock.clone();" } else { "" };
            let arguments = if facility.native_clock { "&binding, &clock" } else { "&binding" };
            format!("({}, {}) => values.with_factory(slot, {{ let binding = binding.clone(); {clock} move || {}::{factory}({arguments}) }}).map_err(|error| anyhow::anyhow!(\"{{error:?}}\"))?,", json!(facility.package_id), json!(facility.slot), facility.alias)
        })).collect::<Vec<_>>().join("\n");
        let helper = include_str!("native_facilities.rs")
            .replace("// LENSO_NATIVE_FACILITY_FACTORIES", &arms);
        format!("{}\n{helper}", source.replace("// LENSO_NATIVE_FACILITY_BIND", "let clock = lenso_native_adapter::NativeHostClock::from_driver(driver.clone());\nlet registry = registry.with_facilities(prepare_host_facilities(&resolution.plan, host_facilities.as_deref(), &clock)?);"))
    }
}

pub(crate) fn valid_slot(slot: &str) -> bool {
    !slot.is_empty()
        && slot
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn valid_factory(factory: &str) -> bool {
    let identifiers = factory.split("::").collect::<Vec<_>>();
    !identifiers.is_empty()
        && identifiers.iter().all(|identifier| {
            let mut bytes = identifier.bytes();
            bytes
                .next()
                .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
                && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                && !matches!(*identifier, "self" | "super" | "crate")
        })
}

pub(crate) fn read_sources(distribution: &Path) -> anyhow::Result<Vec<FacilitySource>> {
    let path = distribution.join(".lenso/host-facility-sources.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    ensure!(
        fs::metadata(&path)?.len() <= 1_048_576,
        "Host facility source manifest size limit"
    );
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

pub(crate) fn render_workers(
    sources: &[FacilitySource],
    grants: &Value,
    plan: &lenso_app_plan::ResolvedAppPlan,
    stage: &Path,
    scope_limits: &Value,
) -> anyhow::Result<(String, String, Vec<Value>)> {
    ensure!(
        grants["schema"] == "lenso.host-facilities.v1"
            && grants.as_object().is_some_and(|value| value.len() == 2),
        "invalid Workers facility grants schema"
    );
    let instances = grants["instances"]
        .as_object()
        .context("Workers facility instances must be an object")?;
    let mut needed = BTreeSet::new();
    for (key, slots) in instances {
        let instance = plan
            .plugin_instances()
            .iter()
            .find(|instance| instance.instance_key() == key)
            .context("Workers facility names an unselected Instance")?;
        for slot in slots
            .as_object()
            .context("Workers facility slots must be an object")?
            .keys()
        {
            let (index, _) = sources
                .iter()
                .enumerate()
                .find(|(_, source)| {
                    source.package_id == instance.package_id()
                        && source.slot == *slot
                        && source.workers.is_some()
                })
                .context("Workers facility has no source-owned target factory")?;
            needed.insert(index);
        }
    }
    let mut rust = String::new();
    let mut imports = String::new();
    let mut factories = Vec::new();
    let mut evidence = Vec::new();
    for index in needed {
        let source = &sources[index];
        source.workers.as_deref().context("Workers factory")?;
        let module = fs::read(source.workers_adapter.as_ref().context("Workers adapter")?)?;
        ensure!(
            module.len() <= 131_072,
            "Workers facility adapter size limit"
        );
        let module_path = format!("facilities/owner_{index}.mjs");
        fs::create_dir_all(stage.join("facilities"))?;
        fs::write(stage.join(&module_path), &module)?;
        imports.push_str(&format!(
            "import {{ create as factory_{index} }} from './{module_path}';\n"
        ));
        factories.push(format!(
            "{{packageId:{},slot:{},create:factory_{index}}}",
            json!(source.package_id),
            json!(source.slot)
        ));
        evidence.push(json!({"package_id":source.package_id,"slot":source.slot,"module":module_path,"digest":super::digest(&stage.join(&module_path))?}));
    }
    for (key, slots) in instances {
        let instance = plan
            .plugin_instances()
            .iter()
            .find(|instance| instance.instance_key() == key)
            .context("Workers selected Instance")?;
        rust.push_str("let mut values = lenso_native_adapter::NativeFacilities::new();\n");
        for slot in slots.as_object().context("Workers facility slots")?.keys() {
            let source = sources
                .iter()
                .find(|source| source.package_id == instance.package_id() && source.slot == *slot)
                .context("Workers selected facility source")?;
            let factory = source
                .workers
                .as_deref()
                .context("Workers target factory")?;
            rust.push_str(&format!("values = values.with({}, {}::{factory}(&instance_facility(&scope, {}, {}).map_err(error)?).map_err(error)?).map_err(error)?;\n", json!(slot), source.alias, json!(key), json!(slot)));
        }
        rust.push_str(&format!(
            "facilities = facilities.with({}, values).map_err(error)?;\n",
            json!(key)
        ));
    }
    let identities = plan.plugin_instances().iter().map(|instance| json!({"instanceKey":instance.instance_key(),"packageId":instance.package_id()})).collect::<Vec<_>>();
    let scope = format!(
        "{imports}import {{ createInstanceFacilityScope }} from './runtime/facilities.mjs';\nconst grants = {};\nconst instances = {};\nconst factories = [{}];\nconst createScope = (_request, env) => createInstanceFacilityScope({{instances,grants,factories,env,limits:{}}});\n",
        serde_json::to_string(grants)?,
        serde_json::to_string(&identities)?,
        factories.join(","),
        serde_json::to_string(scope_limits)?
    );
    Ok((rust, scope, evidence))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_source_factory_is_explicit_and_not_selected_by_plugin_name() {
        let mut sources = Sources::default();
        sources.select("local_plugin_2", "example.state", &json!({"metadata":{"lenso":{"host-facilities":{"state":{"native":"host_facilities::state"}}}}}), Path::new("/unused")).unwrap();
        let generated = sources.render_native("// LENSO_NATIVE_FACILITY_BIND");
        assert!(generated.contains("local_plugin_2::host_facilities::state(&binding)"));
        assert!(generated.contains("\"example.state\", \"state\""));
        assert!(!generated.contains("// LENSO_"));
    }

    #[test]
    fn clock_opt_in_changes_only_the_declared_native_factory_signature() {
        let mut sources = Sources::default();
        sources.select("local_plugin_0", "example.management", &json!({"metadata":{"lenso":{"host-facilities":{"authority":{"native":"host_facilities::authority","native-clock":true}}}}}), Path::new("/unused")).unwrap();
        let generated = sources.render_native("// LENSO_NATIVE_FACILITY_BIND");
        assert!(generated.contains("NativeHostClock::from_driver(driver.clone())"));
        assert!(generated.contains("local_plugin_0::host_facilities::authority(&binding, &clock)"));
        assert!(generated.contains("let clock = clock.clone();"));
    }

    #[test]
    fn metadata_cannot_inject_rust_or_escape_the_source_root() {
        for slot in [
            json!({"native":"::other::factory"}),
            json!({"native":"factory();"}),
            json!({"native":"super::state"}),
            json!({"native":"valid","unknown":true}),
            json!({"workers":"valid","workers-adapter":"../state.mjs"}),
        ] {
            let mut sources = Sources::default();
            assert!(
                sources
                    .select(
                        "local_plugin_0",
                        "example.state",
                        &json!({"metadata":{"lenso":{"host-facilities":{"state":slot}}}}),
                        Path::new("/unused")
                    )
                    .is_err()
            );
        }
    }
}
