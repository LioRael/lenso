use async_trait::async_trait;
use axum::Extension;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use lenso_autonomous_service::{
    ServiceRuntimeConfig, StorySegmentFeedConfig, StorySegmentRecord, StorySegmentTenantAccess,
    append_story_segment, prepare_runtime, service_router,
};
use lenso_service::{
    AuthenticatedTransportBinding, AutonomousServiceContract, AutonomousServiceStore,
    AutonomousServiceWorkload, ReliabilityContract, ReliabilityProfile,
    ReliabilityProfileOverrides, SchemaArtifactReference, ServiceTenancyMode, StorySegment,
    StorySegmentContract, StorySegmentFeed, StorySegmentOperation, StorySegmentSource,
    SystemSandboxWorkloadIdentityProvider, WorkloadCredentialRequest, WorkloadIdentityProvider,
    WorkloadRole,
};
use platform_core::{
    AppError, AppResult, ErrorCode, InMemoryTelemetrySpanProvider, PLATFORM_MIGRATIONS,
    TelemetrySpan, apply_migrations,
};
use platform_testing::TestDatabase;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
    time::Duration,
};
use story::{
    federation::{
        FederatedRuntimeStory, FederatedStoryAggregator, FederatedStoryEnrichmentProvider,
        FederatedStoryFeedClient, FederatedStoryFeedError, FederatedStoryFeedRequest,
        FederatedStoryGapKind, FederatedStorySegment, FederatedStorySource,
        FederatedStoryTechnicalEvidence, FederatedStoryTechnicalEvidenceKind,
        HttpFederatedStoryFeedClient, OpenTelemetryFederatedStoryEnrichmentProvider,
        StaticStorySegmentFeedCredentialProvider,
    },
    migrations::STORY_MIGRATIONS,
};
use utoipa_axum::router::OpenApiRouter;

const TENANT_A: &str = "tenant_a";
const TENANT_B: &str = "tenant_b";
const AGGREGATOR_PRINCIPAL: &str = "service:story-aggregator";
const TRANSPORT_BINDING: &str = "spiffe://lenso.test/service/story-aggregator";
const CURSOR_KEY: &[u8] = b"federated-story-test-cursor-key-minimum-32-bytes";

fn service(service_id: &str) -> AutonomousServiceContract {
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
        ServiceTenancyMode::Required,
        vec!["local".to_owned()],
    );
    service.stores = vec![AutonomousServiceStore::new("primary", service_id)];
    if service_id == "support-sla" {
        let mut reliability = ReliabilityContract::new(
            "support-reliability",
            "v1",
            SchemaArtifactReference::new("contracts/reliability/support.v1.schema.json"),
            "99.9%",
            "43m per 30d",
        );
        reliability.profile = ReliabilityProfile::Critical;
        reliability.overrides = ReliabilityProfileOverrides {
            workflow_backlog_limit: Some(5),
            ..ReliabilityProfileOverrides::default()
        };
        service.reliability_contract = Some(reliability);
    }
    service
}

fn audience(service_id: &str) -> String {
    format!("service:{service_id}/story-segment-feed")
}

fn runtime_config(
    service_id: &str,
    provider: Arc<SystemSandboxWorkloadIdentityProvider>,
) -> ServiceRuntimeConfig {
    ServiceRuntimeConfig::new(service_id, "primary", service_id).with_story_segment_feed(
        StorySegmentFeedConfig::new(
            provider,
            audience(service_id),
            Duration::from_secs(24 * 60 * 60),
            CURSOR_KEY,
        )
        .with_reader(
            AGGREGATOR_PRINCIPAL,
            StorySegmentTenantAccess::Tenants(vec![TENANT_A.to_owned()]),
        ),
    )
}

fn local_segment(
    story_id: &str,
    segment_id: &str,
    tenant_id: &str,
    status: &str,
) -> StorySegmentRecord {
    StorySegmentRecord::new(
        story_id,
        segment_id,
        "event_contract",
        format!("support.{segment_id}"),
        "support-event",
        "v1",
        status,
        Utc::now(),
    )
    .with_tenant(tenant_id)
}

