use async_trait::async_trait;
use lenso_autonomous_service::{
    DeadLetterCleanupApproval, DeadLetterInspectQuery, DeadLetterOperatorEnvironment,
    DeadLetterOperatorErrorCode, LocalTransportAdapter, ServiceEventHandler,
    ServiceEventHandlerError, cleanup_dead_letters, consume_service_events_once,
    inspect_dead_letters, plan_dead_letter_cleanup, plan_dead_letter_replay, prepare_runtime,
    replay_dead_letter, retain_dead_letter_until,
};
use lenso_service::{
    AutonomousServiceContract, AutonomousServiceStore, AutonomousServiceWorkload,
    ServiceTenancyMode, WorkloadRole,
};
use platform_testing::TestDatabase;
use sqlx::{Postgres, Transaction};

#[derive(Debug)]
struct ReplaySupportHandler;

#[async_trait]
impl ServiceEventHandler for ReplaySupportHandler {
    async fn handle(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        envelope: &lenso_service::EventEnvelope,
    ) -> Result<(), ServiceEventHandlerError> {
        sqlx::query("insert into replay_effects (event_id) values ($1) on conflict do nothing")
            .bind(&envelope.event_id)
            .execute(&mut **transaction)
            .await
            .map_err(ServiceEventHandlerError::store)?;
        Ok(())
    }
}

fn service(service_id: &str, store_id: &str) -> AutonomousServiceContract {
    let mut service = AutonomousServiceContract::new(
        service_id,
        vec![
            AutonomousServiceWorkload::new(
                format!("{service_id}-api"),
                service_id,
                WorkloadRole::API,
            ),
            AutonomousServiceWorkload::new(
                format!("{service_id}-migrate"),
                service_id,
                WorkloadRole::MIGRATION,
            ),
            AutonomousServiceWorkload::new(
                format!("{service_id}-worker"),
                service_id,
                WorkloadRole::WORKER,
            ),
        ],
        ServiceTenancyMode::None,
        vec!["local".to_owned()],
    );
    service.stores = vec![AutonomousServiceStore::new(store_id, service_id)];
    service
}

#[tokio::test]
async fn inspect_dead_letters_returns_stable_ordered_operator_json() {
    let Some(database) = TestDatabase::create().await else {
        return;
    };
    let state = prepare_runtime(
        &service("support-sla", "support-sla-store"),
        &lenso_autonomous_service::ServiceRuntimeConfig::new(
            "support-sla",
            "support-sla-store",
            "support-sla",
        ),
        database.pool.clone(),
        &[],
    )
    .await
    .unwrap();

    for (dead_letter_id, event_id, recorded_at) in [
        ("dead-b", "event-b", "2026-07-15T09:00:01Z"),
        ("dead-a", "event-a", "2026-07-15T09:00:00Z"),
    ] {
        sqlx::query(
            r#"
            insert into platform.service_event_dead_letters (
                dead_letter_id, consumer_id, event_id, delivery_id, envelope,
                contract_id, contract_version, failure_reason, reason_code,
                diagnostic, attempt_count, terminal_outcome, delivery_history,
                max_attempts, retry_schedule, next_actions, dead_lettered_at
            ) values ($1, 'support-sla', $2, $3, $4, 'ticket-opened', 'v1',
                      'poison', 'invalid_payload', 'invalid payload', 1,
                      'dead_lettered', '[]', 3, '[]', '["inspect_payload"]', $5)
            "#,
        )
        .bind(dead_letter_id)
        .bind(event_id)
        .bind(format!("delivery-{event_id}"))
        .bind(serde_json::json!({
            "protocol": "lenso.event-envelope.v1",
            "eventId": event_id,
            "contractId": "ticket-opened",
            "contractVersion": "v1",
            "story": {"storyId": "story-1", "segmentId": "segment-1"},
            "causation": {"causationId": "cause-1"}
        }))
        .bind(
            recorded_at
                .parse::<chrono::DateTime<chrono::Utc>>()
                .unwrap(),
        )
        .execute(&database.pool)
        .await
        .unwrap();
    }

    let result = inspect_dead_letters(&state, DeadLetterInspectQuery::default())
        .await
        .unwrap();
    let json = serde_json::to_value(result).unwrap();

    assert_eq!(json["protocol"], "lenso.dead-letter-inspection.v1");
    assert_eq!(json["items"][0]["deadLetterId"], "dead-a");
    assert_eq!(json["items"][1]["deadLetterId"], "dead-b");
    assert_eq!(json["items"][0]["status"], "dead_lettered");
    assert_eq!(
        json["items"][0]["nextActions"],
        serde_json::json!(["inspect_payload"])
    );

    drop(state);
    database.cleanup().await;
}

