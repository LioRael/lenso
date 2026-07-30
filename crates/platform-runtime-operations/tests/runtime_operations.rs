use lenso_service::{
    SystemSandboxWorkloadIdentityProvider,
    system_plane::{
        ManagementActor, ManagementActorKind, ManagementApproval, ManagementIntent,
        RUNTIME_OPERATIONS_PROTOCOL, RuntimeOperationDesiredOutcome, RuntimeOperationSubmission,
        RuntimeOperationTarget, RuntimeOperationTargetKind, RuntimeOperationTargetStatus,
        runtime_operation_plan_digest, runtime_operations_schema_digest,
    },
};
use platform_runtime_operations::{
    RUNTIME_OPERATIONS_MIGRATIONS, RuntimeOperationsErrorCode, RuntimeOperationsProvider,
    SystemSandboxManagementAuthorityVerifier,
};
use platform_system_plane::{
    AuthorizedSystemPlaneCaller, EnrollmentAuthorizer, EnrollmentGrant, SystemPlaneAccess,
    SystemPlaneRegistryBuilder, SystemPlaneRuntime, SystemSandboxEnrollmentAuthorizer,
};
use platform_testing::TestDatabase;
use std::sync::Arc;

fn digest(byte: char) -> String {
    format!("sha256:{}", byte.to_string().repeat(64))
}

#[test]
fn advertisement_binds_exact_runtime_operations_contract_and_features() {
    let advertisement = RuntimeOperationsProvider::advertisement();
    assert_eq!(advertisement.contract_id, RUNTIME_OPERATIONS_PROTOCOL);
    assert_eq!(
        advertisement.schema_digest,
        runtime_operations_schema_digest()
    );
    assert_eq!(
        advertisement.feature_ids.into_iter().collect::<Vec<_>>(),
        vec![
            "function-run-retry",
            "operation-evidence",
            "outbox-event-retry"
        ]
    );
}

