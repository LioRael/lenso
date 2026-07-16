use async_trait::async_trait;
use axum::body::Body;
use http::{Request, StatusCode};
use http_body_util::BodyExt as _;
use lenso_autonomous_service::{
    DeliveryFailureReason, LocalTransportAdapter, NatsJetStreamTransportAdapter,
    ServiceEventHandler, ServiceEventHandlerError, ServiceEventPublisher, ServiceEventRetryPolicy,
    ServiceEventWorkloadIdentity, TransportAdapter, TransportDelivery, TransportDiagnostic,
    TransportError, TransportErrorCode, TransportFailureDisposition, TransportHealth,
    TransportHealthStatus, TransportNegativeAcknowledgement, TransportPublication,
    TransportPublicationReceipt, consume_service_events_once_at,
    consume_service_events_once_at_without_workload_identity,
    consume_service_events_once_without_workload_identity, prepare_runtime,
    relay_service_events_once,
};
use lenso_service::{
    AutonomousServiceContract, AutonomousServiceStore, AutonomousServiceWorkload, EventEnvelope,
    GeneratedEventContract, ServiceTenancyMode, SystemSandboxWorkloadIdentityProvider,
    WorkloadCredentialRequest, WorkloadIdentityProvider, WorkloadRole, validate_event_envelope,
};
use platform_testing::TestDatabase;
use sqlx::{Postgres, Transaction};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tower::ServiceExt as _;
use utoipa_axum::router::OpenApiRouter;

#[path = "support/jetstream.rs"]
mod jetstream_fixture;
use jetstream_fixture::JetStreamFixture;

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

fn support_ticket_opened(event_id: &str, ticket_id: &str) -> EventEnvelope {
    let mut envelope: EventEnvelope = serde_json::from_str(include_str!(
        "../../../contracts/events/support/support.ticket-opened.v1.envelope.json"
    ))
    .unwrap();
    event_id.clone_into(&mut envelope.event_id);
    envelope.content.data["ticketId"] = serde_json::json!(ticket_id);
    let contract: GeneratedEventContract = serde_json::from_str(include_str!(
        "../../../contracts/events/support/support.ticket-opened.v1.artifact.json"
    ))
    .unwrap();
    assert_eq!(validate_event_envelope(&contract, &envelope), vec![]);
    envelope
}

#[test]
fn support_flow_uses_the_authoritative_event_envelope() {
    let envelope = support_ticket_opened("support-event-contract-proof", "ticket_contract_proof");

    assert_eq!(envelope.contract_id, "ticket-opened");
    assert_eq!(envelope.contract_version, "v1");
    assert_eq!(envelope.producer_service_id, "support");
    assert_eq!(envelope.tenancy_mode, ServiceTenancyMode::Required);
}

#[test]
fn delivery_failures_use_stable_protocol_neutral_reasons() {
    let failures = [
        ServiceEventHandlerError::retryable("dependency_unavailable", "temporary outage"),
        ServiceEventHandlerError::rejected_with_code("invalid_state", "invalid state"),
        ServiceEventHandlerError::expired("event_deadline_elapsed", "deadline elapsed"),
        ServiceEventHandlerError::unauthorized("identity_not_allowed", "identity denied"),
        ServiceEventHandlerError::incompatible("contract_not_supported", "contract mismatch"),
        ServiceEventHandlerError::poison("invalid_support_payload", "payload cannot be handled"),
    ];

    assert_eq!(
        failures.each_ref().map(|failure| failure.failure_reason),
        [
            DeliveryFailureReason::Retryable,
            DeliveryFailureReason::NonRetryable,
            DeliveryFailureReason::Expired,
            DeliveryFailureReason::Unauthorized,
            DeliveryFailureReason::Incompatible,
            DeliveryFailureReason::Poison,
        ]
    );
    assert_eq!(failures[0].reason_code, "dependency_unavailable");
    assert_eq!(failures[0].message, "temporary outage");
}

