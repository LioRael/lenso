use async_trait::async_trait;
use axum::body::Body;
use chrono::{DateTime, Duration, Utc};
use http::{Request, StatusCode, header};
use http_body_util::BodyExt as _;
use lenso_autonomous_service::{
    LocalTransportAdapter, ServiceEventHandler, ServiceEventHandlerError, ServiceEventPublisher,
    ServiceRuntimeConfig, ServiceRuntimeState, SystemSandboxWorkflowClock, TransportAdapter,
    TransportPublication, WorkflowErrorCode, WorkflowEventPublication,
    WorkflowFailureClassification, WorkflowFailureDisposition, WorkflowStepFailure,
    WorkflowTimerKind, WorkflowTransitionDisposition,
    advance_claimed_workflow_retry_with_event_in_tx, advance_workflow_step_with_event_in_tx,
    claim_due_workflow_work_at, consume_service_events_once_without_workload_identity,
    prepare_runtime, record_claimed_workflow_step_failure_at, record_workflow_step_failure_at,
    relay_service_events_once, service_router, start_workflow_from_event_in_tx,
};
use lenso_contracts::{
    ModuleManifest, RuntimeSurface, WorkflowDataContract, WorkflowDefinition,
    WorkflowRetryPolicyDeclaration, WorkflowStepDeclaration,
};
use lenso_service::{
    AutonomousServiceContract, AutonomousServiceStore, AutonomousServiceWorkload,
    CommonContextRequirement, ContractContextRequirements, EventArtifactFormat,
    EventArtifactReference, EventContractArtifact, EventEnvelope, ServicePrincipal,
    ServiceTenancyMode, WorkloadRole,
};
use platform_testing::TestDatabase;
use sqlx::{Postgres, Transaction};
use std::sync::Arc;
use tower::ServiceExt as _;
use utoipa_axum::router::OpenApiRouter;

#[path = "support/event.rs"]
mod support_event_fixture;

use support_event_fixture::support_ticket_opened;

fn service() -> AutonomousServiceContract {
    let mut service = AutonomousServiceContract::new(
        "support-sla",
        vec![
            AutonomousServiceWorkload::new("support-sla-api", "support-sla", WorkloadRole::API),
            AutonomousServiceWorkload::new(
                "support-sla-migrate",
                "support-sla",
                WorkloadRole::MIGRATION,
            ),
            AutonomousServiceWorkload::new(
                "support-sla-worker",
                "support-sla",
                WorkloadRole::WORKER,
            ),
        ],
        ServiceTenancyMode::Optional,
        vec!["local".to_owned()],
    );
    service.modules = vec!["support-sla".to_owned()];
    service.stores = vec![AutonomousServiceStore::new("primary", "support-sla")];
    let mut acknowledgement = EventContractArtifact::new(
        "sla-acknowledged",
        "support-sla",
        "v1",
        ServiceTenancyMode::Required,
        EventArtifactReference::new(
            EventArtifactFormat::JsonSchema,
            "contracts/events/support/support.sla-acknowledged.v1.schema.json",
        ),
    );
    acknowledgement.context = ContractContextRequirements::new(vec![
        CommonContextRequirement::Story,
        CommonContextRequirement::Trace,
        CommonContextRequirement::ServicePrincipal,
        CommonContextRequirement::DelegatedActor,
        CommonContextRequirement::Tenant,
        CommonContextRequirement::Deadline,
        CommonContextRequirement::IdempotencyKey,
        CommonContextRequirement::Causation,
        CommonContextRequirement::Region,
    ]);
    service.event_contracts = vec![acknowledgement];
    service
}

fn support_service() -> AutonomousServiceContract {
    let mut service = AutonomousServiceContract::new(
        "support",
        vec![
            AutonomousServiceWorkload::new("support-api", "support", WorkloadRole::API),
            AutonomousServiceWorkload::new("support-migrate", "support", WorkloadRole::MIGRATION),
            AutonomousServiceWorkload::new("support-worker", "support", WorkloadRole::WORKER),
        ],
        ServiceTenancyMode::Required,
        vec!["local".to_owned()],
    );
    service.modules = vec!["support-ticket".to_owned()];
    service.stores = vec![AutonomousServiceStore::new("primary", "support")];
    service
}

fn manifest() -> ModuleManifest {
    ModuleManifest::builder("support-sla")
        .runtime(RuntimeSurface {
            functions: vec![],
            schedules: vec![],
            workflows: vec![workflow("v1"), workflow("v2")],
        })
        .build()
}

