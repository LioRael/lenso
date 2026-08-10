use lenso_service::workload_control::{
    OperationRecord, WORKLOAD_CONTROL_OBSERVE_PATH, WORKLOAD_CONTROL_OPERATION_PATH,
    WORKLOAD_CONTROL_OPERATIONS_PATH, WORKLOAD_CONTROL_PROTOCOL, WorkloadControlAction,
    WorkloadControlActor, WorkloadControlActorKind, WorkloadControlAuthority,
    WorkloadControlAuthorityDecision, WorkloadControlCapability, WorkloadControlError,
    WorkloadControlErrorCode, WorkloadControlFailure, WorkloadControlMessage,
    WorkloadControlValidationIssueCode, WorkloadMutationRequest, WorkloadObservation,
    WorkloadObservationRequest, WorkloadOperationPhase, WorkloadOperationResult,
    WorkloadOperationalState, WorkloadProtection, WorkloadReference,
    validate_workload_control_message, workload_control_schema, workload_control_schema_digest,
};
use std::collections::BTreeSet;
use std::num::NonZeroU32;

fn workload() -> WorkloadReference {
    WorkloadReference {
        system_id: "support-desk-system".to_owned(),
        service_id: "support-desk".to_owned(),
        workload_id: "api".to_owned(),
    }
}

#[test]
fn observation_contract_is_versioned_strict_and_digest_addressed() {
    assert_eq!(WORKLOAD_CONTROL_PROTOCOL, "lenso.workload-control.v1");
    assert_eq!(
        WORKLOAD_CONTROL_OBSERVE_PATH,
        "/workload-control/v1/observe"
    );
    assert_eq!(
        WORKLOAD_CONTROL_OPERATIONS_PATH,
        "/workload-control/v1/operations"
    );
    assert_eq!(
        WORKLOAD_CONTROL_OPERATION_PATH,
        "/workload-control/v1/operations/{operationId}"
    );

    let observation = WorkloadObservation {
        protocol: WORKLOAD_CONTROL_PROTOCOL.to_owned(),
        workload: workload(),
        state: WorkloadOperationalState::Running,
        observed_revision: Some("authority-revision-17".to_owned()),
        capabilities: BTreeSet::from([
            WorkloadControlCapability::Suspend,
            WorkloadControlCapability::Resume,
        ]),
        protection: WorkloadProtection::Controllable,
        active_operation: None,
        observed_at_unix_ms: 1_786_387_200_000,
    };
    let document = serde_json::to_value(WorkloadControlMessage::Observation(observation)).unwrap();
    let schema = workload_control_schema();
    jsonschema::validator_for(&schema)
        .unwrap()
        .validate(&document)
        .unwrap();

    assert_eq!(
        schema["$defs"]["WorkloadObservation"]["properties"]["protocol"]["const"],
        WORKLOAD_CONTROL_PROTOCOL
    );
    assert_eq!(
        schema["$defs"]["WorkloadObservation"]["additionalProperties"],
        false
    );
    assert!(
        workload_control_schema_digest()
            .strip_prefix("sha256:")
            .is_some_and(
                |digest| digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
            )
    );

    let mut observation = document["document"].clone();
    observation["podId"] = serde_json::json!("pod-ephemeral-1");
    let unknown_field = serde_json::json!({
        "kind": "observation",
        "document": observation,
    });
    assert!(
        jsonschema::validator_for(&schema)
            .unwrap()
            .validate(&unknown_field)
            .is_err()
    );
}

#[test]
fn schema_digest_is_stable() {
    assert_eq!(
        workload_control_schema_digest(),
        "sha256:d3666bb1fd85576f9af4205dbcc70029acd81462678c47d2b315c40ef1a9161d"
    );
}

