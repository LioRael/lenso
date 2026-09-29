#[cfg(generated_native_host)]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HostFacilityGrants {
    schema: String,
    instances: std::collections::BTreeMap<String, std::collections::BTreeMap<String, serde_json::Value>>,
}

#[cfg(generated_native_host)]
fn prepare_host_facilities(
    plan: &ResolvedAppPlan,
    path: Option<&std::path::Path>,
    clock: &lenso_native_adapter::NativeHostClock,
) -> anyhow::Result<lenso_native_adapter::NativeInstanceFacilities> {
    let _ = clock;
    let mut facilities = lenso_native_adapter::NativeInstanceFacilities::new();
    let Some(path) = path else { return Ok(facilities); };
    let metadata = fs::metadata(path).context("read Host facility grants")?;
    anyhow::ensure!(metadata.is_file() && metadata.len() <= 1_048_576, "invalid Host facility grants file");
    let grants: HostFacilityGrants = serde_json::from_slice(&fs::read(path)?)
        .map_err(|_| anyhow::anyhow!("invalid Host facility grants"))?;
    anyhow::ensure!(grants.schema == "lenso.host-facilities.v1", "unsupported Host facility grants schema");
    for (key, slots) in &grants.instances {
        let instance = plan.plugin_instances().iter().find(|instance| instance.instance_key() == key)
            .context("Host facility names an unselected Instance")?;
        let mut values = lenso_native_adapter::NativeFacilities::new();
        for (slot, binding) in slots {
            values = match (instance.package_id(), slot.as_str()) {
                // LENSO_NATIVE_FACILITY_FACTORIES
                _ => bail!("Host facility has no source-owned target factory"),
            };
        }
        facilities = facilities.with(key, values).map_err(|error| anyhow::anyhow!("{error:?}"))?;
    }
    Ok(facilities)
}
