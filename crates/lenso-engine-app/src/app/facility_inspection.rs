//! Read-only target/identity checks. Resource readiness belongs to the owner.
use std::{fs, path::Path};

use anyhow::{Context as _, ensure};
use lenso_app_plan::ResolvedAppPlan;
use serde_json::{Value, json};

pub(super) fn report(
    root: &Path,
    plan: &ResolvedAppPlan,
    target: &str,
    profile: Option<&Path>,
) -> anyhow::Result<Value> {
    let sources = super::local_host::facilities::read_sources(root)?;
    let Some(profile) = profile else {
        return Ok(
            json!({"status":"not_supplied","validation":"offline_identity_only","readiness":"not_run","bindings":[]}),
        );
    };
    let metadata = fs::metadata(profile)?;
    ensure!(
        metadata.is_file() && metadata.len() <= 1_048_576,
        "invalid Host facility profile file"
    );
    let grants: Value = serde_json::from_slice(&fs::read(profile)?)
        .context("invalid Host facility profile JSON")?;
    ensure!(
        grants["schema"] == "lenso.host-facilities.v1"
            && grants.as_object().is_some_and(|value| value.len() == 2),
        "invalid Host facility profile schema"
    );
    let instances = grants["instances"]
        .as_object()
        .context("Host facility instances must be an object")?;
    let mut bindings = Vec::new();
    for (key, slots) in instances {
        let selected = plan
            .plugin_instances()
            .iter()
            .find(|instance| instance.instance_key() == key);
        for (slot, binding) in slots
            .as_object()
            .context("Host facility slots must be an object")?
        {
            let source = selected.and_then(|instance| {
                sources.iter().find(|source| {
                    source.package_id == instance.package_id() && source.slot == *slot
                })
            });
            let factory = source.and_then(|source| {
                if target == "workers" {
                    source.workers.as_ref()
                } else {
                    source.native.as_ref()
                }
            });
            let reason = if selected.is_none() {
                Some("unselected_instance")
            } else if !super::local_host::facilities::valid_slot(slot) || source.is_none() {
                Some("no_source_owned_slot")
            } else if factory.is_none() {
                Some("unsupported_target_factory")
            } else if target == "workers"
                && !(binding.as_object().is_some_and(|value| {
                    value.len() == 2
                        && value.contains_key("configuration")
                        && value.contains_key("binding")
                }) && (binding["binding"].is_null()
                    || binding["binding"].as_str().is_some_and(valid_binding)))
            {
                Some("invalid_worker_binding")
            } else {
                None
            };
            bindings.push(json!({
                "instance_key":key,"slot":slot,"package_id":selected.map(|instance| instance.package_id()),
                "target":target,"owner_factory":factory,"selection_source":profile,
                "resource_reference":if target == "workers" { binding["binding"].as_str() } else { None },
                "explicit_none":binding.is_null() || (target == "workers" && binding["binding"].is_null()),
                "status":if reason.is_some() { "rejected" } else { "matched" },"reason":reason,
                "help":if reason.is_some() { "Select an exact plan Instance and a source-owned slot for this target." } else { "Owner schema and live readiness are checked before the App Ready Gate." },
            }));
        }
    }
    for instance in plan.plugin_instances() {
        for source in sources
            .iter()
            .filter(|source| source.package_id == instance.package_id())
        {
            if instances
                .get(instance.instance_key())
                .and_then(|slots| slots.get(&source.slot))
                .is_some()
            {
                continue;
            }
            let factory = if target == "workers" {
                source.workers.as_ref()
            } else {
                source.native.as_ref()
            };
            if factory.is_none() {
                continue;
            }
            bindings.push(json!({"instance_key":instance.instance_key(),"package_id":instance.package_id(),
                "slot":source.slot,"target":target,"owner_factory":factory,"status":"not_supplied",
                "reason":"grant_not_supplied","readiness":"not_run",
                "help":"Provide the selected Instance's slot in a Host facility profile; owner construction checks whether it is required."}));
        }
    }
    let invalid = bindings
        .iter()
        .any(|binding| binding["status"] == "rejected");
    Ok(
        json!({"status":if invalid { "invalid" } else { "matched" },"validation":"offline_identity_only","readiness":"not_run","bindings":bindings}),
    )
}