#[test]
fn observe_request_is_versioned_and_strict() {
    let request = WorkloadObservationRequest {
        protocol: WORKLOAD_CONTROL_PROTOCOL.to_owned(),
        workload: workload(),
    };
    let document =
        serde_json::to_value(WorkloadControlMessage::ObservationRequest(request)).unwrap();
    let schema = workload_control_schema();
    let validator = jsonschema::validator_for(&schema).unwrap();
    validator.validate(&document).unwrap();

    let mut extra_field = document;
    extra_field["document"]["workload"]["namespace"] = serde_json::json!("default");
    assert!(validator.validate(&extra_field).is_err());

    let mut top_level_extra = serde_json::to_value(WorkloadControlMessage::ObservationRequest(
        WorkloadObservationRequest {
            protocol: WORKLOAD_CONTROL_PROTOCOL.to_owned(),
            workload: workload(),
        },
    ))
    .unwrap();
    top_level_extra["host"] = serde_json::json!("127.0.0.1");
    assert!(validator.validate(&top_level_extra).is_err());
    assert!(serde_json::from_value::<WorkloadControlMessage>(top_level_extra).is_err());
}

fn mutation(action: WorkloadControlAction) -> WorkloadMutationRequest {
    WorkloadMutationRequest {
        protocol: WORKLOAD_CONTROL_PROTOCOL.to_owned(),
        workload: workload(),
        action,
        observed_revision: "authority-revision-17".to_owned(),
        idempotency_key: "console-operation-01".to_owned(),
        actor: WorkloadControlActor {
            kind: WorkloadControlActorKind::Operator,
            subject: "operator:console-user-1".to_owned(),
        },
    }
}

#[test]
fn mutation_contract_has_exact_actions_and_positive_scale_capacity() {
    let actions = [
        (WorkloadControlAction::Suspend, "suspend"),
        (WorkloadControlAction::Resume, "resume"),
        (WorkloadControlAction::Restart, "restart"),
        (
            WorkloadControlAction::Scale {
                target_capacity: NonZeroU32::new(3).unwrap(),
            },
            "scale",
        ),
    ];
    let schema = workload_control_schema();
    let validator = jsonschema::validator_for(&schema).unwrap();

    for (action, expected_kind) in actions {
        let message = WorkloadControlMessage::MutationRequest(mutation(action));
        let document = serde_json::to_value(&message).unwrap();
        assert_eq!(document["document"]["action"]["kind"], expected_kind);
        validator.validate(&document).unwrap();
        assert_eq!(
            serde_json::from_value::<WorkloadControlMessage>(document.clone()).unwrap(),
            message
        );
        if expected_kind == "scale" {
            assert_eq!(document["document"]["action"]["targetCapacity"], 3);
            assert!(
                document["document"]["action"]
                    .get("target_capacity")
                    .is_none()
            );
        }
    }

    let mut zero_capacity = serde_json::to_value(WorkloadControlMessage::MutationRequest(
        mutation(WorkloadControlAction::Scale {
            target_capacity: NonZeroU32::new(1).unwrap(),
        }),
    ))
    .unwrap();
    zero_capacity["document"]["action"]["targetCapacity"] = serde_json::json!(0);
    assert!(validator.validate(&zero_capacity).is_err());

    let mut unexpected_capacity = serde_json::to_value(WorkloadControlMessage::MutationRequest(
        mutation(WorkloadControlAction::Suspend),
    ))
    .unwrap();
    unexpected_capacity["document"]["action"]["targetCapacity"] = serde_json::json!(2);
    assert!(validator.validate(&unexpected_capacity).is_err());
    assert!(serde_json::from_value::<WorkloadControlMessage>(unexpected_capacity).is_err());

    let mut blank_reference = serde_json::to_value(WorkloadControlMessage::MutationRequest(
        mutation(WorkloadControlAction::Suspend),
    ))
    .unwrap();
    blank_reference["document"]["workload"]["workloadId"] = serde_json::json!("   ");
    assert!(validator.validate(&blank_reference).is_err());
}

