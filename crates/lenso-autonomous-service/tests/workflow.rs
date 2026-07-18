use async_trait::async_trait;
use axum::body::Body;
use chrono::{DateTime, Duration, Utc};
use http::{Request, StatusCode, header};
use http_body_util::BodyExt as _;
use lenso_autonomous_service::{
    LocalTransportAdapter, SERVICE_RUNTIME_MIGRATIONS, ServiceEventHandler,
    ServiceEventHandlerError, ServiceEventPublisher, ServiceRuntimeConfig, ServiceRuntimeState,
    SystemSandboxWorkflowClock, TransportAdapter, TransportPublication, WorkflowAuthorityGrant,
    WorkflowAuthorityRequest, WorkflowAuthorityVerifier, WorkflowChildStartRequest,
    WorkflowChildStartResult, WorkflowErrorCode, WorkflowEventPublication,
    WorkflowFailureClassification, WorkflowFailureDisposition, WorkflowFailureEvidence,
    WorkflowInstance, WorkflowStepFailure, WorkflowTimerKind, WorkflowTransitionDisposition,
    advance_claimed_workflow_retry_with_event_in_tx, advance_workflow_step_with_event_in_tx,
    claim_due_workflow_work_at, complete_workflow_compensation_from_event_in_tx,
    consume_service_events_once_without_workload_identity,
    dispatch_workflow_compensation_with_event_in_tx, fail_workflow_in_tx, prepare_runtime,
    record_claimed_workflow_step_failure_at, record_workflow_compensation_failure_at,
    record_workflow_step_failure_at, relay_service_events_once, resume_parent_from_child_in_tx,
    select_workflow_compensations_after_timeout_at, service_router, start_child_workflow_in_tx,
    start_workflow_from_event_in_tx,
};
use lenso_contracts::{
    ModuleManifest, RuntimeSurface, WorkflowCompensationDeclaration, WorkflowDataContract,
    WorkflowDefinition, WorkflowRetryPolicyDeclaration, WorkflowStepDeclaration,
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
    let mut compensation_request = EventContractArtifact::new(
        "sla-compensation-requested",
        "support-sla",
        "v1",
        ServiceTenancyMode::Required,
        EventArtifactReference::new(
            EventArtifactFormat::JsonSchema,
            "contracts/events/support/support.sla-compensation-requested.v1.schema.json",
        ),
    );
    compensation_request.context = acknowledgement.context.clone();
    service.event_contracts = vec![acknowledgement, compensation_request];
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
    let mut compensation_completed = EventContractArtifact::new(
        "sla-compensated",
        "support-ticket",
        "v1",
        ServiceTenancyMode::Required,
        EventArtifactReference::new(
            EventArtifactFormat::JsonSchema,
            "contracts/events/support/support.sla-compensated.v1.schema.json",
        ),
    );
    compensation_completed.context = ContractContextRequirements::new(vec![
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
    service.event_contracts = vec![compensation_completed];
    service
}

fn manifest() -> ModuleManifest {
    ModuleManifest::builder("support-sla")
        .runtime(RuntimeSurface {
            functions: vec![],
            schedules: vec![],
            workflows: vec![
                workflow("v1"),
                workflow("v2"),
                compensation_workflow("v1"),
                child_workflow("v1"),
                child_workflow("v2"),
            ],
        })
        .build()
}

fn child_workflow(version: &str) -> WorkflowDefinition {
    WorkflowDefinition::new(
        "support-sla",
        "ticket_escalation",
        version,
        WorkflowDataContract::new("support.escalation.start", "v1"),
        WorkflowDataContract::new("support.escalation.result", "v1"),
        vec![WorkflowStepDeclaration::new("notify_on_call")],
    )
}

fn manifest_without_child_v1() -> ModuleManifest {
    ModuleManifest::builder("support-sla")
        .runtime(RuntimeSurface {
            functions: vec![],
            schedules: vec![],
            workflows: vec![
                workflow("v1"),
                workflow("v2"),
                compensation_workflow("v1"),
                child_workflow("v2"),
            ],
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

fn compensation_workflow(version: &str) -> WorkflowDefinition {
    WorkflowDefinition::new(
        "support-sla",
        "ticket_sla_compensation",
        version,
        WorkflowDataContract::new("support.sla.start", "v1"),
        WorkflowDataContract::new("support.sla.result", "v1"),
        vec![
            WorkflowStepDeclaration::new("acknowledge_ticket").with_compensation(
                WorkflowCompensationDeclaration::new(
                    "withdraw_sla_acknowledgement",
                    2,
                    WorkflowDataContract::new("sla-compensation-requested", "v1"),
                )
                .with_completion_contract(WorkflowDataContract::new("sla-compensated", "v1")),
            ),
            WorkflowStepDeclaration::new("reserve_on_call").with_compensation(
                WorkflowCompensationDeclaration::new(
                    "release_on_call",
                    1,
                    WorkflowDataContract::new("sla-compensation-requested", "v1"),
                )
                .with_completion_contract(WorkflowDataContract::new("sla-compensated", "v1")),
            ),
            WorkflowStepDeclaration::new("await_resolution").with_timeout_ms(5_000),
        ],
    )
}

fn runtime_config(manifest: &ModuleManifest) -> ServiceRuntimeConfig {
    ServiceRuntimeConfig::new("support-sla", "primary", "support-sla")
        .with_module_manifests(std::slice::from_ref(manifest))
}

#[derive(Debug)]
struct TestWorkflowAuthorityVerifier;

impl WorkflowAuthorityVerifier for TestWorkflowAuthorityVerifier {
    fn verify(
        &self,
        request: &WorkflowAuthorityRequest,
        credential: &str,
    ) -> Result<WorkflowAuthorityGrant, String> {
        if credential != "approved-workflow-control" {
            return Err("Workflow operator credential is not authorized".to_owned());
        }
        Ok(WorkflowAuthorityGrant {
            actor_id: "operator:incident-commander".to_owned(),
            authority_id: format!(
                "approved:{}:{}",
                request.required_authority, request.plan_id
            ),
        })
    }
}

fn runtime_config_with_workflow_authority(manifest: &ModuleManifest) -> ServiceRuntimeConfig {
    runtime_config(manifest)
        .with_workflow_authority_verifier(Arc::new(TestWorkflowAuthorityVerifier))
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

#[derive(Debug, Clone, Copy)]
struct SupportTicketSlaHandler;

#[async_trait]
impl ServiceEventHandler for SupportTicketSlaHandler {
    async fn handle(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        envelope: &EventEnvelope,
    ) -> Result<(), ServiceEventHandlerError> {
        if envelope.contract_id == "sla-acknowledged" {
            let Some(effect_id) = envelope.content.data["effectId"].as_str() else {
                return Ok(());
            };
            sqlx::query(
                r#"
                insert into support_ticket_sla_effects (
                    effect_id, ticket_id, compensation_action, source_event_id, active
                ) values ($1, $2, $3, $4, true)
                on conflict (effect_id) do nothing
                "#,
            )
            .bind(effect_id)
            .bind(envelope.content.data["ticketId"].as_str().unwrap())
            .bind(
                envelope.content.data["compensationAction"]
                    .as_str()
                    .unwrap(),
            )
            .bind(&envelope.event_id)
            .execute(&mut **transaction)
            .await
            .map_err(ServiceEventHandlerError::store)?;
            return Ok(());
        }
        if envelope.contract_id != "sla-compensation-requested" {
            return Ok(());
        }
        let compensation_id = envelope.content.data["compensationId"].as_str().unwrap();
        let effect_id = envelope.content.data["effectId"].as_str().unwrap();
        let action = envelope.content.data["action"].as_str().unwrap();
        let reversed = sqlx::query(
            r#"
            update support_ticket_sla_effects
            set active = false, compensated_by = $2
            where effect_id = $1 and compensation_action = $3 and active = true
            "#,
        )
        .bind(effect_id)
        .bind(compensation_id)
        .bind(action)
        .execute(&mut **transaction)
        .await
        .map_err(ServiceEventHandlerError::store)?;
        if reversed.rows_affected() != 1 {
            return Err(ServiceEventHandlerError::rejected_with_code(
                "compensation_effect_not_active",
                format!("Effect `{effect_id}` is not active for compensation `{compensation_id}`"),
            ));
        }
        sqlx::query(
            r#"
            insert into support_ticket_sla_compensations (
                compensation_id, effect_id, action, envelope
            ) values ($1, $2, $3, $4)
            on conflict (compensation_id) do nothing
            "#,
        )
        .bind(compensation_id)
        .bind(effect_id)
        .bind(action)
        .bind(serde_json::to_value(envelope).unwrap())
        .execute(&mut **transaction)
        .await
        .map_err(ServiceEventHandlerError::store)?;
        let mut completed = envelope.clone();
        completed.event_id = format!("{compensation_id}:completed");
        completed.event_type = "support.sla-compensated.v1".to_owned();
        completed.contract_id = "sla-compensated".to_owned();
        completed.producer_service_id = "support".to_owned();
        completed.module_id = "support-ticket".to_owned();
        completed.content.schema =
            "contracts/events/support/support.sla-compensated.v1.schema.json".to_owned();
        let principal = completed.context.service_principal.as_mut().unwrap();
        principal.subject = "spiffe://example.com/service/support".to_owned();
        principal.audiences = vec!["support-sla".to_owned()];
        principal.credential_id = "credential_support_01".to_owned();
        completed.context.causation = Some(lenso_service::CausationContext {
            causation_id: envelope.event_id.clone(),
            correlation_id: envelope
                .context
                .causation
                .as_ref()
                .and_then(|causation| causation.correlation_id.clone()),
        });
        ServiceEventPublisher
            .publish_in_tx(transaction, "support-sla", &completed)
            .await
            .map_err(|error| {
                ServiceEventHandlerError::retryable(
                    "compensation_completion_publish_failed",
                    error.message,
                )
            })?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct SupportSlaCompensationCompletedHandler {
    state: ServiceRuntimeState,
}

#[async_trait]
impl ServiceEventHandler for SupportSlaCompensationCompletedHandler {
    async fn handle(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        envelope: &EventEnvelope,
    ) -> Result<(), ServiceEventHandlerError> {
        complete_workflow_compensation_from_event_in_tx(&self.state, transaction, envelope)
            .await
            .map_err(|error| {
                ServiceEventHandlerError::retryable(error.code.as_str(), error.message)
            })?;
        Ok(())
    }
}

fn support_effect_publication(
    instance_id: &str,
    step_id: &str,
    compensation_action: &str,
    source: &EventEnvelope,
) -> WorkflowEventPublication {
    WorkflowEventPublication::new(
        "support",
        format!("{step_id}:effect:event"),
        "sla-acknowledged",
        "v1",
        "2026-07-17T01:00:00Z",
        support_sla_principal(source),
        serde_json::json!({
            "ticketId": source.content.data["ticketId"],
            "workflowInstanceId": instance_id,
            "workflowStepId": step_id,
            "effectId": format!("{step_id}:effect"),
            "compensationAction": compensation_action,
        }),
    )
}

fn compensation_request_publication(
    instance_id: &str,
    compensation_id: &str,
    source: &EventEnvelope,
) -> WorkflowEventPublication {
    WorkflowEventPublication::new(
        "support",
        format!("{compensation_id}:request"),
        "sla-compensation-requested",
        "v1",
        "2026-07-17T01:00:05Z",
        support_sla_principal(source),
        serde_json::json!({
            "ticketId": source.content.data["ticketId"],
            "workflowInstanceId": format!("caller-controlled-{instance_id}"),
            "compensationId": format!("caller-controlled-{compensation_id}"),
            "effectId": "caller-controlled-effect",
            "action": "caller-controlled-action",
        }),
    )
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

fn migration_dry_run_request(from_version: &str, target_version: &str) -> Request<Body> {
    Request::post("/runtime/workflows/support-sla/ticket_sla/migration-plans/dry-run")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({
                "fromVersion": from_version,
                "targetVersion": target_version
            })
            .to_string(),
        ))
        .unwrap()
}

fn workflow_operator_plan_request(
    instance_id: &str,
    action: &str,
    selected_step_id: Option<&str>,
) -> Request<Body> {
    Request::post(format!(
        "/runtime/workflows/instances/{instance_id}/operator-actions/{action}/dry-run"
    ))
    .header(header::CONTENT_TYPE, "application/json")
    .body(Body::from(
        serde_json::json!({"selectedStepId": selected_step_id}).to_string(),
    ))
    .unwrap()
}

fn workflow_operator_apply_request(
    instance_id: &str,
    action: &str,
    selected_step_id: Option<&str>,
    plan_id: &str,
    credential: Option<&str>,
) -> Request<Body> {
    let mut request = Request::post(format!(
        "/runtime/workflows/instances/{instance_id}/operator-actions/{action}"
    ))
    .header(header::CONTENT_TYPE, "application/json")
    .body(Body::from(
        serde_json::json!({
            "planId": plan_id,
            "selectedStepId": selected_step_id,
            "reason": "Contain and recover the support SLA incident"
        })
        .to_string(),
    ))
    .unwrap();
    if let Some(credential) = credential {
        request.headers_mut().insert(
            header::AUTHORIZATION,
            format!("Bearer {credential}").parse().unwrap(),
        );
    }
    request
}

async fn json_body(response: axum::response::Response) -> serde_json::Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

async fn force_cleanup_test_databases(databases: Vec<TestDatabase>) {
    let names = databases
        .iter()
        .map(|database| {
            database
                .url
                .rsplit('/')
                .next()
                .unwrap()
                .split('?')
                .next()
                .unwrap()
                .to_owned()
        })
        .collect::<Vec<_>>();
    let admin_url = std::env::var("DATABASE_URL").unwrap();
    let admin_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&admin_url)
        .await
        .unwrap();
    for name in names {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            r#"drop database if exists "{name}" with (force)"#
        )))
        .execute(&admin_pool)
        .await
        .unwrap();
    }
    admin_pool.close().await;
    drop(databases);
}

async fn start_parent_and_child(
    state: &ServiceRuntimeState,
    pool: &sqlx::PgPool,
    source: &EventEnvelope,
    child_version: &str,
) -> (WorkflowInstance, WorkflowChildStartResult) {
    let mut transaction = pool.begin().await.unwrap();
    let parent = start_workflow_from_event_in_tx(
        state,
        &mut transaction,
        "support-sla",
        "ticket_sla",
        "v1",
        source,
    )
    .await
    .unwrap();
    let child = start_child_workflow_in_tx(
        state,
        &mut transaction,
        &parent.instance_id,
        &parent.initial_step_id,
        &WorkflowChildStartRequest {
            start_id: format!("{}:ticket_escalation", source.event_id),
            definition_owner: "support-sla".to_owned(),
            definition_name: "ticket_escalation".to_owned(),
            definition_version: child_version.to_owned(),
            input: serde_json::json!({"ticketId": source.content.data["ticketId"]}),
        },
    )
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    (parent, child)
}

async fn complete_compensatable_support_effects(
    state: &ServiceRuntimeState,
    pool: &sqlx::PgPool,
    source: &EventEnvelope,
) -> (String, String) {
    let mut start = pool.begin().await.unwrap();
    let instance = start_workflow_from_event_in_tx(
        state,
        &mut start,
        "support-sla",
        "ticket_sla_compensation",
        "v1",
        source,
    )
    .await
    .unwrap();
    start.commit().await.unwrap();

    let mut acknowledge = pool.begin().await.unwrap();
    let acknowledged = advance_workflow_step_with_event_in_tx(
        state,
        &mut acknowledge,
        &instance.instance_id,
        &instance.initial_step_id,
        &format!("{}:acknowledge_ticket", source.event_id),
        support_effect_publication(
            &instance.instance_id,
            &instance.initial_step_id,
            "withdraw_sla_acknowledgement",
            source,
        ),
    )
    .await
    .unwrap();
    acknowledge.commit().await.unwrap();
    let reserve_step_id = acknowledged.next_step_id.unwrap();

    let mut reserve = pool.begin().await.unwrap();
    let reserved = advance_workflow_step_with_event_in_tx(
        state,
        &mut reserve,
        &instance.instance_id,
        &reserve_step_id,
        &format!("{}:reserve_on_call", source.event_id),
        support_effect_publication(
            &instance.instance_id,
            &reserve_step_id,
            "release_on_call",
            source,
        ),
    )
    .await
    .unwrap();
    reserve.commit().await.unwrap();
    (instance.instance_id, reserved.next_step_id.unwrap())
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
    let segment: (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        i32,
    ) = sqlx::query_as(
        r#"
        select story_id, tenant_id, workflow_instance_id,
               workflow_definition_owner, workflow_definition_name,
               workflow_definition_version, workflow_step_id,
               parent_segment_id, evidence_revision
        from platform.service_story_segments
        where workflow_instance_id = $1 and status = 'started'
        "#,
    )
    .bind(&instance_id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(segment.0, "story_support_01");
    assert_eq!(segment.1, "tenant_01");
    assert_eq!(segment.2, instance_id);
    assert_eq!(segment.3, "support-sla");
    assert_eq!(segment.4, "ticket_sla");
    assert_eq!(segment.5, "v1");
    assert_eq!(segment.6, initial_step_id);
    assert_eq!(segment.7, "segment_start_01");
    assert_eq!(segment.8, 1);
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

    let first_plan_response = restarted_app
        .clone()
        .oneshot(migration_dry_run_request("v1", "v2"))
        .await
        .unwrap();
    assert_eq!(first_plan_response.status(), StatusCode::OK);
    let first_plan = json_body(first_plan_response).await;
    let second_plan_response = restarted_app
        .clone()
        .oneshot(migration_dry_run_request("v1", "v2"))
        .await
        .unwrap();
    assert_eq!(second_plan_response.status(), StatusCode::OK);
    let second_plan = json_body(second_plan_response).await;
    assert_eq!(first_plan["protocol"], "lenso.workflow-migration-plan.v1");
    assert_eq!(first_plan["mutatesState"], false);
    assert_eq!(first_plan["sourceDefinition"]["version"], "v1");
    assert!(
        first_plan["sourceDefinitionDigest"]
            .as_str()
            .unwrap()
            .starts_with("sha256:")
    );
    assert_eq!(first_plan["targetDefinition"]["version"], "v2");
    assert!(
        first_plan["targetDefinitionDigest"]
            .as_str()
            .unwrap()
            .starts_with("sha256:")
    );
    assert_eq!(first_plan["compatibility"]["category"], "safe");
    assert_eq!(first_plan["affectedInstances"].as_array().unwrap().len(), 1);
    assert_eq!(
        first_plan["affectedInstances"][0]["instanceId"],
        instance_id
    );
    assert_eq!(first_plan["stateMapping"][0]["status"], "preserved");
    assert_eq!(
        first_plan["approvalBoundary"],
        "in_flight_workflow_migration"
    );
    assert_eq!(first_plan["planId"], second_plan["planId"]);
    assert_eq!(
        first_plan["affectedInstances"],
        second_plan["affectedInstances"]
    );

    let pinned_version: String = sqlx::query_scalar(
        "select definition_version from platform.service_workflow_instances where instance_id = $1",
    )
    .bind(&instance_id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(pinned_version, "v1");

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

    force_cleanup_test_databases(vec![db]).await;
}

#[tokio::test]
async fn pinned_definition_migration_rejects_legacy_running_instances() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let guard_index = SERVICE_RUNTIME_MIGRATIONS
        .iter()
        .position(|migration| {
            migration.name == "autonomous-service/0014_pin_workflow_definition_artifacts"
        })
        .expect("pinned Workflow Definition migration must be registered");
    platform_core::apply_migrations(&db.pool, &SERVICE_RUNTIME_MIGRATIONS[..guard_index])
        .await
        .unwrap();
    sqlx::query(
        r#"
        insert into platform.service_workflow_instances (
            instance_id, service_id, definition_owner, definition_name,
            definition_version, state, input, story_context, initial_step_id,
            created_at, updated_at
        ) values (
            'workflow_legacy', 'support-sla', 'support-sla', 'ticket_sla',
            'v1', 'running', '{}'::jsonb, '{}'::jsonb, 'workflow_step_legacy',
            now(), now()
        )
        "#,
    )
    .execute(&db.pool)
    .await
    .unwrap();

    let error = platform_core::apply_migrations(
        &db.pool,
        &SERVICE_RUNTIME_MIGRATIONS[guard_index..=guard_index],
    )
    .await
    .unwrap_err();
    assert_eq!(error.public_message, "Database migration failed");
    let migration_applied: bool = sqlx::query_scalar(
        "select exists(select 1 from platform.schema_migrations where name = $1)",
    )
    .bind(SERVICE_RUNTIME_MIGRATIONS[guard_index].name)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let artifact_column_exists: bool = sqlx::query_scalar(
        r#"
        select exists(
            select 1 from information_schema.columns
            where table_schema = 'platform'
              and table_name = 'service_workflow_instances'
              and column_name = 'definition_artifact'
        )
        "#,
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert!(!migration_applied);
    assert!(!artifact_column_exists);

    db.cleanup().await;
}

#[tokio::test]
async fn incompatible_worker_rejects_pinned_claim_without_mutating_workflow_state() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let service = service();
    let original_manifest = manifest();
    let original_state = prepare_runtime(
        &service,
        &runtime_config(&original_manifest),
        db.pool.clone(),
        &[],
    )
    .await
    .unwrap();
    let app = service_router(OpenApiRouter::new(), original_state.clone());
    let started_response = app.clone().oneshot(start_request("v1")).await.unwrap();
    assert_eq!(started_response.status(), StatusCode::CREATED);
    let started = json_body(started_response).await;
    let instance_id = started["instance"]["instanceId"].as_str().unwrap();
    let step_id = started["instance"]["initialStepId"].as_str().unwrap();
    let due_at: DateTime<Utc> = sqlx::query_scalar(
        "select due_at from platform.service_workflow_timers where instance_id = $1",
    )
    .bind(instance_id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    drop(app);
    drop(original_state);

    let mut reinterpreted_v1 = workflow("v1");
    reinterpreted_v1.steps[0].timeout_ms = Some(9_000);
    let reinterpreted_manifest = ModuleManifest::builder("support-sla")
        .runtime(RuntimeSurface {
            functions: vec![],
            schedules: vec![],
            workflows: vec![reinterpreted_v1, workflow("v2")],
        })
        .build();
    let incompatible_state = prepare_runtime(
        &service,
        &runtime_config(&reinterpreted_manifest),
        db.pool.clone(),
        &[],
    )
    .await
    .unwrap();
    let incompatible_app = service_router(OpenApiRouter::new(), incompatible_state.clone());
    let blocked_plan_response = incompatible_app
        .oneshot(migration_dry_run_request("v1", "v2"))
        .await
        .unwrap();
    assert_eq!(blocked_plan_response.status(), StatusCode::OK);
    let blocked_plan = json_body(blocked_plan_response).await;
    assert_eq!(blocked_plan["compatibility"]["category"], "blocked");
    assert_eq!(
        blocked_plan["compatibility"]["reasons"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|reason| { reason["code"] == "workflow_in_flight_source_artifact_mismatch" })
            .count(),
        1
    );
    let error = claim_due_workflow_work_at(
        &incompatible_state,
        "support-sla-worker/new-deployment",
        due_at + Duration::milliseconds(1),
        Duration::seconds(30),
        10,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, WorkflowErrorCode::DefinitionVersionUnsupported);

    let timer: (String, Option<String>, Option<DateTime<Utc>>) = sqlx::query_as(
        "select state, claimed_by, claimed_at from platform.service_workflow_timers where instance_id = $1",
    )
    .bind(instance_id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let attempt_count: i64 = sqlx::query_scalar(
        "select count(*) from platform.service_workflow_step_attempts where instance_id = $1",
    )
    .bind(instance_id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let states: (String, String) = sqlx::query_as(
        r#"
        select instance.state, step.state
        from platform.service_workflow_instances instance
        join platform.service_workflow_steps step on step.instance_id = instance.instance_id
        where instance.instance_id = $1 and step.step_id = $2
        "#,
    )
    .bind(instance_id)
    .bind(step_id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(timer, ("pending".to_owned(), None, None));
    assert_eq!(attempt_count, 0);
    assert_eq!(states, ("running".to_owned(), "pending".to_owned()));

    let compatible_state = prepare_runtime(
        &service,
        &runtime_config(&original_manifest),
        db.pool.clone(),
        &[],
    )
    .await
    .unwrap();
    let claims = claim_due_workflow_work_at(
        &compatible_state,
        "support-sla-worker/v1-compatible",
        due_at + Duration::milliseconds(1),
        Duration::seconds(30),
        10,
    )
    .await
    .unwrap();
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].definition.version, "v1");

    drop(compatible_state);
    drop(incompatible_state);
    db.cleanup().await;
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn child_workflow_inherits_context_and_resumes_parent_once_across_restarts() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let service = service();
    let manifest = manifest();
    let state = prepare_runtime(&service, &runtime_config(&manifest), db.pool.clone(), &[])
        .await
        .unwrap();
    let source = support_ticket_opened("support-event-child", "ticket_child");
    let (parent, child_start) = start_parent_and_child(&state, &db.pool, &source, "v1").await;
    assert_eq!(
        child_start.disposition,
        WorkflowTransitionDisposition::Applied
    );
    let child_instance_id = child_start.child_instance_id.clone().unwrap();
    assert_ne!(parent.instance_id, child_instance_id);
    drop(state);

    let restarted = prepare_runtime(&service, &runtime_config(&manifest), db.pool.clone(), &[])
        .await
        .expect("child worker restart should recover pinned state");
    let (child_step_id, child_version, child_context): (String, String, serde_json::Value) =
        sqlx::query_as(
            r#"
            select initial_step_id, definition_version, workflow_context
            from platform.service_workflow_instances
            where instance_id = $1
            "#,
        )
        .bind(&child_instance_id)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(child_version, "v1");
    let child_context: lenso_service::EventContext = serde_json::from_value(child_context).unwrap();
    assert_eq!(child_context.story, source.context.story);
    assert_eq!(
        child_context.delegated_actor,
        source.context.delegated_actor
    );
    assert_eq!(child_context.tenant, source.context.tenant);
    assert_eq!(child_context.deadline, source.context.deadline);
    assert_eq!(
        child_context.idempotency_key,
        source.context.idempotency_key
    );
    assert_eq!(
        child_context.causation.as_ref().unwrap().causation_id,
        parent.initial_step_id
    );
    assert_eq!(
        child_context.causation.as_ref().unwrap().correlation_id,
        source.context.causation.as_ref().unwrap().correlation_id
    );

    let mut child_completion = db.pool.begin().await.unwrap();
    let completed = advance_workflow_step_with_event_in_tx(
        &restarted,
        &mut child_completion,
        &child_instance_id,
        &child_step_id,
        "support-event-child:notify_on_call",
        acknowledgement_publication(&child_instance_id, &child_step_id, &source),
    )
    .await
    .unwrap();
    assert_eq!(
        completed.disposition,
        WorkflowTransitionDisposition::Applied
    );
    assert!(completed.next_step_id.is_none());
    child_completion.commit().await.unwrap();
    drop(restarted);

    let restarted = prepare_runtime(&service, &runtime_config(&manifest), db.pool.clone(), &[])
        .await
        .expect("parent worker restart should recover child wait state");
    let mut resume = db.pool.begin().await.unwrap();
    let resumed = resume_parent_from_child_in_tx(
        &restarted,
        &mut resume,
        &parent.instance_id,
        &parent.initial_step_id,
        &child_instance_id,
        "child-completion-delivery-01",
    )
    .await
    .unwrap();
    assert_eq!(resumed.disposition, WorkflowTransitionDisposition::Applied);
    assert!(resumed.next_step_id.is_some());
    resume.commit().await.unwrap();

    let mut redelivery = db.pool.begin().await.unwrap();
    let duplicate = resume_parent_from_child_in_tx(
        &restarted,
        &mut redelivery,
        &parent.instance_id,
        &parent.initial_step_id,
        &child_instance_id,
        "child-completion-delivery-01",
    )
    .await
    .unwrap();
    assert_eq!(
        duplicate.disposition,
        WorkflowTransitionDisposition::Duplicate
    );
    assert_eq!(duplicate.next_step_id, resumed.next_step_id);
    redelivery.commit().await.unwrap();

    let app = service_router(OpenApiRouter::new(), restarted);
    let parent_inspection = app
        .clone()
        .oneshot(
            Request::get(format!(
                "/runtime/workflows/instances/{}",
                parent.instance_id
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(parent_inspection.status(), StatusCode::OK);
    let parent_inspection = json_body(parent_inspection).await;
    assert_eq!(parent_inspection["instance"]["definition"]["version"], "v1");
    assert_eq!(parent_inspection["instance"]["state"], "running");
    assert_eq!(
        parent_inspection["instance"]["steps"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        parent_inspection["instance"]["steps"][0]["state"],
        "completed"
    );
    assert_eq!(
        parent_inspection["instance"]["steps"][0]["childWorkflow"]["instanceId"],
        child_instance_id
    );
    assert_eq!(
        parent_inspection["instance"]["steps"][0]["childWorkflow"]["state"],
        "completed"
    );
    assert_eq!(
        parent_inspection["instance"]["steps"][0]["childWorkflow"]["completionDeliveryId"],
        "child-completion-delivery-01"
    );
    assert_eq!(
        parent_inspection["instance"]["steps"][1]["state"],
        "pending"
    );

    let child_inspection = app
        .oneshot(
            Request::get(format!("/runtime/workflows/instances/{child_instance_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(child_inspection.status(), StatusCode::OK);
    let child_inspection = json_body(child_inspection).await;
    assert_eq!(child_inspection["instance"]["definition"]["version"], "v1");
    assert_eq!(child_inspection["instance"]["state"], "completed");
    assert_eq!(
        child_inspection["instance"]["parent"]["instanceId"],
        parent.instance_id
    );
    assert_eq!(
        child_inspection["instance"]["parent"]["stepId"],
        parent.initial_step_id
    );
    assert_eq!(
        child_inspection["instance"]["parent"]["causationId"],
        parent.initial_step_id
    );
    let resumed_step_count: i64 = sqlx::query_scalar(
        "select count(*) from platform.service_workflow_steps where instance_id = $1",
    )
    .bind(&parent.instance_id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(resumed_step_count, 2);

    db.cleanup().await;
}

#[tokio::test]
async fn child_failure_becomes_durable_parent_evidence() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let service = service();
    let manifest = manifest();
    let state = prepare_runtime(&service, &runtime_config(&manifest), db.pool.clone(), &[])
        .await
        .unwrap();
    let source = support_ticket_opened("support-event-child-failure", "ticket_child_failure");
    let (parent, child_start) = start_parent_and_child(&state, &db.pool, &source, "v1").await;
    let child_instance_id = child_start.child_instance_id.unwrap();
    let failure = WorkflowFailureEvidence::new(
        "workflow_child_business_failure",
        "On-call acknowledgement was rejected",
        "retry_child_workflow",
    );
    let mut fail = db.pool.begin().await.unwrap();
    let failed = fail_workflow_in_tx(
        &state,
        &mut fail,
        &child_instance_id,
        "child-failure-01",
        &failure,
    )
    .await
    .unwrap();
    assert_eq!(failed.disposition, WorkflowTransitionDisposition::Applied);
    fail.commit().await.unwrap();
    drop(state);

    let restarted = prepare_runtime(&service, &runtime_config(&manifest), db.pool.clone(), &[])
        .await
        .unwrap();
    let mut resume = db.pool.begin().await.unwrap();
    let resumed = resume_parent_from_child_in_tx(
        &restarted,
        &mut resume,
        &parent.instance_id,
        &parent.initial_step_id,
        &child_instance_id,
        "child-failure-delivery-01",
    )
    .await
    .unwrap();
    assert_eq!(resumed.disposition, WorkflowTransitionDisposition::Applied);
    assert_eq!(resumed.failure, Some(failure.clone()));
    resume.commit().await.unwrap();

    let mut redelivery = db.pool.begin().await.unwrap();
    let duplicate = resume_parent_from_child_in_tx(
        &restarted,
        &mut redelivery,
        &parent.instance_id,
        &parent.initial_step_id,
        &child_instance_id,
        "child-failure-delivery-01",
    )
    .await
    .unwrap();
    assert_eq!(
        duplicate.disposition,
        WorkflowTransitionDisposition::Duplicate
    );
    redelivery.commit().await.unwrap();

    let app = service_router(OpenApiRouter::new(), restarted);
    let inspected = app
        .oneshot(
            Request::get(format!(
                "/runtime/workflows/instances/{}",
                parent.instance_id
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    let inspected = json_body(inspected).await;
    assert_eq!(inspected["instance"]["state"], "failed");
    assert_eq!(
        inspected["instance"]["failure"]["code"],
        "workflow_child_business_failure"
    );
    assert_eq!(inspected["nextActions"][0], "retry_child_workflow");
    assert_eq!(inspected["instance"]["steps"][0]["state"], "failed");
    assert_eq!(
        inspected["instance"]["steps"][0]["childWorkflow"]["state"],
        "failed"
    );
    assert_eq!(
        inspected["instance"]["steps"][0]["childWorkflow"]["nextActions"][0],
        "retry_child_workflow"
    );

    db.cleanup().await;
}

#[tokio::test]
async fn unsupported_pinned_child_version_is_durable_parent_evidence() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let service = service();
    let manifest = manifest();
    let state = prepare_runtime(&service, &runtime_config(&manifest), db.pool.clone(), &[])
        .await
        .unwrap();
    let source = support_ticket_opened(
        "support-event-child-unsupported",
        "ticket_child_unsupported",
    );
    let (parent, child_start) = start_parent_and_child(&state, &db.pool, &source, "v1").await;
    let child_instance_id = child_start.child_instance_id.unwrap();
    drop(state);

    let unsupported_manifest = manifest_without_child_v1();
    let restarted = prepare_runtime(
        &service,
        &runtime_config(&unsupported_manifest),
        db.pool.clone(),
        &[],
    )
    .await
    .unwrap();
    let mut resume = db.pool.begin().await.unwrap();
    let unsupported = resume_parent_from_child_in_tx(
        &restarted,
        &mut resume,
        &parent.instance_id,
        &parent.initial_step_id,
        &child_instance_id,
        "child-unsupported-delivery-01",
    )
    .await
    .unwrap();
    assert_eq!(
        unsupported.disposition,
        WorkflowTransitionDisposition::Applied
    );
    assert_eq!(
        unsupported.failure.as_ref().unwrap().code,
        "workflow_child_definition_version_unsupported"
    );
    resume.commit().await.unwrap();

    let app = service_router(OpenApiRouter::new(), restarted);
    let parent_inspection = app
        .clone()
        .oneshot(
            Request::get(format!(
                "/runtime/workflows/instances/{}",
                parent.instance_id
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    let parent_inspection = json_body(parent_inspection).await;
    assert_eq!(parent_inspection["instance"]["state"], "failed");
    assert_eq!(
        parent_inspection["instance"]["failure"]["code"],
        "workflow_child_definition_version_unsupported"
    );
    assert_eq!(
        parent_inspection["nextActions"][0],
        "deploy_worker_supporting_child_workflow_version"
    );
    assert_eq!(
        parent_inspection["instance"]["steps"][0]["childWorkflow"]["definition"]["version"],
        "v1"
    );
    assert_eq!(
        parent_inspection["instance"]["steps"][0]["childWorkflow"]["state"],
        "failed"
    );
    let child_inspection = app
        .oneshot(
            Request::get(format!("/runtime/workflows/instances/{child_instance_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let child_inspection = json_body(child_inspection).await;
    assert_eq!(child_inspection["instance"]["definition"]["version"], "v1");
    assert_eq!(child_inspection["instance"]["state"], "failed");

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

    drop(sla_state);
    drop(support_state);
    drop(adapter);
    force_cleanup_test_databases(vec![support_db, sla_db, transport_db]).await;
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
    let exhausted_state: (
        String,
        String,
        i32,
        Option<DateTime<Utc>>,
        Option<serde_json::Value>,
        Option<String>,
    ) = sqlx::query_as(
        r"
        select instance.state, step.state, step.attempt_count, step.next_attempt_at,
               instance.failure_evidence, instance.terminal_transition_id
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
    assert_eq!(exhausted_state.0, "failed");
    assert_eq!(exhausted_state.1, "exhausted");
    assert_eq!(exhausted_state.2, 3);
    assert_eq!(exhausted_state.3, None);
    assert_eq!(
        exhausted_state.4,
        Some(
            serde_json::to_value(WorkflowFailureEvidence::new(
                "dependency_unavailable",
                "retry budget exhausted",
                "inspect_workflow",
            ))
            .unwrap()
        )
    );
    assert_eq!(
        exhausted_state.5.as_deref(),
        Some(retry_three.attempt_transition_id.as_str())
    );
    let attempt_story_states: Vec<(i32, String)> = sqlx::query_as(
        r#"
        select attempt, status
        from platform.service_story_segments
        where workflow_instance_id = $1
          and contract_id = 'lenso.workflow-attempt'
        order by attempt
        "#,
    )
    .bind(&exhausted_instance.instance_id)
    .fetch_all(&db.pool)
    .await
    .unwrap();
    assert_eq!(
        attempt_story_states,
        vec![
            (1, "retry_scheduled".to_owned()),
            (2, "retry_scheduled".to_owned()),
            (3, "exhausted".to_owned()),
        ]
    );

    db.cleanup().await;
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn timed_out_support_sla_compensates_cross_service_effects_once_in_declared_order() {
    let Some(sla_db) = TestDatabase::create().await else {
        return;
    };
    let Some(support_db) = TestDatabase::create().await else {
        return;
    };
    let Some(transport_db) = TestDatabase::create().await else {
        return;
    };
    let initial_time = DateTime::parse_from_rfc3339("2026-07-17T01:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let clock = Arc::new(SystemSandboxWorkflowClock::new(initial_time));
    let manifest = manifest();
    let mut sla_state = prepare_runtime(
        &service(),
        &runtime_config(&manifest),
        sla_db.pool.clone(),
        &[],
    )
    .await
    .unwrap()
    .with_workflow_clock(Arc::clone(&clock) as Arc<dyn platform_core::Clock>);
    let support_migrations = [platform_core::Migration {
        name: "support-ticket/0001_create_sla_compensations",
        sql: r#"
            create table support_ticket_sla_effects (
                effect_id text primary key,
                ticket_id text not null,
                compensation_action text not null,
                source_event_id text not null unique,
                active boolean not null,
                compensated_by text unique
            );
            create table support_ticket_sla_compensations (
                compensation_id text primary key,
                effect_id text not null unique,
                action text not null,
                envelope jsonb not null
            );
        "#,
    }];
    let support_state = prepare_runtime(
        &support_service(),
        &ServiceRuntimeConfig::new("support", "primary", "support"),
        support_db.pool.clone(),
        &support_migrations,
    )
    .await
    .unwrap();
    let adapter = LocalTransportAdapter::prepare(transport_db.pool.clone())
        .await
        .unwrap();
    let source = support_ticket_opened("support-event-compensation", "ticket_compensation");
    let (instance_id, timeout_step_id) =
        complete_compensatable_support_effects(&sla_state, &sla_db.pool, &source).await;

    assert_eq!(
        relay_service_events_once(&sla_state, &adapter, 10)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        consume_service_events_once_without_workload_identity(
            &support_state,
            &adapter,
            "support",
            &SupportTicketSlaHandler,
            10,
        )
        .await
        .unwrap(),
        2
    );
    let active_business_effects: Vec<(String, String, bool)> = sqlx::query_as(
        "select effect_id, compensation_action, active from support_ticket_sla_effects order by compensation_action",
    )
    .fetch_all(&support_db.pool)
    .await
    .unwrap();
    assert_eq!(active_business_effects.len(), 2);
    assert!(active_business_effects.iter().all(|effect| effect.2));

    let effects_before_timeout: Vec<(String, String, i32, String)> = sqlx::query_as(
        r#"
        select effect_id, compensation_name, compensation_order, state
        from platform.service_workflow_effects
        where instance_id = $1
        order by compensation_order
        "#,
    )
    .bind(&instance_id)
    .fetch_all(&sla_db.pool)
    .await
    .unwrap();
    assert_eq!(effects_before_timeout.len(), 2);
    assert_eq!(effects_before_timeout[0].1, "release_on_call");
    assert_eq!(effects_before_timeout[0].2, 1);
    assert_eq!(effects_before_timeout[1].1, "withdraw_sla_acknowledgement");
    assert_eq!(effects_before_timeout[1].2, 2);
    assert!(
        effects_before_timeout
            .iter()
            .all(|effect| effect.3 == "completed")
    );
    let instance_state_before_timeout: String = sqlx::query_scalar(
        "select state from platform.service_workflow_instances where instance_id = $1",
    )
    .bind(&instance_id)
    .fetch_one(&sla_db.pool)
    .await
    .unwrap();
    assert_eq!(instance_state_before_timeout, "running");

    let timeout_time = clock.advance(Duration::seconds(5));
    let timeout_claim = claim_due_workflow_work_at(
        &sla_state,
        "support-sla-compensation-worker",
        timeout_time,
        Duration::seconds(30),
        10,
    )
    .await
    .unwrap()
    .remove(0);
    assert_eq!(timeout_claim.kind, WorkflowTimerKind::StepTimeout);
    assert_eq!(timeout_claim.step_id, timeout_step_id);
    let selection =
        select_workflow_compensations_after_timeout_at(&sla_state, &timeout_claim, timeout_time)
            .await
            .unwrap();
    assert_eq!(
        selection.disposition,
        WorkflowTransitionDisposition::Applied
    );
    assert_eq!(selection.compensations.len(), 2);
    assert_eq!(selection.compensations[0].name, "release_on_call");
    assert_eq!(selection.compensations[0].execution_order, 1);
    assert_eq!(
        selection.compensations[1].name,
        "withdraw_sla_acknowledgement"
    );
    assert_eq!(selection.compensations[1].execution_order, 2);
    let duplicate_selection =
        select_workflow_compensations_after_timeout_at(&sla_state, &timeout_claim, timeout_time)
            .await
            .unwrap();
    assert_eq!(
        duplicate_selection.disposition,
        WorkflowTransitionDisposition::Duplicate
    );

    drop(sla_state);
    sla_state = prepare_runtime(
        &service(),
        &runtime_config(&manifest),
        sla_db.pool.clone(),
        &[],
    )
    .await
    .unwrap()
    .with_workflow_clock(Arc::clone(&clock) as Arc<dyn platform_core::Clock>);
    let first = &selection.compensations[0];
    let second = &selection.compensations[1];
    let mut out_of_order = sla_db.pool.begin().await.unwrap();
    let out_of_order_error = dispatch_workflow_compensation_with_event_in_tx(
        &sla_state,
        &mut out_of_order,
        &second.compensation_id,
        &format!("{}:attempt:1", second.compensation_id),
        compensation_request_publication(&instance_id, &second.compensation_id, &source),
    )
    .await
    .unwrap_err();
    out_of_order.rollback().await.unwrap();
    assert_eq!(
        out_of_order_error.code,
        WorkflowErrorCode::TransitionConflict
    );

    let first_transition = format!("{}:attempt:1", first.compensation_id);
    let first_publication =
        compensation_request_publication(&instance_id, &first.compensation_id, &source);
    let mut first_tx = sla_db.pool.begin().await.unwrap();
    let first_result = dispatch_workflow_compensation_with_event_in_tx(
        &sla_state,
        &mut first_tx,
        &first.compensation_id,
        &first_transition,
        first_publication.clone(),
    )
    .await
    .unwrap();
    assert_eq!(
        first_result.disposition,
        WorkflowTransitionDisposition::Applied
    );
    assert_eq!(
        first_result.workflow_state,
        lenso_autonomous_service::WorkflowInstanceState::Compensating
    );
    first_tx.commit().await.unwrap();

    let mut first_duplicate_tx = sla_db.pool.begin().await.unwrap();
    let first_duplicate = dispatch_workflow_compensation_with_event_in_tx(
        &sla_state,
        &mut first_duplicate_tx,
        &first.compensation_id,
        &first_transition,
        first_publication,
    )
    .await
    .unwrap();
    first_duplicate_tx.commit().await.unwrap();
    assert_eq!(
        first_duplicate.disposition,
        WorkflowTransitionDisposition::Duplicate
    );
    assert_eq!(
        first_duplicate.workflow_state,
        lenso_autonomous_service::WorkflowInstanceState::Compensating
    );
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
            &SupportTicketSlaHandler,
            10,
        )
        .await
        .unwrap(),
        1
    );
    let first_reversed: (bool, Option<String>) = sqlx::query_as(
        "select active, compensated_by from support_ticket_sla_effects where effect_id = $1",
    )
    .bind(&first.effect_id)
    .fetch_one(&support_db.pool)
    .await
    .unwrap();
    assert_eq!(first_reversed, (false, Some(first.compensation_id.clone())));
    let still_active: bool =
        sqlx::query_scalar("select active from support_ticket_sla_effects where effect_id = $1")
            .bind(&second.effect_id)
            .fetch_one(&support_db.pool)
            .await
            .unwrap();
    assert!(still_active);
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
            &SupportSlaCompensationCompletedHandler {
                state: sla_state.clone(),
            },
            10,
        )
        .await
        .unwrap(),
        1
    );
    let state_after_first_completion: String = sqlx::query_scalar(
        "select state from platform.service_workflow_instances where instance_id = $1",
    )
    .bind(&instance_id)
    .fetch_one(&sla_db.pool)
    .await
    .unwrap();
    assert_eq!(state_after_first_completion, "compensating");

    drop(sla_state);
    sla_state = prepare_runtime(
        &service(),
        &runtime_config(&manifest),
        sla_db.pool.clone(),
        &[],
    )
    .await
    .unwrap()
    .with_workflow_clock(Arc::clone(&clock) as Arc<dyn platform_core::Clock>);
    let second_transition = format!("{}:attempt:1", second.compensation_id);
    let second_publication =
        compensation_request_publication(&instance_id, &second.compensation_id, &source);
    let mut second_tx = sla_db.pool.begin().await.unwrap();
    let second_result = dispatch_workflow_compensation_with_event_in_tx(
        &sla_state,
        &mut second_tx,
        &second.compensation_id,
        &second_transition,
        second_publication.clone(),
    )
    .await
    .unwrap();
    assert_eq!(
        second_result.disposition,
        WorkflowTransitionDisposition::Applied
    );
    assert_eq!(
        second_result.workflow_state,
        lenso_autonomous_service::WorkflowInstanceState::Compensating
    );
    second_tx.commit().await.unwrap();

    let mut second_duplicate_tx = sla_db.pool.begin().await.unwrap();
    let second_duplicate = dispatch_workflow_compensation_with_event_in_tx(
        &sla_state,
        &mut second_duplicate_tx,
        &second.compensation_id,
        &second_transition,
        second_publication,
    )
    .await
    .unwrap();
    second_duplicate_tx.commit().await.unwrap();
    assert_eq!(
        second_duplicate.disposition,
        WorkflowTransitionDisposition::Duplicate
    );
    assert_eq!(
        second_duplicate.workflow_state,
        lenso_autonomous_service::WorkflowInstanceState::Compensating
    );
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
            &SupportTicketSlaHandler,
            10,
        )
        .await
        .unwrap(),
        1
    );
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
            &SupportSlaCompensationCompletedHandler {
                state: sla_state.clone(),
            },
            10,
        )
        .await
        .unwrap(),
        1
    );
    let compensation_outbox_count: i64 = sqlx::query_scalar(
        "select count(*) from platform.service_event_outbox where event_id like '%:compensation:%:request'",
    )
    .fetch_one(&sla_db.pool)
    .await
    .unwrap();
    assert_eq!(compensation_outbox_count, 2);
    let redelivered_envelope: serde_json::Value = sqlx::query_scalar(
        "select envelope from platform.service_event_outbox where event_id = $1",
    )
    .bind(format!("{}:request", first.compensation_id))
    .fetch_one(&sla_db.pool)
    .await
    .unwrap();
    adapter
        .publish(TransportPublication {
            consumer_id: "support".to_owned(),
            envelope: serde_json::from_value(redelivered_envelope).unwrap(),
        })
        .await
        .unwrap();
    assert_eq!(
        consume_service_events_once_without_workload_identity(
            &support_state,
            &adapter,
            "support",
            &SupportTicketSlaHandler,
            10,
        )
        .await
        .unwrap(),
        0
    );
    let business_compensations: Vec<(String, String, String)> = sqlx::query_as(
        "select compensation_id, effect_id, action from support_ticket_sla_compensations order by action",
    )
    .fetch_all(&support_db.pool)
    .await
    .unwrap();
    assert_eq!(business_compensations.len(), 2);
    assert_eq!(
        business_compensations
            .iter()
            .map(|entry| entry.2.as_str())
            .collect::<Vec<_>>(),
        vec!["release_on_call", "withdraw_sla_acknowledgement"]
    );
    assert!(
        business_compensations
            .iter()
            .any(|entry| { entry.0 == first.compensation_id && entry.1 == first.effect_id })
    );
    assert!(
        business_compensations
            .iter()
            .any(|entry| { entry.0 == second.compensation_id && entry.1 == second.effect_id })
    );
    let active_effect_count: i64 =
        sqlx::query_scalar("select count(*) from support_ticket_sla_effects where active")
            .fetch_one(&support_db.pool)
            .await
            .unwrap();
    assert_eq!(active_effect_count, 0);

    let app = service_router(OpenApiRouter::new(), sla_state.clone());
    let inspected = app
        .oneshot(
            Request::get(format!("/runtime/workflows/instances/{instance_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(inspected.status(), StatusCode::OK);
    let inspected = json_body(inspected).await;
    assert_eq!(inspected["instance"]["state"], "compensated");
    assert_eq!(
        inspected["instance"]["effects"].as_array().unwrap().len(),
        2
    );
    assert!(
        inspected["instance"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .all(|effect| effect["state"] == "compensated")
    );
    assert_eq!(
        inspected["instance"]["compensations"][0]["name"],
        "release_on_call"
    );
    assert_eq!(
        inspected["instance"]["compensations"][1]["name"],
        "withdraw_sla_acknowledgement"
    );
    assert!(
        inspected["instance"]["compensations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|compensation| {
                compensation["state"] == "compensated"
                    && compensation["attemptCount"] == 1
                    && compensation["attempts"].as_array().unwrap().len() == 1
            })
    );
    let history_kinds = inspected["instance"]["history"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["kind"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(history_kinds.contains(&"effect_completed"));
    assert!(history_kinds.contains(&"step_timed_out"));
    assert!(history_kinds.contains(&"compensation_selected"));
    assert!(history_kinds.contains(&"compensation_attempt_succeeded"));
    assert!(history_kinds.contains(&"workflow_compensated"));
    assert_eq!(inspected["nextActions"][0], "no_action_required");
    let story_statuses: Vec<String> = sqlx::query_scalar(
        "select status from platform.service_story_segments where operation like 'workflow %' order by segment_id",
    )
    .fetch_all(&sla_db.pool)
    .await
    .unwrap();
    assert_eq!(
        story_statuses
            .iter()
            .filter(|status| *status == "completed")
            .count(),
        2
    );
    assert_eq!(
        story_statuses
            .iter()
            .filter(|status| *status == "timed_out")
            .count(),
        1
    );
    assert_eq!(
        story_statuses
            .iter()
            .filter(|status| *status == "compensated")
            .count(),
        2
    );

    drop(sla_state);
    drop(support_state);
    drop(adapter);
    force_cleanup_test_databases(vec![support_db, sla_db, transport_db]).await;
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn failed_compensation_requires_intervention_without_becoming_workflow_failure() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let initial_time = DateTime::parse_from_rfc3339("2026-07-17T02:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let clock = Arc::new(SystemSandboxWorkflowClock::new(initial_time));
    let manifest = manifest();
    let state = prepare_runtime(&service(), &runtime_config(&manifest), db.pool.clone(), &[])
        .await
        .unwrap()
        .with_workflow_clock(Arc::clone(&clock) as Arc<dyn platform_core::Clock>);
    let source = support_ticket_opened(
        "support-event-compensation-failure",
        "ticket_compensation_failure",
    );
    let (instance_id, timeout_step_id) =
        complete_compensatable_support_effects(&state, &db.pool, &source).await;
    let timeout_time = clock.advance(Duration::seconds(5));
    let timeout_claim = claim_due_workflow_work_at(
        &state,
        "support-sla-compensation-failure-worker",
        timeout_time,
        Duration::seconds(30),
        10,
    )
    .await
    .unwrap()
    .remove(0);
    assert_eq!(timeout_claim.step_id, timeout_step_id);
    let selection =
        select_workflow_compensations_after_timeout_at(&state, &timeout_claim, timeout_time)
            .await
            .unwrap();
    let compensation = &selection.compensations[0];
    let failure = WorkflowFailureEvidence::new(
        "support_compensation_rejected",
        "Support Service rejected the compensation effect",
        "open_support_compensation_runbook",
    );
    let transition_id = format!("{}:attempt:1", compensation.compensation_id);
    let mut dispatch = db.pool.begin().await.unwrap();
    let dispatched = dispatch_workflow_compensation_with_event_in_tx(
        &state,
        &mut dispatch,
        &compensation.compensation_id,
        &transition_id,
        compensation_request_publication(&instance_id, &compensation.compensation_id, &source),
    )
    .await
    .unwrap();
    dispatch.commit().await.unwrap();
    assert_eq!(
        dispatched.disposition,
        WorkflowTransitionDisposition::Applied
    );
    let failed = record_workflow_compensation_failure_at(
        &state,
        &compensation.compensation_id,
        &transition_id,
        failure.clone(),
        timeout_time,
    )
    .await
    .unwrap();
    assert_eq!(failed.disposition, WorkflowTransitionDisposition::Applied);
    assert_eq!(
        failed.workflow_state,
        lenso_autonomous_service::WorkflowInstanceState::CompensationFailed
    );
    let duplicate = record_workflow_compensation_failure_at(
        &state,
        &compensation.compensation_id,
        &transition_id,
        failure,
        timeout_time,
    )
    .await
    .unwrap();
    assert_eq!(
        duplicate.disposition,
        WorkflowTransitionDisposition::Duplicate
    );

    let app = service_router(OpenApiRouter::new(), state);
    let inspected = app
        .oneshot(
            Request::get(format!("/runtime/workflows/instances/{instance_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let inspected = json_body(inspected).await;
    assert_eq!(inspected["instance"]["state"], "compensation_failed");
    assert_ne!(inspected["instance"]["state"], "failed");
    assert_eq!(
        inspected["instance"]["failure"]["code"],
        "support_compensation_rejected"
    );
    assert_eq!(inspected["instance"]["compensations"][0]["state"], "failed");
    assert!(
        inspected["instance"]["compensations"][0]["outgoingWork"].is_object(),
        "a remote rejection must retain the dispatched request as durable evidence"
    );
    assert_eq!(
        inspected["instance"]["compensations"][0]["attempts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        inspected["instance"]["compensations"][0]["attempts"][0]["state"],
        "failed"
    );
    assert_eq!(
        inspected["instance"]["compensations"][0]["failure"]["nextAction"],
        "open_support_compensation_runbook"
    );
    assert_eq!(
        inspected["instance"]["compensations"][1]["state"],
        "pending"
    );
    assert_eq!(
        inspected["nextActions"][0],
        "open_support_compensation_runbook"
    );
    assert!(
        inspected["instance"]["history"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| {
                entry["kind"] == "compensation_attempt_failed"
                    && entry["detail"]["interventionRequired"] == true
            })
    );
    assert!(
        inspected["instance"]["history"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| {
                entry["kind"] == "workflow_compensation_failed"
                    && entry["detail"]["finalOutcome"] == "compensation_failed"
                    && entry["detail"]["failure"]["code"] == "support_compensation_rejected"
            })
    );
    let intervention_segments: i64 = sqlx::query_scalar(
        "select count(*) from platform.service_story_segments where status = 'intervention_required'",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(intervention_segments, 1);
    let failure_operation: String = sqlx::query_scalar(
        "select operation from platform.service_story_segments where status = 'intervention_required'",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert!(failure_operation.contains("support_compensation_rejected"));
    assert!(failure_operation.contains("Support Service rejected the compensation effect"));

    db.cleanup().await;
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn workflow_operator_controls_are_deterministic_authorized_and_audited() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let initial_time = DateTime::parse_from_rfc3339("2026-07-17T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let clock = Arc::new(SystemSandboxWorkflowClock::new(initial_time));
    let manifest = manifest();
    let state = prepare_runtime(
        &service(),
        &runtime_config_with_workflow_authority(&manifest),
        db.pool.clone(),
        &[],
    )
    .await
    .unwrap()
    .with_workflow_clock(Arc::clone(&clock) as Arc<dyn platform_core::Clock>);
    let source = support_ticket_opened("support-event-operator", "ticket_operator");
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
    let instance_id = instance.instance_id.clone();
    let step_id = instance.initial_step_id.clone();
    let original_context: serde_json::Value = sqlx::query_scalar(
        "select workflow_context from platform.service_workflow_instances where instance_id = $1",
    )
    .bind(&instance_id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let original_step_count: i64 = sqlx::query_scalar(
        "select count(*) from platform.service_workflow_steps where instance_id = $1",
    )
    .bind(&instance_id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let app = service_router(OpenApiRouter::new(), state.clone());

    let inspected = app
        .clone()
        .oneshot(
            Request::get(format!(
                "/runtime/workflows/instances/{instance_id}?stepId={step_id}"
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(inspected.status(), StatusCode::OK);
    let inspected = json_body(inspected).await;
    assert_eq!(inspected["instance"]["definition"]["version"], "v1");
    assert_eq!(inspected["instance"]["control"]["state"], "active");
    assert_eq!(inspected["selectedStep"]["stepId"], step_id);
    assert_eq!(inspected["selectedStep"]["attempts"], serde_json::json!([]));
    assert_eq!(
        inspected["availableActions"],
        serde_json::json!(["pause", "cancel", "terminate", "intervene"])
    );
    assert!(
        inspected["pendingWork"]
            .as_array()
            .unwrap()
            .iter()
            .any(|work| work["kind"] == "timer" && work["state"] == "pending")
    );

    let first_pause_plan = app
        .clone()
        .oneshot(workflow_operator_plan_request(&instance_id, "pause", None))
        .await
        .unwrap();
    assert_eq!(first_pause_plan.status(), StatusCode::OK);
    let first_pause_plan = json_body(first_pause_plan).await;
    let repeated_pause_plan = app
        .clone()
        .oneshot(workflow_operator_plan_request(&instance_id, "pause", None))
        .await
        .unwrap();
    let repeated_pause_plan = json_body(repeated_pause_plan).await;
    assert_eq!(first_pause_plan, repeated_pause_plan);
    assert_eq!(first_pause_plan["mutatesState"], false);
    assert_eq!(
        first_pause_plan["authorization"]["requiredAuthority"],
        "workflow_instance_pause"
    );
    let intervention_count: i64 =
        sqlx::query_scalar("select count(*) from platform.service_workflow_interventions")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(intervention_count, 0, "dry run must remain read-only");

    let claim_time = clock.advance(Duration::seconds(5));
    let claimed = claim_due_workflow_work_at(
        &state,
        "support-sla-worker-claimed-before-pause",
        claim_time,
        Duration::seconds(5),
        10,
    )
    .await
    .unwrap()
    .remove(0);
    assert_eq!(claimed.kind, WorkflowTimerKind::StepTimeout);
    let stale = app
        .clone()
        .oneshot(workflow_operator_apply_request(
            &instance_id,
            "pause",
            None,
            first_pause_plan["planId"].as_str().unwrap(),
            Some("approved-workflow-control"),
        ))
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    let stale = json_body(stale).await;
    assert_eq!(stale["code"], "workflow_stale_plan");
    assert_eq!(stale["next_actions"][0], "plan_workflow_action_again");

    let pause_plan = app
        .clone()
        .oneshot(workflow_operator_plan_request(&instance_id, "pause", None))
        .await
        .unwrap();
    let pause_plan = json_body(pause_plan).await;
    assert_eq!(
        pause_plan["affectedResources"]["inFlightClaimIds"][0],
        claimed.timer_id
    );
    let missing_authority = app
        .clone()
        .oneshot(workflow_operator_apply_request(
            &instance_id,
            "pause",
            None,
            pause_plan["planId"].as_str().unwrap(),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(missing_authority.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        json_body(missing_authority).await["code"],
        "workflow_authority_required"
    );
    let paused = app
        .clone()
        .oneshot(workflow_operator_apply_request(
            &instance_id,
            "pause",
            None,
            pause_plan["planId"].as_str().unwrap(),
            Some("approved-workflow-control"),
        ))
        .await
        .unwrap();
    assert_eq!(paused.status(), StatusCode::OK);
    let paused = json_body(paused).await;
    assert_eq!(paused["disposition"], "applied");
    assert_eq!(
        paused["intervention"]["actorId"],
        "operator:incident-commander"
    );
    assert_eq!(
        paused["intervention"]["priorState"]["executionState"],
        "running"
    );
    assert_eq!(
        paused["intervention"]["resultingState"]["controlState"],
        "paused"
    );
    let duplicate_pause = app
        .clone()
        .oneshot(workflow_operator_apply_request(
            &instance_id,
            "pause",
            None,
            pause_plan["planId"].as_str().unwrap(),
            Some("approved-workflow-control"),
        ))
        .await
        .unwrap();
    assert_eq!(json_body(duplicate_pause).await["disposition"], "duplicate");

    let blocked_claims = claim_due_workflow_work_at(
        &state,
        "support-sla-worker-while-paused",
        claim_time + Duration::seconds(10),
        Duration::seconds(1),
        10,
    )
    .await
    .unwrap();
    assert!(blocked_claims.is_empty());
    let paused_inspection = app
        .clone()
        .oneshot(
            Request::get(format!(
                "/runtime/workflows/instances/{instance_id}?stepId={step_id}"
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    let paused_inspection = json_body(paused_inspection).await;
    assert_eq!(paused_inspection["instance"]["state"], "running");
    assert_eq!(paused_inspection["instance"]["control"]["state"], "paused");
    assert_eq!(
        paused_inspection["availableActions"],
        serde_json::json!(["resume", "cancel", "terminate", "intervene"])
    );
    assert_eq!(
        paused_inspection["selectedStep"]["timers"][0]["state"],
        "claimed"
    );

    let resume_plan = app
        .clone()
        .oneshot(workflow_operator_plan_request(&instance_id, "resume", None))
        .await
        .unwrap();
    let resume_plan = json_body(resume_plan).await;
    let resumed = app
        .clone()
        .oneshot(workflow_operator_apply_request(
            &instance_id,
            "resume",
            None,
            resume_plan["planId"].as_str().unwrap(),
            Some("approved-workflow-control"),
        ))
        .await
        .unwrap();
    assert_eq!(resumed.status(), StatusCode::OK);
    let resumed = json_body(resumed).await;
    assert_eq!(
        resumed["intervention"]["priorState"]["controlState"],
        "paused"
    );
    assert_eq!(
        resumed["intervention"]["resultingState"]["controlState"],
        "active"
    );
    let resumed_step_count: i64 = sqlx::query_scalar(
        "select count(*) from platform.service_workflow_steps where instance_id = $1",
    )
    .bind(&instance_id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(resumed_step_count, original_step_count);

    let first_failure = record_claimed_workflow_step_failure_at(
        &state,
        &claimed,
        WorkflowStepFailure::timeout("step_timeout", "support dependency timed out"),
        claim_time,
    )
    .await
    .unwrap();
    assert_eq!(
        first_failure.disposition,
        WorkflowFailureDisposition::RetryScheduled
    );
    let second_attempt_time = clock.advance(Duration::seconds(1));
    let second_attempt = claim_due_workflow_work_at(
        &state,
        "support-sla-worker-terminal-failure",
        second_attempt_time,
        Duration::seconds(5),
        10,
    )
    .await
    .unwrap()
    .remove(0);
    let terminal = record_claimed_workflow_step_failure_at(
        &state,
        &second_attempt,
        WorkflowStepFailure::permanent(
            "support_dependency_rejected",
            "support dependency rejected the attempt",
        ),
        second_attempt_time,
    )
    .await
    .unwrap();
    assert_eq!(terminal.disposition, WorkflowFailureDisposition::Exhausted);

    let failed_inspection = app
        .clone()
        .oneshot(
            Request::get(format!(
                "/runtime/workflows/instances/{instance_id}?stepId={step_id}"
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    let failed_inspection = json_body(failed_inspection).await;
    assert_eq!(failed_inspection["instance"]["state"], "failed");
    assert_eq!(failed_inspection["selectedStep"]["state"], "exhausted");
    assert_eq!(
        failed_inspection["selectedStep"]["attempts"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        failed_inspection["availableActions"],
        serde_json::json!(["retry", "cancel", "terminate", "intervene"])
    );

    let retry_plan = app
        .clone()
        .oneshot(workflow_operator_plan_request(
            &instance_id,
            "retry",
            Some(&step_id),
        ))
        .await
        .unwrap();
    assert_eq!(retry_plan.status(), StatusCode::OK);
    let retry_plan = json_body(retry_plan).await;
    assert_eq!(retry_plan["mutatesState"], false);
    assert_eq!(retry_plan["selectedStepId"], step_id);
    assert_eq!(
        retry_plan["affectedResources"]["attemptIds"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(retry_plan["resultingState"]["executionState"], "running");
    let denied = app
        .clone()
        .oneshot(workflow_operator_apply_request(
            &instance_id,
            "retry",
            Some(&step_id),
            retry_plan["planId"].as_str().unwrap(),
            Some("forged-workflow-control"),
        ))
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        json_body(denied).await["code"],
        "workflow_authorization_denied"
    );
    let retried = app
        .clone()
        .oneshot(workflow_operator_apply_request(
            &instance_id,
            "retry",
            Some(&step_id),
            retry_plan["planId"].as_str().unwrap(),
            Some("approved-workflow-control"),
        ))
        .await
        .unwrap();
    assert_eq!(retried.status(), StatusCode::OK);
    let retried = json_body(retried).await;
    assert_eq!(retried["disposition"], "applied");
    let attempt_transition_id = retried["intervention"]["attemptTransitionId"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(attempt_transition_id.starts_with(&format!("{step_id}:operator-retry:")));
    let duplicate_retry = app
        .clone()
        .oneshot(workflow_operator_apply_request(
            &instance_id,
            "retry",
            Some(&step_id),
            retry_plan["planId"].as_str().unwrap(),
            Some("approved-workflow-control"),
        ))
        .await
        .unwrap();
    assert_eq!(json_body(duplicate_retry).await["disposition"], "duplicate");

    let manual_claim = claim_due_workflow_work_at(
        &state,
        "support-sla-worker-manual-retry",
        second_attempt_time,
        Duration::seconds(5),
        10,
    )
    .await
    .unwrap()
    .remove(0);
    assert_eq!(manual_claim.step_id, step_id);
    assert_eq!(manual_claim.attempt_number, 3);
    assert_eq!(manual_claim.attempt_transition_id, attempt_transition_id);
    let final_state: (String, String, i64, serde_json::Value) = sqlx::query_as(
        r#"
        select instance.state, step.state, instance.control_revision,
               instance.workflow_context
        from platform.service_workflow_instances instance
        join platform.service_workflow_steps step on step.instance_id = instance.instance_id
        where instance.instance_id = $1 and step.step_id = $2
        "#,
    )
    .bind(&instance_id)
    .bind(&step_id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(final_state.0, "running");
    assert_eq!(final_state.1, "pending");
    assert_eq!(final_state.2, 3);
    assert_eq!(final_state.3, original_context);
    let interventions: Vec<(String, String, String, String)> = sqlx::query_as(
        r#"
        select action, actor_id, reason, next_action
        from platform.service_workflow_interventions
        where instance_id = $1
        order by ((resulting_state ->> 'controlRevision')::bigint), intervention_id
        "#,
    )
    .bind(&instance_id)
    .fetch_all(&db.pool)
    .await
    .unwrap();
    assert_eq!(interventions.len(), 3);
    assert_eq!(
        interventions
            .iter()
            .map(|entry| entry.0.as_str())
            .collect::<Vec<_>>(),
        vec!["pause", "resume", "retry"]
    );
    assert!(interventions.iter().all(|entry| {
        entry.1 == "operator:incident-commander"
            && entry.2 == "Contain and recover the support SLA incident"
            && !entry.3.is_empty()
    }));
    let operator_story_states: Vec<(String, String)> = sqlx::query_as(
        r#"
        select operation, status
        from platform.service_story_segments
        where workflow_instance_id = $1
          and contract_id = 'lenso.workflow-operator-result'
        order by feed_sequence
        "#,
    )
    .bind(&instance_id)
    .fetch_all(&db.pool)
    .await
    .unwrap();
    assert_eq!(
        operator_story_states,
        vec![
            ("workflow.instance.pause".to_owned(), "paused".to_owned()),
            ("workflow.instance.resume".to_owned(), "running".to_owned()),
            (
                "workflow.instance.retry".to_owned(),
                "retry_scheduled".to_owned()
            ),
        ]
    );

    drop(app);
    drop(state);
    force_cleanup_test_databases(vec![db]).await;
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn cooperative_cancel_stops_ordinary_work_and_selects_declared_compensation() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let initial_time = DateTime::parse_from_rfc3339("2026-07-18T01:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let clock = Arc::new(SystemSandboxWorkflowClock::new(initial_time));
    let manifest = manifest();
    let state = prepare_runtime(
        &service(),
        &runtime_config_with_workflow_authority(&manifest),
        db.pool.clone(),
        &[],
    )
    .await
    .unwrap()
    .with_workflow_clock(Arc::clone(&clock) as Arc<dyn platform_core::Clock>);
    let source = support_ticket_opened("support-event-cancel", "ticket_cancel");
    let (instance_id, pending_step_id) =
        complete_compensatable_support_effects(&state, &db.pool, &source).await;
    let app = service_router(OpenApiRouter::new(), state.clone());

    let plan = app
        .clone()
        .oneshot(workflow_operator_plan_request(&instance_id, "cancel", None))
        .await
        .unwrap();
    assert_eq!(plan.status(), StatusCode::OK);
    let plan = json_body(plan).await;
    assert_eq!(plan["mutatesState"], false);
    assert_eq!(plan["expectedTerminalState"], "cancelled");
    assert_eq!(plan["resultingState"]["executionState"], "compensating");
    assert_eq!(plan["approvalBoundary"], "workflow_terminal_operation");
    assert_eq!(
        plan["authorization"]["requiredAuthority"],
        "workflow_instance_cancel"
    );
    assert_eq!(
        plan["affectedResources"]["affectedStepIds"],
        serde_json::json!([pending_step_id])
    );
    assert_eq!(
        plan["affectedResources"]["compensationIds"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(
        plan["affectedResources"]["timerIds"]
            .as_array()
            .is_some_and(|timers| !timers.is_empty())
    );
    assert_eq!(
        plan["affectedResources"]["irreversibleEffects"],
        serde_json::json!([])
    );
    let intervention_count: i64 =
        sqlx::query_scalar("select count(*) from platform.service_workflow_interventions")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(intervention_count, 0, "cancel dry run must be read-only");

    let applied = app
        .clone()
        .oneshot(workflow_operator_apply_request(
            &instance_id,
            "cancel",
            None,
            plan["planId"].as_str().unwrap(),
            Some("approved-workflow-control"),
        ))
        .await
        .unwrap();
    assert_eq!(applied.status(), StatusCode::OK);
    let applied = json_body(applied).await;
    assert_eq!(applied["disposition"], "applied");
    assert_eq!(
        applied["intervention"]["expectedTerminalState"],
        "cancelled"
    );
    assert_eq!(
        applied["intervention"]["tenantScope"]["tenantId"],
        "tenant_01"
    );
    assert_eq!(
        applied["intervention"]["affectedResources"]["compensationIds"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let duplicate = app
        .clone()
        .oneshot(workflow_operator_apply_request(
            &instance_id,
            "cancel",
            None,
            plan["planId"].as_str().unwrap(),
            Some("approved-workflow-control"),
        ))
        .await
        .unwrap();
    assert_eq!(json_body(duplicate).await["disposition"], "duplicate");

    let stored: (String, Option<String>, String, String, i64) = sqlx::query_as(
        r#"
        select instance.state, instance.terminal_intent, step.state, timer.state,
               instance.control_revision
        from platform.service_workflow_instances instance
        join platform.service_workflow_steps step
          on step.instance_id = instance.instance_id and step.step_id = $2
        join platform.service_workflow_timers timer
          on timer.instance_id = instance.instance_id and timer.step_id = step.step_id
        where instance.instance_id = $1
        "#,
    )
    .bind(&instance_id)
    .bind(&pending_step_id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(stored.0, "compensating");
    assert_eq!(stored.1.as_deref(), Some("cancelled"));
    assert_eq!(stored.2, "cancelled");
    assert_eq!(stored.3, "cancelled");
    assert_eq!(stored.4, 1);
    let compensations: Vec<(String, String, String, i32)> = sqlx::query_as(
        r#"
        select compensation_id, state, selection_kind, execution_order
        from platform.service_workflow_compensations
        where instance_id = $1
        order by execution_order
        "#,
    )
    .bind(&instance_id)
    .fetch_all(&db.pool)
    .await
    .unwrap();
    assert_eq!(compensations.len(), 2);
    assert!(
        compensations
            .iter()
            .all(|entry| entry.1 == "pending" && entry.2 == "cancel")
    );
    assert_eq!(
        compensations
            .iter()
            .map(|entry| entry.3)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    let claims = claim_due_workflow_work_at(
        &state,
        "support-sla-worker-after-cancel",
        clock.advance(Duration::seconds(30)),
        Duration::seconds(5),
        10,
    )
    .await
    .unwrap();
    assert!(claims.is_empty());

    let inspection = app
        .clone()
        .oneshot(
            Request::get(format!("/runtime/workflows/instances/{instance_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let inspection = json_body(inspection).await;
    assert_eq!(inspection["instance"]["state"], "compensating");
    assert_eq!(
        inspection["instance"]["terminalOperation"]["action"],
        "cancel"
    );
    assert_eq!(
        inspection["instance"]["terminalOperation"]["expectedTerminalState"],
        "cancelled"
    );
    assert_eq!(
        inspection["instance"]["compensations"][0]["selectionKind"],
        "cancel"
    );
    assert_eq!(
        inspection["availableActions"],
        serde_json::json!(["pause", "terminate", "intervene"])
    );
    assert_eq!(
        inspection["nextActions"],
        serde_json::json!(["execute_next_workflow_compensation"])
    );
    let second_plan = app
        .clone()
        .oneshot(workflow_operator_plan_request(&instance_id, "cancel", None))
        .await
        .unwrap();
    assert_eq!(second_plan.status(), StatusCode::CONFLICT);
    assert_eq!(
        json_body(second_plan).await["code"],
        "workflow_action_not_eligible"
    );

    let mut final_compensation = None;
    for (compensation_id, _, _, _) in &compensations {
        let transition_id = format!("{compensation_id}:attempt:1");
        let mut dispatch = db.pool.begin().await.unwrap();
        let dispatched = dispatch_workflow_compensation_with_event_in_tx(
            &state,
            &mut dispatch,
            compensation_id,
            &transition_id,
            compensation_request_publication(&instance_id, compensation_id, &source),
        )
        .await
        .unwrap();
        assert_eq!(
            dispatched.workflow_state,
            lenso_autonomous_service::WorkflowInstanceState::Compensating
        );
        dispatch.commit().await.unwrap();

        let request_envelope: serde_json::Value = sqlx::query_scalar(
            "select envelope from platform.service_event_outbox where event_id = $1",
        )
        .bind(format!("{compensation_id}:request"))
        .fetch_one(&db.pool)
        .await
        .unwrap();
        let mut completion: EventEnvelope = serde_json::from_value(request_envelope).unwrap();
        completion.event_id = format!("{compensation_id}:completed");
        completion.event_type = "support.sla-compensated.v1".to_owned();
        completion.contract_id = "sla-compensated".to_owned();
        completion.contract_version = "v1".to_owned();
        completion.producer_service_id = "support".to_owned();
        completion.module_id = "support-ticket".to_owned();
        completion.content.schema =
            "contracts/events/support/support.sla-compensated.v1.schema.json".to_owned();
        completion.context.causation = Some(lenso_service::CausationContext {
            causation_id: format!("{compensation_id}:request"),
            correlation_id: completion
                .context
                .causation
                .as_ref()
                .and_then(|causation| causation.correlation_id.clone()),
        });
        let mut complete = db.pool.begin().await.unwrap();
        let completed =
            complete_workflow_compensation_from_event_in_tx(&state, &mut complete, &completion)
                .await
                .unwrap();
        complete.commit().await.unwrap();
        final_compensation = Some(completed);
    }
    let final_compensation = final_compensation.unwrap();
    assert_eq!(
        final_compensation.workflow_state,
        lenso_autonomous_service::WorkflowInstanceState::Cancelled
    );
    let final_inspection = app
        .clone()
        .oneshot(
            Request::get(format!("/runtime/workflows/instances/{instance_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let final_inspection = json_body(final_inspection).await;
    assert_eq!(final_inspection["instance"]["state"], "cancelled");
    assert_eq!(
        final_inspection["instance"]["terminalOperation"]["cleanupReported"],
        true
    );
    assert_eq!(final_inspection["pendingWork"], serde_json::json!([]));
    assert_eq!(
        final_inspection["availableActions"],
        serde_json::json!(["intervene"])
    );
    assert!(
        final_inspection["instance"]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .all(|effect| effect["state"] == "compensated")
    );
    assert!(
        final_inspection["instance"]["history"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| {
                entry["kind"] == "workflow_cancelled"
                    && entry["detail"]["finalOutcome"] == "cancelled"
            })
    );

    drop(app);
    drop(state);
    force_cleanup_test_databases(vec![db]).await;
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn terminate_is_strong_without_cleanup_and_human_intervention_is_audited() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let manifest = manifest();
    let state = prepare_runtime(
        &service(),
        &runtime_config_with_workflow_authority(&manifest),
        db.pool.clone(),
        &[],
    )
    .await
    .unwrap();
    let source = support_ticket_opened("support-event-terminate", "ticket_terminate");
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
    let instance_id = instance.instance_id;
    let step_id = instance.initial_step_id;
    let app = service_router(OpenApiRouter::new(), state.clone());

    let plan = app
        .clone()
        .oneshot(workflow_operator_plan_request(
            &instance_id,
            "terminate",
            None,
        ))
        .await
        .unwrap();
    assert_eq!(plan.status(), StatusCode::OK);
    let plan = json_body(plan).await;
    assert_eq!(plan["expectedTerminalState"], "terminated");
    assert_eq!(plan["resultingState"]["executionState"], "terminated");
    assert_eq!(plan["approvalBoundary"], "workflow_terminal_operation");
    assert_eq!(
        plan["authorization"]["requiredAuthority"],
        "workflow_instance_terminate"
    );
    assert_eq!(
        plan["affectedResources"]["affectedStepIds"],
        serde_json::json!([step_id.clone()])
    );
    assert!(
        plan["affectedResources"]["compensationIds"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let terminated = app
        .clone()
        .oneshot(workflow_operator_apply_request(
            &instance_id,
            "terminate",
            None,
            plan["planId"].as_str().unwrap(),
            Some("approved-workflow-control"),
        ))
        .await
        .unwrap();
    assert_eq!(terminated.status(), StatusCode::OK);
    let terminated = json_body(terminated).await;
    assert_eq!(terminated["disposition"], "applied");
    assert_eq!(
        terminated["intervention"]["approvalBoundary"],
        "workflow_terminal_operation"
    );
    let duplicate = app
        .clone()
        .oneshot(workflow_operator_apply_request(
            &instance_id,
            "terminate",
            None,
            plan["planId"].as_str().unwrap(),
            Some("approved-workflow-control"),
        ))
        .await
        .unwrap();
    assert_eq!(json_body(duplicate).await["disposition"], "duplicate");

    let terminal: (String, String, String, serde_json::Value) = sqlx::query_as(
        r#"
        select instance.state, step.state, timer.state, instance.terminal_evidence
        from platform.service_workflow_instances instance
        join platform.service_workflow_steps step
          on step.instance_id = instance.instance_id and step.step_id = $2
        join platform.service_workflow_timers timer
          on timer.instance_id = instance.instance_id and timer.step_id = step.step_id
        where instance.instance_id = $1
        "#,
    )
    .bind(&instance_id)
    .bind(&step_id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(terminal.0, "terminated");
    assert_eq!(terminal.1, "terminated");
    assert_eq!(terminal.2, "cancelled");
    assert_eq!(terminal.3["action"], "terminate");
    assert_eq!(terminal.3["compensationRequired"], false);
    assert_eq!(terminal.3["cleanupReported"], false);
    let compensation_count: i64 = sqlx::query_scalar(
        "select count(*) from platform.service_workflow_compensations where instance_id = $1",
    )
    .bind(&instance_id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(compensation_count, 0);

    let inspection = app
        .clone()
        .oneshot(
            Request::get(format!(
                "/runtime/workflows/instances/{instance_id}?stepId={step_id}"
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    let inspection = json_body(inspection).await;
    assert_eq!(inspection["instance"]["state"], "terminated");
    assert_eq!(inspection["pendingWork"], serde_json::json!([]));
    assert_eq!(
        inspection["availableActions"],
        serde_json::json!(["intervene"])
    );
    assert_eq!(
        inspection["nextActions"],
        serde_json::json!(["inspect_terminated_workflow_without_cleanup_assumptions"])
    );

    let intervention_plan = app
        .clone()
        .oneshot(workflow_operator_plan_request(
            &instance_id,
            "intervene",
            Some(&step_id),
        ))
        .await
        .unwrap();
    assert_eq!(intervention_plan.status(), StatusCode::OK);
    let intervention_plan = json_body(intervention_plan).await;
    assert_eq!(
        intervention_plan["approvalBoundary"],
        "workflow_human_intervention"
    );
    assert_eq!(
        intervention_plan["authorization"]["requiredAuthority"],
        "workflow_human_intervention"
    );
    assert_eq!(
        intervention_plan["resultingState"]["executionState"],
        "terminated"
    );
    let intervened = app
        .clone()
        .oneshot(workflow_operator_apply_request(
            &instance_id,
            "intervene",
            Some(&step_id),
            intervention_plan["planId"].as_str().unwrap(),
            Some("approved-workflow-control"),
        ))
        .await
        .unwrap();
    assert_eq!(intervened.status(), StatusCode::OK);
    let intervened = json_body(intervened).await;
    assert_eq!(intervened["disposition"], "applied");
    assert_eq!(
        intervened["intervention"]["actorId"],
        "operator:incident-commander"
    );
    assert_eq!(
        intervened["intervention"]["tenantScope"]["tenantId"],
        "tenant_01"
    );
    assert_eq!(intervened["intervention"]["stepId"], step_id);
    assert_eq!(
        intervened["intervention"]["affectedResources"]["selectedStepId"],
        step_id
    );
    let duplicate_intervention = app
        .clone()
        .oneshot(workflow_operator_apply_request(
            &instance_id,
            "intervene",
            Some(&step_id),
            intervention_plan["planId"].as_str().unwrap(),
            Some("approved-workflow-control"),
        ))
        .await
        .unwrap();
    assert_eq!(
        json_body(duplicate_intervention).await["disposition"],
        "duplicate"
    );
    let fresh_terminate = app
        .clone()
        .oneshot(workflow_operator_plan_request(
            &instance_id,
            "terminate",
            None,
        ))
        .await
        .unwrap();
    assert_eq!(fresh_terminate.status(), StatusCode::CONFLICT);

    let interventions: Vec<(String, String, Option<serde_json::Value>, serde_json::Value)> =
        sqlx::query_as(
            r#"
            select action, approval_boundary, tenant_scope, affected_resources
            from platform.service_workflow_interventions
            where instance_id = $1
            order by recorded_at, intervention_id
            "#,
        )
        .bind(&instance_id)
        .fetch_all(&db.pool)
        .await
        .unwrap();
    assert_eq!(interventions.len(), 2);
    assert_eq!(interventions[0].0, "terminate");
    assert_eq!(interventions[0].1, "workflow_terminal_operation");
    assert_eq!(interventions[1].0, "intervene");
    assert_eq!(interventions[1].1, "workflow_human_intervention");
    assert!(
        interventions
            .iter()
            .all(|entry| entry.2.as_ref().unwrap()["tenantId"] == "tenant_01")
    );
    assert_eq!(interventions[1].3["selectedStepId"], step_id);
    let operator_story_states: Vec<(String, String)> = sqlx::query_as(
        r#"
        select operation, status
        from platform.service_story_segments
        where workflow_instance_id = $1
          and contract_id = 'lenso.workflow-operator-result'
        order by feed_sequence
        "#,
    )
    .bind(&instance_id)
    .fetch_all(&db.pool)
    .await
    .unwrap();
    assert_eq!(
        operator_story_states,
        vec![
            (
                "workflow.instance.terminate".to_owned(),
                "terminated".to_owned()
            ),
            (
                "workflow.instance.intervene".to_owned(),
                "intervention_recorded".to_owned()
            ),
        ]
    );

    drop(app);
    drop(state);
    force_cleanup_test_databases(vec![db]).await;
}
