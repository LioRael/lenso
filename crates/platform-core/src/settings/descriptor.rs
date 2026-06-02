use crate::error::{AppError, AppResult, ErrorCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Which running service a setting applies to. `Shared` is stored under the
/// reserved service key `*` and used as a fallback for every service.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum SettingScope {
    Shared,
    Service(&'static str),
}

impl SettingScope {
    /// The string stored in the `service` column: `*` for shared.
    #[must_use]
    pub fn as_service_key(&self) -> &str {
        match self {
            Self::Shared => "*",
            Self::Service(name) => name,
        }
    }
}

/// The type and constraints of a setting value. Drives write validation and the
/// console edit control.
///
/// Serialized to JSON via the explicit [`SettingType::to_json`] rather than a
/// derive: an internally-tagged serde enum cannot represent the tuple variant
/// `Enum(&[&str])` (sequences can't carry a tag), so the shape is built by hand.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingType {
    Bool,
    Int { min: Option<i64>, max: Option<i64> },
    Float { min: Option<f64>, max: Option<f64> },
    String,
    Enum(&'static [&'static str]),
    Json,
}

impl SettingType {
    /// A stable JSON description for the console: `{ "kind": "...", ... }`.
    #[must_use]
    pub fn to_json(&self) -> Value {
        match self {
            Self::Bool => serde_json::json!({ "kind": "bool" }),
            Self::Int { min, max } => serde_json::json!({ "kind": "int", "min": min, "max": max }),
            Self::Float { min, max } => {
                serde_json::json!({ "kind": "float", "min": min, "max": max })
            }
            Self::String => serde_json::json!({ "kind": "string" }),
            Self::Enum(allowed) => serde_json::json!({ "kind": "enum", "values": allowed }),
            Self::Json => serde_json::json!({ "kind": "json" }),
        }
    }
}

/// A single declared, typed, editable configuration key.
#[derive(Debug, Clone)]
pub struct SettingDescriptor {
    pub key: &'static str,
    pub scope: SettingScope,
    pub value_type: SettingType,
    pub default: Value,
    pub editable: bool,
    pub restart_only: bool,
    pub description: &'static str,
}

impl SettingDescriptor {
    /// Validate a candidate value against this descriptor's type and constraints.
    pub fn validate(&self, value: &Value) -> AppResult<()> {
        let ok = match &self.value_type {
            SettingType::Bool => value.is_boolean(),
            SettingType::Int { min, max } => value
                .as_i64()
                .is_some_and(|n| min.is_none_or(|lo| n >= lo) && max.is_none_or(|hi| n <= hi)),
            SettingType::Float { min, max } => value
                .as_f64()
                .is_some_and(|n| min.is_none_or(|lo| n >= lo) && max.is_none_or(|hi| n <= hi)),
            SettingType::String => value.is_string(),
            SettingType::Enum(allowed) => value.as_str().is_some_and(|s| allowed.contains(&s)),
            SettingType::Json => true,
        };
        if ok {
            Ok(())
        } else {
            Err(AppError::new(
                ErrorCode::Validation,
                format!("value for `{}` failed validation", self.key),
            ))
        }
    }
}

/// An immutable, validated set of descriptors indexed by `(service_key, key)`.
#[derive(Debug, Clone, Default)]
pub struct SettingsRegistry {
    by_scope_key: BTreeMap<(String, String), SettingDescriptor>,
}

impl SettingsRegistry {
    /// Build a registry, rejecting duplicate `(scope, key)` pairs.
    pub fn try_new(descriptors: Vec<SettingDescriptor>) -> AppResult<Self> {
        let mut by_scope_key = BTreeMap::new();
        for descriptor in descriptors {
            let index = (
                descriptor.scope.as_service_key().to_owned(),
                descriptor.key.to_owned(),
            );
            if by_scope_key.contains_key(&index) {
                return Err(AppError::new(
                    ErrorCode::Internal,
                    format!("duplicate setting descriptor for {index:?}"),
                ));
            }
            by_scope_key.insert(index, descriptor);
        }
        Ok(Self { by_scope_key })
    }

    /// Look up a descriptor by exact scope and key.
    #[must_use]
    pub fn get(&self, scope: &SettingScope, key: &str) -> Option<&SettingDescriptor> {
        self.by_scope_key
            .get(&(scope.as_service_key().to_owned(), key.to_owned()))
    }

    /// Look up a descriptor by raw service-key string and key.
    #[must_use]
    pub fn get_raw(&self, service_key: &str, key: &str) -> Option<&SettingDescriptor> {
        self.by_scope_key.get(&(service_key.to_owned(), key.to_owned()))
    }

    /// All descriptors, ordered by `(service_key, key)`.
    pub fn iter(&self) -> impl Iterator<Item = &SettingDescriptor> {
        self.by_scope_key.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn bool_descriptor() -> SettingDescriptor {
        SettingDescriptor {
            key: "demo.enabled",
            scope: SettingScope::Shared,
            value_type: SettingType::Bool,
            default: json!(true),
            editable: true,
            restart_only: false,
            description: "demo flag",
        }
    }

    #[test]
    fn validates_bool() {
        let d = bool_descriptor();
        assert!(d.validate(&json!(false)).is_ok());
        assert!(d.validate(&json!("nope")).is_err());
    }

    #[test]
    fn validates_int_range() {
        let d = SettingDescriptor {
            key: "demo.count",
            scope: SettingScope::Service("api"),
            value_type: SettingType::Int { min: Some(1), max: Some(10) },
            default: json!(5),
            editable: true,
            restart_only: false,
            description: "count",
        };
        assert!(d.validate(&json!(5)).is_ok());
        assert!(d.validate(&json!(0)).is_err());
        assert!(d.validate(&json!(11)).is_err());
        assert!(d.validate(&json!("x")).is_err());
    }

    #[test]
    fn validates_enum() {
        let d = SettingDescriptor {
            key: "demo.mode",
            scope: SettingScope::Shared,
            value_type: SettingType::Enum(&["a", "b"]),
            default: json!("a"),
            editable: true,
            restart_only: false,
            description: "mode",
        };
        assert!(d.validate(&json!("a")).is_ok());
        assert!(d.validate(&json!("c")).is_err());
    }

    #[test]
    fn registry_rejects_duplicate_scope_key() {
        let result = SettingsRegistry::try_new(vec![bool_descriptor(), bool_descriptor()]);
        assert!(result.is_err());
    }

    #[test]
    fn registry_looks_up_by_scope_and_key() {
        let registry = SettingsRegistry::try_new(vec![bool_descriptor()]).unwrap();
        assert!(registry.get(&SettingScope::Shared, "demo.enabled").is_some());
        assert!(registry.get(&SettingScope::Service("api"), "demo.enabled").is_none());
    }
}