#[test]
fn observation_validation_requires_fresh_revision_for_known_state() {
    let mut observation = WorkloadObservation {
        protocol: WORKLOAD_CONTROL_PROTOCOL.to_owned(),
        workload: workload(),
        state: WorkloadOperationalState::Running,
        observed_revision: None,
        capabilities: BTreeSet::new(),
        protection: WorkloadProtection::Controllable,
        active_operation: None,
        observed_at_unix_ms: 1_786_387_200_000,
    };

    let issues = validate_workload_control_message(&WorkloadControlMessage::Observation(
        observation.clone(),
    ));
    assert!(issues.iter().any(|issue| {
        issue.code == WorkloadControlValidationIssueCode::KnownStateMissingRevision
            && issue.path == "$.document.observedRevision"
    }));

    observation.state = WorkloadOperationalState::Unknown;
    observation.observed_revision = Some("stale-revision".to_owned());
    let issues =
        validate_workload_control_message(&WorkloadControlMessage::Observation(observation));
    assert!(issues.iter().any(|issue| {
        issue.code == WorkloadControlValidationIssueCode::UnknownStateHasRevision
            && issue.path == "$.document.observedRevision"
    }));
}

#[test]
fn public_validation_rejects_invalid_protocol_reference_and_mutation_inputs() {
    let request = WorkloadObservationRequest {
        protocol: "lenso.workload-control.v2".to_owned(),
        workload: WorkloadReference {
            system_id: "   ".to_owned(),
            service_id: "support-desk".to_owned(),
            workload_id: "api".to_owned(),
        },
    };
    let issues =
        validate_workload_control_message(&WorkloadControlMessage::ObservationRequest(request));
    assert!(issues.iter().any(|issue| {
        issue.code == WorkloadControlValidationIssueCode::InvalidProtocol
            && issue.path == "$.document.protocol"
    }));
    assert!(issues.iter().any(|issue| {
        issue.code == WorkloadControlValidationIssueCode::InvalidWorkloadReference
            && issue.path == "$.document.workload.systemId"
    }));

    let mut request = mutation(WorkloadControlAction::Resume);
    request.observed_revision.clear();
    request.idempotency_key.clear();
    request.actor.subject.clear();
    let issues =
        validate_workload_control_message(&WorkloadControlMessage::MutationRequest(request));
    for path in [
        "$.document.observedRevision",
        "$.document.idempotencyKey",
        "$.document.actor.subject",
    ] {
        assert!(issues.iter().any(|issue| {
            issue.code == WorkloadControlValidationIssueCode::InvalidMutationRequest
                && issue.path == path
        }));
    }
}

