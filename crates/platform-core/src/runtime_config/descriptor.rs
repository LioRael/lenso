use crate::error::{AppError, AppResult, ErrorCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Which running service a config value applies to. `Shared` is stored under the
/// reserved service key `*` and used as a fallback for every service.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum RuntimeConfigScope {
    Shared,
    Service(&'static str),
}

impl RuntimeConfigScope {
    /// The string stored in the `service` column: `*` for shared.
    #[must_use]
    pub fn as_service_key(&self) -> &str {
        match self {
            Self::Shared => "*",
            Self::Service(name) => name,
        }
    }
}

/// The type and constraints of a config value. Drives write validation and the
/// console edit control.
///
/// Serialized to JSON via the explicit [`RuntimeConfigType::to_json`] rather than a
/// derive: an internally-tagged serde enum cannot represent the tuple variant
/// `Enum(&[&str])` (sequences can't carry a tag), so the shape is built by hand.
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeConfigType {
    Bool,
    Int { min: Option<i64>, max: Option<i64> },
    Float { min: Option<f64>, max: Option<f64> },
    String,
    Enum(&'static [&'static str]),
    Json,
}

impl RuntimeConfigType {
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
pub struct RuntimeConfigDescriptor {
    pub key: String,
    pub scope: RuntimeConfigScope,
    pub group: Option<&'static str>,
    pub section: Option<&'static str>,
    pub order: i32,
    pub visible_when: Option<RuntimeConfigVisibilityCondition>,
    pub generated: Option<RuntimeConfigGeneratedValue>,
    pub value_type: RuntimeConfigType,
    pub default: Value,
    pub editable: bool,
    pub restart_only: bool,
    pub description: &'static str,
}

impl RuntimeConfigDescriptor {
    /// Validate a candidate value against this descriptor's type and constraints.
    pub fn validate(&self, value: &Value) -> AppResult<()> {
        let ok = match &self.value_type {
            RuntimeConfigType::Bool => value.is_boolean(),
            RuntimeConfigType::Int { min, max } => value
                .as_i64()
                .is_some_and(|n| min.is_none_or(|lo| n >= lo) && max.is_none_or(|hi| n <= hi)),
            RuntimeConfigType::Float { min, max } => value
                .as_f64()
                .is_some_and(|n| min.is_none_or(|lo| n >= lo) && max.is_none_or(|hi| n <= hi)),
            RuntimeConfigType::String => value.is_string(),
            RuntimeConfigType::Enum(allowed) => {
                value.as_str().is_some_and(|s| allowed.contains(&s))
            }
            RuntimeConfigType::Json => true,
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

/// A declarative visibility condition for console presentation.
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeConfigVisibilityCondition {
    Equals {
        service: &'static str,
        key: &'static str,
        value: Value,
    },
}

/// A value that the config backend may initialize when its condition becomes true.
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeConfigGeneratedValue {
    Secret {
        bytes: usize,
        when: RuntimeConfigVisibilityCondition,
    },
}

/// Presentation metadata for a set of related config keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfigGroupDescriptor {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub order: i32,
}

/// An immutable, validated set of descriptors indexed by `(service_key, key)`.
#[derive(Debug, Clone, Default)]
pub struct RuntimeConfigRegistry {
    by_scope_key: BTreeMap<(String, String), RuntimeConfigDescriptor>,
    groups: BTreeMap<String, RuntimeConfigGroupDescriptor>,
}

impl RuntimeConfigRegistry {
    /// Build a registry, rejecting duplicate `(scope, key)` pairs.
    pub fn try_new(descriptors: Vec<RuntimeConfigDescriptor>) -> AppResult<Self> {
        Self::try_new_with_groups(descriptors, Vec::new())
    }

    /// Build a registry with presentation groups, rejecting duplicate descriptors
    /// and conflicting group ids.
    pub fn try_new_with_groups(
        descriptors: Vec<RuntimeConfigDescriptor>,
        groups: Vec<RuntimeConfigGroupDescriptor>,
    ) -> AppResult<Self> {
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

        let mut by_id = BTreeMap::new();
        for group in groups {
            if let Some(existing) = by_id.get(group.id) {
                if existing != &group {
                    return Err(AppError::new(
                        ErrorCode::Internal,
                        format!("conflicting setting group descriptor for `{}`", group.id),
                    ));
                }
                continue;
            }
            by_id.insert(group.id.to_owned(), group);
        }

        Ok(Self {
            by_scope_key,
            groups: by_id,
        })
    }

    /// Look up a descriptor by exact scope and key.
    #[must_use]
    pub fn get(&self, scope: &RuntimeConfigScope, key: &str) -> Option<&RuntimeConfigDescriptor> {
        self.by_scope_key
            .get(&(scope.as_service_key().to_owned(), key.to_owned()))
    }

    /// Look up a descriptor by raw service-key string and key.
    #[must_use]
    pub fn get_raw(&self, service_key: &str, key: &str) -> Option<&RuntimeConfigDescriptor> {
        self.by_scope_key
            .get(&(service_key.to_owned(), key.to_owned()))
    }

    /// All descriptors, ordered by `(service_key, key)`.
    pub fn iter(&self) -> impl Iterator<Item = &RuntimeConfigDescriptor> {
        self.by_scope_key.values()
    }

    /// All presentation groups, ordered by id.
    pub fn groups(&self) -> impl Iterator<Item = &RuntimeConfigGroupDescriptor> {
        self.groups.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn bool_descriptor() -> RuntimeConfigDescriptor {
        RuntimeConfigDescriptor {
            key: "demo.enabled".to_owned(),
            scope: RuntimeConfigScope::Shared,
            group: None,
            section: None,
            order: 0,
            visible_when: None,
            generated: None,
            value_type: RuntimeConfigType::Bool,
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
        let d = RuntimeConfigDescriptor {
            key: "demo.count".to_owned(),
            scope: RuntimeConfigScope::Service("api"),
            group: None,
            section: None,
            order: 0,
            visible_when: None,
            generated: None,
            value_type: RuntimeConfigType::Int {
                min: Some(1),
                max: Some(10),
            },
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
        let d = RuntimeConfigDescriptor {
            key: "demo.mode".to_owned(),
            scope: RuntimeConfigScope::Shared,
            group: None,
            section: None,
            order: 0,
            visible_when: None,
            generated: None,
            value_type: RuntimeConfigType::Enum(&["a", "b"]),
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
        let result = RuntimeConfigRegistry::try_new(vec![bool_descriptor(), bool_descriptor()]);
        assert!(result.is_err());
    }

    #[test]
    fn registry_looks_up_by_scope_and_key() {
        let registry = RuntimeConfigRegistry::try_new(vec![bool_descriptor()]).unwrap();
        assert!(
            registry
                .get(&RuntimeConfigScope::Shared, "demo.enabled")
                .is_some()
        );
        assert!(
            registry
                .get(&RuntimeConfigScope::Service("api"), "demo.enabled")
                .is_none()
        );
    }

    #[test]
    fn registry_deduplicates_matching_groups() {
        let group = RuntimeConfigGroupDescriptor {
            id: "demo",
            label: "Demo",
            description: "demo settings",
            order: 10,
        };

        let registry = RuntimeConfigRegistry::try_new_with_groups(
            vec![bool_descriptor()],
            vec![group.clone(), group],
        )
        .unwrap();

        assert_eq!(registry.groups().count(), 1);
    }
}
