//! Snapshot-local, opaque evidence of complete Plan validation.
//!
//! Only immutable topology is retained. There is no public unchecked constructor,
//! wire authority, or Adapter opt-in. Existing validation calls all use this memo.

use super::{PLAN_SCHEMA_VERSION, PlanResolutionError, ResolvedAppPlan, resolution};

#[derive(Clone, Debug)]
pub(super) struct CheckedTopology {
    pub activation_order: Vec<String>,
}

impl ResolvedAppPlan {
    pub(super) fn checked_topology(&self) -> Result<&CheckedTopology, PlanResolutionError> {
        self.checked
            .get_or_init(|| {
                if self.schema_version != PLAN_SCHEMA_VERSION {
                    return Err(PlanResolutionError::UnsupportedSchemaVersion {
                        expected: PLAN_SCHEMA_VERSION,
                        actual: self.schema_version,
                    });
                }
                resolution::validate_execution_lanes(
                    &self.execution_lanes,
                    &self.plugin_instances,
                )?;
                let parts =
                    resolution::resolve_parts(&self.plugin_instances, &self.capability_bindings)?;
                // Keep the original error precedence: topology before policy.
                self.terminal_policy
                    .validate(&parts.instances, &parts.bindings)?;
                Ok(CheckedTopology {
                    activation_order: parts.activation_order,
                })
            })
            .as_ref()
            .map_err(Clone::clone)
    }
}

// Memo population cannot affect observable Plan identity or diagnostic output.
impl PartialEq for ResolvedAppPlan {
    fn eq(&self, other: &Self) -> bool {
        self.terminal_policy == other.terminal_policy
            && self.schema_version == other.schema_version
            && self.plugin_instances == other.plugin_instances
            && self.capability_bindings == other.capability_bindings
            && self.execution_lanes == other.execution_lanes
    }
}

impl Eq for ResolvedAppPlan {}

impl std::fmt::Debug for ResolvedAppPlan {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ResolvedAppPlan")
            .field("terminal_policy", &self.terminal_policy)
            .field("schema_version", &self.schema_version)
            .field("plugin_instances", &self.plugin_instances)
            .field("capability_bindings", &self.capability_bindings)
            .field("execution_lanes", &self.execution_lanes)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AppComposition, CapabilityBinding, CapabilityEndpointPlan, CapabilityRequirementPlan,
        PluginInstancePlan, TerminalPolicy,
    };

    fn chain(size: usize) -> ResolvedAppPlan {
        let instances = (0..size)
            .map(|index| {
                let instance =
                    PluginInstancePlan::new(format!("{index:08}"), "test.plugin").with_capability(
                        CapabilityEndpointPlan::new("test.chain@1", "1.0.0", ["call"]),
                    );
                if index == 0 {
                    instance
                } else {
                    instance
                        .with_requirement(CapabilityRequirementPlan::one("test.chain@1", "1.0.0"))
                }
            })
            .collect();
        let bindings = (1..size)
            .map(|index| {
                CapabilityBinding::new(
                    format!("{index:08}"),
                    "test.chain@1",
                    "1.0.0",
                    format!("{:08}", index - 1),
                )
            })
            .collect();
        ResolvedAppPlan::new(instances, bindings)
    }

    #[test]
    fn startup_and_adapter_call_sequence_runs_one_resolution_and_topology_pass() {
        for size in [10, 100, 500] {
            let plan = chain(size);
            resolution::PASS_COUNTS.with(|counts| counts.set((0, 0)));
            let order = plan.activation_order().unwrap();
            // Multiple external Adapters may independently validate and ask for order.
            for _ in 0..3 {
                plan.validate().unwrap();
                assert_eq!(plan.activation_order().unwrap(), order);
            }
            assert_eq!(
                order,
                (0..size)
                    .map(|index| format!("{index:08}"))
                    .collect::<Vec<_>>()
            );
            resolution::PASS_COUNTS.with(|counts| assert_eq!(counts.get(), (1, 1)));
        }
    }

    #[test]
    fn resolution_retains_its_checked_order() {
        let input = chain(10);
        resolution::PASS_COUNTS.with(|counts| counts.set((0, 0)));
        let plan = AppComposition::new(input.plugin_instances, input.capability_bindings)
            .resolve()
            .unwrap();
        plan.validate().unwrap();
        assert_eq!(plan.activation_order().unwrap().len(), 10);
        resolution::PASS_COUNTS.with(|counts| assert_eq!(counts.get(), (1, 1)));
    }

    #[test]
    fn memo_is_neither_wire_authority_nor_plan_identity() {
        let plan = chain(3);
        let original = plan.clone();
        let bytes = serde_json::to_vec(&plan).unwrap();
        let debug = format!("{plan:?}");
        plan.validate().unwrap();
        assert_eq!(plan, original);
        assert_eq!(serde_json::to_vec(&plan).unwrap(), bytes);
        assert_eq!(format!("{plan:?}"), debug);
        assert!(plan.clone().checked.get().is_some());

        let decoded: ResolvedAppPlan = serde_json::from_slice(&bytes).unwrap();
        assert!(decoded.checked.get().is_none());
        decoded.validate().unwrap();
        assert_eq!(decoded, plan);

        let mut wire: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        wire["capability_bindings"][0]["provider_instance"] = "absent".into();
        let invalid: ResolvedAppPlan = serde_json::from_value(wire.clone()).unwrap();
        let expected = PlanResolutionError::InvalidProviderReference {
            consumer_instance: "00000001".into(),
            capability_id: "test.chain@1".into(),
            provider_instance: "absent".into(),
        };
        assert_eq!(invalid.validate(), Err(expected.clone()));
        assert_eq!(invalid.activation_order(), Err(expected));
        wire["checked"] = serde_json::json!({"activation_order": ["00000000"]});
        assert!(serde_json::from_value::<ResolvedAppPlan>(wire).is_err());
    }

    #[test]
    fn changed_policy_cannot_reuse_success_or_failure_from_another_snapshot() {
        let plan = chain(3);
        plan.validate().unwrap();
        let changed = plan
            .clone()
            .with_terminal_policy(TerminalPolicy::HostEssential {
                roots: vec!["absent".into()],
                closure: vec![],
            });
        let expected = PlanResolutionError::InvalidTerminalPolicy {
            detail: "unknown Plugin Instance `absent`".into(),
        };
        assert_eq!(changed.validate(), Err(expected.clone()));
        assert_eq!(changed.activation_order(), Err(expected));
        assert_eq!(plan.validate(), Ok(()));
        assert_eq!(
            changed
                .with_terminal_policy(TerminalPolicy::RequiredPath)
                .validate(),
            Ok(())
        );
    }

    #[test]
    fn cycle_error_precedes_invalid_terminal_policy_on_every_entry_point() {
        let plan = ResolvedAppPlan::new(
            vec![
                PluginInstancePlan::new("cycle", "test.plugin")
                    .with_capability(CapabilityEndpointPlan::new(
                        "test.chain@1",
                        "1.0.0",
                        ["call"],
                    ))
                    .with_requirement(CapabilityRequirementPlan::one("test.chain@1", "1.0.0")),
            ],
            vec![CapabilityBinding::new(
                "cycle",
                "test.chain@1",
                "1.0.0",
                "cycle",
            )],
        )
        .with_terminal_policy(TerminalPolicy::HostEssential {
            roots: vec!["absent".into()],
            closure: vec![],
        });
        let expected = PlanResolutionError::ActivationCycle {
            instances: vec!["cycle".into()],
        };
        assert_eq!(plan.validate(), Err(expected.clone()));
        assert_eq!(plan.activation_order(), Err(expected));
    }
}
