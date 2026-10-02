//! Add a name for an already linked typed contract without requesting features.
use anyhow::Context as _;
use serde_json::Value;

pub(super) fn dependency(mut dependency: Value) -> anyhow::Result<Value> {
    // Original target-normal edges retain their explicit/default requests.
    // Apply this only at alias insertion: Ingress has its own default features
    // and must still be derived from the original identity-only dependency.
    dependency
        .as_object_mut()
        .context("typed contract alias must be a Cargo dependency table")?
        .insert("default-features".into(), Value::Bool(false));
    Ok(dependency)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[test]
fn endpoint_codec_and_ingress_share_the_same_neutral_alias() {
    let raw = serde_json::json!({"package":"lenso-capability-http-endpoint","path":"/framework/endpoint"});
    let name =
        super::contract_dependency_alias("lenso.http.endpoint@1", 0, &raw, Some(&raw)).unwrap();
    let mut dependencies = std::collections::BTreeMap::new();
    dependencies.insert(name, dependency(raw.clone()).unwrap());
    let web_alias = dependency(raw).unwrap();
    assert_eq!(
        dependencies.insert("local_web_contract".into(), web_alias.clone()),
        Some(web_alias)
    );
    assert_eq!(dependencies.len(), 1);
    assert_eq!(
        dependencies["local_web_contract"]["default-features"],
        false
    );
}