#[tokio::test]
async fn event_consumer_authenticates_service_principal_before_module_handling() {
    let Some(consumer) = TestDatabase::create().await else {
        return;
    };
    let Some(transport_store) = TestDatabase::create().await else {
        consumer.cleanup().await;
        return;
    };
    let consumer_state = prepare_runtime(
        &service("support-sla", "support-sla-store"),
        &lenso_autonomous_service::ServiceRuntimeConfig::new(
            "support-sla",
            "support-sla-store",
            "support-sla",
        ),
        consumer.pool.clone(),
        &[platform_core::Migration {
            name: "support-sla/0001_create_escalations",
            sql: "create table support_sla_escalations (ticket_id text primary key, source_event_id text not null);",
        }],
    )
    .await
    .unwrap();
    let adapter = LocalTransportAdapter::prepare(transport_store.pool.clone())
        .await
        .unwrap();
    let provider = Arc::new(
        SystemSandboxWorkloadIdentityProvider::new("local", "event-sandbox-secret").unwrap(),
    );
    let now = chrono::Utc::now();
    let now_ms = u64::try_from(now.timestamp_millis()).unwrap();
    let credential = provider
        .issue(WorkloadCredentialRequest::new(
            "service:support",
            "support-sla",
            "sandbox-event:local-transport",
            now_ms,
            30_000,
        ))
        .unwrap();
    let receiver = ServiceEventWorkloadIdentity::new(provider, "support-sla");

    adapter
        .publish(TransportPublication {
            consumer_id: "support-sla".to_owned(),
            envelope: support_ticket_opened("event-unauthenticated", "ticket-denied"),
        })
        .await
        .unwrap();
    assert_eq!(
        consume_service_events_once_at(
            &consumer_state,
            &adapter,
            "support-sla",
            &SupportSlaHandler,
            1,
            now,
            &ServiceEventRetryPolicy::default(),
            &receiver,
        )
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("select count(*) from support_sla_escalations")
            .fetch_one(&consumer.pool)
            .await
            .unwrap(),
        0
    );

    let mut authenticated = support_ticket_opened("event-authenticated", "ticket-accepted");
    authenticated.context.service_principal = Some(credential.service_principal_context());
    adapter
        .publish(TransportPublication {
            consumer_id: "support-sla".to_owned(),
            envelope: authenticated,
        })
        .await
        .unwrap();
    assert_eq!(
        consume_service_events_once_at(
            &consumer_state,
            &adapter,
            "support-sla",
            &SupportSlaHandler,
            1,
            now + chrono::Duration::milliseconds(1),
            &ServiceEventRetryPolicy::default(),
            &receiver,
        )
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "select ticket_id from support_sla_escalations where source_event_id = 'event-authenticated'",
        )
        .fetch_one(&consumer.pool)
        .await
        .unwrap(),
        "ticket-accepted"
    );

    drop(consumer_state);
    drop(adapter);
    consumer.cleanup().await;
    transport_store.cleanup().await;
}

#[derive(Debug)]
struct SupportSlaHandler;

#[derive(Debug)]
struct SupportSlaWatermarkHandler {
    accepted_after: chrono::DateTime<chrono::FixedOffset>,
}

#[derive(Debug)]
struct RetryOnceSupportSlaHandler {
    should_fail: AtomicBool,
}

#[derive(Debug)]
struct ClassifiedFailureSupportSlaHandler;

#[derive(Debug)]
struct PublishThenFailAdapter<'a> {
    adapter: &'a LocalTransportAdapter,
    should_fail: AtomicBool,
}

#[derive(Debug)]
struct FailingAcknowledgementAdapter<'a>(&'a LocalTransportAdapter);

#[derive(Debug)]
struct ReversingDeliveryAdapter<'a>(&'a LocalTransportAdapter);

#[async_trait]
impl TransportAdapter for PublishThenFailAdapter<'_> {
    async fn publish(
        &self,
        publication: TransportPublication,
    ) -> Result<TransportPublicationReceipt, TransportError> {
        let receipt = self.adapter.publish(publication).await?;
        if self.should_fail.swap(false, Ordering::SeqCst) {
            return Err(TransportError::new(
                TransportErrorCode::DeliveryFailed,
                "producer stopped before recording the publication receipt",
            ));
        }
        Ok(receipt)
    }

    async fn receive(
        &self,
        consumer_id: &str,
        limit: i64,
    ) -> Result<Vec<TransportDelivery>, TransportError> {
        self.adapter.receive(consumer_id, limit).await
    }

    async fn acknowledge(&self, delivery: &TransportDelivery) -> Result<(), TransportError> {
        self.adapter.acknowledge(delivery).await
    }

    async fn negative_acknowledge(
        &self,
        delivery: &TransportDelivery,
        acknowledgement: TransportNegativeAcknowledgement,
    ) -> Result<(), TransportError> {
        self.adapter
            .negative_acknowledge(delivery, acknowledgement)
            .await
    }

    async fn health(&self) -> Result<TransportHealth, TransportError> {
        self.adapter.health().await
    }

    async fn diagnostics(&self) -> Result<Vec<TransportDiagnostic>, TransportError> {
        self.adapter.diagnostics().await
    }
}

#[async_trait]
impl TransportAdapter for FailingAcknowledgementAdapter<'_> {
    async fn publish(
        &self,
        publication: TransportPublication,
    ) -> Result<TransportPublicationReceipt, TransportError> {
        self.0.publish(publication).await
    }

    async fn receive(
        &self,
        consumer_id: &str,
        limit: i64,
    ) -> Result<Vec<TransportDelivery>, TransportError> {
        self.0.receive(consumer_id, limit).await
    }

    async fn acknowledge(&self, _delivery: &TransportDelivery) -> Result<(), TransportError> {
        Err(TransportError::new(
            TransportErrorCode::DeliveryFailed,
            "acknowledgement unavailable",
        ))
    }

    async fn negative_acknowledge(
        &self,
        delivery: &TransportDelivery,
        acknowledgement: TransportNegativeAcknowledgement,
    ) -> Result<(), TransportError> {
        self.0.negative_acknowledge(delivery, acknowledgement).await
    }

    async fn health(&self) -> Result<TransportHealth, TransportError> {
        self.0.health().await
    }

    async fn diagnostics(&self) -> Result<Vec<TransportDiagnostic>, TransportError> {
        self.0.diagnostics().await
    }
}