#[tokio::test]
async fn retry_flow_is_revision_bound_durable_idempotent_and_evidence_backed() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    platform_core::apply_migrations(&db.pool, platform_core::PLATFORM_MIGRATIONS)
        .await
        .unwrap();
    platform_core::apply_migrations(&db.pool, platform_runtime::RUNTIME_MIGRATIONS)
        .await
        .unwrap();
    platform_core::apply_migrations(&db.pool, RUNTIME_OPERATIONS_MIGRATIONS)
        .await
        .unwrap();
    sqlx::query(
        r#"
        insert into runtime.function_runs (
            id, function_name, input_json, status, attempts, max_attempts,
            correlation_id, actor
        ) values ('run-1', 'tickets.notify', '{}', 'dead', 3, 3, 'story-1', '{}')
        "#,
    )
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        insert into platform.outbox (
            id, event_name, event_version, source_module, aggregate_type,
            aggregate_id, correlation_id, occurred_at, payload, status,
            attempts, max_attempts
        ) values (
            'event-1', 'tickets.notified.v1', 1, 'tickets', 'ticket',
            'ticket-1', 'story-1', now(), '{}', 'dead', 3, 3
        )
        "#,
    )
    .execute(&db.pool)
    .await
    .unwrap();
    let provider = RuntimeOperationsProvider::new(
        db.pool.clone(),
        "support",
        "release:sha256:0123456789abcdef",
    )
    .with_authority_verifier(Arc::new(
        SystemSandboxManagementAuthorityVerifier::new("test").unwrap(),
    ));
    let function_target = RuntimeOperationTarget {
        kind: RuntimeOperationTargetKind::FunctionRun,
        target_id: "run-1".to_owned(),
    };
    let snapshot = provider
        .target_snapshot(&function_target, 1_000)
        .await
        .unwrap();
    assert_eq!(snapshot.status, RuntimeOperationTargetStatus::Dead);
    assert_eq!(snapshot.target_name, "tickets.notify");
    let intent = ManagementIntent {
        protocol: RUNTIME_OPERATIONS_PROTOCOL.to_owned(),
        intent_id: "intent:retry-run-1".to_owned(),
        service_id: "support".to_owned(),
        service_revision: "release:sha256:0123456789abcdef".to_owned(),
        target: function_target,
        desired_outcome: RuntimeOperationDesiredOutcome::Retry,
        expected_target_revision: snapshot.target_revision,
        actor: ManagementActor {
            kind: ManagementActorKind::Operator,
            subject: "operator:console-user-1".to_owned(),
            delegated_authority_digest: digest('b'),
        },
        approvals: vec![ManagementApproval {
            approval_id: "approval:retry-run-1".to_owned(),
            approval_digest: digest('c'),
        }],
        deadline_unix_ms: 100_000,
        idempotency_key: "retry-run-1".to_owned(),
        capability_contract_id: RUNTIME_OPERATIONS_PROTOCOL.to_owned(),
        capability_schema_digest: runtime_operations_schema_digest(),
    };
    let plan = provider.plan(&intent, 2_000).await.unwrap();
    let submission = RuntimeOperationSubmission { intent, plan };
    let caller = caller().await;

    let acknowledgement = provider.submit(&submission, &caller, 3_000).await.unwrap();
    let replay = provider
        .submit(&submission, &caller, 200_000)
        .await
        .unwrap();
    assert_eq!(acknowledgement, replay);
    let evidence = provider
        .evidence(&acknowledgement.operation_id)
        .await
        .unwrap();
    assert_eq!(
        evidence.state,
        lenso_service::system_plane::RuntimeOperationState::Succeeded
    );
    assert_eq!(evidence.sequence, 2);
    assert_ne!(
        evidence.target_revision_before.as_str(),
        evidence.target_revision_after.as_deref().unwrap()
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "select status from runtime.function_runs where id = 'run-1'"
        )
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        "pending"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "select count(*) from platform.system_plane_runtime_operations where operation_id = $1",
        )
        .bind(&acknowledgement.operation_id)
        .fetch_one(&db.pool)
        .await
        .unwrap(),
        1
    );
    let first_page = provider
        .evidence_page(&acknowledgement.operation_id, None, 1)
        .await
        .unwrap();
    assert_eq!(first_page.items.len(), 1);
    assert_eq!(
        first_page.items[0].state,
        lenso_service::system_plane::RuntimeOperationState::Accepted
    );
    let cursor = first_page.next_cursor.unwrap();
    let second_page = provider
        .evidence_page(&acknowledgement.operation_id, Some(&cursor), 1)
        .await
        .unwrap();
    assert_eq!(second_page.items, [evidence.clone()]);
    assert!(second_page.next_cursor.is_none());
    assert_eq!(
        provider
            .evidence_page("runtime-operation:other", Some(&cursor), 1)
            .await
            .unwrap_err()
            .code,
        RuntimeOperationsErrorCode::InvalidEvidenceCursor
    );
    let recovery = provider
        .recover_by_idempotency_key("retry-run-1")
        .await
        .unwrap();
    assert_eq!(recovery.acknowledgement, acknowledgement);
    assert_eq!(recovery.latest_evidence, evidence);

    let mut conflicting = submission.clone();
    conflicting.intent.actor.subject = "operator:other".to_owned();
    conflicting.plan.intent_digest =
        lenso_service::system_plane::management_intent_digest(&conflicting.intent);
    conflicting.plan.plan_digest = runtime_operation_plan_digest(&conflicting.plan);
    assert_eq!(
        provider
            .submit(&conflicting, &caller, 3_200)
            .await
            .unwrap_err()
            .code,
        RuntimeOperationsErrorCode::IdempotencyConflict
    );

    let outbox_target = RuntimeOperationTarget {
        kind: RuntimeOperationTargetKind::OutboxEvent,
        target_id: "event-1".to_owned(),
    };
    let snapshot = provider
        .target_snapshot(&outbox_target, 4_000)
        .await
        .unwrap();
    assert_eq!(snapshot.status, RuntimeOperationTargetStatus::Dead);
    assert_eq!(snapshot.target_name, "tickets.notified.v1");
    let mut outbox_intent = submission.intent;
    outbox_intent.intent_id = "intent:retry-event-1".to_owned();
    outbox_intent.target = outbox_target;
    outbox_intent.expected_target_revision = snapshot.target_revision;
    outbox_intent.idempotency_key = "retry-event-1".to_owned();
    let outbox_plan = provider.plan(&outbox_intent, 4_100).await.unwrap();
    assert_eq!(
        outbox_plan.expected_effects,
        ["schedule the exact Outbox Event for one additional runtime attempt"]
    );
    let outbox_acknowledgement = provider
        .submit(
            &RuntimeOperationSubmission {
                intent: outbox_intent,
                plan: outbox_plan,
            },
            &caller,
            4_200,
        )
        .await
        .unwrap();
    let outbox_evidence = provider
        .evidence(&outbox_acknowledgement.operation_id)
        .await
        .unwrap();
    assert_eq!(outbox_evidence.code, "outbox_event_retry_scheduled");
    assert_eq!(
        sqlx::query_scalar::<_, String>("select status from platform.outbox where id = 'event-1'")
            .fetch_one(&db.pool)
            .await
            .unwrap(),
        "pending"
    );

    db.cleanup().await;
}

async fn caller() -> AuthorizedSystemPlaneCaller {
    let identity = Arc::new(
        SystemSandboxWorkloadIdentityProvider::new("test", "runtime-operations-secret").unwrap(),
    );
    let enrollment = Arc::new(
        SystemSandboxEnrollmentAuthorizer::new(
            "test",
            EnrollmentGrant::system_sandbox("support", "service:console", 7, 4_000_000_000_000),
        )
        .unwrap(),
    );
    let authorization = enrollment
        .authorize("support", "service:console", 3_000)
        .await
        .unwrap();
    let runtime = Arc::new(SystemPlaneRuntime::new(
        SystemPlaneRegistryBuilder::new(
            "support",
            "service:support",
            "release:sha256:0123456789abcdef",
        )
        .register(RuntimeOperationsProvider::advertisement())
        .build()
        .unwrap(),
        SystemPlaneAccess::new(identity, "service:support", enrollment),
    ));
    AuthorizedSystemPlaneCaller {
        runtime,
        service_principal: "service:console".to_owned(),
        enrollment: authorization,
    }
}