#[test]
fn schema_and_runtime_apply_the_same_scalar_limits() {
    let schema = workload_control_schema();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let invalid_values = ["   ".to_owned(), "a".repeat(256)];

    for value in &invalid_values {
        let request = WorkloadObservationRequest {
            protocol: WORKLOAD_CONTROL_PROTOCOL.to_owned(),
            workload: WorkloadReference {
                system_id: value.clone(),
                service_id: "support-desk".to_owned(),
                workload_id: "api".to_owned(),
            },
        };
        let message = WorkloadControlMessage::ObservationRequest(request);
        assert!(
            validator
                .validate(&serde_json::to_value(&message).unwrap())
                .is_err()
        );
        assert!(
            validate_workload_control_message(&message)
                .iter()
                .any(|issue| {
                    issue.code == WorkloadControlValidationIssueCode::InvalidWorkloadReference
                        && issue.path == "$.document.workload.systemId"
                })
        );
    }

    for value in &invalid_values {
        for field in ["observedRevision", "idempotencyKey", "actor.subject"] {
            let mut request = mutation(WorkloadControlAction::Resume);
            match field {
                "observedRevision" => request.observed_revision.clone_from(value),
                "idempotencyKey" => request.idempotency_key.clone_from(value),
                "actor.subject" => request.actor.subject.clone_from(value),
                _ => unreachable!(),
            }
            let message = WorkloadControlMessage::MutationRequest(request);
            assert!(
                validator
                    .validate(&serde_json::to_value(&message).unwrap())
                    .is_err(),
                "schema accepted invalid {field}"
            );
            assert!(
                validate_workload_control_message(&message)
                    .iter()
                    .any(|issue| {
                        issue.code == WorkloadControlValidationIssueCode::InvalidMutationRequest
                            && issue.path == format!("$.document.{field}")
                    })
            );
        }

        let observation = WorkloadObservation {
            protocol: WORKLOAD_CONTROL_PROTOCOL.to_owned(),
            workload: workload(),
            state: WorkloadOperationalState::Running,
            observed_revision: Some(value.clone()),
            capabilities: BTreeSet::new(),
            protection: WorkloadProtection::Controllable,
            active_operation: None,
            observed_at_unix_ms: 1_786_387_200_000,
        };
        let message = WorkloadControlMessage::Observation(observation);
        assert!(
            validator
                .validate(&serde_json::to_value(&message).unwrap())
                .is_err()
        );
        assert!(
            validate_workload_control_message(&message)
                .iter()
                .any(|issue| {
                    issue.code == WorkloadControlValidationIssueCode::KnownStateMissingRevision
                        && issue.path == "$.document.observedRevision"
                })
        );
    }
}

fn succeeded_record() -> OperationRecord {
    OperationRecord {
        protocol: WORKLOAD_CONTROL_PROTOCOL.to_owned(),
        operation_id: "workload-operation-01".to_owned(),
        request: mutation(WorkloadControlAction::Suspend),
        authority: WorkloadControlAuthority {
            adapter_id: "local-control-adapter".to_owned(),
            decision: WorkloadControlAuthorityDecision::Accepted,
        },
        phase: WorkloadOperationPhase::Succeeded,
        requested_at_unix_ms: 1_786_387_200_000,
        decided_at_unix_ms: 1_786_387_200_010,
        updated_at_unix_ms: 1_786_387_200_030,
        finished_at_unix_ms: Some(1_786_387_200_030),
        result: Some(WorkloadOperationResult {
            state: WorkloadOperationalState::Suspended,
            observed_revision: "authority-revision-18".to_owned(),
        }),
        failure: None,
    }
}

#[test]
fn operation_record_contains_complete_request_and_safe_authority_outcome() {
    let record = succeeded_record();
    let document = serde_json::to_value(WorkloadControlMessage::OperationRecord(record)).unwrap();
    jsonschema::validator_for(&workload_control_schema())
        .unwrap()
        .validate(&document)
        .unwrap();
    assert_eq!(
        document["document"]["request"]["idempotencyKey"],
        "console-operation-01"
    );
    assert_eq!(
        document["document"]["authority"]["adapterId"],
        "local-control-adapter"
    );

    let codes = [
        WorkloadControlErrorCode::Unauthenticated,
        WorkloadControlErrorCode::Unauthorized,
        WorkloadControlErrorCode::UnsupportedAction,
        WorkloadControlErrorCode::ProtectedWorkload,
        WorkloadControlErrorCode::StaleRevision,
        WorkloadControlErrorCode::ActiveMutation,
        WorkloadControlErrorCode::IdempotencyConflict,
        WorkloadControlErrorCode::AuthorityUnavailable,
        WorkloadControlErrorCode::IncompatibleProtocol,
        WorkloadControlErrorCode::WorkloadNotFound,
        WorkloadControlErrorCode::OperationNotFound,
        WorkloadControlErrorCode::InvalidCapacity,
    ];
    assert_eq!(
        serde_json::to_value(codes).unwrap(),
        serde_json::json!([
            "unauthenticated",
            "unauthorized",
            "unsupported_action",
            "protected_workload",
            "stale_revision",
            "active_mutation",
            "idempotency_conflict",
            "authority_unavailable",
            "incompatible_protocol",
            "workload_not_found",
            "operation_not_found",
            "invalid_capacity"
        ])
    );
}

