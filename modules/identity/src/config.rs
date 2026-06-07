use platform_core::{RuntimeConfigDescriptor, RuntimeConfigScope, RuntimeConfigType};
use serde::Deserialize;
use serde_json::json;
use std::sync::LazyLock;

/// Identity module configuration, resolved from the settings snapshot under the
/// `identity.` key prefix.
#[derive(Debug, Clone, Deserialize)]
pub struct IdentityConfig {
    pub password_reset_ttl_minutes: u64,
}

impl Default for IdentityConfig {
    fn default() -> Self {
        Self {
            password_reset_ttl_minutes: 30,
        }
    }
}

/// Editable settings owned by the identity module.
pub static RUNTIME_CONFIG: LazyLock<Vec<RuntimeConfigDescriptor>> = LazyLock::new(|| {
    vec![RuntimeConfigDescriptor {
        key: "identity.password_reset_ttl_minutes".to_owned(),
        scope: RuntimeConfigScope::Shared,
        value_type: RuntimeConfigType::Int {
            min: Some(5),
            max: Some(1440),
        },
        default: json!(30),
        editable: true,
        restart_only: false,
        description: "Minutes a password reset token remains valid.",
    }]
});

#[cfg(test)]
mod tests {
    use super::*;
    use platform_core::{RuntimeConfigRegistry, RuntimeConfigSnapshot};
    use std::collections::BTreeMap;

    #[test]
    fn reads_default_ttl_from_snapshot() {
        let registry = RuntimeConfigRegistry::try_new(RUNTIME_CONFIG.clone()).unwrap();
        let snapshot = RuntimeConfigSnapshot::resolve(&registry, "api", &BTreeMap::new());
        let config: IdentityConfig = snapshot.get("identity").unwrap();
        assert_eq!(config.password_reset_ttl_minutes, 30);
    }

    #[test]
    fn default_impl_matches_descriptor_default() {
        let descriptor_default = RUNTIME_CONFIG[0]
            .default
            .as_u64()
            .expect("descriptor default is a number");
        assert_eq!(
            IdentityConfig::default().password_reset_ttl_minutes,
            descriptor_default
        );
    }
}
