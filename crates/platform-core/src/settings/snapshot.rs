use crate::error::{AppError, AppResult, ErrorCode};
use crate::settings::descriptor::SettingsRegistry;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// Where an effective value came from, for display in the console.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingSource {
    /// A stored row scoped to this service.
    Override,
    /// A stored row scoped to `*` (shared).
    Shared,
    /// No stored row; the descriptor default.
    Default,
}

/// Effective configuration for a single running service: every registered key
/// resolved to a concrete value plus the source it came from.
#[derive(Debug, Clone, Default)]
pub struct SettingsSnapshot {
    /// key -> (value, source)
    values: BTreeMap<String, (Value, SettingSource)>,
}

impl SettingsSnapshot {
    /// Resolve every descriptor for `service_key` against the stored rows.
    ///
    /// `stored` maps `(service_key, key)` to the stored JSON value. Resolution
    /// order per key: a row for this service, else a `*` row, else the default.
    /// Stored values failing validation fall back to the default.
    #[must_use]
    pub fn resolve(
        registry: &SettingsRegistry,
        service_key: &str,
        stored: &BTreeMap<(String, String), Value>,
    ) -> Self {
        let mut values = BTreeMap::new();
        for descriptor in registry.iter() {
            // Only descriptors applicable to this service or shared.
            let applies = descriptor.scope.as_service_key() == service_key
                || descriptor.scope.as_service_key() == "*";
            if !applies {
                continue;
            }
            let key = descriptor.key.to_owned();
            let service_row = stored.get(&(service_key.to_owned(), key.clone()));
            let shared_row = stored.get(&("*".to_owned(), key.clone()));

            let (value, source) = match (service_row, shared_row) {
                (Some(v), _) if descriptor.validate(v).is_ok() => {
                    (v.clone(), SettingSource::Override)
                }
                (_, Some(v)) if descriptor.validate(v).is_ok() => {
                    (v.clone(), SettingSource::Shared)
                }
                _ => (descriptor.default.clone(), SettingSource::Default),
            };
            values.insert(key, (value, source));
        }
        Self { values }
    }

    /// The raw effective value for a key, if registered.
    #[must_use]
    pub fn raw(&self, key: &str) -> Option<&Value> {
        self.values.get(key).map(|(value, _)| value)
    }

    /// The source of a key's effective value, if registered.
    #[must_use]
    pub fn source(&self, key: &str) -> Option<SettingSource> {
        self.values.get(key).map(|(_, source)| *source)
    }

    /// Deserialize a single key into a typed value.
    pub fn get_value<T: DeserializeOwned>(&self, key: &str) -> AppResult<T> {
        let value = self.raw(key).ok_or_else(|| {
            AppError::new(ErrorCode::Internal, format!("unknown setting key `{key}`"))
        })?;
        serde_json::from_value(value.clone()).map_err(|source| {
            AppError::new(ErrorCode::Internal, format!("setting `{key}` deserialize failed"))
                .with_source(source)
        })
    }

    /// Build a typed struct whose fields are keys sharing `prefix` + `.`.
    ///
    /// Example: prefix `"identity"` with key `"identity.password_reset_ttl_minutes"`
    /// produces an object field `password_reset_ttl_minutes`.
    pub fn get<T: DeserializeOwned>(&self, prefix: &str) -> AppResult<T> {
        let dotted = format!("{prefix}.");
        let mut object = Map::new();
        for (key, (value, _)) in &self.values {
            if let Some(field) = key.strip_prefix(&dotted) {
                object.insert(field.to_owned(), value.clone());
            }
        }
        serde_json::from_value(Value::Object(object)).map_err(|source| {
            AppError::new(ErrorCode::Internal, format!("settings `{prefix}` deserialize failed"))
                .with_source(source)
        })
    }

    /// All resolved keys with their value and source, for the console values API.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &Value, SettingSource)> {
        self.values.iter().map(|(k, (v, s))| (k.as_str(), v, *s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::descriptor::{SettingDescriptor, SettingScope, SettingType};
    use serde::Deserialize;
    use serde_json::json;

    fn registry() -> SettingsRegistry {
        SettingsRegistry::try_new(vec![
            SettingDescriptor {
                key: "identity.password_reset_ttl_minutes",
                scope: SettingScope::Shared,
                value_type: SettingType::Int { min: Some(1), max: Some(1440) },
                default: json!(30),
                editable: true,
                restart_only: false,
                description: "ttl",
            },
            SettingDescriptor {
                key: "api.feature.enabled",
                scope: SettingScope::Service("api"),
                value_type: SettingType::Bool,
                default: json!(false),
                editable: true,
                restart_only: false,
                description: "flag",
            },
        ])
        .unwrap()
    }

    #[test]
    fn falls_back_to_default() {
        let snapshot = SettingsSnapshot::resolve(&registry(), "api", &BTreeMap::new());
        assert_eq!(snapshot.raw("identity.password_reset_ttl_minutes"), Some(&json!(30)));
        assert_eq!(snapshot.source("api.feature.enabled"), Some(SettingSource::Default));
    }

    #[test]
    fn service_row_overrides_shared_and_default() {
        let mut stored = BTreeMap::new();
        stored.insert(("api".to_owned(), "api.feature.enabled".to_owned()), json!(true));
        let snapshot = SettingsSnapshot::resolve(&registry(), "api", &stored);
        assert_eq!(snapshot.raw("api.feature.enabled"), Some(&json!(true)));
        assert_eq!(snapshot.source("api.feature.enabled"), Some(SettingSource::Override));
    }

    #[test]
    fn invalid_stored_value_falls_back_to_default() {
        let mut stored = BTreeMap::new();
        stored.insert(
            ("*".to_owned(), "identity.password_reset_ttl_minutes".to_owned()),
            json!(99999),
        );
        let snapshot = SettingsSnapshot::resolve(&registry(), "api", &stored);
        assert_eq!(snapshot.raw("identity.password_reset_ttl_minutes"), Some(&json!(30)));
        assert_eq!(
            snapshot.source("identity.password_reset_ttl_minutes"),
            Some(SettingSource::Default)
        );
    }

    #[test]
    fn typed_struct_get_by_prefix() {
        #[derive(Debug, Deserialize, PartialEq)]
        struct IdentityConfig {
            password_reset_ttl_minutes: u64,
        }
        let snapshot = SettingsSnapshot::resolve(&registry(), "api", &BTreeMap::new());
        let cfg: IdentityConfig = snapshot.get("identity").unwrap();
        assert_eq!(cfg, IdentityConfig { password_reset_ttl_minutes: 30 });
    }

    #[test]
    fn service_scoped_key_excluded_for_other_service() {
        let snapshot = SettingsSnapshot::resolve(&registry(), "worker", &BTreeMap::new());
        assert!(snapshot.raw("api.feature.enabled").is_none());
    }
}