#[test]
fn error_document_is_typed_strict_and_provider_neutral() {
    let error = WorkloadControlError {
        protocol: WORKLOAD_CONTROL_PROTOCOL.to_owned(),
        code: WorkloadControlErrorCode::ActiveMutation,
        message: "Another mutation is active for this Workload.".to_owned(),
        operation_id: None,
        current_revision: Some("authority-revision-18".to_owned()),
        active_operation: Some("workload-operation-01".to_owned()),
    };
    let mut document = serde_json::to_value(WorkloadControlMessage::Error(error)).unwrap();
    let schema = workload_control_schema();
    let validator = jsonschema::validator_for(&schema).unwrap();
    validator.validate(&document).unwrap();
    assert_eq!(document["kind"], "error");
    assert_eq!(document["document"]["code"], "active_mutation");

    document["document"]["providerDetails"] = serde_json::json!({ "namespace": "default" });
    document["document"]["retryAfter"] = serde_json::json!(30);
    assert!(validator.validate(&document).is_err());
}

#[test]
fn operation_phase_and_terminal_record_validation_are_monotonic() {
    assert!(WorkloadOperationPhase::Accepted.can_advance_to(WorkloadOperationPhase::Executing));
    assert!(WorkloadOperationPhase::Executing.can_advance_to(WorkloadOperationPhase::Verifying));
    assert!(WorkloadOperationPhase::Verifying.can_advance_to(WorkloadOperationPhase::Succeeded));
    assert!(WorkloadOperationPhase::Verifying.can_advance_to(WorkloadOperationPhase::Failed));
    assert!(!WorkloadOperationPhase::Verifying.can_advance_to(WorkloadOperationPhase::Executing));
    assert!(!WorkloadOperationPhase::Succeeded.can_advance_to(WorkloadOperationPhase::Executing));

    let valid = succeeded_record();
    assert!(
        validate_workload_control_message(&WorkloadControlMessage::OperationRecord(valid.clone()))
            .is_empty()
    );

    let mut missing_result = valid.clone();
    missing_result.result = None;
    let issues =
        validate_workload_control_message(&WorkloadControlMessage::OperationRecord(missing_result));
    assert!(issues.iter().any(|issue| {
        issue.code == WorkloadControlValidationIssueCode::TerminalOutcomeMismatch
    }));

    let mut failed_with_result = valid.clone();
    failed_with_result.phase = WorkloadOperationPhase::Failed;
    failed_with_result.failure = Some(WorkloadControlFailure {
        code: WorkloadControlErrorCode::AuthorityUnavailable,
        message: "The bound authority became unavailable.".to_owned(),
    });
    let issues = validate_workload_control_message(&WorkloadControlMessage::OperationRecord(
        failed_with_result,
    ));
    assert!(issues.iter().any(|issue| {
        issue.code == WorkloadControlValidationIssueCode::TerminalOutcomeMismatch
    }));

    let mut denied_by_accepted_authority = valid.clone();
    denied_by_accepted_authority.phase = WorkloadOperationPhase::Denied;
    denied_by_accepted_authority.result = None;
    denied_by_accepted_authority.failure = Some(WorkloadControlFailure {
        code: WorkloadControlErrorCode::ProtectedWorkload,
        message: "Control-plane Workloads are protected.".to_owned(),
    });
    let issues = validate_workload_control_message(&WorkloadControlMessage::OperationRecord(
        denied_by_accepted_authority,
    ));
    assert!(issues.iter().any(|issue| {
        issue.code == WorkloadControlValidationIssueCode::AuthorityDecisionMismatch
    }));

    let mut reversed_time = valid;
    reversed_time.updated_at_unix_ms = reversed_time.decided_at_unix_ms - 1;
    let issues =
        validate_workload_control_message(&WorkloadControlMessage::OperationRecord(reversed_time));
    assert!(
        issues.iter().any(|issue| {
            issue.code == WorkloadControlValidationIssueCode::NonMonotonicTimestamps
        })
    );
}