fn valid_binding(binding: &str) -> bool {
    let mut bytes = binding.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

pub(super) fn workers(root: &Path, profile: Option<&Path>) -> anyhow::Result<Value> {
    let receipt: Value = serde_json::from_slice(&fs::read(root.join("workers-build.json"))?)?;
    ensure!(
        receipt["schema"] == "lenso.workers-app-build.v1"
            && receipt["profile"] == "lenso.linked-rust-workers@1",
        "unsupported Workers explanation profile"
    );
    for (path, field) in [
        ("host_bg.wasm", "host_wasm_digest"),
        ("host.js", "host_bindings_digest"),
        ("worker.mjs", "worker_entry_digest"),
    ] {
        ensure!(
            super::local_host::digest(&root.join(path))? == receipt[field],
            "Workers artifact differs from its build receipt"
        );
    }
    let modules = receipt["workers_runtime"]["module_digests"]
        .as_object()
        .context("missing Workers runtime digest evidence")?;
    for (name, expected) in modules {
        ensure!(
            Path::new(name).components().count() == 1
                && name.ends_with(".mjs")
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.')),
            "invalid Workers runtime evidence path"
        );
        ensure!(
            super::local_host::digest(&root.join("runtime").join(name))? == *expected,
            "Workers runtime differs from its build receipt"
        );
    }
    for owner in receipt["host_facilities"]["owner_modules"]
        .as_array()
        .context("missing owner module digest evidence")?
    {
        let name = owner["module"]
            .as_str()
            .context("invalid owner module path")?;
        ensure!(
            name.starts_with("facilities/owner_")
                && name.ends_with(".mjs")
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'_' | b'.'))
                && Path::new(name).components().count() == 2,
            "invalid owner module evidence path"
        );
        ensure!(
            super::local_host::digest(&root.join(name))? == owner["digest"],
            "Workers owner adapter differs from its build receipt"
        );
    }
    let plan_bytes = fs::read(root.join(".lenso/generated-host/src/plan.json"))?;
    ensure!(
        format!(
            "sha256:{}",
            super::local_host::digest_text(std::str::from_utf8(&plan_bytes)?)
        ) == receipt["plan_digest"],
        "Workers plan differs from its build receipt"
    );
    let plan: ResolvedAppPlan = serde_json::from_slice(&plan_bytes)?;
    plan.validate()
        .map_err(|error| anyhow::anyhow!("invalid persisted Workers plan: {error:?}"))?;
    let baked_profile = root.join(".lenso/host-facility-grants.json");
    if !receipt["host_facilities"]["grants_digest"].is_null() {
        let baked: Value = serde_json::from_slice(&fs::read(&baked_profile)?)?;
        ensure!(
            format!(
                "sha256:{}",
                super::local_host::digest_text(&serde_json::to_string(&baked)?)
            ) == receipt["host_facilities"]["grants_digest"],
            "Workers facility grants differ from their build receipt"
        );
    }
    let facilities = report(
        root,
        &plan,
        "workers",
        profile.or_else(|| baked_profile.is_file().then_some(baked_profile.as_path())),
    )?;
    let requirements = plan.plugin_instances().iter().flat_map(|instance| instance.required_capabilities().iter().map(|requirement| {
        let providers = plan.capability_bindings().iter().filter(|binding| binding.consumer_instance() == instance.instance_key() && binding.requirement_id() == requirement.requirement_id()).map(|binding| json!({"provider_instance":binding.provider_instance(),"provider_order":binding.provider_order()})).collect::<Vec<_>>();
        json!({"consumer_instance":instance.instance_key(),"requirement_id":requirement.requirement_id(),"capability_id":requirement.capability_id(),"descriptor_version":requirement.descriptor_version(),"cardinality":requirement.cardinality(),"selected_providers":providers,"satisfied":!providers.is_empty() || requirement.cardinality() != lenso_app_plan::CapabilityCardinality::One})
    })).collect::<Vec<_>>();
    Ok(json!({
        "schema":"lenso.app-explain.v1","host":{"target":"workers","profile":receipt["profile"]},
        "target_capability_profiles":[{"profile":receipt["profile"],"state_lifetime":"event","interactions":["request"]}],
        "implementation_selection":plan.plugin_instances().iter().map(|instance| json!({"instance_key":instance.instance_key(),"package_id":instance.package_id(),"package_revision":instance.package_revision(),"execution_class":instance.execution_class(),"runtime_profile":instance.runtime_profile()})).collect::<Vec<_>>(),
        "consumer_requirements":requirements,"unmet_consumer_requirements":[],"host_facilities":facilities,
        "engine_execution":{"status":"not_observed","detail":"Persisted build evidence; live readiness is not inspected."},
        "build_receipt":receipt,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persisted_workers_explanation_is_read_only_and_rejects_changed_entry_bytes() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(".lenso/generated-host/src")).unwrap();
        fs::create_dir_all(root.path().join("runtime")).unwrap();
        for (name, bytes) in [
            ("host_bg.wasm", "wasm fixture"),
            ("host.js", "binding fixture"),
            ("worker.mjs", "entry fixture"),
            ("runtime/clock.mjs", "clock fixture"),
        ] {
            fs::write(root.path().join(name), bytes).unwrap();
        }
        let plan = lenso_app_plan::AppComposition::new(vec![], vec![])
            .resolve()
            .unwrap();
        let plan = serde_json::to_string(&plan).unwrap();
        fs::write(
            root.path().join(".lenso/generated-host/src/plan.json"),
            &plan,
        )
        .unwrap();
        let receipt = json!({"schema":"lenso.workers-app-build.v1", "profile":"lenso.linked-rust-workers@1",
            "host_wasm_digest":super::super::local_host::digest(&root.path().join("host_bg.wasm")).unwrap(),
            "host_bindings_digest":super::super::local_host::digest(&root.path().join("host.js")).unwrap(),
            "worker_entry_digest":super::super::local_host::digest(&root.path().join("worker.mjs")).unwrap(),
            "plan_digest":format!("sha256:{}",super::super::local_host::digest_text(&plan)),
            "workers_runtime":{"module_digests":{"clock.mjs":super::super::local_host::digest(&root.path().join("runtime/clock.mjs")).unwrap()}},
            "host_facilities":{"grants_digest":null,"owner_modules":[]}});
        let bytes = serde_json::to_vec(&receipt).unwrap();
        fs::write(root.path().join("workers-build.json"), &bytes).unwrap();
        let explanation = workers(root.path(), None).unwrap();
        assert_eq!(explanation["engine_execution"]["status"], "not_observed");
        assert_eq!(explanation["host_facilities"]["readiness"], "not_run");
        assert_eq!(
            fs::read(root.path().join("workers-build.json")).unwrap(),
            bytes
        );
        fs::write(root.path().join("worker.mjs"), "changed entry").unwrap();
        assert!(
            workers(root.path(), None)
                .unwrap_err()
                .to_string()
                .contains("differs from its build receipt")
        );
    }

    #[test]
    fn inspection_redacts_native_values_and_rejects_unselected_instances_without_writes() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".lenso")).unwrap();
        fs::write(root.path().join(".lenso/host-facility-sources.json"), serde_json::to_vec(&json!([{"alias":"local_plugin_0","package_id":"example.state","slot":"state","native":"host_facilities::state","workers":null,"workers_adapter":null}])).unwrap()).unwrap();
        let profile = root.path().join("grants.json");
        let bytes = serde_json::to_vec(&json!({"schema":"lenso.host-facilities.v1","instances":{"primary":{"state":{"connection_uri":"secret://value"}},"outsider":{"state":null}}})).unwrap();
        fs::write(&profile, &bytes).unwrap();
        let plan = ResolvedAppPlan::new(
            vec![lenso_app_plan::PluginInstancePlan::new(
                "primary",
                "example.state",
            )],
            vec![],
        );
        let explained = report(root.path(), &plan, "native", Some(&profile)).unwrap();
        assert_eq!(explained["status"], "invalid");
        assert!(
            !serde_json::to_string(&explained)
                .unwrap()
                .contains("secret://value")
        );
        assert_eq!(fs::read(profile).unwrap(), bytes);
    }
}
