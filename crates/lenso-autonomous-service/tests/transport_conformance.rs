use async_trait::async_trait;
use lenso_autonomous_service::{
    LocalTransportAdapter, NatsJetStreamConsumerBinding, NatsJetStreamTransportAdapter,
    NatsJetStreamTransportConfig, ServiceEventHandler, ServiceEventHandlerError, TransportAdapter,
    TransportDeploymentClass, TransportHealthStatus, TransportNegativeAcknowledgement,
    TransportPublication, consume_service_events_once_without_workload_identity, prepare_runtime,
};
use lenso_service::{
    AuthenticatedTransportBinding, AutonomousServiceContract, AutonomousServiceStore,
    AutonomousServiceWorkload, EventEnvelope, GeneratedEventContract, ServiceTenancyMode,
    WorkloadRole, validate_event_envelope,
};
use platform_testing::TestDatabase;
use sqlx::{Postgres, Transaction};
use std::time::Duration;

const CONSUMER_ID: &str = "support-sla";

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

#[derive(Debug)]
struct SupportSlaHandler;

#[async_trait]
impl ServiceEventHandler for SupportSlaHandler {
    async fn handle(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        envelope: &EventEnvelope,
    ) -> Result<(), ServiceEventHandlerError> {
        sqlx::query(
            "insert into support_sla_escalations (ticket_id, source_event_id) values ($1, $2)",
        )
        .bind(envelope.content.data["ticketId"].as_str().unwrap())
        .bind(&envelope.event_id)
        .execute(&mut **transaction)
        .await
        .map_err(ServiceEventHandlerError::store)?;
        Ok(())
    }
}

#[test]
fn jetstream_adapter_is_a_production_transport() {
    fn assert_production_adapter<T: TransportAdapter>() {}

    assert_production_adapter::<NatsJetStreamTransportAdapter>();
    assert_eq!(
        NatsJetStreamTransportAdapter::DEPLOYMENT_CLASS,
        TransportDeploymentClass::Production,
    );
}