fn issue_credential(
    provider: &SystemSandboxWorkloadIdentityProvider,
    feed_audience: &str,
) -> String {
    provider
        .issue(WorkloadCredentialRequest::new(
            AGGREGATOR_PRINCIPAL,
            feed_audience,
            TRANSPORT_BINDING,
            now_ms(),
            60_000,
        ))
        .unwrap()
        .token
}

async fn spawn_feed(
    state: lenso_autonomous_service::ServiceRuntimeState,
) -> (String, tokio::task::JoinHandle<()>) {
    let app = service_router(OpenApiRouter::new(), state).layer(Extension(
        AuthenticatedTransportBinding::new(TRANSPORT_BINDING),
    ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{address}"), task)
}

async fn prepare_aggregation_store(db: &TestDatabase) {
    let migrations = PLATFORM_MIGRATIONS
        .iter()
        .chain(STORY_MIGRATIONS)
        .copied()
        .collect::<Vec<_>>();
    apply_migrations(&db.pool, &migrations).await.unwrap();
}

fn sources(ticket_url: &str, sla_url: &str) -> Vec<FederatedStorySource> {
    vec![
        FederatedStorySource::new(
            "support-ticket",
            ticket_url,
            audience("support-ticket"),
            Duration::from_secs(300),
        ),
        FederatedStorySource::new(
            "support-sla",
            sla_url,
            audience("support-sla"),
            Duration::from_secs(300),
        ),
    ]
}

#[derive(Debug)]
struct FailedEnrichment;

#[async_trait]
impl FederatedStoryEnrichmentProvider for FailedEnrichment {
    async fn enrich(
        &self,
        _segments: &[FederatedStorySegment],
    ) -> AppResult<BTreeMap<String, Vec<FederatedStoryTechnicalEvidence>>> {
        Err(AppError::new(
            ErrorCode::Internal,
            "test telemetry unavailable",
        ))
    }
}

#[derive(Debug)]
struct MixedTechnicalEnrichment;

#[async_trait]
impl FederatedStoryEnrichmentProvider for MixedTechnicalEnrichment {
    async fn enrich(
        &self,
        segments: &[FederatedStorySegment],
    ) -> AppResult<BTreeMap<String, Vec<FederatedStoryTechnicalEvidence>>> {
        let Some(segment) = segments.first() else {
            return Ok(BTreeMap::new());
        };
        Ok(BTreeMap::from([(
            segment.id.clone(),
            vec![
                FederatedStoryTechnicalEvidence {
                    kind: FederatedStoryTechnicalEvidenceKind::Trace,
                    id: "trace-1".to_owned(),
                    source: "test-otel".to_owned(),
                    attributes: json!({}),
                },
                FederatedStoryTechnicalEvidence {
                    kind: FederatedStoryTechnicalEvidenceKind::Metric,
                    id: "metric-1".to_owned(),
                    source: "test-otel".to_owned(),
                    attributes: json!({}),
                },
                FederatedStoryTechnicalEvidence {
                    kind: FederatedStoryTechnicalEvidenceKind::Log,
                    id: "log-1".to_owned(),
                    source: "test-otel".to_owned(),
                    attributes: json!({}),
                },
            ],
        )]))
    }
}

#[tokio::test]
async fn two_authenticated_service_feeds_resume_and_accept_late_evidence() {
    let (Some(ticket_db), Some(sla_db), Some(aggregation_db)) = (
        TestDatabase::create().await,
        TestDatabase::create().await,
        TestDatabase::create().await,
    ) else {
        return;
    };
    prepare_aggregation_store(&aggregation_db).await;
    let provider = Arc::new(
        SystemSandboxWorkloadIdentityProvider::new(
            "test",
            "federated-story-workload-identity-secret",
        )
        .unwrap(),
    );
    let ticket_state = prepare_runtime(
        &service("support-ticket"),
        &runtime_config("support-ticket", provider.clone()),
        ticket_db.pool.clone(),
        &[],
    )
    .await
    .unwrap();
    let sla_state = prepare_runtime(
        &service("support-sla"),
        &runtime_config("support-sla", provider.clone()),
        sla_db.pool.clone(),
        &[],
    )
    .await
    .unwrap();
    append_story_segment(
        &ticket_state,
        &local_segment("story-support-1", "ticket-opened", TENANT_A, "completed"),
    )
    .await
    .unwrap();
    append_story_segment(
        &sla_state,
        &local_segment("story-support-1", "sla-started", TENANT_A, "started"),
    )
    .await
    .unwrap();
    let (ticket_url, ticket_task) = spawn_feed(ticket_state.clone()).await;
    let (sla_url, sla_task) = spawn_feed(sla_state.clone()).await;
    let source_config = sources(&ticket_url, &sla_url);
    let credentials = Arc::new(StaticStorySegmentFeedCredentialProvider::new([
        (
            "support-ticket".to_owned(),
            issue_credential(&provider, &audience("support-ticket")),
        ),
        (
            "support-sla".to_owned(),
            issue_credential(&provider, &audience("support-sla")),
        ),
    ]));
    let client = Arc::new(HttpFederatedStoryFeedClient::new(
        reqwest::Client::new(),
        credentials,
    ));
    let telemetry_now = Utc::now();
    let telemetry = Arc::new(InMemoryTelemetrySpanProvider::new([
        TelemetrySpan {
            id: "trace-ticket".to_owned(),
            name: "support.ticket.opened".to_owned(),
            status: Some("ok".to_owned()),
            started_at: telemetry_now,
            ended_at: telemetry_now,
            attributes: json!({
                "lenso.story_id": "story-support-1",
                "lenso.segment_id": "ticket-opened",
                "lenso.service_id": "support-ticket",
                "credential": "must-not-leak",
            }),
        },
        TelemetrySpan {
            id: "trace-sla".to_owned(),
            name: "support.sla.started".to_owned(),
            status: Some("ok".to_owned()),
            started_at: telemetry_now,
            ended_at: telemetry_now,
            attributes: json!({
                "lenso.story_id": "story-support-1",
                "lenso.story_segment_id": "sla-started",
                "lenso.service_id": "support-sla",
            }),
        },
    ]));
    let aggregator = FederatedStoryAggregator::new(
        aggregation_db.pool.clone(),
        source_config.clone(),
        client.clone(),
    )
    .unwrap()
    .with_enrichment_provider(Arc::new(
        OpenTelemetryFederatedStoryEnrichmentProvider::new(telemetry),
    ));
    let first = aggregator.collect_once(Some(TENANT_A)).await.unwrap();
    assert!(
        first
            .iter()
            .all(|result| result.ingested_revisions == 1 && result.gap.is_none())
    );
    let initial_story = aggregator
        .story("story-support-1", Some(TENANT_A))
        .await
        .unwrap();
    assert_eq!(initial_story.segments.len(), 2);
    let sla_reliability = initial_story
        .reliability
        .iter()
        .find(|evidence| evidence.source_service_id == "support-sla")
        .and_then(|evidence| evidence.report.as_ref())
        .expect("support-sla reliability should be collected beside the story");
    assert_eq!(sla_reliability.profile, ReliabilityProfile::Critical);
    assert_eq!(sla_reliability.overrides.workflow_backlog_limit, Some(5));
    assert_eq!(sla_reliability.effective_values.workflow_backlog_limit, 5);
    assert!(
        initial_story
            .segments
            .iter()
            .all(|segment| segment.technical_evidence.len() == 1)
    );
    assert!(
        !serde_json::to_string(&initial_story)
            .unwrap()
            .contains("must-not-leak")
    );
    let ticket_node_id = initial_story
        .segments
        .iter()
        .find(|segment| segment.segment.segment_id == "ticket-opened")
        .unwrap()
        .id
        .clone();

    // The collector is absent while both Services continue writing their own
    // Store. Recreating it later must resume each independent durable cursor.
    drop(aggregator);
    append_story_segment(
        &ticket_state,
        &local_segment("story-support-1", "ticket-opened", TENANT_A, "closed")
            .with_revision(2)
            .with_attempt(2),
    )
    .await
    .unwrap();
    append_story_segment(
        &sla_state,
        &local_segment("story-support-1", "sla-compensated", TENANT_A, "completed"),
    )
    .await
    .unwrap();
    append_story_segment(
        &ticket_state,
        &local_segment("story-support-1", "tenant-b-only", TENANT_B, "completed"),
    )
    .await
    .unwrap();

    let restarted =
        FederatedStoryAggregator::new(aggregation_db.pool.clone(), source_config, client).unwrap();
    let resumed = restarted.collect_once(Some(TENANT_A)).await.unwrap();
    assert!(
        resumed
            .iter()
            .all(|result| result.ingested_revisions == 1 && result.gap.is_none())
    );
    let story = restarted
        .story("story-support-1", Some(TENANT_A))
        .await
        .unwrap();
    assert_eq!(story.segments.len(), 3);
    let ticket = story
        .segments
        .iter()
        .find(|segment| segment.segment.segment_id == "ticket-opened")
        .unwrap();
    assert_eq!(ticket.id, ticket_node_id);
    assert_eq!(ticket.segment.evidence_revision, 2);
    assert_eq!(ticket.segment.status, "closed");
    assert!(
        story
            .segments
            .iter()
            .all(|segment| segment.segment.tenant_id.as_deref() == Some(TENANT_A))
    );

    for source in ["support-ticket", "support-sla"] {
        restarted
            .restart_source_cursor(source, Some(TENANT_A))
            .await
            .unwrap();
    }
    let duplicate_retry = restarted.collect_once(Some(TENANT_A)).await.unwrap();
    assert!(
        duplicate_retry
            .iter()
            .all(|result| result.ingested_revisions == 0 && result.duplicate_revisions >= 1)
    );

    let unauthorized_tenant = restarted.collect_once(Some(TENANT_B)).await.unwrap();
    assert!(unauthorized_tenant.iter().all(|result| {
        result.gap == Some(FederatedStoryGapKind::Unauthorized) && result.ingested_revisions == 0
    }));
    let tenant_a_story = restarted
        .story("story-support-1", Some(TENANT_A))
        .await
        .unwrap();
    assert!(tenant_a_story.gaps.is_empty());
    assert!(
        restarted
            .story("story-support-1", Some(TENANT_B))
            .await
            .is_err()
    );

    let without_telemetry = FederatedStoryAggregator::new(
        aggregation_db.pool.clone(),
        sources(&ticket_url, &sla_url),
        Arc::new(HttpFederatedStoryFeedClient::new(
            reqwest::Client::new(),
            Arc::new(StaticStorySegmentFeedCredentialProvider::new([
                (
                    "support-ticket".to_owned(),
                    issue_credential(&provider, &audience("support-ticket")),
                ),
                (
                    "support-sla".to_owned(),
                    issue_credential(&provider, &audience("support-sla")),
                ),
            ])),
        )),
    )
    .unwrap()
    .with_enrichment_provider(Arc::new(FailedEnrichment))
    .story("story-support-1", Some(TENANT_A))
    .await
    .unwrap();
    assert_eq!(story_identity(&without_telemetry), story_identity(&story));
    assert!(
        without_telemetry
            .segments
            .iter()
            .all(|segment| segment.technical_evidence.is_empty())
    );

    drop(restarted);
    ticket_task.abort();
    sla_task.abort();
    let _ = ticket_task.await;
    let _ = sla_task.await;
    ticket_db.cleanup().await;
    sla_db.cleanup().await;
    aggregation_db.cleanup().await;
}

fn story_identity(story: &FederatedRuntimeStory) -> Vec<(String, String, u32)> {
    story
        .segments
        .iter()
        .map(|segment| {
            (
                segment.id.clone(),
                segment.segment.story_id.clone(),
                segment.segment.evidence_revision,
            )
        })
        .collect()
}

#[derive(Debug, Clone)]
enum ScriptedOutcome {
    Feed(StorySegmentFeed),
    Error(FederatedStoryFeedError),
}

#[derive(Debug, Clone)]
struct ScriptedFeedClient {
    outcomes: Arc<Mutex<BTreeMap<String, ScriptedOutcome>>>,
}

impl ScriptedFeedClient {
    fn new(outcomes: BTreeMap<String, ScriptedOutcome>) -> Self {
        Self {
            outcomes: Arc::new(Mutex::new(outcomes)),
        }
    }

    fn set(&self, service_id: &str, outcome: ScriptedOutcome) {
        self.outcomes
            .lock()
            .unwrap()
            .insert(service_id.to_owned(), outcome);
    }
}

#[async_trait]
impl FederatedStoryFeedClient for ScriptedFeedClient {
    async fn read_feed(
        &self,
        request: FederatedStoryFeedRequest,
    ) -> Result<StorySegmentFeed, FederatedStoryFeedError> {
        self.outcomes
            .lock()
            .unwrap()
            .get(request.source.service_id())
            .cloned()
            .expect("scripted source should exist")
            .into_result()
    }
}

impl ScriptedOutcome {
    fn into_result(self) -> Result<StorySegmentFeed, FederatedStoryFeedError> {
        match self {
            Self::Feed(feed) => Ok(feed),
            Self::Error(error) => Err(error),
        }
    }
}

fn shared_segment(service_id: &str, segment_id: &str, recorded_at: DateTime<Utc>) -> StorySegment {
    StorySegment {
        story_id: "story-with-gaps".to_owned(),
        segment_id: segment_id.to_owned(),
        evidence_revision: 1,
        source: StorySegmentSource {
            service_id: service_id.to_owned(),
            workload_id: format!("{service_id}-worker"),
        },
        operation: StorySegmentOperation {
            kind: "durable_workflow".to_owned(),
            operation_id: segment_id.to_owned(),
        },
        contract: StorySegmentContract {
            contract_id: "support-sla".to_owned(),
            version: "v1".to_owned(),
        },
        status: "completed".to_owned(),
        attempt: 1,
        started_at: recorded_at,
        completed_at: recorded_at,
        recorded_at,
        tenant_id: Some(TENANT_A.to_owned()),
        parent_segment_id: None,
        causation_id: None,
        workflow: None,
    }
}

fn scripted_feed(
    service_id: &str,
    as_of: DateTime<Utc>,
    segments: Vec<StorySegment>,
) -> StorySegmentFeed {
    StorySegmentFeed {
        protocol: "lenso.story-segment-feed.v1".to_owned(),
        source_service_id: service_id.to_owned(),
        tenant_id: Some(TENANT_A.to_owned()),
        retention_window_seconds: 86_400,
        as_of,
        segments,
        next_cursor: format!("cursor-{service_id}"),
    }
}

#[tokio::test]
async fn every_source_failure_is_an_explicit_typed_gap_and_late_data_resolves_transient_gaps() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    prepare_aggregation_store(&db).await;
    let observed_at = DateTime::parse_from_rfc3339("2026-07-18T08:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let source_ids = [
        "support-ticket",
        "unreachable-service",
        "stale-service",
        "unauthorized-service",
        "truncated-service",
        "retention-service",
    ];
    let client = ScriptedFeedClient::new(BTreeMap::from([
        (
            "support-ticket".to_owned(),
            ScriptedOutcome::Feed(scripted_feed(
                "support-ticket",
                observed_at,
                vec![shared_segment(
                    "support-ticket",
                    "ticket-opened",
                    observed_at,
                )],
            )),
        ),
        (
            "unreachable-service".to_owned(),
            ScriptedOutcome::Error(FederatedStoryFeedError::new(
                FederatedStoryGapKind::Unreachable,
                "source unavailable",
            )),
        ),
        (
            "stale-service".to_owned(),
            ScriptedOutcome::Feed(scripted_feed(
                "stale-service",
                observed_at - ChronoDuration::minutes(10),
                Vec::new(),
            )),
        ),
        (
            "unauthorized-service".to_owned(),
            ScriptedOutcome::Error(FederatedStoryFeedError::new(
                FederatedStoryGapKind::Unauthorized,
                "reader forbidden",
            )),
        ),
        (
            "truncated-service".to_owned(),
            ScriptedOutcome::Error(FederatedStoryFeedError::new(
                FederatedStoryGapKind::Truncated,
                "cursor continuity lost",
            )),
        ),
        (
            "retention-service".to_owned(),
            ScriptedOutcome::Error(FederatedStoryFeedError::new(
                FederatedStoryGapKind::RetentionExpired,
                "cursor outside retention",
            )),
        ),
    ]));
    let sources: Vec<FederatedStorySource> = source_ids
        .into_iter()
        .map(|service_id| {
            FederatedStorySource::new(
                service_id,
                format!("mock://{service_id}"),
                format!("service:{service_id}/feed"),
                Duration::from_secs(60),
            )
        })
        .collect();
    let aggregator =
        FederatedStoryAggregator::new(db.pool.clone(), sources.clone(), Arc::new(client.clone()))
            .unwrap();
    aggregator
        .collect_once_at(Some(TENANT_A), observed_at)
        .await
        .unwrap();
    let story = aggregator
        .story_at("story-with-gaps", Some(TENANT_A), observed_at)
        .await
        .unwrap();
    let kinds = story
        .gaps
        .iter()
        .map(|gap| gap.kind)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        kinds,
        BTreeSet::from([
            FederatedStoryGapKind::Unreachable,
            FederatedStoryGapKind::Stale,
            FederatedStoryGapKind::Unauthorized,
            FederatedStoryGapKind::Truncated,
            FederatedStoryGapKind::RetentionExpired,
        ])
    );

    client.set(
        "unreachable-service",
        ScriptedOutcome::Feed(scripted_feed(
            "unreachable-service",
            observed_at + ChronoDuration::minutes(1),
            vec![shared_segment(
                "unreachable-service",
                "late-evidence",
                observed_at + ChronoDuration::minutes(1),
            )],
        )),
    );
    aggregator
        .collect_once_at(Some(TENANT_A), observed_at + ChronoDuration::minutes(1))
        .await
        .unwrap();
    let completed = aggregator
        .story_at(
            "story-with-gaps",
            Some(TENANT_A),
            observed_at + ChronoDuration::minutes(1),
        )
        .await
        .unwrap();
    assert_eq!(completed.story_id, story.story_id);
    assert_eq!(completed.segments.len(), 2);
    assert!(
        completed
            .gaps
            .iter()
            .all(|gap| gap.kind != FederatedStoryGapKind::Unreachable)
    );
    assert!(
        completed
            .gaps
            .iter()
            .any(|gap| gap.kind == FederatedStoryGapKind::RetentionExpired)
    );

    let technically_enriched =
        FederatedStoryAggregator::new(db.pool.clone(), sources, Arc::new(client))
            .unwrap()
            .with_enrichment_provider(Arc::new(MixedTechnicalEnrichment))
            .story("story-with-gaps", Some(TENANT_A))
            .await
            .unwrap();
    let technical_kinds = technically_enriched
        .segments
        .iter()
        .flat_map(|segment| &segment.technical_evidence)
        .map(|evidence| evidence.kind.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        technical_kinds,
        vec![
            FederatedStoryTechnicalEvidenceKind::Trace,
            FederatedStoryTechnicalEvidenceKind::Metric,
            FederatedStoryTechnicalEvidenceKind::Log,
        ]
    );
    assert_eq!(
        story_identity(&technically_enriched),
        story_identity(&completed)
    );

    db.cleanup().await;
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis()
        .try_into()
        .unwrap()
}