#[tokio::test]
async fn replay_dry_run_is_non_mutating_and_execution_preserves_business_identity() {
    let Some(database) = TestDatabase::create().await else {
        return;
    };
    let Some(transport_database) = TestDatabase::create().await else {
        database.cleanup().await;
        return;
    };
    let state = prepare_runtime(
        &service("support-sla", "support-sla-store"),
        &lenso_autonomous_service::ServiceRuntimeConfig::new(
            "support-sla",
            "support-sla-store",
            "support-sla",
        ),
        database.pool.clone(),
        &[platform_core::Migration {
            name: "support-sla/0001_create_replay_effects",
            sql: "create table replay_effects (event_id text primary key);",
        }],
    )
    .await
    .unwrap();
    let adapter = LocalTransportAdapter::prepare(transport_database.pool.clone())
        .await
        .unwrap();
    let envelope: lenso_service::EventEnvelope = serde_json::from_str(include_str!(
        "../../../contracts/events/support/support.ticket-opened.v1.envelope.json"
    ))
    .unwrap();
    let envelope_json = serde_json::to_value(&envelope).unwrap();
    sqlx::query(
        r#"
        insert into platform.service_event_inbox (
            delivery_id, consumer_id, event_id, envelope, status, attempt_count,
            failure_reason, reason_code, terminal_outcome, delivery_history,
            original_envelope, max_attempts, retry_schedule
        ) values ('delivery-original', 'support-sla', $1, $2, 'dead_lettered', 1,
                  'poison', 'invalid_payload', 'dead_lettered', '[]', $2, 3, '[]')
        "#,
    )
    .bind(&envelope.event_id)
    .bind(&envelope_json)
    .execute(&database.pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        insert into platform.service_event_dead_letters (
            dead_letter_id, consumer_id, event_id, delivery_id, envelope,
            contract_id, contract_version, failure_reason, reason_code,
            diagnostic, attempt_count, terminal_outcome, delivery_history,
            max_attempts, retry_schedule, next_actions, dead_lettered_at
        ) values ('dead-replay', 'support-sla', $1, 'delivery-original', $2,
                  $3, $4, 'poison', 'invalid_payload', 'invalid payload', 1,
                  'dead_lettered', '[]', 3, '[]', '["replay_event"]',
                  '2026-07-15T09:00:00Z')
        "#,
    )
    .bind(&envelope.event_id)
    .bind(&envelope_json)
    .bind(&envelope.contract_id)
    .bind(&envelope.contract_version)
    .execute(&database.pool)
    .await
    .unwrap();

    let plan = plan_dead_letter_replay(
        &state,
        "dead-replay",
        DeadLetterOperatorEnvironment::LocalSandbox,
    )
    .await
    .unwrap();
    let json = serde_json::to_value(&plan).unwrap();
    assert_eq!(json["protocol"], "lenso.dead-letter-replay-plan.v1");
    assert_eq!(json["mutatesState"], false);
    assert_eq!(json["affectedServiceId"], "support-sla");
    assert_eq!(json["identity"]["eventId"], envelope.event_id);
    assert_eq!(json["identity"]["contractVersion"], "v1");
    assert_eq!(
        json["identity"]["storyContext"],
        envelope_json["context"]["story"]
    );
    assert_eq!(
        json["identity"]["causation"],
        envelope_json["context"]["causation"]
    );
    assert_eq!(json["authorization"]["status"], "not_required");
    assert_eq!(json["approvalBoundary"], "local_sandbox_only");
    assert_eq!(json["environment"], "local_sandbox");
    let replay_count: i64 =
        sqlx::query_scalar("select count(*) from platform.service_event_replays")
            .fetch_one(&database.pool)
            .await
            .unwrap();
    assert_eq!(replay_count, 0);

    let production_plan = plan_dead_letter_replay(
        &state,
        "dead-replay",
        DeadLetterOperatorEnvironment::Production,
    )
    .await
    .unwrap();
    let production_json = serde_json::to_value(&production_plan).unwrap();
    assert_eq!(production_json["approvalBoundary"], "production_replay");
    assert_eq!(production_json["authorization"]["status"], "required");
    let error = replay_dead_letter(&state, &adapter, &production_plan, None)
        .await
        .unwrap_err();
    assert_eq!(error.code, DeadLetterOperatorErrorCode::ApprovalRequired);
    let mut tampered_plan = production_plan.clone();
    tampered_plan.environment = DeadLetterOperatorEnvironment::LocalSandbox;
    let error = replay_dead_letter(&state, &adapter, &tampered_plan, None)
        .await
        .unwrap_err();
    assert_eq!(error.code, DeadLetterOperatorErrorCode::StateChanged);

    let result = replay_dead_letter(&state, &adapter, &plan, None)
        .await
        .unwrap();
    assert_eq!(result.event_id, envelope.event_id);
    assert_eq!(result.contract_version, envelope.contract_version);
    assert_ne!(result.delivery_id, "delivery-original");
    assert_eq!(
        consume_service_events_once(&state, &adapter, "support-sla", &ReplaySupportHandler, 10,)
            .await
            .unwrap(),
        1
    );
    let first_replay_status: String = sqlx::query_scalar(
        "select status from platform.service_event_replays where replay_id = $1",
    )
    .bind(&result.replay_id)
    .fetch_one(&database.pool)
    .await
    .unwrap();
    assert_eq!(first_replay_status, "completed");

    let duplicate_plan = plan_dead_letter_replay(
        &state,
        "dead-replay",
        DeadLetterOperatorEnvironment::LocalSandbox,
    )
    .await
    .unwrap();
    let duplicate_result = replay_dead_letter(&state, &adapter, &duplicate_plan, None)
        .await
        .unwrap();
    assert_ne!(duplicate_result.delivery_id, result.delivery_id);
    assert_eq!(
        consume_service_events_once(&state, &adapter, "support-sla", &ReplaySupportHandler, 10,)
            .await
            .unwrap(),
        0
    );
    let effect_count: i64 = sqlx::query_scalar("select count(*) from replay_effects")
        .fetch_one(&database.pool)
        .await
        .unwrap();
    assert_eq!(effect_count, 1);
    let duplicate_status: String = sqlx::query_scalar(
        "select status from platform.service_event_replays where replay_id = $1",
    )
    .bind(&duplicate_result.replay_id)
    .fetch_one(&database.pool)
    .await
    .unwrap();
    assert_eq!(duplicate_status, "duplicate_completed");

    drop(adapter);
    drop(state);
    database.cleanup().await;
    transport_database.cleanup().await;
}

