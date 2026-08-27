use lenso_app_plan::{
    AppComposition, CapabilityBinding, CapabilityEndpointPlan, CapabilityRequirementPlan,
    ExecutionLaneId, ExecutionLanePlan, PlanResolutionError, PluginInstancePlan,
};

#[test]
fn resolved_plan_preserves_declared_execution_lanes_and_instance_placement() {
    let plan = AppComposition::new(
        vec![
            PluginInstancePlan::new("provider", "package.provider")
                .with_execution_lane(ExecutionLaneId::new("lane-b")),
            PluginInstancePlan::new("consumer", "package.consumer")
                .with_execution_lane(ExecutionLaneId::new("lane-a")),
        ],
        vec![],
    )
    .with_execution_lanes(vec![
        ExecutionLanePlan::new("lane-b"),
        ExecutionLanePlan::new("lane-a"),
    ])
    .resolve()
    .expect("declared lane placement should resolve");

    assert_eq!(
        plan.execution_lanes()
            .iter()
            .map(|lane| lane.id().as_str())
            .collect::<Vec<_>>(),
        vec!["lane-a", "lane-b"]
    );
    assert_eq!(
        plan.plugin_instance("consumer")
            .expect("consumer should exist")
            .execution_lane()
            .as_str(),
        "lane-a"
    );
    assert_eq!(
        plan.plugin_instance("provider")
            .expect("provider should exist")
            .execution_lane()
            .as_str(),
        "lane-b"
    );
}

#[test]
fn placement_rejects_an_instance_on_an_undeclared_lane() {
    let error = AppComposition::new(
        vec![
            PluginInstancePlan::new("worker", "package.worker")
                .with_execution_lane(ExecutionLaneId::new("missing")),
        ],
        vec![],
    )
    .resolve()
    .expect_err("unknown placement must fail before boot");

    assert_eq!(
        error,
        PlanResolutionError::UndeclaredExecutionLane {
            instance_key: "worker".to_owned(),
            execution_lane: "missing".to_owned(),
        }
    );
}

#[test]
fn placement_rejects_duplicate_execution_lane_ids() {
    let error = AppComposition::new(vec![], vec![])
        .with_execution_lanes(vec![
            ExecutionLanePlan::new("workers"),
            ExecutionLanePlan::new("workers"),
        ])
        .resolve()
        .expect_err("lane identities must be unique");

    assert_eq!(
        error,
        PlanResolutionError::DuplicateExecutionLane {
            execution_lane: "workers".to_owned(),
        }
    );
}

#[test]
fn placement_rejects_an_explicitly_empty_lane_set() {
    let error = AppComposition::new(vec![], vec![])
        .with_execution_lanes(vec![])
        .resolve()
        .expect_err("a native App needs at least one Kernel lane");

    assert_eq!(error, PlanResolutionError::MissingExecutionLane);
}

#[test]
fn placement_rejects_an_empty_execution_lane_id() {
    let error = AppComposition::new(vec![], vec![])
        .with_execution_lanes(vec![ExecutionLanePlan::new(" ")])
        .resolve()
        .expect_err("lane identities must be auditable non-empty values");

    assert_eq!(
        error,
        PlanResolutionError::InvalidExecutionLane {
            execution_lane: " ".to_owned(),
        }
    );
}

#[test]
fn placement_rejects_cross_lane_binding_without_contract_transfer_support() {
    let error = AppComposition::new(
        vec![
            PluginInstancePlan::new("consumer", "package.consumer")
                .with_execution_lane(ExecutionLaneId::new("lane-a"))
                .with_requirement(CapabilityRequirementPlan::one(
                    "example.greeting@1",
                    "1.0.0",
                )),
            PluginInstancePlan::new("provider", "package.provider")
                .with_execution_lane(ExecutionLaneId::new("lane-b"))
                .with_capability(CapabilityEndpointPlan::new(
                    "example.greeting@1",
                    "1.0.0",
                    ["greet"],
                )),
        ],
        vec![CapabilityBinding::new(
            "consumer",
            "example.greeting@1",
            "1.0.0",
            "provider",
        )],
    )
    .with_execution_lanes(vec![
        ExecutionLanePlan::new("lane-a"),
        ExecutionLanePlan::new("lane-b"),
    ])
    .resolve()
    .expect_err("cross-lane binding must require transfer-capable contract types");

    assert_eq!(
        error,
        PlanResolutionError::CrossLaneTransferUnsupported {
            consumer_instance: "consumer".to_owned(),
            provider_instance: "provider".to_owned(),
            capability_id: "example.greeting@1".to_owned(),
        }
    );
}

#[test]
fn placement_accepts_cross_lane_binding_with_contract_transfer_support() {
    let plan = AppComposition::new(
        vec![
            PluginInstancePlan::new("consumer", "package.consumer")
                .with_execution_lane(ExecutionLaneId::new("lane-a"))
                .with_requirement(CapabilityRequirementPlan::one(
                    "example.greeting@1",
                    "1.0.0",
                )),
            PluginInstancePlan::new("provider", "package.provider")
                .with_execution_lane(ExecutionLaneId::new("lane-b"))
                .with_capability(
                    CapabilityEndpointPlan::new("example.greeting@1", "1.0.0", ["greet"])
                        .with_cross_lane_transfer(),
                ),
        ],
        vec![CapabilityBinding::new(
            "consumer",
            "example.greeting@1",
            "1.0.0",
            "provider",
        )],
    )
    .with_execution_lanes(vec![
        ExecutionLanePlan::new("lane-a"),
        ExecutionLanePlan::new("lane-b"),
    ])
    .resolve()
    .expect("transfer-capable contract should permit cross-lane placement");

    assert_eq!(plan.capability_bindings().len(), 1);
}

#[test]
fn placement_accepts_transfer_capable_stream_and_event_interactions() {
    let composition = AppComposition::new(
        vec![
            PluginInstancePlan::new("consumer", "package.consumer")
                .with_execution_lane(ExecutionLaneId::new("lane-a"))
                .with_requirement(CapabilityRequirementPlan::one(
                    "example.greeting@1",
                    "1.0.0",
                )),
            PluginInstancePlan::new("provider", "package.provider")
                .with_execution_lane(ExecutionLaneId::new("lane-b"))
                .with_capability(
                    CapabilityEndpointPlan::new("example.greeting@1", "1.0.0", ["watch", "notify"])
                        .with_stream_operation("watch")
                        .with_event_operation("notify")
                        .with_event_capacity(1)
                        .with_cross_lane_transfer(),
                ),
        ],
        vec![CapabilityBinding::new(
            "consumer",
            "example.greeting@1",
            "1.0.0",
            "provider",
        )],
    )
    .with_execution_lanes(vec![
        ExecutionLanePlan::new("lane-a"),
        ExecutionLanePlan::new("lane-b"),
    ]);

    let plan = composition
        .resolve()
        .expect("transfer-capable Stream and Event Operations should permit cross-lane placement");
    assert_eq!(plan.capability_bindings().len(), 1);
}