#[async_trait]
impl TransportAdapter for ReversingDeliveryAdapter<'_> {
    async fn publish(
        &self,
        publication: TransportPublication,
    ) -> Result<TransportPublicationReceipt, TransportError> {
        self.0.publish(publication).await
    }

    async fn receive(
        &self,
        consumer_id: &str,
        limit: i64,
    ) -> Result<Vec<TransportDelivery>, TransportError> {
        let mut deliveries = self.0.receive(consumer_id, limit).await?;
        deliveries.reverse();
        Ok(deliveries)
    }

    async fn receive_at(
        &self,
        consumer_id: &str,
        limit: i64,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<TransportDelivery>, TransportError> {
        let mut deliveries = self.0.receive_at(consumer_id, limit, now).await?;
        deliveries.reverse();
        Ok(deliveries)
    }

    async fn acknowledge(&self, delivery: &TransportDelivery) -> Result<(), TransportError> {
        self.0.acknowledge(delivery).await
    }

    async fn negative_acknowledge(
        &self,
        delivery: &TransportDelivery,
        acknowledgement: TransportNegativeAcknowledgement,
    ) -> Result<(), TransportError> {
        self.0.negative_acknowledge(delivery, acknowledgement).await
    }

    async fn record_failure(
        &self,
        delivery: &TransportDelivery,
        disposition: TransportFailureDisposition,
    ) -> Result<(), TransportError> {
        self.0.record_failure(delivery, disposition).await
    }

    async fn health(&self) -> Result<TransportHealth, TransportError> {
        self.0.health().await
    }

    async fn diagnostics(&self) -> Result<Vec<TransportDiagnostic>, TransportError> {
        self.0.diagnostics().await
    }
}

#[async_trait]
impl ServiceEventHandler for SupportSlaHandler {
    async fn handle(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        envelope: &EventEnvelope,
    ) -> Result<(), lenso_autonomous_service::ServiceEventHandlerError> {
        sqlx::query(
            "insert into support_sla_escalations (ticket_id, source_event_id) values ($1, $2)",
        )
        .bind(envelope.content.data["ticketId"].as_str().unwrap())
        .bind(&envelope.event_id)
        .execute(&mut **transaction)
        .await
        .map_err(lenso_autonomous_service::ServiceEventHandlerError::store)?;
        Ok(())
    }
}

#[async_trait]
impl ServiceEventHandler for SupportSlaWatermarkHandler {
    async fn handle(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        envelope: &EventEnvelope,
    ) -> Result<(), lenso_autonomous_service::ServiceEventHandlerError> {
        let occurred_at = chrono::DateTime::parse_from_rfc3339(&envelope.occurred_at)
            .expect("validated Event Envelope must carry an RFC 3339 occurredAt");
        if occurred_at < self.accepted_after {
            return Err(
                lenso_autonomous_service::ServiceEventHandlerError::rejected_with_code(
                    "support_event_out_of_order",
                    "Support event occurred before the accepted SLA watermark",
                ),
            );
        }
        SupportSlaHandler.handle(transaction, envelope).await
    }
}

#[async_trait]
impl ServiceEventHandler for RetryOnceSupportSlaHandler {
    async fn handle(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        envelope: &EventEnvelope,
    ) -> Result<(), lenso_autonomous_service::ServiceEventHandlerError> {
        if self.should_fail.swap(false, Ordering::SeqCst) {
            return Err(
                lenso_autonomous_service::ServiceEventHandlerError::retryable(
                    "support_sla_temporarily_unavailable",
                    "Support SLA handler is temporarily unavailable",
                ),
            );
        }
        SupportSlaHandler.handle(transaction, envelope).await
    }
}

#[async_trait]
impl ServiceEventHandler for ClassifiedFailureSupportSlaHandler {
    async fn handle(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        envelope: &EventEnvelope,
    ) -> Result<(), ServiceEventHandlerError> {
        match envelope.event_id.as_str() {
            "support-event-poison" => Err(ServiceEventHandlerError::poison(
                "invalid_support_payload",
                "Support payload cannot be handled",
            )),
            "support-event-exhausted" => Err(ServiceEventHandlerError::retryable(
                "support_sla_temporarily_unavailable",
                "Support SLA handler is temporarily unavailable",
            )),
            _ => SupportSlaHandler.handle(transaction, envelope).await,
        }
    }
}