#[test]
fn succeeded_record_requires_final_known_state_and_nonblank_revision() {
    for state in [
        WorkloadOperationalState::Transitioning,
        WorkloadOperationalState::Failed,
        WorkloadOperationalState::Unknown,
    ] {
        let mut record = succeeded_record();
        record.result.as_mut().unwrap().state = state;
        let issues =
            validate_workload_control_message(&WorkloadControlMessage::OperationRecord(record));
        assert!(issues.iter().any(|issue| {
            issue.code == WorkloadControlValidationIssueCode::InvalidOperationResult
                && issue.path == "$.document.result.state"
        }));
    }

    for revision in ["   ".to_owned(), "r".repeat(256)] {
        let mut record = succeeded_record();
        record.result.as_mut().unwrap().observed_revision = revision;
        let issues =
            validate_workload_control_message(&WorkloadControlMessage::OperationRecord(record));
        assert!(issues.iter().any(|issue| {
            issue.code == WorkloadControlValidationIssueCode::InvalidOperationResult
                && issue.path == "$.document.result.observedRevision"
        }));
    }
}

#[test]
fn succeeded_result_state_matches_the_requested_action() {
    let mismatches = [
        (
            WorkloadControlAction::Suspend,
            WorkloadOperationalState::Running,
        ),
        (
            WorkloadControlAction::Resume,
            WorkloadOperationalState::Suspended,
        ),
        (
            WorkloadControlAction::Restart,
            WorkloadOperationalState::Suspended,
        ),
        (
            WorkloadControlAction::Scale {
                target_capacity: NonZeroU32::new(2).unwrap(),
            },
            WorkloadOperationalState::Suspended,
        ),
    ];
    for (action, state) in mismatches {
        let mut record = succeeded_record();
        record.request.action = action;
        record.result.as_mut().unwrap().state = state;
        let issues =
            validate_workload_control_message(&WorkloadControlMessage::OperationRecord(record));
        assert!(issues.iter().any(|issue| {
            issue.code == WorkloadControlValidationIssueCode::InvalidOperationResult
                && issue.path == "$.document.result.state"
        }));
    }

    for (action, state) in [
        (
            WorkloadControlAction::Suspend,
            WorkloadOperationalState::Suspended,
        ),
        (
            WorkloadControlAction::Resume,
            WorkloadOperationalState::Running,
        ),
        (
            WorkloadControlAction::Restart,
            WorkloadOperationalState::Running,
        ),
        (
            WorkloadControlAction::Scale {
                target_capacity: NonZeroU32::new(2).unwrap(),
            },
            WorkloadOperationalState::Running,
        ),
    ] {
        let mut record = succeeded_record();
        record.request.action = action;
        record.result.as_mut().unwrap().state = state;
        assert!(
            !validate_workload_control_message(&WorkloadControlMessage::OperationRecord(record))
                .iter()
                .any(|issue| {
                    issue.code == WorkloadControlValidationIssueCode::InvalidOperationResult
                })
        );
    }
}

