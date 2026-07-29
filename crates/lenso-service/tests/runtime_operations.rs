use lenso_service::system_plane::{
    ManagementActor, ManagementActorKind, ManagementApproval, ManagementIntent,
    RUNTIME_OPERATIONS_PROTOCOL, RuntimeOperationAvailabilityImpact,
    RuntimeOperationCompensationSupport, RuntimeOperationDesiredOutcome,
    RuntimeOperationPlanReceipt, RuntimeOperationRisk, RuntimeOperationTarget,
    RuntimeOperationTargetKind, management_intent_digest, runtime_operation_plan_digest,
    runtime_operations_schema, runtime_operations_schema_digest,
};

fn digest(byte: char) -> String {
    format!("sha256:{}", byte.to_string().repeat(64))
}

fn intent() -> ManagementIntent {
    ManagementIntent {
        protocol: RUNTIME_OPERATIONS_PROTOCOL.to_owned(),
        intent_id: "intent:retry-function-run:01".to_owned(),
        service_id: "support".to_owned(),
        service_revision: "release:sha256:0123456789abcdef".to_owned(),
        target: RuntimeOperationTarget {
            kind: RuntimeOperationTargetKind::FunctionRun,
            target_id: "function-run-1".to_owned(),
        },
        desired_outcome: RuntimeOperationDesiredOutcome::Retry,
        expected_target_revision: digest('a'),
        actor: ManagementActor {
            kind: ManagementActorKind::Operator,
            subject: "operator:console-user-1".to_owned(),
            delegated_authority_digest: digest('b'),
        },
        approvals: vec![ManagementApproval {
            approval_id: "approval:01".to_owned(),
            approval_digest: digest('c'),
        }],
        deadline_unix_ms: 20_000,
        idempotency_key: "retry-function-run-1".to_owned(),
        capability_contract_id: RUNTIME_OPERATIONS_PROTOCOL.to_owned(),
        capability_schema_digest: runtime_operations_schema_digest(),
    }
}

#[test]
fn intent_and_plan_digests_bind_exact_management_inputs() {
    let intent = intent();
    let mut changed = intent.clone();
    changed.expected_target_revision = digest('d');
    assert_ne!(
        management_intent_digest(&intent),
        management_intent_digest(&changed)
    );

    let mut plan = RuntimeOperationPlanReceipt {
        protocol: RUNTIME_OPERATIONS_PROTOCOL.to_owned(),
        intent_digest: management_intent_digest(&intent),
        plan_digest: String::new(),
        service_id: intent.service_id.clone(),
        service_revision: intent.service_revision.clone(),
        target: intent.target.clone(),
        expected_target_revision: intent.expected_target_revision.clone(),
        expected_effects: vec![
            "schedule the exact Function Run for one additional runtime attempt".to_owned(),
        ],
        risks: vec![
            RuntimeOperationRisk::DuplicateExternalEffect,
            RuntimeOperationRisk::RepeatedBusinessNotification,
        ],
        availability_impact: RuntimeOperationAvailabilityImpact::None,
        compensation_support: RuntimeOperationCompensationSupport::NotAvailable,
        approval_required: true,
        expires_at_unix_ms: 10_000,
    };
    plan.plan_digest = runtime_operation_plan_digest(&plan);
    let stable = plan.plan_digest.clone();
    assert_eq!(runtime_operation_plan_digest(&plan), stable);
    plan.approval_required = false;
    assert_ne!(runtime_operation_plan_digest(&plan), stable);
}

#[test]
fn runtime_operations_schema_is_strict_and_digest_addressed() {
    let schema = runtime_operations_schema();
    assert!(jsonschema::validator_for(&schema).is_ok());
    assert!(
        runtime_operations_schema_digest()
            .strip_prefix("sha256:")
            .is_some_and(|digest| digest.len() == 64)
    );
    assert_eq!(
        schema["$defs"]["ManagementIntent"]["properties"]["protocol"]["const"],
        RUNTIME_OPERATIONS_PROTOCOL
    );
    assert_eq!(
        schema["$defs"]["ManagementIntent"]["additionalProperties"],
        false
    );
}