fn workflow(version: &str) -> WorkflowDefinition {
    WorkflowDefinition::new(
        "support-sla",
        "ticket_sla",
        version,
        WorkflowDataContract::new("support.sla.start", "v1"),
        WorkflowDataContract::new("support.sla.result", "v1"),
        vec![
            WorkflowStepDeclaration::new("acknowledge_ticket")
                .with_retry_policy(WorkflowRetryPolicyDeclaration::new(3, vec![1_000, 2_000]))
                .with_timeout_ms(5_000),
            WorkflowStepDeclaration::new("await_resolution"),
        ],
    )
}

fn runtime_config(manifest: &ModuleManifest) -> ServiceRuntimeConfig {
    ServiceRuntimeConfig::new("support-sla", "primary", "support-sla")
        .with_module_manifests(std::slice::from_ref(manifest))
}

fn support_sla_principal(source: &EventEnvelope) -> ServicePrincipal {
    let mut principal = source
        .context
        .service_principal
        .clone()
        .expect("support ticket event carries Service Principal context");
    "spiffe://example.com/service/support-sla".clone_into(&mut principal.subject);
    principal.audiences = vec!["support".to_owned()];
    "credential_support_sla_01".clone_into(&mut principal.credential_id);
    principal
}

fn acknowledgement_publication(
    instance_id: &str,
    step_id: &str,
    source: &EventEnvelope,
) -> WorkflowEventPublication {
    WorkflowEventPublication::new(
        "support",
        format!("sla-acknowledged-{}", source.event_id),
        "sla-acknowledged",
        "v1",
        "2026-07-16T15:00:00Z",
        support_sla_principal(source),
        serde_json::json!({
            "ticketId": source.content.data["ticketId"],
            "workflowInstanceId": instance_id,
            "workflowStepId": step_id,
        }),
    )
}

#[derive(Debug, Clone)]
struct SupportSlaWorkflowHandler {
    state: ServiceRuntimeState,
}

#[async_trait]
impl ServiceEventHandler for SupportSlaWorkflowHandler {
    async fn handle(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        envelope: &EventEnvelope,
    ) -> Result<(), ServiceEventHandlerError> {
        let instance = start_workflow_from_event_in_tx(
            &self.state,
            transaction,
            "support-sla",
            "ticket_sla",
            "v1",
            envelope,
        )
        .await
        .map_err(|error| ServiceEventHandlerError::retryable(error.code.as_str(), error.message))?;
        let transition_id = format!("{}:acknowledge_ticket", envelope.event_id);
        let result = advance_workflow_step_with_event_in_tx(
            &self.state,
            transaction,
            &instance.instance_id,
            &instance.initial_step_id,
            &transition_id,
            acknowledgement_publication(&instance.instance_id, &instance.initial_step_id, envelope),
        )
        .await
        .map_err(|error| ServiceEventHandlerError::retryable(error.code.as_str(), error.message))?;
        assert_eq!(result.disposition, WorkflowTransitionDisposition::Applied);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct SupportTicketAcknowledgementHandler;

#[async_trait]
impl ServiceEventHandler for SupportTicketAcknowledgementHandler {
    async fn handle(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        envelope: &EventEnvelope,
    ) -> Result<(), ServiceEventHandlerError> {
        let envelope_json = serde_json::to_value(envelope)
            .expect("validated Event Envelope must remain serializable");
        sqlx::query(
            r"
            insert into support_ticket_sla_acknowledgements (
                ticket_id, source_event_id, envelope
            ) values ($1, $2, $3)
            ",
        )
        .bind(envelope.content.data["ticketId"].as_str().unwrap())
        .bind(&envelope.event_id)
        .bind(envelope_json)
        .execute(&mut **transaction)
        .await
        .map_err(ServiceEventHandlerError::store)?;
        Ok(())
    }
}

fn start_request(version: &str) -> Request<Body> {
    Request::post("/runtime/workflows/support-sla/ticket_sla/instances")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({
                "definitionVersion": version,
                "input": {"ticketId": "ticket_01"},
                "storyContext": {
                    "storyId": "story_support_01",
                    "segmentId": "segment_start_01"
                },
                "tenantScope": {"tenantId": "tenant_01"}
            })
            .to_string(),
        ))
        .unwrap()
}

