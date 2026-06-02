use platform_core::{SettingDescriptor, SettingScope, SettingType};
use serde::Deserialize;
use serde_json::json;
use std::sync::LazyLock;

/// Identity domain configuration, resolved from the settings snapshot under the
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

/// Editable settings owned by the identity domain.
pub static SETTINGS: LazyLock<Vec<SettingDescriptor>> = LazyLock::new(|| {
    vec![SettingDescriptor {
        key: "identity.password_reset_ttl_minutes",
        scope: SettingScope::Shared,
        value_type: SettingType::Int {
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
    use platform_core::{SettingsRegistry, SettingsSnapshot};
    use std::collections::BTreeMap;

    #[test]
    fn reads_default_ttl_from_snapshot() {
        let registry = SettingsRegistry::try_new(SETTINGS.clone()).unwrap();
        let snapshot = SettingsSnapshot::resolve(&registry, "api", &BTreeMap::new());
        let config: IdentityConfig = snapshot.get("identity").unwrap();
        assert_eq!(config.password_reset_ttl_minutes, 30);
    }

    #[test]
    fn default_impl_matches_descriptor_default() {
        let descriptor_default = SETTINGS[0]
            .default
            .as_u64()
            .expect("descriptor default is a number");
        assert_eq!(
            IdentityConfig::default().password_reset_ttl_minutes,
            descriptor_default
        );
    }
}