#[tokio::test]
async fn controlled_retries_dead_letter_poison_without_blocking_healthy_events() {
    let Some(consumer) = TestDatabase::create().await else {
        return;
    };
    let Some(transport_store) = TestDatabase::create().await else {
        consumer.cleanup().await;
        return;
    };
    let consumer_state = prepare_runtime(
        &service("support-sla", "support-sla-store"),
        &lenso_autonomous_service::ServiceRuntimeConfig::new(
            "support-sla",
            "support-sla-store",
            "support-sla",
        ),
        consumer.pool.clone(),
        &[platform_core::Migration {
            name: "support-sla/0001_create_escalations",
            sql: "create table support_sla_escalations (ticket_id text primary key, source_event_id text not null);",
        }],
    )
    .await
    .unwrap();
    let adapter = LocalTransportAdapter::prepare(transport_store.pool.clone())
        .await
        .unwrap();
    let policy = ServiceEventRetryPolicy::new(2, vec![chrono::Duration::seconds(5)]);
    let now = chrono::DateTime::parse_from_rfc3339("2026-07-15T09:00:00Z")
        .unwrap()
        .to_utc();

    for (event_id, ticket_id) in [
        ("support-event-poison", "ticket_poison"),
        ("support-event-healthy", "ticket_healthy"),
    ] {
        adapter
            .publish(TransportPublication {
                consumer_id: "support-sla".to_owned(),
                envelope: support_ticket_opened(event_id, ticket_id),
            })
            .await
            .unwrap();
    }
    assert_eq!(
        consume_service_events_once_at_without_workload_identity(
            &consumer_state,
            &adapter,
            "support-sla",
            &ClassifiedFailureSupportSlaHandler,
            10,
            now,
            &policy,
        )
        .await
        .unwrap(),
        1
    );
    let healthy_effects: i64 = sqlx::query_scalar(
        "select count(*) from support_sla_escalations where source_event_id = 'support-event-healthy'",
    )
    .fetch_one(&consumer.pool)
    .await
    .unwrap();
    assert_eq!(healthy_effects, 1);

    let poison_dead_letter: (
        String,
        String,
        String,
        i32,
        serde_json::Value,
        serde_json::Value,
        serde_json::Value,
    ) = sqlx::query_as(
        r"
            select failure_reason, contract_id, contract_version, attempt_count,
                   delivery_history, next_actions, envelope
            from platform.service_event_dead_letters
            where consumer_id = 'support-sla' and event_id = 'support-event-poison'
            ",
    )
    .fetch_one(&consumer.pool)
    .await
    .unwrap();
    assert_eq!(poison_dead_letter.0, "poison");
    assert_eq!(poison_dead_letter.1, "ticket-opened");
    assert_eq!(poison_dead_letter.2, "v1");
    assert_eq!(poison_dead_letter.3, 1);
    assert_eq!(poison_dead_letter.4.as_array().unwrap().len(), 1);
    assert_eq!(
        poison_dead_letter.5,
        serde_json::json!(["inspect_payload", "correct_producer", "replay_event"])
    );
    assert_eq!(poison_dead_letter.6["eventId"], "support-event-poison");

    adapter
        .publish(TransportPublication {
            consumer_id: "support-sla".to_owned(),
            envelope: support_ticket_opened("support-event-exhausted", "ticket_exhausted"),
        })
        .await
        .unwrap();
    adapter
        .publish(TransportPublication {
            consumer_id: "support-sla".to_owned(),
            envelope: support_ticket_opened(
                "support-event-healthy-after-retry",
                "ticket_healthy_after_retry",
            ),
        })
        .await
        .unwrap();
    let first_failure = consume_service_events_once_at_without_workload_identity(
        &consumer_state,
        &adapter,
        "support-sla",
        &ClassifiedFailureSupportSlaHandler,
        2,
        now,
        &policy,
    )
    .await
    .unwrap_err();
    assert_eq!(first_failure.code, TransportErrorCode::HandlerFailed);
    let healthy_after_retry: i64 = sqlx::query_scalar(
        "select count(*) from support_sla_escalations where source_event_id = 'support-event-healthy-after-retry'",
    )
    .fetch_one(&consumer.pool)
    .await
    .unwrap();
    assert_eq!(healthy_after_retry, 1);
    let scheduled: (
        i32,
        chrono::DateTime<chrono::Utc>,
        Option<String>,
        i32,
        serde_json::Value,
    ) = sqlx::query_as(
        r"
        select attempt_count, next_attempt_at, terminal_outcome,
               max_attempts, retry_schedule
        from platform.service_event_inbox
        where consumer_id = 'support-sla' and event_id = 'support-event-exhausted'
        ",
    )
    .fetch_one(&consumer.pool)
    .await
    .unwrap();
    assert_eq!(scheduled.0, 1);
    assert_eq!(scheduled.1, now + chrono::Duration::seconds(5));
    assert_eq!(scheduled.2, None);
    assert_eq!(scheduled.3, 2);
    assert_eq!(scheduled.4, serde_json::json!([5000]));

    let mut changed_redelivery =
        support_ticket_opened("support-event-exhausted", "ticket_changed_on_redelivery");
    changed_redelivery.content.data["priority"] = serde_json::json!("changed");
    adapter
        .publish(TransportPublication {
            consumer_id: "support-sla".to_owned(),
            envelope: changed_redelivery,
        })
        .await
        .unwrap();

    assert_eq!(
        consume_service_events_once_at_without_workload_identity(
            &consumer_state,
            &adapter,
            "support-sla",
            &ClassifiedFailureSupportSlaHandler,
            1,
            now + chrono::Duration::seconds(4),
            &policy,
        )
        .await
        .unwrap(),
        0
    );
    let drifted_policy = ServiceEventRetryPolicy::new(
        10,
        vec![chrono::Duration::hours(1), chrono::Duration::hours(2)],
    );
    assert_eq!(
        consume_service_events_once_at_without_workload_identity(
            &consumer_state,
            &ReversingDeliveryAdapter(&adapter),
            "support-sla",
            &ClassifiedFailureSupportSlaHandler,
            2,
            now + chrono::Duration::seconds(5),
            &drifted_policy,
        )
        .await
        .unwrap(),
        0
    );
    let exhausted: (String, i32, String, serde_json::Value, serde_json::Value) = sqlx::query_as(
        r"
        select failure_reason, attempt_count, terminal_outcome, delivery_history, envelope
        from platform.service_event_dead_letters
        where consumer_id = 'support-sla' and event_id = 'support-event-exhausted'
        ",
    )
    .fetch_one(&consumer.pool)
    .await
    .unwrap();
    assert_eq!(exhausted.0, "exhausted");
    assert_eq!(exhausted.1, 2);
    assert_eq!(exhausted.2, "dead_lettered");
    assert_eq!(exhausted.3.as_array().unwrap().len(), 2);
    assert_eq!(
        exhausted.4["content"]["data"]["ticketId"],
        "ticket_exhausted"
    );
    let diagnostics = adapter.diagnostics().await.unwrap();
    assert!(diagnostics.iter().any(|entry| {
        entry.event_id == "support-event-poison"
            && entry.outcome == "failure_recorded"
            && entry.detail["failureReason"] == "poison"
            && entry.detail["reasonCode"] == "invalid_support_payload"
    }));
    assert!(diagnostics.iter().any(|entry| {
        entry.event_id == "support-event-exhausted"
            && entry.detail["failureReason"] == "exhausted"
            && entry.detail["terminalOutcome"] == "dead_lettered"
    }));
    let terminal_evidence: (String, String, String) = sqlx::query_as(
        r"
        select outcome, detail ->> 'failureReason', detail ->> 'terminalOutcome'
        from platform.service_event_delivery_evidence
        where event_id = 'support-event-exhausted' and outcome = 'dead_lettered'
        ",
    )
    .fetch_one(&consumer.pool)
    .await
    .unwrap();
    assert_eq!(
        terminal_evidence,
        (
            "dead_lettered".to_owned(),
            "exhausted".to_owned(),
            "dead_lettered".to_owned(),
        )
    );

    drop(consumer_state);
    drop(adapter);
    consumer.cleanup().await;
    transport_store.cleanup().await;
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn local_transport_delivers_support_event_from_outbox_through_inbox() {
    let Some(producer) = TestDatabase::create().await else {
        return;
    };
    let Some(consumer) = TestDatabase::create().await else {
        producer.cleanup().await;
        return;
    };
    let Some(transport_store) = TestDatabase::create().await else {
        producer.cleanup().await;
        consumer.cleanup().await;
        return;
    };

    let producer_state = prepare_runtime(
        &service("support", "support-store"),
        &lenso_autonomous_service::ServiceRuntimeConfig::new("support", "support-store", "support"),
        producer.pool.clone(),
        &[platform_core::Migration {
            name: "support-ticket/0001_create_tickets",
            sql: "create table support_tickets (id text primary key, priority text not null);",
        }],
    )
    .await
    .unwrap();
    let consumer_state = prepare_runtime(
        &service("support-sla", "support-sla-store"),
        &lenso_autonomous_service::ServiceRuntimeConfig::new(
            "support-sla",
            "support-sla-store",
            "support-sla",
        ),
        consumer.pool.clone(),
        &[platform_core::Migration {
            name: "support-sla/0001_create_escalations",
            sql: "create table support_sla_escalations (ticket_id text primary key, source_event_id text not null);",
        }],
    )
    .await
    .unwrap();
    let adapter = LocalTransportAdapter::prepare(transport_store.pool.clone())
        .await
        .unwrap();

    let envelope = support_ticket_opened("support-event-1", "ticket_01");
    let mut transaction = producer.pool.begin().await.unwrap();
    sqlx::query("insert into support_tickets (id, priority) values ('ticket_01', 'urgent')")
        .execute(&mut *transaction)
        .await
        .unwrap();
    ServiceEventPublisher
        .publish_in_tx(&mut transaction, "support-sla", &envelope)
        .await
        .unwrap();
    transaction.commit().await.unwrap();

    assert_eq!(
        relay_service_events_once(&producer_state, &adapter, 10)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        consume_service_events_once_without_workload_identity(
            &consumer_state,
            &adapter,
            "support-sla",
            &SupportSlaHandler,
            10,
        )
        .await
        .unwrap(),
        1
    );

    let effect: (String, String) =
        sqlx::query_as("select ticket_id, source_event_id from support_sla_escalations")
            .fetch_one(&consumer.pool)
            .await
            .unwrap();
    assert_eq!(
        effect,
        ("ticket_01".to_owned(), "support-event-1".to_owned())
    );

    let outbox: (String, i32) = sqlx::query_as(
        "select status, attempts from platform.service_event_outbox where event_id = 'support-event-1'",
    )
    .fetch_one(&producer.pool)
    .await
    .unwrap();
    assert_eq!(outbox, ("published".to_owned(), 1));
    drop(producer_state);
    let producer_state = prepare_runtime(
        &service("support", "support-store"),
        &lenso_autonomous_service::ServiceRuntimeConfig::new("support", "support-store", "support"),
        producer.pool.clone(),
        &[],
    )
    .await
    .unwrap();
    assert_eq!(
        relay_service_events_once(&producer_state, &adapter, 10)
            .await
            .unwrap(),
        0
    );

    let producer_restart_envelope =
        support_ticket_opened("support-event-producer-restart", "ticket_producer_restart");
    let mut transaction = producer.pool.begin().await.unwrap();
    sqlx::query(
        "insert into support_tickets (id, priority) values ('ticket_producer_restart', 'urgent')",
    )
    .execute(&mut *transaction)
    .await
    .unwrap();
    ServiceEventPublisher
        .publish_in_tx(&mut transaction, "support-sla", &producer_restart_envelope)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    let publish_then_fail = PublishThenFailAdapter {
        adapter: &adapter,
        should_fail: AtomicBool::new(true),
    };
    let publication_error = relay_service_events_once(&producer_state, &publish_then_fail, 10)
        .await
        .unwrap_err();
    assert_eq!(publication_error.code, TransportErrorCode::DeliveryFailed);
    drop(producer_state);
    let producer_state = prepare_runtime(
        &service("support", "support-store"),
        &lenso_autonomous_service::ServiceRuntimeConfig::new("support", "support-store", "support"),
        producer.pool.clone(),
        &[],
    )
    .await
    .unwrap();
    assert_eq!(
        relay_service_events_once(&producer_state, &adapter, 10)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        consume_service_events_once_without_workload_identity(
            &consumer_state,
            &adapter,
            "support-sla",
            &SupportSlaHandler,
            10,
        )
        .await
        .unwrap(),
        1
    );
    let producer_restart_effects: i64 = sqlx::query_scalar(
        "select count(*) from support_sla_escalations where source_event_id = 'support-event-producer-restart'",
    )
    .fetch_one(&consumer.pool)
    .await
    .unwrap();
    assert_eq!(producer_restart_effects, 1);
    let producer_restart_duplicates: i64 = sqlx::query_scalar(
        "select count(*) from platform.service_event_delivery_evidence where event_id = 'support-event-producer-restart' and stage = 'inbox' and outcome = 'duplicate'",
    )
    .fetch_one(&consumer.pool)
    .await
    .unwrap();
    assert_eq!(producer_restart_duplicates, 1);
    let producer_restart_outbox: (String, i32) = sqlx::query_as(
        "select status, attempts from platform.service_event_outbox where event_id = 'support-event-producer-restart'",
    )
    .fetch_one(&producer.pool)
    .await
    .unwrap();
    assert_eq!(producer_restart_outbox, ("published".to_owned(), 2));
    let inbox: String = sqlx::query_scalar(
        "select status from platform.service_event_inbox where event_id = 'support-event-1'",
    )
    .fetch_one(&consumer.pool)
    .await
    .unwrap();
    assert_eq!(inbox, "completed");

    let evidence =
        lenso_autonomous_service::service_router(OpenApiRouter::new(), consumer_state.clone())
            .oneshot(
                Request::get("/runtime/event-deliveries")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
    assert_eq!(evidence.status(), StatusCode::OK);
    let body = evidence.into_body().collect().await.unwrap().to_bytes();
    let evidence: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(evidence.as_array().unwrap().iter().any(|entry| {
        entry["eventId"] == "support-event-1" && entry["outcome"] == "acknowledged"
    }));

    assert_eq!(
        adapter.health().await.unwrap().status,
        TransportHealthStatus::Ready
    );
    let diagnostics = adapter.diagnostics().await.unwrap();
    assert!(diagnostics.iter().any(|entry| entry.outcome == "published"));
    assert!(
        diagnostics
            .iter()
            .any(|entry| entry.outcome == "acknowledged")
    );

    adapter
        .publish(TransportPublication {
            consumer_id: "support-sla".to_owned(),
            envelope: support_ticket_opened("support-event-1", "ticket_01"),
        })
        .await
        .unwrap();
    assert_eq!(
        consume_service_events_once_without_workload_identity(
            &consumer_state,
            &adapter,
            "support-sla",
            &SupportSlaHandler,
            1,
        )
        .await
        .unwrap(),
        0
    );
    let effect_count: i64 = sqlx::query_scalar(
        "select count(*) from support_sla_escalations where source_event_id = 'support-event-1'",
    )
    .fetch_one(&consumer.pool)
    .await
    .unwrap();
    assert_eq!(effect_count, 1);
    let duplicate_count: i64 = sqlx::query_scalar(
        "select count(*) from platform.service_event_delivery_evidence where event_id = 'support-event-1' and stage = 'inbox' and outcome = 'duplicate'",
    )
    .fetch_one(&consumer.pool)
    .await
    .unwrap();
    assert_eq!(duplicate_count, 1);

    for _ in 0..2 {
        adapter
            .publish(TransportPublication {
                consumer_id: "support-sla".to_owned(),
                envelope: support_ticket_opened("support-event-concurrent", "ticket_concurrent"),
            })
            .await
            .unwrap();
    }
    let (first, second) = tokio::join!(
        consume_service_events_once_without_workload_identity(
            &consumer_state,
            &adapter,
            "support-sla",
            &SupportSlaHandler,
            1,
        ),
        consume_service_events_once_without_workload_identity(
            &consumer_state,
            &adapter,
            "support-sla",
            &SupportSlaHandler,
            1,
        ),
    );
    let mut completed = [first.unwrap(), second.unwrap()];
    completed.sort_unstable();
    assert_eq!(completed, [0, 1]);
    let concurrent_effect_count: i64 = sqlx::query_scalar(
        "select count(*) from support_sla_escalations where source_event_id = 'support-event-concurrent'",
    )
    .fetch_one(&consumer.pool)
    .await
    .unwrap();
    assert_eq!(concurrent_effect_count, 1);
    let concurrent_duplicate_count: i64 = sqlx::query_scalar(
        "select count(*) from platform.service_event_delivery_evidence where event_id = 'support-event-concurrent' and stage = 'inbox' and outcome = 'duplicate'",
    )
    .fetch_one(&consumer.pool)
    .await
    .unwrap();
    assert_eq!(concurrent_duplicate_count, 1);

    let retry_once_handler = RetryOnceSupportSlaHandler {
        should_fail: AtomicBool::new(true),
    };
    let retry_now = chrono::DateTime::parse_from_rfc3339("2026-07-15T09:00:00Z")
        .unwrap()
        .to_utc();
    let immediate_retry_policy = ServiceEventRetryPolicy::new(3, vec![chrono::Duration::zero()]);
    adapter
        .publish(TransportPublication {
            consumer_id: "support-sla".to_owned(),
            envelope: support_ticket_opened("support-event-retry", "ticket_retry"),
        })
        .await
        .unwrap();
    let retryable_error = consume_service_events_once_at_without_workload_identity(
        &consumer_state,
        &adapter,
        "support-sla",
        &retry_once_handler,
        1,
        retry_now,
        &immediate_retry_policy,
    )
    .await
    .unwrap_err();
    assert_eq!(retryable_error.code, TransportErrorCode::HandlerFailed);
    adapter
        .publish(TransportPublication {
            consumer_id: "support-sla".to_owned(),
            envelope: support_ticket_opened("support-event-retry", "ticket_retry"),
        })
        .await
        .unwrap();
    let (first_retry, second_retry) = tokio::join!(
        consume_service_events_once_at_without_workload_identity(
            &consumer_state,
            &adapter,
            "support-sla",
            &retry_once_handler,
            1,
            retry_now,
            &immediate_retry_policy,
        ),
        consume_service_events_once_at_without_workload_identity(
            &consumer_state,
            &adapter,
            "support-sla",
            &retry_once_handler,
            1,
            retry_now,
            &immediate_retry_policy,
        ),
    );
    let mut retry_completions = [first_retry.unwrap(), second_retry.unwrap()];
    retry_completions.sort_unstable();
    assert_eq!(retry_completions, [0, 1]);
    let retry_effect_count: i64 = sqlx::query_scalar(
        "select count(*) from support_sla_escalations where source_event_id = 'support-event-retry'",
    )
    .fetch_one(&consumer.pool)
    .await
    .unwrap();
    assert_eq!(retry_effect_count, 1);
    let retry_inbox_status: String = sqlx::query_scalar(
        "select status from platform.service_event_inbox where consumer_id = 'support-sla' and event_id = 'support-event-retry'",
    )
    .fetch_one(&consumer.pool)
    .await
    .unwrap();
    assert_eq!(retry_inbox_status, "completed");

    let watermark_handler = SupportSlaWatermarkHandler {
        accepted_after: chrono::DateTime::parse_from_rfc3339("2026-07-14T10:00:00Z").unwrap(),
    };
    let mut newer = support_ticket_opened("support-event-newer", "ticket_newer");
    newer.occurred_at = "2026-07-14T11:15:30Z".to_owned();
    adapter
        .publish(TransportPublication {
            consumer_id: "support-sla".to_owned(),
            envelope: newer,
        })
        .await
        .unwrap();
    assert_eq!(
        consume_service_events_once_without_workload_identity(
            &consumer_state,
            &adapter,
            "support-sla",
            &watermark_handler,
            1,
        )
        .await
        .unwrap(),
        1
    );
    let mut delayed = support_ticket_opened("support-event-delayed", "ticket_delayed");
    delayed.occurred_at = "2026-07-14T09:15:30Z".to_owned();
    adapter
        .publish(TransportPublication {
            consumer_id: "support-sla".to_owned(),
            envelope: delayed,
        })
        .await
        .unwrap();
    assert_eq!(
        consume_service_events_once_without_workload_identity(
            &consumer_state,
            &adapter,
            "support-sla",
            &watermark_handler,
            1,
        )
        .await
        .unwrap(),
        0
    );
    let rejected_inbox: (String, String) = sqlx::query_as(
        "select status, last_error from platform.service_event_inbox where consumer_id = 'support-sla' and event_id = 'support-event-delayed'",
    )
    .fetch_one(&consumer.pool)
    .await
    .unwrap();
    assert_eq!(
        rejected_inbox,
        (
            "rejected".to_owned(),
            "Support event occurred before the accepted SLA watermark".to_owned(),
        )
    );
    let rejected_reason_code: String = sqlx::query_scalar(
        "select detail ->> 'reasonCode' from platform.service_event_delivery_evidence where event_id = 'support-event-delayed' and stage = 'inbox' and outcome = 'rejected'",
    )
    .fetch_one(&consumer.pool)
    .await
    .unwrap();
    assert_eq!(rejected_reason_code, "support_event_out_of_order");

    let acknowledgement_failure_receipt = adapter
        .publish(TransportPublication {
            consumer_id: "support-sla".to_owned(),
            envelope: support_ticket_opened("support-event-ack-failure", "ticket_ack_failure"),
        })
        .await
        .unwrap();
    let error = consume_service_events_once_without_workload_identity(
        &consumer_state,
        &FailingAcknowledgementAdapter(&adapter),
        "support-sla",
        &SupportSlaHandler,
        1,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, TransportErrorCode::DeliveryFailed);
    let outcomes: Vec<String> = sqlx::query_scalar(
        "select outcome from platform.service_event_delivery_evidence where event_id = 'support-event-ack-failure' order by recorded_at",
    )
    .fetch_all(&consumer.pool)
    .await
    .unwrap();
    assert!(outcomes.contains(&"acknowledgement_failed".to_owned()));
    assert!(!outcomes.contains(&"acknowledged".to_owned()));

    adapter
        .publish(TransportPublication {
            consumer_id: "support-sla".to_owned(),
            envelope: support_ticket_opened("support-event-nack", "ticket_nack"),
        })
        .await
        .unwrap();
    let delivery = adapter.receive("support-sla", 1).await.unwrap().remove(0);
    adapter
        .negative_acknowledge(
            &delivery,
            TransportNegativeAcknowledgement {
                reason: "support-sla temporarily unavailable".to_owned(),
                retryable: true,
            },
        )
        .await
        .unwrap();
    let redelivery = adapter.receive("support-sla", 1).await.unwrap().remove(0);
    assert_eq!(redelivery.attempt, 2);
    adapter
        .negative_acknowledge(
            &redelivery,
            TransportNegativeAcknowledgement {
                reason: "unsupported event".to_owned(),
                retryable: false,
            },
        )
        .await
        .unwrap();

    drop(consumer_state);
    drop(adapter);
    let consumer_state = prepare_runtime(
        &service("support-sla", "support-sla-store"),
        &lenso_autonomous_service::ServiceRuntimeConfig::new(
            "support-sla",
            "support-sla-store",
            "support-sla",
        ),
        consumer.pool.clone(),
        &[],
    )
    .await
    .unwrap();
    let adapter = LocalTransportAdapter::prepare(transport_store.pool.clone())
        .await
        .unwrap();
    assert_eq!(
        consume_service_events_once_without_workload_identity(
            &consumer_state,
            &adapter,
            "support-sla",
            &SupportSlaHandler,
            1,
        )
        .await
        .unwrap(),
        0
    );
    let restart_diagnostics = adapter.diagnostics().await.unwrap();
    assert!(restart_diagnostics.iter().any(|entry| {
        entry.delivery_id == acknowledgement_failure_receipt.delivery_id
            && entry.outcome == "recovered_unacknowledged"
    }));
    assert!(restart_diagnostics.iter().any(|entry| {
        entry.delivery_id == acknowledgement_failure_receipt.delivery_id
            && entry.outcome == "acknowledged"
    }));
    let restart_duplicate_count: i64 = sqlx::query_scalar(
        "select count(*) from platform.service_event_delivery_evidence where event_id = 'support-event-ack-failure' and stage = 'inbox' and outcome = 'duplicate'",
    )
    .fetch_one(&consumer.pool)
    .await
    .unwrap();
    assert_eq!(restart_duplicate_count, 1);

    drop(producer_state);
    drop(consumer_state);
    drop(adapter);
    producer.cleanup().await;
    consumer.cleanup().await;
    transport_store.cleanup().await;
}

#[tokio::test]
async fn jetstream_restart_preserves_authoritative_support_behavior_once() {
    let Some(diagnostic_store) = TestDatabase::create().await else {
        return;
    };
    let Some(consumer) = TestDatabase::create().await else {
        diagnostic_store.cleanup().await;
        return;
    };
    let Some(fixture) = JetStreamFixture::create("support-sla").await else {
        consumer.cleanup().await;
        diagnostic_store.cleanup().await;
        return;
    };
    let consumer_state = prepare_runtime(
        &service("support-sla", "support-sla-store"),
        &lenso_autonomous_service::ServiceRuntimeConfig::new(
            "support-sla",
            "support-sla-store",
            "support-sla",
        ),
        consumer.pool.clone(),
        &[platform_core::Migration {
            name: "support-sla/0001_create_escalations",
            sql: "create table support_sla_escalations (ticket_id text primary key, source_event_id text not null);",
        }],
    )
    .await
    .unwrap();
    let adapter = NatsJetStreamTransportAdapter::bind(
        async_nats::connect(&fixture.url).await.unwrap(),
        diagnostic_store.pool.clone(),
        fixture.config.clone(),
    )
    .await
    .unwrap();
    let event_id = "nats-support-event-after-restart";
    adapter
        .publish(TransportPublication {
            consumer_id: "support-sla".to_owned(),
            envelope: support_ticket_opened(event_id, "ticket_nats_restart"),
        })
        .await
        .unwrap();
    let interrupted = adapter.receive("support-sla", 1).await.unwrap().remove(0);
    assert_eq!(interrupted.envelope.event_id, event_id);
    drop(interrupted);
    drop(adapter);

    tokio::time::sleep(std::time::Duration::from_millis(350)).await;
    let restarted_adapter = NatsJetStreamTransportAdapter::bind(
        async_nats::connect(&fixture.url).await.unwrap(),
        diagnostic_store.pool.clone(),
        fixture.config.clone(),
    )
    .await
    .unwrap();
    assert_eq!(
        consume_service_events_once_without_workload_identity(
            &consumer_state,
            &restarted_adapter,
            "support-sla",
            &SupportSlaHandler,
            1,
        )
        .await
        .unwrap(),
        1
    );
    restarted_adapter
        .publish(TransportPublication {
            consumer_id: "support-sla".to_owned(),
            envelope: support_ticket_opened(event_id, "ticket_nats_restart"),
        })
        .await
        .unwrap();
    assert_eq!(
        consume_service_events_once_without_workload_identity(
            &consumer_state,
            &restarted_adapter,
            "support-sla",
            &SupportSlaHandler,
            1,
        )
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "select count(*) from support_sla_escalations where source_event_id = $1",
        )
        .bind(event_id)
        .fetch_one(&consumer.pool)
        .await
        .unwrap(),
        1
    );

    drop(consumer_state);
    drop(restarted_adapter);
    fixture.cleanup().await;
    consumer.cleanup().await;
    diagnostic_store.cleanup().await;
}
