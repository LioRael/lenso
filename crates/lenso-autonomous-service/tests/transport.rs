use async_trait::async_trait;
use axum::body::Body;
use http::{Request, StatusCode};
use http_body_util::BodyExt as _;
use lenso_autonomous_service::{
    LocalTransportAdapter, ServiceEventHandler, ServiceEventPublisher, TransportAdapter,
    TransportDelivery, TransportDiagnostic, TransportError, TransportErrorCode, TransportHealth,
    TransportHealthStatus, TransportNegativeAcknowledgement, TransportPublication,
    TransportPublicationReceipt, consume_service_events_once, prepare_runtime,
    relay_service_events_once,
};
use lenso_service::{
    AutonomousServiceContract, AutonomousServiceStore, AutonomousServiceWorkload, EventEnvelope,
    GeneratedEventContract, ServiceTenancyMode, WorkloadRole, validate_event_envelope,
};
use platform_testing::TestDatabase;
use sqlx::{Postgres, Transaction};
use std::sync::atomic::{AtomicBool, Ordering};
use tower::ServiceExt as _;
use utoipa_axum::router::OpenApiRouter;

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
struct PublishThenFailAdapter<'a> {
    adapter: &'a LocalTransportAdapter,
    should_fail: AtomicBool,
}

#[derive(Debug)]
struct FailingAcknowledgementAdapter<'a>(&'a LocalTransportAdapter);

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
        consume_service_events_once(
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
        consume_service_events_once(
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
        consume_service_events_once(
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
        consume_service_events_once(
            &consumer_state,
            &adapter,
            "support-sla",
            &SupportSlaHandler,
            1,
        ),
        consume_service_events_once(
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
    adapter
        .publish(TransportPublication {
            consumer_id: "support-sla".to_owned(),
            envelope: support_ticket_opened("support-event-retry", "ticket_retry"),
        })
        .await
        .unwrap();
    let retryable_error = consume_service_events_once(
        &consumer_state,
        &adapter,
        "support-sla",
        &retry_once_handler,
        1,
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
        consume_service_events_once(
            &consumer_state,
            &adapter,
            "support-sla",
            &retry_once_handler,
            1,
        ),
        consume_service_events_once(
            &consumer_state,
            &adapter,
            "support-sla",
            &retry_once_handler,
            1,
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
        consume_service_events_once(
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
    let rejected = consume_service_events_once(
        &consumer_state,
        &adapter,
        "support-sla",
        &watermark_handler,
        1,
    )
    .await
    .unwrap_err();
    assert_eq!(rejected.code, TransportErrorCode::HandlerFailed);
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
    let error = consume_service_events_once(
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
        consume_service_events_once(
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