async fn json_body(response: axum::response::Response) -> serde_json::Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn versioned_workflow_start_and_inspection_survive_runtime_restart() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let service = service();
    let manifest = manifest();
    let state = prepare_runtime(&service, &runtime_config(&manifest), db.pool.clone(), &[])
        .await
        .expect("Service runtime should prepare workflow storage");
    let app = service_router(OpenApiRouter::new(), state);

    let started_response = app.clone().oneshot(start_request("v1")).await.unwrap();
    assert_eq!(started_response.status(), StatusCode::CREATED);
    let started = json_body(started_response).await;
    assert_eq!(started["protocol"], "lenso.workflow-start-result.v1");
    assert_eq!(started["instance"]["definition"]["version"], "v1");
    assert_eq!(started["instance"]["state"], "running");
    assert_eq!(
        started["instance"]["steps"][0]["definitionStepName"],
        "acknowledge_ticket"
    );
    let instance_id = started["instance"]["instanceId"]
        .as_str()
        .unwrap()
        .to_owned();
    let initial_step_id = started["instance"]["initialStepId"]
        .as_str()
        .unwrap()
        .to_owned();
    let created_at = started["instance"]["createdAt"].clone();
    drop(app);

    let restarted_state =
        prepare_runtime(&service, &runtime_config(&manifest), db.pool.clone(), &[])
            .await
            .expect("restarted Service runtime should reuse its owned Store");
    let restarted_app = service_router(OpenApiRouter::new(), restarted_state);
    let inspected_response = restarted_app
        .clone()
        .oneshot(
            Request::get(format!("/runtime/workflows/instances/{instance_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(inspected_response.status(), StatusCode::OK);
    let inspected = json_body(inspected_response).await;

    assert_eq!(inspected["protocol"], "lenso.workflow-inspection.v1");
    assert_eq!(inspected["instance"]["instanceId"], instance_id);
    assert_eq!(inspected["instance"]["serviceId"], "support-sla");
    assert_eq!(inspected["instance"]["definition"]["owner"], "support-sla");
    assert_eq!(inspected["instance"]["definition"]["name"], "ticket_sla");
    assert_eq!(inspected["instance"]["definition"]["version"], "v1");
    assert_eq!(inspected["instance"]["state"], "running");
    assert_eq!(
        inspected["instance"]["storyContext"]["storyId"],
        "story_support_01"
    );
    assert_eq!(
        inspected["instance"]["tenantScope"]["tenantId"],
        "tenant_01"
    );
    assert_eq!(inspected["instance"]["initialStepId"], initial_step_id);
    assert_eq!(inspected["instance"]["steps"][0]["stepId"], initial_step_id);
    assert_eq!(inspected["instance"]["createdAt"], created_at);
    assert_eq!(inspected["instance"]["updatedAt"], created_at);

    let newer_response = restarted_app
        .clone()
        .oneshot(start_request("v2"))
        .await
        .unwrap();
    assert_eq!(newer_response.status(), StatusCode::CREATED);
    let newer = json_body(newer_response).await;
    assert_eq!(newer["instance"]["definition"]["version"], "v2");
    assert_ne!(newer["instance"]["instanceId"], instance_id);
    assert_eq!(inspected["instance"]["definition"]["version"], "v1");

    let unknown_response = restarted_app.oneshot(start_request("v3")).await.unwrap();
    assert_eq!(unknown_response.status(), StatusCode::NOT_FOUND);
    let unknown = json_body(unknown_response).await;
    assert_eq!(unknown["code"], "workflow_definition_version_not_found");
    assert_eq!(
        unknown["next_actions"][0],
        "select_registered_workflow_version"
    );

    let persisted_instances: i64 = sqlx::query_scalar(
        "select count(*) from platform.service_workflow_instances where service_id = 'support-sla'",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let persisted_steps: i64 =
        sqlx::query_scalar("select count(*) from platform.service_workflow_steps")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(persisted_instances, 2);
    assert_eq!(persisted_steps, 2);

    db.cleanup().await;
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn support_event_advances_workflow_and_outbox_atomically_across_services() {
    let Some(support_db) = TestDatabase::create().await else {
        return;
    };
    let Some(sla_db) = TestDatabase::create().await else {
        support_db.cleanup().await;
        return;
    };
    let Some(transport_db) = TestDatabase::create().await else {
        support_db.cleanup().await;
        sla_db.cleanup().await;
        return;
    };

    let support_state = prepare_runtime(
        &support_service(),
        &ServiceRuntimeConfig::new("support", "primary", "support"),
        support_db.pool.clone(),
        &[platform_core::Migration {
            name: "support-ticket/0001_create_sla_acknowledgements",
            sql: r"
                create table support_ticket_sla_acknowledgements (
                    ticket_id text primary key,
                    source_event_id text not null,
                    envelope jsonb not null
                );
            ",
        }],
    )
    .await
    .unwrap();
    let manifest = manifest();
    let sla_state = prepare_runtime(
        &service(),
        &runtime_config(&manifest),
        sla_db.pool.clone(),
        &[],
    )
    .await
    .unwrap();
    let adapter = LocalTransportAdapter::prepare(transport_db.pool.clone())
        .await
        .unwrap();

    let rollback_source = support_ticket_opened("support-event-rollback", "ticket_rollback");
    let mut rollback = sla_db.pool.begin().await.unwrap();
    let rollback_instance = start_workflow_from_event_in_tx(
        &sla_state,
        &mut rollback,
        "support-sla",
        "ticket_sla",
        "v1",
        &rollback_source,
    )
    .await
    .unwrap();
    let rollback_transition = advance_workflow_step_with_event_in_tx(
        &sla_state,
        &mut rollback,
        &rollback_instance.instance_id,
        &rollback_instance.initial_step_id,
        "support-event-rollback:acknowledge_ticket",
        acknowledgement_publication(
            &rollback_instance.instance_id,
            &rollback_instance.initial_step_id,
            &rollback_source,
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        rollback_transition.disposition,
        WorkflowTransitionDisposition::Applied
    );
    rollback.rollback().await.unwrap();
    let rolled_back_instances: i64 = sqlx::query_scalar(
        "select count(*) from platform.service_workflow_instances where start_trigger_id = 'support-event-rollback'",
    )
    .fetch_one(&sla_db.pool)
    .await
    .unwrap();
    let rolled_back_outbox: i64 = sqlx::query_scalar(
        "select count(*) from platform.service_event_outbox where event_id = 'sla-acknowledged-support-event-rollback'",
    )
    .fetch_one(&sla_db.pool)
    .await
    .unwrap();
    assert_eq!((rolled_back_instances, rolled_back_outbox), (0, 0));

    let source = support_ticket_opened("support-event-workflow", "ticket_workflow");
    let mut producer_transaction = support_db.pool.begin().await.unwrap();
    ServiceEventPublisher
        .publish_in_tx(&mut producer_transaction, "support-sla", &source)
        .await
        .unwrap();
    producer_transaction.commit().await.unwrap();
    assert_eq!(
        relay_service_events_once(&support_state, &adapter, 10)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        consume_service_events_once_without_workload_identity(
            &sla_state,
            &adapter,
            "support-sla",
            &SupportSlaWorkflowHandler {
                state: sla_state.clone(),
            },
            10,
        )
        .await
        .unwrap(),
        1
    );

    let (instance_id, initial_step_id): (String, String) = sqlx::query_as(
        r"
        select instance_id, initial_step_id
        from platform.service_workflow_instances
        where start_trigger_kind = 'event' and start_trigger_id = 'support-event-workflow'
        ",
    )
    .fetch_one(&sla_db.pool)
    .await
    .unwrap();
    let mut duplicate_command = sla_db.pool.begin().await.unwrap();
    let duplicate_transition = advance_workflow_step_with_event_in_tx(
        &sla_state,
        &mut duplicate_command,
        &instance_id,
        &initial_step_id,
        "support-event-workflow:acknowledge_ticket",
        acknowledgement_publication(&instance_id, &initial_step_id, &source),
    )
    .await
    .unwrap();
    assert_eq!(
        duplicate_transition.disposition,
        WorkflowTransitionDisposition::Duplicate
    );
    duplicate_command.commit().await.unwrap();

    let workflow_outbox_count: i64 = sqlx::query_scalar(
        "select count(*) from platform.service_event_outbox where event_id = 'sla-acknowledged-support-event-workflow'",
    )
    .fetch_one(&sla_db.pool)
    .await
    .unwrap();
    assert_eq!(workflow_outbox_count, 1);
    assert_eq!(
        relay_service_events_once(&sla_state, &adapter, 10)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        consume_service_events_once_without_workload_identity(
            &support_state,
            &adapter,
            "support",
            &SupportTicketAcknowledgementHandler,
            10,
        )
        .await
        .unwrap(),
        1
    );

    let persisted_envelope: serde_json::Value = sqlx::query_scalar(
        "select envelope from support_ticket_sla_acknowledgements where ticket_id = 'ticket_workflow'",
    )
    .fetch_one(&support_db.pool)
    .await
    .unwrap();
    let persisted_envelope: EventEnvelope = serde_json::from_value(persisted_envelope).unwrap();
    assert_eq!(persisted_envelope.producer_service_id, "support-sla");
    assert_eq!(persisted_envelope.module_id, "support-sla");
    assert_eq!(persisted_envelope.contract_id, "sla-acknowledged");
    assert_eq!(persisted_envelope.contract_version, "v1");
    assert_eq!(persisted_envelope.context.story, source.context.story);
    assert_eq!(
        persisted_envelope.context.delegated_actor,
        source.context.delegated_actor
    );
    assert_eq!(persisted_envelope.context.tenant, source.context.tenant);
    assert_eq!(persisted_envelope.context.deadline, source.context.deadline);
    assert_eq!(
        persisted_envelope.context.idempotency_key,
        source.context.idempotency_key
    );
    assert_eq!(
        persisted_envelope
            .context
            .service_principal
            .as_ref()
            .unwrap()
            .subject,
        "spiffe://example.com/service/support-sla"
    );
    assert_eq!(
        persisted_envelope
            .context
            .causation
            .as_ref()
            .unwrap()
            .causation_id,
        initial_step_id
    );
    assert_eq!(
        persisted_envelope
            .context
            .causation
            .as_ref()
            .unwrap()
            .correlation_id,
        source.context.causation.as_ref().unwrap().correlation_id
    );

    let app = service_router(OpenApiRouter::new(), sla_state.clone());
    let inspected = app
        .clone()
        .oneshot(
            Request::get(format!("/runtime/workflows/instances/{instance_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(inspected.status(), StatusCode::OK);
    let inspected = json_body(inspected).await;
    assert_eq!(inspected["instance"]["steps"].as_array().unwrap().len(), 2);
    assert_eq!(inspected["instance"]["steps"][0]["state"], "completed");
    assert_eq!(
        inspected["instance"]["steps"][0]["transitionId"],
        "support-event-workflow:acknowledge_ticket"
    );
    assert_eq!(
        inspected["instance"]["steps"][0]["outgoingWork"]["kind"],
        "event_contract"
    );
    assert_eq!(
        inspected["instance"]["steps"][0]["outgoingWork"]["contractId"],
        "sla-acknowledged"
    );
    assert_eq!(inspected["instance"]["steps"][1]["state"], "pending");
    assert_eq!(
        inspected["instance"]["steps"][1]["definitionStepName"],
        "await_resolution"
    );

    let delivery_evidence = app
        .oneshot(
            Request::get("/runtime/event-deliveries")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delivery_evidence.status(), StatusCode::OK);
    let delivery_evidence = json_body(delivery_evidence).await;
    assert!(delivery_evidence.as_array().unwrap().iter().any(|entry| {
        entry["eventId"] == "sla-acknowledged-support-event-workflow"
            && entry["stage"] == "outbox"
            && entry["outcome"] == "published"
    }));

    adapter
        .publish(TransportPublication {
            consumer_id: "support-sla".to_owned(),
            envelope: source,
        })
        .await
        .unwrap();
    assert_eq!(
        consume_service_events_once_without_workload_identity(
            &sla_state,
            &adapter,
            "support-sla",
            &SupportSlaWorkflowHandler {
                state: sla_state.clone(),
            },
            10,
        )
        .await
        .unwrap(),
        0
    );
    let duplicate_evidence: i64 = sqlx::query_scalar(
        r"
        select count(*) from platform.service_event_delivery_evidence
        where event_id = 'support-event-workflow' and stage = 'inbox' and outcome = 'duplicate'
        ",
    )
    .fetch_one(&sla_db.pool)
    .await
    .unwrap();
    let instance_count: i64 = sqlx::query_scalar(
        "select count(*) from platform.service_workflow_instances where start_trigger_id = 'support-event-workflow'",
    )
    .fetch_one(&sla_db.pool)
    .await
    .unwrap();
    let acknowledgement_count: i64 = sqlx::query_scalar(
        "select count(*) from support_ticket_sla_acknowledgements where ticket_id = 'ticket_workflow'",
    )
    .fetch_one(&support_db.pool)
    .await
    .unwrap();
    assert_eq!(
        (duplicate_evidence, instance_count, acknowledgement_count),
        (1, 1, 1)
    );

    support_db.cleanup().await;
    sla_db.cleanup().await;
    transport_db.cleanup().await;
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn retries_and_timers_recover_after_restart_with_controlled_time() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let initial_time = DateTime::parse_from_rfc3339("2026-07-16T16:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let clock = Arc::new(SystemSandboxWorkflowClock::new(initial_time));
    let manifest = manifest();
    let module_migrations = [platform_core::Migration {
        name: "support-sla/0001_create_workflow_retry_effects",
        sql: r"
            create table support_sla_workflow_retry_effects (
                step_id text primary key,
                transition_id text not null
            );
        ",
    }];
    let config = || runtime_config(&manifest);
    let mut state = prepare_runtime(&service(), &config(), db.pool.clone(), &module_migrations)
        .await
        .unwrap()
        .with_workflow_clock(Arc::clone(&clock) as Arc<dyn platform_core::Clock>);
    let source = support_ticket_opened("support-event-recovery", "ticket_recovery");
    let mut start = db.pool.begin().await.unwrap();
    let instance = start_workflow_from_event_in_tx(
        &state,
        &mut start,
        "support-sla",
        "ticket_sla",
        "v1",
        &source,
    )
    .await
    .unwrap();
    start.commit().await.unwrap();
    let step_id = instance.initial_step_id.clone();

    let initial_failure = record_workflow_step_failure_at(
        &state,
        &instance.instance_id,
        &step_id,
        "support-event-recovery:attempt:1",
        WorkflowStepFailure::retryable(
            "dependency_unavailable",
            "support dependency is temporarily unavailable",
        ),
        initial_time,
    )
    .await
    .unwrap();
    assert_eq!(
        initial_failure.disposition,
        WorkflowFailureDisposition::RetryScheduled
    );
    assert_eq!(initial_failure.attempt_number, 1);
    assert_eq!(
        initial_failure.next_attempt_at,
        Some(initial_time + Duration::seconds(1))
    );
    let duplicate_initial_failure = record_workflow_step_failure_at(
        &state,
        &instance.instance_id,
        &step_id,
        "support-event-recovery:attempt:1",
        WorkflowStepFailure::permanent("changed_replay", "duplicate replay changed its failure"),
        initial_time,
    )
    .await
    .unwrap();
    assert_eq!(
        duplicate_initial_failure.disposition,
        WorkflowFailureDisposition::Duplicate
    );
    assert_eq!(
        duplicate_initial_failure.classification,
        WorkflowFailureClassification::Retryable
    );

    let mut premature_retry = db.pool.begin().await.unwrap();
    let premature_retry_error = advance_workflow_step_with_event_in_tx(
        &state,
        &mut premature_retry,
        &instance.instance_id,
        &step_id,
        "support-event-recovery:attempt:2-without-claim",
        acknowledgement_publication(&instance.instance_id, &step_id, &source),
    )
    .await
    .unwrap_err();
    premature_retry.rollback().await.unwrap();
    assert_eq!(
        premature_retry_error.code,
        WorkflowErrorCode::TransitionConflict
    );

    drop(state);
    state = prepare_runtime(&service(), &config(), db.pool.clone(), &module_migrations)
        .await
        .unwrap()
        .with_workflow_clock(Arc::clone(&clock) as Arc<dyn platform_core::Clock>);
    let retry_time = clock.advance(Duration::seconds(1));
    let retry_claims = claim_due_workflow_work_at(
        &state,
        "support-sla-worker-before-restart",
        retry_time,
        Duration::seconds(5),
        10,
    )
    .await
    .unwrap();
    assert_eq!(retry_claims.len(), 1);
    let abandoned_retry = retry_claims[0].clone();
    assert_eq!(abandoned_retry.kind, WorkflowTimerKind::Retry);
    assert_eq!(abandoned_retry.step_id, step_id);
    assert_eq!(abandoned_retry.attempt_number, 2);

    drop(state);
    state = prepare_runtime(&service(), &config(), db.pool.clone(), &module_migrations)
        .await
        .unwrap()
        .with_workflow_clock(Arc::clone(&clock) as Arc<dyn platform_core::Clock>);
    let timeout_time = clock.advance(Duration::seconds(6));
    let timeout_claims = claim_due_workflow_work_at(
        &state,
        "support-sla-worker-timeout-before-restart",
        timeout_time,
        Duration::seconds(5),
        10,
    )
    .await
    .unwrap();
    assert_eq!(timeout_claims.len(), 1);
    let abandoned_timeout = timeout_claims[0].clone();
    assert_eq!(abandoned_timeout.kind, WorkflowTimerKind::StepTimeout);
    assert_eq!(abandoned_timeout.attempt_number, 2);
    assert_eq!(
        abandoned_timeout.due_at,
        abandoned_retry.claimed_at + Duration::seconds(5)
    );

    drop(state);
    state = prepare_runtime(&service(), &config(), db.pool.clone(), &module_migrations)
        .await
        .unwrap()
        .with_workflow_clock(Arc::clone(&clock) as Arc<dyn platform_core::Clock>);
    let reclaimed_time = clock.advance(Duration::seconds(6));
    let reclaimed_claims = claim_due_workflow_work_at(
        &state,
        "support-sla-worker-after-restart",
        reclaimed_time,
        Duration::seconds(5),
        10,
    )
    .await
    .unwrap();
    assert_eq!(reclaimed_claims.len(), 1);
    let reclaimed_timeout = reclaimed_claims[0].clone();
    assert_eq!(reclaimed_timeout.timer_id, abandoned_timeout.timer_id);
    assert_eq!(
        reclaimed_timeout.transition_id,
        abandoned_timeout.transition_id
    );
    let timeout_failure = record_claimed_workflow_step_failure_at(
        &state,
        &reclaimed_timeout,
        WorkflowStepFailure::timeout("step_timeout", "workflow step exceeded its timeout"),
        reclaimed_time,
    )
    .await
    .unwrap();
    assert_eq!(
        timeout_failure.disposition,
        WorkflowFailureDisposition::RetryScheduled
    );
    assert_eq!(
        timeout_failure.classification,
        WorkflowFailureClassification::Timeout
    );
    assert_eq!(timeout_failure.attempt_number, 2);
    assert_eq!(
        timeout_failure.next_attempt_at,
        Some(reclaimed_time + Duration::seconds(2))
    );

    let final_retry_time = clock.advance(Duration::seconds(2));
    let final_retry_claims = claim_due_workflow_work_at(
        &state,
        "support-sla-worker-success",
        final_retry_time,
        Duration::seconds(5),
        10,
    )
    .await
    .unwrap();
    assert_eq!(final_retry_claims.len(), 1);
    let final_retry = final_retry_claims[0].clone();
    assert_eq!(final_retry.kind, WorkflowTimerKind::Retry);
    assert_eq!(final_retry.attempt_number, 3);

    let mut success = db.pool.begin().await.unwrap();
    let inserted_effect = sqlx::query(
        r"
        insert into support_sla_workflow_retry_effects (step_id, transition_id)
        values ($1, $2)
        on conflict (step_id) do nothing
        ",
    )
    .bind(&step_id)
    .bind(&final_retry.attempt_transition_id)
    .execute(&mut *success)
    .await
    .unwrap();
    let completed = advance_claimed_workflow_retry_with_event_in_tx(
        &state,
        &mut success,
        &final_retry,
        acknowledgement_publication(&instance.instance_id, &step_id, &source),
    )
    .await
    .unwrap();
    assert_eq!(inserted_effect.rows_affected(), 1);
    assert_eq!(
        completed.disposition,
        WorkflowTransitionDisposition::Applied
    );
    success.commit().await.unwrap();

    let mut duplicate = db.pool.begin().await.unwrap();
    let duplicate_effect = sqlx::query(
        r"
        insert into support_sla_workflow_retry_effects (step_id, transition_id)
        values ($1, $2)
        on conflict (step_id) do nothing
        ",
    )
    .bind(&step_id)
    .bind(&final_retry.attempt_transition_id)
    .execute(&mut *duplicate)
    .await
    .unwrap();
    let duplicate_completion = advance_claimed_workflow_retry_with_event_in_tx(
        &state,
        &mut duplicate,
        &final_retry,
        acknowledgement_publication(&instance.instance_id, &step_id, &source),
    )
    .await
    .unwrap();
    duplicate.commit().await.unwrap();
    assert_eq!(duplicate_effect.rows_affected(), 0);
    assert_eq!(
        duplicate_completion.disposition,
        WorkflowTransitionDisposition::Duplicate
    );
    let effect_count: i64 =
        sqlx::query_scalar("select count(*) from support_sla_workflow_retry_effects")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    let outgoing_count: i64 = sqlx::query_scalar(
        "select count(*) from platform.service_event_outbox where event_id = 'sla-acknowledged-support-event-recovery'",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!((effect_count, outgoing_count), (1, 1));

    let app = service_router(OpenApiRouter::new(), state.clone());
    let inspected = app
        .oneshot(
            Request::get(format!(
                "/runtime/workflows/instances/{}",
                instance.instance_id
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    let inspected = json_body(inspected).await;
    let recovered_step = &inspected["instance"]["steps"][0];
    assert_eq!(recovered_step["stepId"], step_id);
    assert_eq!(recovered_step["state"], "completed");
    assert_eq!(recovered_step["attemptCount"], 3);
    assert_eq!(recovered_step["attempts"].as_array().unwrap().len(), 3);
    assert_eq!(
        recovered_step["attempts"][0]["failure"]["classification"],
        "retryable"
    );
    assert_eq!(
        recovered_step["attempts"][1]["failure"]["classification"],
        "timeout"
    );
    assert_eq!(recovered_step["attempts"][2]["state"], "succeeded");
    assert!(
        recovered_step["timers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|timer| {
                timer["timerId"] == abandoned_timeout.timer_id
                    && timer["transitionId"] == abandoned_timeout.transition_id
                    && timer["state"] == "completed"
            })
    );

    let exhaustion_start = clock.advance(Duration::seconds(1));
    let exhaustion_source = support_ticket_opened("support-event-exhaustion", "ticket_exhaustion");
    let mut exhaustion_start_tx = db.pool.begin().await.unwrap();
    let exhausted_instance = start_workflow_from_event_in_tx(
        &state,
        &mut exhaustion_start_tx,
        "support-sla",
        "ticket_sla",
        "v1",
        &exhaustion_source,
    )
    .await
    .unwrap();
    exhaustion_start_tx.commit().await.unwrap();
    let exhaustion_timeout_time = clock.advance(Duration::seconds(5));
    let first_timeout = claim_due_workflow_work_at(
        &state,
        "support-sla-worker-exhaustion-timeout",
        exhaustion_timeout_time,
        Duration::seconds(5),
        10,
    )
    .await
    .unwrap()
    .remove(0);
    assert_eq!(first_timeout.kind, WorkflowTimerKind::StepTimeout);
    assert_eq!(
        first_timeout.due_at,
        exhaustion_start + Duration::seconds(5)
    );
    let first_timeout_result = record_claimed_workflow_step_failure_at(
        &state,
        &first_timeout,
        WorkflowStepFailure::timeout("step_timeout", "workflow step exceeded its timeout"),
        exhaustion_timeout_time,
    )
    .await
    .unwrap();
    assert_eq!(
        first_timeout_result.classification,
        timeout_failure.classification
    );

    let exhaustion_retry_two_time = clock.advance(Duration::seconds(1));
    let retry_two = claim_due_workflow_work_at(
        &state,
        "support-sla-worker-exhaustion-2",
        exhaustion_retry_two_time,
        Duration::seconds(5),
        10,
    )
    .await
    .unwrap()
    .remove(0);
    let retry_two_failure = record_claimed_workflow_step_failure_at(
        &state,
        &retry_two,
        WorkflowStepFailure::retryable("dependency_unavailable", "dependency remains unavailable"),
        exhaustion_retry_two_time,
    )
    .await
    .unwrap();
    assert_eq!(
        retry_two_failure.disposition,
        WorkflowFailureDisposition::RetryScheduled
    );

    let exhaustion_retry_three_time = clock.advance(Duration::seconds(2));
    let retry_three = claim_due_workflow_work_at(
        &state,
        "support-sla-worker-exhaustion-3",
        exhaustion_retry_three_time,
        Duration::seconds(5),
        10,
    )
    .await
    .unwrap()
    .remove(0);
    let exhaustion = record_claimed_workflow_step_failure_at(
        &state,
        &retry_three,
        WorkflowStepFailure::retryable("dependency_unavailable", "retry budget exhausted"),
        exhaustion_retry_three_time,
    )
    .await
    .unwrap();
    assert_eq!(
        exhaustion.disposition,
        WorkflowFailureDisposition::Exhausted
    );
    assert!(exhaustion.terminal_exhausted);
    assert_eq!(exhaustion.attempt_count, 3);
    assert_eq!(exhaustion.step_id, exhausted_instance.initial_step_id);
    let exhausted_state: (String, String, i32, Option<DateTime<Utc>>) = sqlx::query_as(
        r"
        select instance.state, step.state, step.attempt_count, step.next_attempt_at
        from platform.service_workflow_instances instance
        join platform.service_workflow_steps step
          on step.instance_id = instance.instance_id
        where instance.instance_id = $1 and step.step_id = $2
        ",
    )
    .bind(&exhausted_instance.instance_id)
    .bind(&exhausted_instance.initial_step_id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(
        exhausted_state,
        ("failed".to_owned(), "exhausted".to_owned(), 3, None)
    );

    db.cleanup().await;
}
