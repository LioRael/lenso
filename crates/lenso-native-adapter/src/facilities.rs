//! Immutable Host attachments supplied to one selected Plugin instance.
use std::{
    any::{Any, TypeId},
    collections::BTreeMap,
    fmt,
    rc::Rc,
};

use lenso_app_plan::ResolvedAppPlan;
use lenso_kernel::RuntimeFailure;

#[derive(Clone, Default)]
pub struct NativeFacilities {
    values: BTreeMap<String, FacilityValue>,
}

#[derive(Clone)]
enum FacilityValue {
    Value(Rc<dyn Any>),
    Factory {
        type_id: TypeId,
        create: Rc<dyn Fn() -> Result<Rc<dyn Any>, RuntimeFailure>>,
    },
}

impl fmt::Debug for NativeFacilities {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeFacilities")
            .field("names", &self.values.keys())
            .finish()
    }
}

impl NativeFacilities {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with<T: Any>(
        mut self,
        name: impl Into<String>,
        value: T,
    ) -> Result<Self, RuntimeFailure> {
        let name = name.into();
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err(invalid("invalid Host facility name"));
        }
        if self
            .values
            .insert(name.clone(), FacilityValue::Value(Rc::new(value)))
            .is_some()
        {
            return Err(invalid(format!("duplicate Host facility `{name}`")));
        }
        Ok(self)
    }

    /// Registers an owner factory called when a generation constructs its typed input.
    pub fn with_factory<T: Any>(
        mut self,
        name: impl Into<String>,
        create: impl Fn() -> Result<T, RuntimeFailure> + 'static,
    ) -> Result<Self, RuntimeFailure> {
        let name = name.into();
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err(invalid("invalid Host facility name"));
        }
        if self.values.contains_key(&name) {
            return Err(invalid(format!("duplicate Host facility `{name}`")));
        }
        self.values.insert(
            name,
            FacilityValue::Factory {
                type_id: TypeId::of::<T>(),
                create: Rc::new(move || create().map(|value| Rc::new(value) as Rc<dyn Any>)),
            },
        );
        Ok(self)
    }

    pub fn require<T: Any + Clone>(&self, name: &str) -> Result<T, RuntimeFailure> {
        let missing = || invalid(format!("missing or incompatible Host facility `{name}`"));
        let value = match self.values.get(name).ok_or_else(missing)? {
            FacilityValue::Value(value) => value.clone(),
            FacilityValue::Factory { type_id, create } if *type_id == TypeId::of::<T>() => {
                create()?
            }
            FacilityValue::Factory { .. } => return Err(missing()),
        };
        value.downcast_ref::<T>().cloned().ok_or_else(missing)
    }

    /// Returns explicit absence when no attachment exists; present inputs stay type checked.
    pub fn optional<T: Any + Clone>(&self, name: &str) -> Result<Option<T>, RuntimeFailure> {
        if self.values.contains_key(name) {
            self.require(name).map(Some)
        } else {
            Ok(None)
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct NativeInstanceFacilities {
    values: BTreeMap<String, NativeFacilities>,
    empty: NativeFacilities,
}

impl NativeInstanceFacilities {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(
        mut self,
        instance: impl Into<String>,
        facilities: NativeFacilities,
    ) -> Result<Self, RuntimeFailure> {
        let instance = instance.into();
        if instance.is_empty() || self.values.insert(instance.clone(), facilities).is_some() {
            return Err(invalid(format!(
                "invalid or duplicate facilities for Instance `{instance}`"
            )));
        }
        Ok(self)
    }

    pub(crate) fn for_instance(&self, instance: &str) -> &NativeFacilities {
        self.values.get(instance).unwrap_or(&self.empty)
    }

    pub(crate) fn validate(&self, plan: &ResolvedAppPlan) -> Result<(), RuntimeFailure> {
        for instance in self.values.keys() {
            if !plan
                .plugin_instances()
                .iter()
                .any(|selected| selected.instance_key() == instance)
            {
                return Err(invalid(format!(
                    "Host facilities name an unselected Instance `{instance}`"
                )));
            }
        }
        Ok(())
    }
}

fn invalid(detail: impl Into<String>) -> RuntimeFailure {
    RuntimeFailure::InvalidResolvedPlan {
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachments_are_typed_and_isolated_without_exposing_values() {
        let first = NativeFacilities::new()
            .with("state", "private-primary".to_owned())
            .unwrap();
        let second = NativeFacilities::new().with("state", 7_u64).unwrap();
        let catalog = NativeInstanceFacilities::new()
            .with("store/primary", first)
            .unwrap()
            .with("store/secondary", second)
            .unwrap();
        assert_eq!(
            catalog
                .for_instance("store/primary")
                .require::<String>("state")
                .unwrap(),
            "private-primary"
        );
        assert_eq!(
            catalog
                .for_instance("store/secondary")
                .require::<u64>("state")
                .unwrap(),
            7
        );
        assert!(
            catalog
                .for_instance("store/primary")
                .require::<u64>("state")
                .is_err()
        );
        assert!(
            catalog
                .for_instance("store/unselected")
                .require::<String>("state")
                .is_err()
        );
        assert!(!format!("{catalog:?}").contains("private-primary"));
    }

    #[test]
    fn duplicate_and_unselected_attachments_fail_closed() {
        assert!(
            NativeFacilities::new()
                .with("state", 1_u64)
                .unwrap()
                .with("state", 2_u64)
                .is_err()
        );
        let catalog = NativeInstanceFacilities::new()
            .with("store/primary", NativeFacilities::new())
            .unwrap();
        let empty = lenso_app_plan::AppComposition::new(vec![], vec![])
            .resolve()
            .unwrap();
        assert!(catalog.validate(&empty).is_err());
    }

    #[test]
    fn owner_factory_creates_distinct_generation_inputs_and_does_not_run_for_wrong_types() {
        let created = Rc::new(std::cell::Cell::new(0_u64));
        let count = created.clone();
        let facilities = NativeFacilities::new()
            .with_factory("state", move || {
                count.set(count.get() + 1);
                Ok(count.get())
            })
            .unwrap();
        assert!(facilities.require::<String>("state").is_err());
        assert_eq!(created.get(), 0);
        assert_eq!(facilities.require::<u64>("state").unwrap(), 1);
        assert_eq!(facilities.require::<u64>("state").unwrap(), 2);
    }
}
