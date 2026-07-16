use lenso_autonomous_service::{
    LocalTransportAdapter, NatsJetStreamTransportAdapter, TransportAdapter,
    TransportDeploymentClass, TransportHealthStatus, TransportNegativeAcknowledgement,
    TransportPublication,
};
use lenso_service::{EventEnvelope, GeneratedEventContract, validate_event_envelope};
use platform_testing::TestDatabase;
use std::time::Duration;

#[path = "support/jetstream.rs"]
mod jetstream_fixture;
use jetstream_fixture::JetStreamFixture;

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

#[tokio::test]
async fn local_adapter_passes_the_shared_transport_conformance_suite() {
    let Some(transport_store) = TestDatabase::create().await else {
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

    drop(restarted_adapter);
    transport_store.cleanup().await;
}

#[tokio::test]
async fn jetstream_adapter_passes_real_environment_conformance() {
    let Some(diagnostic_store) = TestDatabase::create().await else {
        return;
    };
    let Some(fixture) = JetStreamFixture::create(CONSUMER_ID).await else {
        diagnostic_store.cleanup().await;
        return;
    };
    let adapter_client = async_nats::connect(&fixture.url).await.unwrap();
    let adapter = NatsJetStreamTransportAdapter::bind(
        adapter_client.clone(),
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
    adapter_client.drain().await.unwrap();
    for _ in 0..20 {
        if adapter.health().await.unwrap().status == TransportHealthStatus::Unavailable {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(
        adapter.health().await.unwrap().status,
        TransportHealthStatus::Unavailable
    );
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
    drop(restarted_adapter);
    fixture.cleanup().await;
    diagnostic_store.cleanup().await;
}