#[test]
fn operation_and_error_scalars_have_schema_runtime_parity() {
    let schema = workload_control_schema();
    let validator = jsonschema::validator_for(&schema).unwrap();

    for value in ["   ".to_owned(), "i".repeat(256)] {
        let observation = WorkloadObservation {
            protocol: WORKLOAD_CONTROL_PROTOCOL.to_owned(),
            workload: workload(),
            state: WorkloadOperationalState::Running,
            observed_revision: Some("authority-revision-17".to_owned()),
            capabilities: BTreeSet::new(),
            protection: WorkloadProtection::Controllable,
            active_operation: Some(value.clone()),
            observed_at_unix_ms: 1_786_387_200_000,
        };
        let message = WorkloadControlMessage::Observation(observation);
        assert!(
            validator
                .validate(&serde_json::to_value(&message).unwrap())
                .is_err(),
            "schema accepted an invalid observation activeOperation"
        );
        assert!(
            validate_workload_control_message(&message)
                .iter()
                .any(|issue| {
                    issue.code == WorkloadControlValidationIssueCode::InvalidOperationRecord
                        && issue.path == "$.document.activeOperation"
                })
        );

        for field in ["operationId", "authority.adapterId"] {
            let mut record = succeeded_record();
            match field {
                "operationId" => record.operation_id.clone_from(&value),
                "authority.adapterId" => record.authority.adapter_id.clone_from(&value),
                _ => unreachable!(),
            }
            let message = WorkloadControlMessage::OperationRecord(record);
            assert!(
                validator
                    .validate(&serde_json::to_value(&message).unwrap())
                    .is_err(),
                "schema accepted invalid {field}"
            );
            assert!(
                validate_workload_control_message(&message)
                    .iter()
                    .any(|issue| {
                        issue.code == WorkloadControlValidationIssueCode::InvalidOperationRecord
                            && issue.path == format!("$.document.{field}")
                    })
            );
        }

        for field in ["operationId", "currentRevision", "activeOperation"] {
            let mut error = WorkloadControlError {
                protocol: WORKLOAD_CONTROL_PROTOCOL.to_owned(),
                code: WorkloadControlErrorCode::ActiveMutation,
                message: "Another mutation is active for this Workload.".to_owned(),
                operation_id: None,
                current_revision: None,
                active_operation: None,
            };
            match field {
                "operationId" => error.operation_id = Some(value.clone()),
                "currentRevision" => error.current_revision = Some(value.clone()),
                "activeOperation" => error.active_operation = Some(value.clone()),
                _ => unreachable!(),
            }
            let message = WorkloadControlMessage::Error(error);
            assert!(
                validator
                    .validate(&serde_json::to_value(&message).unwrap())
                    .is_err(),
                "schema accepted invalid error {field}"
            );
            assert!(
                validate_workload_control_message(&message)
                    .iter()
                    .any(|issue| {
                        issue.code == WorkloadControlValidationIssueCode::InvalidErrorDocument
                            && issue.path == format!("$.document.{field}")
                    })
            );
        }
    }

    for value in ["   ".to_owned(), "m".repeat(1_025)] {
        let mut record = succeeded_record();
        record.phase = WorkloadOperationPhase::Failed;
        record.result = None;
        record.failure = Some(WorkloadControlFailure {
            code: WorkloadControlErrorCode::AuthorityUnavailable,
            message: value.clone(),
        });
        let message = WorkloadControlMessage::OperationRecord(record);
        assert!(
            validator
                .validate(&serde_json::to_value(&message).unwrap())
                .is_err()
        );
        assert!(
            validate_workload_control_message(&message)
                .iter()
                .any(|issue| {
                    issue.code == WorkloadControlValidationIssueCode::InvalidOperationFailure
                        && issue.path == "$.document.failure.message"
                })
        );

        let error = WorkloadControlError {
            protocol: WORKLOAD_CONTROL_PROTOCOL.to_owned(),
            code: WorkloadControlErrorCode::AuthorityUnavailable,
            message: value,
            operation_id: None,
            current_revision: None,
            active_operation: None,
        };
        let message = WorkloadControlMessage::Error(error);
        assert!(
            validator
                .validate(&serde_json::to_value(&message).unwrap())
                .is_err()
        );
        assert!(
            validate_workload_control_message(&message)
                .iter()
                .any(|issue| {
                    issue.code == WorkloadControlValidationIssueCode::InvalidErrorDocument
                        && issue.path == "$.document.message"
                })
        );
    }
}