#[tokio::test]
async fn cleanup_requires_authority_and_preserves_deduplication_audit_and_active_replays() {
    let Some(database) = TestDatabase::create().await else {
        return;
    };
    let state = prepare_runtime(
        &service("support-sla", "support-sla-store"),
        &lenso_autonomous_service::ServiceRuntimeConfig::new(
            "support-sla",
            "support-sla-store",
            "support-sla",
        ),
        database.pool.clone(),
        &[],
    )
    .await
    .unwrap();
    let envelope = serde_json::json!({
        "protocol": "lenso.event-envelope.v1",
        "eventId": "event-clean",
        "contractId": "ticket-opened",
        "contractVersion": "v1"
    });
    for (dead_letter_id, event_id, status) in [
        ("dead-clean", "event-clean", "resolved"),
        ("dead-active", "event-active", "replay_active"),
        ("dead-unresolved", "event-unresolved", "dead_lettered"),
        ("dead-retained", "event-retained", "resolved"),
    ] {
        sqlx::query(
            r#"
            insert into platform.service_event_dead_letters (
                dead_letter_id, consumer_id, event_id, delivery_id, envelope,
                contract_id, contract_version, failure_reason, reason_code,
                diagnostic, attempt_count, terminal_outcome, delivery_history,
                max_attempts, retry_schedule, next_actions, dead_lettered_at,
                status, resolved_at
            ) values ($1, 'support-sla', $2, $3, $4, 'ticket-opened', 'v1',
                      'poison', 'invalid_payload', 'invalid payload', 1,
                      'dead_lettered', '[]', 3, '[]', '["inspect_payload"]',
                      '2026-06-01T00:00:00Z', $5,
                      case when $5 = 'resolved'
                           then '2026-06-02T00:00:00Z'::timestamptz
                           else null end)
            "#,
        )
        .bind(dead_letter_id)
        .bind(event_id)
        .bind(format!("delivery-{event_id}"))
        .bind(&envelope)
        .bind(status)
        .execute(&database.pool)
        .await
        .unwrap();
    }
    sqlx::query(
        r#"
        insert into platform.service_event_inbox (
            delivery_id, consumer_id, event_id, envelope, status
        ) values ('delivery-event-clean', 'support-sla', 'event-clean', $1, 'completed')
        "#,
    )
    .bind(&envelope)
    .execute(&database.pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        insert into platform.service_event_delivery_evidence (
            evidence_id, stage, outcome, event_id, delivery_id, detail,
            recorded_at
        ) values ('evidence-clean', 'replay', 'completed', 'event-clean',
                  'delivery-event-clean', '{}', '2026-06-02T00:00:00Z')
        "#,
    )
    .execute(&database.pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        insert into platform.service_event_replays (
            replay_id, dead_letter_id, consumer_id, event_id,
            original_delivery_id, replay_delivery_id, environment, plan_id,
            status, created_at, completed_at
        ) values ('replay-clean', 'dead-clean', 'support-sla', 'event-clean',
                  'delivery-original', 'delivery-event-clean', 'local_sandbox',
                  'plan-clean', 'completed', '2026-06-01T00:00:00Z',
                  '2026-06-02T00:00:00Z')
        "#,
    )
    .execute(&database.pool)
    .await
    .unwrap();
    retain_dead_letter_until(
        &state,
        "dead-retained",
        "2026-09-01T00:00:00Z".parse().unwrap(),
    )
    .await
    .unwrap();

    let plan = plan_dead_letter_cleanup(
        &state,
        DeadLetterOperatorEnvironment::LocalSandbox,
        "2026-08-01T00:00:00Z".parse().unwrap(),
    )
    .await
    .unwrap();
    let json = serde_json::to_value(&plan).unwrap();
    assert_eq!(json["protocol"], "lenso.dead-letter-cleanup-plan.v1");
    assert_eq!(json["mutatesState"], false);
    assert_eq!(json["deadLetterIds"], serde_json::json!(["dead-clean"]));
    assert_eq!(
        json["preservedState"],
        serde_json::json!([
            "service_event_inbox",
            "service_event_delivery_evidence",
            "service_event_replays"
        ])
    );
    let error = cleanup_dead_letters(&state, &plan, None).await.unwrap_err();
    assert_eq!(error.code, DeadLetterOperatorErrorCode::ApprovalRequired);

    let result = cleanup_dead_letters(
        &state,
        &plan,
        Some(&DeadLetterCleanupApproval {
            approval_id: "cleanup-approved-1".to_owned(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(result.deleted_dead_letter_ids, vec!["dead-clean"]);
    let remaining: Vec<String> = sqlx::query_scalar(
        "select dead_letter_id from platform.service_event_dead_letters order by dead_letter_id",
    )
    .fetch_all(&database.pool)
    .await
    .unwrap();
    assert_eq!(
        remaining,
        vec!["dead-active", "dead-retained", "dead-unresolved"]
    );
    let preserved: (i64, i64, i64) = (
        sqlx::query_scalar("select count(*) from platform.service_event_inbox where event_id = 'event-clean'")
            .fetch_one(&database.pool).await.unwrap(),
        sqlx::query_scalar("select count(*) from platform.service_event_delivery_evidence where event_id = 'event-clean'")
            .fetch_one(&database.pool).await.unwrap(),
        sqlx::query_scalar("select count(*) from platform.service_event_replays where event_id = 'event-clean'")
            .fetch_one(&database.pool).await.unwrap(),
    );
    assert_eq!(preserved, (1, 2, 1));

    drop(state);
    database.cleanup().await;
}
