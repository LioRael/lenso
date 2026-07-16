use lenso_autonomous_service::{NatsJetStreamConsumerBinding, NatsJetStreamTransportConfig};
use lenso_service::AuthenticatedTransportBinding;
use std::time::Duration;

pub(crate) struct JetStreamFixture {
    context: async_nats::jetstream::Context,
    pub(crate) url: String,
    stream_name: String,
    pub(crate) config: NatsJetStreamTransportConfig,
}

impl JetStreamFixture {
    pub(crate) async fn create(consumer_id: &str) -> Option<Self> {
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
        let context = async_nats::jetstream::new(client);
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
            consumer_id,
            NatsJetStreamConsumerBinding {
                subject,
                durable_consumer_name,
            },
        )
        .with_receive_timeout(Duration::from_millis(100));
        Some(Self {
            context,
            url,
            stream_name,
            config,
        })
    }

    pub(crate) async fn cleanup(self) {
        self.context.delete_stream(&self.stream_name).await.unwrap();
    }
}
