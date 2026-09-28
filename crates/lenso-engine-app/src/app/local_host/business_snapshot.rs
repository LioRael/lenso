//! Source-selected lifecycle glue; policy and factory overrides belong to the Plugin.

use anyhow::bail;
use serde_json::{Value, json};

/// At most one linked source may own the single `--business-snapshot-policy` input.
#[derive(Debug, Default)]
pub(super) struct HostBinding {
    alias: Option<String>,
}

impl HostBinding {
    pub(super) fn select(&mut self, alias: &str, package: &Value) -> anyhow::Result<()> {
        match package.pointer("/metadata/lenso/host-bindings") {
            None => return Ok(()),
            Some(value) if value == &json!([]) => return Ok(()),
            Some(value) if value == &json!(["business-snapshot@1"]) => {}
            Some(_) => bail!(
                "native Plugin declares unsupported Host bindings; expected business-snapshot@1"
            ),
        }
        if self.alias.is_some() {
            bail!("Host has competing business snapshot bindings");
        }
        // Only a generator-assigned dependency alias reaches emitted Rust, never
        // a Plugin ID or an authored function path.
        self.alias = Some(alias.to_owned());
        Ok(())
    }

    pub(super) fn render(&self, source: &str) -> String {
        let Some(alias) = &self.alias else {
            return source
                .replace("// LENSO_BUSINESS_SNAPSHOT_DECL", "")
                .replace(
                    "// LENSO_BUSINESS_SNAPSHOT_BIND",
                    "if business_snapshot_policy.is_some() { bail!(\"this Host has no selected business snapshot binding\"); } registry",
                )
                .replace("// LENSO_BUSINESS_SNAPSHOT_READY", "");
        };
        source
            .replace(
                "// LENSO_BUSINESS_SNAPSHOT_DECL",
                "#[cfg(generated_native_host)] let mut business_snapshot_poller = None;",
            )
            .replace(
                "// LENSO_BUSINESS_SNAPSHOT_BIND",
                &r#"
        if let Some(path) = business_snapshot_policy.as_deref() {
            let (bound, poller) = __PLUGIN_CRATE__::business_snapshot::bind(registry, &resolution.plan, path)?;
            business_snapshot_poller = Some(poller);
            bound
        } else {
            registry
        }
"#
                .replace("__PLUGIN_CRATE__", alias),
            )
            .replace(
                "// LENSO_BUSINESS_SNAPSHOT_READY",
                r#"
            let _business_snapshot_guard = if let Some(poller) = business_snapshot_poller {
                if let Err(error) = poller.recheck().await {
                    let outcome = app.shutdown(Duration::from_secs(10)).await;
                    bail!("business snapshot source failed before readiness: {error}; shutdown: {outcome:?}");
                }
                Some(poller.spawn())
            } else {
                None
            };
"#,
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEMPLATE: &str = include_str!("../local_runtime_template.rs");

    #[test]
    fn requires_explicit_source_opt_in() {
        for package in [
            json!({"metadata":{"lenso":{"plugin-id":"example.policy"}}}),
            json!({"metadata":{"lenso":{"host-bindings":[]}}}),
        ] {
            let mut binding = HostBinding::default();
            binding.select("local_plugin_0", &package).unwrap();
            let generated = binding.render(TEMPLATE);
            assert!(generated.contains("this Host has no selected business snapshot binding"));
            assert!(!generated.contains("business_snapshot::bind"));
            assert!(!generated.contains("// LENSO_BUSINESS_SNAPSHOT_"));
        }
    }

    #[test]
    fn plugin_identity_does_not_select_or_change_binding() {
        let mut sources = Vec::new();
        for id in ["example.policy", "another.renamed-plugin"] {
            let mut binding = HostBinding::default();
            binding
                .select(
                    "local_plugin_7",
                    &json!({"metadata":{"lenso":{
                        "plugin-id":id,
                        "host-bindings":["business-snapshot@1"]
                    }}}),
                )
                .unwrap();
            let source = binding.render(TEMPLATE);
            assert!(source.contains("local_plugin_7::business_snapshot::bind"));
            assert!(source.contains("poller.recheck().await"));
            assert!(source.contains("app.shutdown(Duration::from_secs(10)).await"));
            assert!(source.contains("Some(poller.spawn())"));
            assert!(!source.contains("// LENSO_BUSINESS_SNAPSHOT_"));
            assert!(!source.contains(id));
            sources.push(source);
        }
        assert_eq!(sources[0], sources[1]);
    }

    #[test]
    fn rejects_unknown_malformed_or_repeated_bindings() {
        for value in [
            json!(["attachment-policy@1"]),
            json!(["business-snapshot@2"]),
            json!(["business-snapshot@1", "other@1"]),
            json!(["business-snapshot@1", "business-snapshot@1"]),
            json!("business-snapshot@1"),
            json!(null),
        ] {
            let mut binding = HostBinding::default();
            assert!(
                binding
                    .select(
                        "local_plugin_0",
                        &json!({"metadata":{"lenso":{"host-bindings":value}}})
                    )
                    .is_err(),
                "{value}"
            );
            assert!(binding.alias.is_none());
        }
    }

    #[test]
    fn rejects_competing_source_owners_without_replacing_first() {
        let package = json!({"metadata":{"lenso":{"host-bindings":["business-snapshot@1"]}}});
        let mut binding = HostBinding::default();
        binding.select("local_plugin_0", &package).unwrap();
        let error = binding.select("local_plugin_1", &package).unwrap_err();
        assert!(error.to_string().contains("competing business snapshot"));
        assert_eq!(binding.alias.as_deref(), Some("local_plugin_0"));
    }
}