async fn receive_one(
    adapter: &dyn TransportAdapter,
    consumer_id: &str,
) -> lenso_autonomous_service::TransportDelivery {
    for _ in 0..20 {
        if let Some(delivery) = adapter.receive(consumer_id, 1).await.unwrap().pop() {
            return delivery;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("transport did not deliver an Event Envelope before the conformance deadline");
}

async fn begin_transport_conformance(adapter: &dyn TransportAdapter, prefix: &str) -> String {
    assert_eq!(
        adapter.health().await.unwrap().status,
        TransportHealthStatus::Ready
    );

    let acknowledged_event_id = format!("{prefix}-acknowledged");
    adapter
        .publish(TransportPublication {
            consumer_id: CONSUMER_ID.to_owned(),
            envelope: support_ticket_opened(&acknowledged_event_id, "ticket-acknowledged"),
        })
        .await
        .unwrap();
    let delivery = receive_one(adapter, CONSUMER_ID).await;
    assert_eq!(delivery.attempt, 1);
    adapter.acknowledge(&delivery).await.unwrap();

    let negative_event_id = format!("{prefix}-negative-acknowledgement");
    adapter
        .publish(TransportPublication {
            consumer_id: CONSUMER_ID.to_owned(),
            envelope: support_ticket_opened(&negative_event_id, "ticket-negative"),
        })
        .await
        .unwrap();
    let delivery = receive_one(adapter, CONSUMER_ID).await;
    adapter
        .negative_acknowledge(
            &delivery,
            TransportNegativeAcknowledgement {
                reason: "temporary conformance interruption".to_owned(),
                retryable: true,
            },
        )
        .await
        .unwrap();
    let redelivery = receive_one(adapter, CONSUMER_ID).await;
    assert_eq!(redelivery.envelope.event_id, negative_event_id);
    assert!(redelivery.attempt >= 2);
    adapter.acknowledge(&redelivery).await.unwrap();

    let interrupted_event_id = format!("{prefix}-interrupted");
    adapter
        .publish(TransportPublication {
            consumer_id: CONSUMER_ID.to_owned(),
            envelope: support_ticket_opened(&interrupted_event_id, "ticket-interrupted"),
        })
        .await
        .unwrap();
    let interrupted = receive_one(adapter, CONSUMER_ID).await;
    assert_eq!(interrupted.envelope.event_id, interrupted_event_id);
    interrupted_event_id
}

async fn finish_transport_conformance(
    restarted_adapter: &dyn TransportAdapter,
    interrupted_event_id: &str,
) {
    let redelivery = receive_one(restarted_adapter, CONSUMER_ID).await;
    assert_eq!(redelivery.envelope.event_id, interrupted_event_id);
    assert!(redelivery.attempt >= 2);
    restarted_adapter.acknowledge(&redelivery).await.unwrap();

    let diagnostics = restarted_adapter.diagnostics().await.unwrap();
    assert!(
        diagnostics.iter().any(|entry| {
            entry.event_id == interrupted_event_id && entry.outcome == "published"
        })
    );
    assert!(diagnostics.iter().any(|entry| {
        entry.event_id == interrupted_event_id && entry.outcome == "acknowledged"
    }));
}

async fn assert_authoritative_support_behavior(
    adapter: &dyn TransportAdapter,
    consumer: &TestDatabase,
    event_id: &str,
) {
    let state = prepare_runtime(
        &service("support-sla", "support-sla-store"),
        &lenso_autonomous_service::ServiceRuntimeConfig::new(
            "support-sla",
            "support-sla-store",
            "support-sla",
        ),
        consumer.pool.clone(),
        &[platform_core::Migration {
            name: "support-sla/conformance_create_escalations",
            sql: "create table if not exists support_sla_escalations (ticket_id text primary key, source_event_id text not null unique);",
        }],
    )
    .await
    .unwrap();
    adapter
        .publish(TransportPublication {
            consumer_id: CONSUMER_ID.to_owned(),
            envelope: support_ticket_opened(event_id, event_id),
        })
        .await
        .unwrap();
    assert_eq!(
        consume_service_events_once_without_workload_identity(
            &state,
            adapter,
            CONSUMER_ID,
            &SupportSlaHandler,
            1,
        )
        .await
        .unwrap(),
        1
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
}

#[tokio::test]
async fn local_adapter_passes_the_shared_transport_conformance_suite() {
    let Some(transport_store) = TestDatabase::create().await else {
        return;
    };
    let Some(consumer) = TestDatabase::create().await else {
        transport_store.cleanup().await;
        return;
    };
    let adapter = LocalTransportAdapter::prepare(transport_store.pool.clone())
        .await
        .unwrap();
    let interrupted_event_id = begin_transport_conformance(&adapter, "local-conformance").await;
    drop(adapter);
    let restarted_adapter = LocalTransportAdapter::prepare(transport_store.pool.clone())
        .await
        .unwrap();
    finish_transport_conformance(&restarted_adapter, &interrupted_event_id).await;
    assert_authoritative_support_behavior(
        &restarted_adapter,
        &consumer,
        "local-authoritative-support",
    )
    .await;

    drop(restarted_adapter);
    consumer.cleanup().await;
    transport_store.cleanup().await;
}

struct JetStreamFixture {
    context: async_nats::jetstream::Context,
    url: String,
    stream_name: String,
    config: NatsJetStreamTransportConfig,
}

async fn create_jetstream_fixture() -> Option<JetStreamFixture> {
    if std::env::var("LENSO_NATS_TEST_INFRASTRUCTURE_APPROVED").as_deref() != Ok("true") {
        eprintln!(
            "skipping NATS JetStream conformance: LENSO_NATS_TEST_INFRASTRUCTURE_APPROVED=true is not set"
        );
        return None;
    }
    let url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".to_owned());
    let client = async_nats::connect(&url)
        .await
        .expect("approved NATS JetStream conformance infrastructure must be reachable");
    let context = async_nats::jetstream::new(client.clone());
    let suffix = uuid::Uuid::now_v7().simple().to_string();
    let stream_name = format!("LENSO_CONFORMANCE_{suffix}").to_uppercase();
    let subject = format!("lenso.conformance.{suffix}.support_sla");
    let durable_consumer_name = format!("support_sla_{suffix}");
    let stream = context
        .create_stream(async_nats::jetstream::stream::Config {
            name: stream_name.clone(),
            subjects: vec![subject.clone()],
            max_age: Duration::from_secs(300),
            storage: async_nats::jetstream::stream::StorageType::File,
            num_replicas: 1,
            ..Default::default()
        })
        .await
        .unwrap();
    stream
        .create_consumer(async_nats::jetstream::consumer::pull::Config {
            durable_name: Some(durable_consumer_name.clone()),
            filter_subject: subject.clone(),
            ack_policy: async_nats::jetstream::consumer::AckPolicy::Explicit,
            ack_wait: Duration::from_millis(300),
            max_deliver: 10,
            ..Default::default()
        })
        .await
        .unwrap();
    let config = NatsJetStreamTransportConfig::new(
        &stream_name,
        AuthenticatedTransportBinding::new("nats-jetstream:conformance"),
    )
    .with_consumer(
        CONSUMER_ID,
        NatsJetStreamConsumerBinding::new(subject, durable_consumer_name),
    )
    .with_receive_timeout(Duration::from_millis(100));
    Some(JetStreamFixture {
        context,
        url,
        stream_name,
        config,
    })
}

#[tokio::test]
async fn jetstream_adapter_passes_real_environment_conformance() {
    let Some(diagnostic_store) = TestDatabase::create().await else {
        return;
    };
    let Some(consumer) = TestDatabase::create().await else {
        diagnostic_store.cleanup().await;
        return;
    };
    let Some(fixture) = create_jetstream_fixture().await else {
        consumer.cleanup().await;
        diagnostic_store.cleanup().await;
        return;
    };
    let adapter_client = async_nats::connect(&fixture.url).await.unwrap();
    let adapter = NatsJetStreamTransportAdapter::bind(
        adapter_client,
        diagnostic_store.pool.clone(),
        fixture.config.clone(),
    )
    .await
    .unwrap();
    assert_eq!(
        adapter.deployment_class(),
        TransportDeploymentClass::Production
    );
    let interrupted_event_id = begin_transport_conformance(&adapter, "nats-conformance").await;
    drop(adapter);

    tokio::time::sleep(Duration::from_millis(350)).await;
    let restarted_client = async_nats::connect(&fixture.url).await.unwrap();
    let restarted_adapter = NatsJetStreamTransportAdapter::bind(
        restarted_client,
        diagnostic_store.pool.clone(),
        fixture.config.clone(),
    )
    .await
    .unwrap();
    finish_transport_conformance(&restarted_adapter, &interrupted_event_id).await;
    assert_authoritative_support_behavior(
        &restarted_adapter,
        &consumer,
        "nats-authoritative-support",
    )
    .await;

    let restart_event_id = "nats-restart-support";
    restarted_adapter
        .publish(TransportPublication {
            consumer_id: CONSUMER_ID.to_owned(),
            envelope: support_ticket_opened(restart_event_id, restart_event_id),
        })
        .await
        .unwrap();
    let interrupted = receive_one(&restarted_adapter, CONSUMER_ID).await;
    assert_eq!(interrupted.envelope.event_id, restart_event_id);
    drop(interrupted);
    drop(restarted_adapter);

    tokio::time::sleep(Duration::from_millis(350)).await;
    let restarted_client = async_nats::connect(&fixture.url).await.unwrap();
    let restarted_adapter = NatsJetStreamTransportAdapter::bind(
        restarted_client,
        diagnostic_store.pool.clone(),
        fixture.config.clone(),
    )
    .await
    .unwrap();
    let state = prepare_runtime(
        &service("support-sla", "support-sla-store"),
        &lenso_autonomous_service::ServiceRuntimeConfig::new(
            "support-sla",
            "support-sla-store",
            "support-sla",
        ),
        consumer.pool.clone(),
        &[platform_core::Migration {
            name: "support-sla/conformance_create_escalations",
            sql: "create table if not exists support_sla_escalations (ticket_id text primary key, source_event_id text not null unique);",
        }],
    )
    .await
    .unwrap();
    assert_eq!(
        consume_service_events_once_without_workload_identity(
            &state,
            &restarted_adapter,
            CONSUMER_ID,
            &SupportSlaHandler,
            1,
        )
        .await
        .unwrap(),
        1
    );
    restarted_adapter
        .publish(TransportPublication {
            consumer_id: CONSUMER_ID.to_owned(),
            envelope: support_ticket_opened(restart_event_id, restart_event_id),
        })
        .await
        .unwrap();
    assert_eq!(
        consume_service_events_once_without_workload_identity(
            &state,
            &restarted_adapter,
            CONSUMER_ID,
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
        .bind(restart_event_id)
        .fetch_one(&consumer.pool)
        .await
        .unwrap(),
        1
    );

    drop(state);
    drop(restarted_adapter);
    fixture
        .context
        .delete_stream(&fixture.stream_name)
        .await
        .unwrap();
    consumer.cleanup().await;
    diagnostic_store.cleanup().await;
}
