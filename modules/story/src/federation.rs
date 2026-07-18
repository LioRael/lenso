//! Delayed, read-only federation of Service-owned Story Segment Feeds.
//!
//! This module belongs to the observability plane. It never acknowledges feed
//! entries, advances a workflow, fires a timer, or writes a Service Store.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lenso_service::{
    RELIABILITY_REPORT_PROTOCOL, ReliabilityReport, STORY_SEGMENT_FEED_PROTOCOL, StorySegment,
    StorySegmentFeed,
};
use platform_core::{AppError, AppResult, ErrorCode, TelemetrySpanProvider, TelemetrySpanQuery};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::Arc,
    time::Duration,
};
use utoipa::ToSchema;

pub const FEDERATED_RUNTIME_STORY_PROTOCOL: &str = "lenso.federated-runtime-story.v1";
const DEFAULT_FEED_LIMIT: u16 = 500;

#[derive(Debug, Clone)]
pub struct FederatedStorySource {
    service_id: String,
    feed_base_url: String,
    audience: String,
    stale_after: Duration,
}

impl FederatedStorySource {
    #[must_use]
    pub fn new(
        service_id: impl Into<String>,
        feed_base_url: impl Into<String>,
        audience: impl Into<String>,
        stale_after: Duration,
    ) -> Self {
        Self {
            service_id: service_id.into(),
            feed_base_url: feed_base_url.into(),
            audience: audience.into(),
            stale_after,
        }
    }

    #[must_use]
    pub fn service_id(&self) -> &str {
        &self.service_id
    }

    #[must_use]
    pub fn feed_base_url(&self) -> &str {
        &self.feed_base_url
    }

    #[must_use]
    pub fn audience(&self) -> &str {
        &self.audience
    }

    #[must_use]
    pub const fn stale_after(&self) -> Duration {
        self.stale_after
    }
}

#[derive(Debug, Clone)]
pub struct FederatedStoryFeedRequest {
    pub source: FederatedStorySource,
    pub tenant_id: Option<String>,
    pub cursor: Option<String>,
    pub limit: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FederatedStoryGapKind {
    Unreachable,
    Stale,
    Unauthorized,
    Truncated,
    RetentionExpired,
}

impl FederatedStoryGapKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Unreachable => "unreachable",
            Self::Stale => "stale",
            Self::Unauthorized => "unauthorized",
            Self::Truncated => "truncated",
            Self::RetentionExpired => "retention_expired",
        }
    }

    fn from_stored(value: &str) -> AppResult<Self> {
        match value {
            "unreachable" => Ok(Self::Unreachable),
            "stale" => Ok(Self::Stale),
            "unauthorized" => Ok(Self::Unauthorized),
            "truncated" => Ok(Self::Truncated),
            "retention_expired" => Ok(Self::RetentionExpired),
            _ => Err(AppError::new(
                ErrorCode::Internal,
                "Stored Federated Runtime Story gap kind is invalid",
            )),
        }
    }

    const fn default_next_action(self) -> &'static str {
        match self {
            Self::Unreachable => "restore_story_segment_feed",
            Self::Stale => "inspect_story_segment_feed_freshness",
            Self::Unauthorized => "refresh_story_segment_feed_authorization",
            Self::Truncated => "restart_story_segment_collection_with_gap",
            Self::RetentionExpired => "restart_story_segment_collection_after_retention_gap",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FederatedStoryFeedError {
    pub kind: FederatedStoryGapKind,
    pub detail: String,
    pub next_action: String,
}

impl FederatedStoryFeedError {
    #[must_use]
    pub fn new(kind: FederatedStoryGapKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
            next_action: kind.default_next_action().to_owned(),
        }
    }

    #[must_use]
    pub fn with_next_action(mut self, next_action: impl Into<String>) -> Self {
        self.next_action = next_action.into();
        self
    }
}

impl fmt::Display for FederatedStoryFeedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.detail)
    }
}

impl std::error::Error for FederatedStoryFeedError {}

#[async_trait]
pub trait FederatedStoryFeedClient: fmt::Debug + Send + Sync {
    async fn read_feed(
        &self,
        request: FederatedStoryFeedRequest,
    ) -> Result<StorySegmentFeed, FederatedStoryFeedError>;

    /// Reads report-only Service reliability evidence beside Story collection.
    /// Implementations that do not expose reliability may keep the default.
    async fn read_reliability(
        &self,
        _source: &FederatedStorySource,
    ) -> Result<Option<ReliabilityReport>, FederatedStoryFeedError> {
        Ok(None)
    }
}

#[async_trait]
pub trait StorySegmentFeedCredentialProvider: fmt::Debug + Send + Sync {
    async fn bearer_credential(
        &self,
        source: &FederatedStorySource,
    ) -> Result<String, FederatedStoryFeedError>;
}

#[derive(Clone, Default)]
pub struct StaticStorySegmentFeedCredentialProvider {
    credentials: BTreeMap<String, String>,
}

impl fmt::Debug for StaticStorySegmentFeedCredentialProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StaticStorySegmentFeedCredentialProvider")
            .field("service_ids", &self.credentials.keys().collect::<Vec<_>>())
            .field("credentials", &"[REDACTED]")
            .finish()
    }
}

impl StaticStorySegmentFeedCredentialProvider {
    #[must_use]
    pub fn new(credentials: impl IntoIterator<Item = (String, String)>) -> Self {
        Self {
            credentials: credentials.into_iter().collect(),
        }
    }
}

#[async_trait]
impl StorySegmentFeedCredentialProvider for StaticStorySegmentFeedCredentialProvider {
    async fn bearer_credential(
        &self,
        source: &FederatedStorySource,
    ) -> Result<String, FederatedStoryFeedError> {
        self.credentials
            .get(source.service_id())
            .cloned()
            .ok_or_else(|| {
                FederatedStoryFeedError::new(
                    FederatedStoryGapKind::Unauthorized,
                    format!(
                        "No Story Segment Feed credential is configured for Service `{}`",
                        source.service_id()
                    ),
                )
            })
    }
}

#[derive(Clone)]
pub struct HttpFederatedStoryFeedClient {
    client: reqwest::Client,
    credentials: Arc<dyn StorySegmentFeedCredentialProvider>,
}

impl fmt::Debug for HttpFederatedStoryFeedClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpFederatedStoryFeedClient")
            .field("client", &self.client)
            .field("credentials", &self.credentials)
            .finish()
    }
}

impl HttpFederatedStoryFeedClient {
    #[must_use]
    pub fn new(
        client: reqwest::Client,
        credentials: Arc<dyn StorySegmentFeedCredentialProvider>,
    ) -> Self {
        Self {
            client,
            credentials,
        }
    }
}

#[async_trait]
impl FederatedStoryFeedClient for HttpFederatedStoryFeedClient {
    async fn read_feed(
        &self,
        request: FederatedStoryFeedRequest,
    ) -> Result<StorySegmentFeed, FederatedStoryFeedError> {
        let credential = self.credentials.bearer_credential(&request.source).await?;
        let url = format!(
            "{}/runtime/story-segments",
            request.source.feed_base_url().trim_end_matches('/')
        );
        let mut query = vec![("limit", request.limit.to_string())];
        if let Some(tenant_id) = request.tenant_id {
            query.push(("tenantId", tenant_id));
        }
        if let Some(cursor) = request.cursor {
            query.push(("cursor", cursor));
        }
        let response = self
            .client
            .get(url)
            .bearer_auth(credential)
            .query(&query)
            .send()
            .await
            .map_err(|_| {
                FederatedStoryFeedError::new(
                    FederatedStoryGapKind::Unreachable,
                    format!(
                        "Story Segment Feed for Service `{}` could not be reached",
                        request.source.service_id()
                    ),
                )
            })?;
        let status = response.status();
        if status.is_success() {
            return response.json().await.map_err(|_| {
                FederatedStoryFeedError::new(
                    FederatedStoryGapKind::Truncated,
                    format!(
                        "Story Segment Feed for Service `{}` returned an invalid payload",
                        request.source.service_id()
                    ),
                )
            });
        }
        let kind = if status == reqwest::StatusCode::UNAUTHORIZED
            || status == reqwest::StatusCode::FORBIDDEN
        {
            FederatedStoryGapKind::Unauthorized
        } else if status == reqwest::StatusCode::GONE {
            FederatedStoryGapKind::RetentionExpired
        } else if status == reqwest::StatusCode::PARTIAL_CONTENT
            || status == reqwest::StatusCode::BAD_REQUEST
        {
            FederatedStoryGapKind::Truncated
        } else {
            FederatedStoryGapKind::Unreachable
        };
        Err(FederatedStoryFeedError::new(
            kind,
            format!(
                "Story Segment Feed for Service `{}` returned HTTP {}",
                request.source.service_id(),
                status.as_u16()
            ),
        ))
    }

    async fn read_reliability(
        &self,
        source: &FederatedStorySource,
    ) -> Result<Option<ReliabilityReport>, FederatedStoryFeedError> {
        let url = format!(
            "{}/runtime/reliability",
            source.feed_base_url().trim_end_matches('/')
        );
        let response = self.client.get(url).send().await.map_err(|_| {
            FederatedStoryFeedError::new(
                FederatedStoryGapKind::Unreachable,
                format!(
                    "Reliability Report for Service `{}` could not be reached",
                    source.service_id()
                ),
            )
            .with_next_action("restore_reliability_report")
        })?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(FederatedStoryFeedError::new(
                if matches!(
                    response.status(),
                    reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
                ) {
                    FederatedStoryGapKind::Unauthorized
                } else {
                    FederatedStoryGapKind::Unreachable
                },
                format!(
                    "Reliability Report for Service `{}` returned HTTP {}",
                    source.service_id(),
                    response.status().as_u16()
                ),
            )
            .with_next_action("restore_reliability_report"));
        }
        let report = response.json::<ReliabilityReport>().await.map_err(|_| {
            FederatedStoryFeedError::new(
                FederatedStoryGapKind::Truncated,
                format!(
                    "Reliability Report for Service `{}` returned an invalid payload",
                    source.service_id()
                ),
            )
            .with_next_action("repair_reliability_report_contract")
        })?;
        if report.protocol != RELIABILITY_REPORT_PROTOCOL
            || report.service_id != source.service_id()
        {
            return Err(FederatedStoryFeedError::new(
                FederatedStoryGapKind::Truncated,
                format!(
                    "Reliability Report for Service `{}` changed protocol or Service identity",
                    source.service_id()
                ),
            )
            .with_next_action("repair_reliability_report_identity"));
        }
        Ok(Some(report))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FederatedStoryTechnicalEvidenceKind {
    Trace,
    Metric,
    Log,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FederatedStoryTechnicalEvidence {
    pub kind: FederatedStoryTechnicalEvidenceKind,
    pub id: String,
    pub source: String,
    pub attributes: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FederatedStorySegment {
    pub id: String,
    pub segment: StorySegment,
    pub technical_evidence: Vec<FederatedStoryTechnicalEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FederatedStoryGap {
    pub source_service_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    pub kind: FederatedStoryGapKind,
    pub detected_at: DateTime<Utc>,
    pub last_observed_at: DateTime<Utc>,
    pub detail: String,
    pub next_action: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FederatedStoryReliabilityStatus {
    Available,
    Unavailable,
    NotDeclared,
}

impl FederatedStoryReliabilityStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Unavailable => "unavailable",
            Self::NotDeclared => "not_declared",
        }
    }

    fn from_stored(value: &str) -> AppResult<Self> {
        match value {
            "available" => Ok(Self::Available),
            "unavailable" => Ok(Self::Unavailable),
            "not_declared" => Ok(Self::NotDeclared),
            _ => Err(AppError::new(
                ErrorCode::Internal,
                "Stored Federated Story reliability status is invalid",
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FederatedStoryReliabilityEvidence {
    pub source_service_id: String,
    pub observed_at: DateTime<Utc>,
    pub status: FederatedStoryReliabilityStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<ReliabilityReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_action: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FederatedRuntimeStory {
    pub protocol: String,
    pub story_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    pub assembled_at: DateTime<Utc>,
    pub segments: Vec<FederatedStorySegment>,
    pub gaps: Vec<FederatedStoryGap>,
    pub reliability: Vec<FederatedStoryReliabilityEvidence>,
}

#[async_trait]
pub trait FederatedStoryEnrichmentProvider: fmt::Debug + Send + Sync {
    async fn enrich(
        &self,
        segments: &[FederatedStorySegment],
    ) -> AppResult<BTreeMap<String, Vec<FederatedStoryTechnicalEvidence>>>;
}

#[derive(Debug, Default)]
pub struct NoopFederatedStoryEnrichmentProvider;

#[async_trait]
impl FederatedStoryEnrichmentProvider for NoopFederatedStoryEnrichmentProvider {
    async fn enrich(
        &self,
        _segments: &[FederatedStorySegment],
    ) -> AppResult<BTreeMap<String, Vec<FederatedStoryTechnicalEvidence>>> {
        Ok(BTreeMap::new())
    }
}

#[derive(Clone)]
pub struct OpenTelemetryFederatedStoryEnrichmentProvider {
    spans: Arc<dyn TelemetrySpanProvider>,
}

impl fmt::Debug for OpenTelemetryFederatedStoryEnrichmentProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenTelemetryFederatedStoryEnrichmentProvider")
            .field("spans", &self.spans)
            .finish()
    }
}

impl OpenTelemetryFederatedStoryEnrichmentProvider {
    #[must_use]
    pub fn new(spans: Arc<dyn TelemetrySpanProvider>) -> Self {
        Self { spans }
    }
}

#[async_trait]
impl FederatedStoryEnrichmentProvider for OpenTelemetryFederatedStoryEnrichmentProvider {
    async fn enrich(
        &self,
        segments: &[FederatedStorySegment],
    ) -> AppResult<BTreeMap<String, Vec<FederatedStoryTechnicalEvidence>>> {
        let Some(story_id) = segments.first().map(|segment| &segment.segment.story_id) else {
            return Ok(BTreeMap::new());
        };
        let spans = self
            .spans
            .query_spans(TelemetrySpanQuery {
                story_id: Some(story_id.clone()),
                ..TelemetrySpanQuery::default()
            })
            .await?;
        let mut enrichment = BTreeMap::<String, Vec<_>>::new();
        for span in spans {
            let Some(segment_id) = telemetry_attribute(
                &span.attributes,
                &["lenso.story_segment_id", "lenso.segment_id"],
            ) else {
                continue;
            };
            let source_service_id = telemetry_attribute(&span.attributes, &["lenso.service_id"]);
            let Some(segment) = segments.iter().find(|segment| {
                segment.segment.segment_id == segment_id
                    && source_service_id
                        .is_none_or(|service_id| segment.segment.source.service_id == service_id)
            }) else {
                continue;
            };
            enrichment.entry(segment.id.clone()).or_default().push(
                FederatedStoryTechnicalEvidence {
                    kind: FederatedStoryTechnicalEvidenceKind::Trace,
                    id: span.id,
                    source: "opentelemetry".to_owned(),
                    attributes: serde_json::json!({
                        "name": span.name,
                        "status": span.status,
                        "startedAt": span.started_at,
                        "endedAt": span.ended_at,
                        "attributes": safe_telemetry_attributes(&span.attributes),
                    }),
                },
            );
        }
        Ok(enrichment)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FederatedStoryCollectionResult {
    pub source_service_id: String,
    pub ingested_revisions: u64,
    pub duplicate_revisions: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gap: Option<FederatedStoryGapKind>,
}

#[derive(Clone)]
pub struct FederatedStoryAggregator {
    pool: PgPool,
    sources: Vec<FederatedStorySource>,
    feed_client: Arc<dyn FederatedStoryFeedClient>,
    enrichment: Arc<dyn FederatedStoryEnrichmentProvider>,
}

impl fmt::Debug for FederatedStoryAggregator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FederatedStoryAggregator")
            .field("sources", &self.sources)
            .field("feed_client", &self.feed_client)
            .field("enrichment", &self.enrichment)
            .finish_non_exhaustive()
    }
}

impl FederatedStoryAggregator {
    pub fn new(
        pool: PgPool,
        sources: Vec<FederatedStorySource>,
        feed_client: Arc<dyn FederatedStoryFeedClient>,
    ) -> AppResult<Self> {
        if sources.len() < 2 {
            return Err(AppError::new(
                ErrorCode::Validation,
                "Federated Runtime Story aggregation requires at least two Service feeds",
            ));
        }
        let unique_sources = sources
            .iter()
            .map(|source| source.service_id.as_str())
            .collect::<BTreeSet<_>>();
        if unique_sources.len() != sources.len()
            || sources.iter().any(|source| {
                source.service_id.trim().is_empty()
                    || source.feed_base_url.trim().is_empty()
                    || source.audience.trim().is_empty()
                    || source.stale_after.is_zero()
            })
        {
            return Err(AppError::new(
                ErrorCode::Validation,
                "Federated Story sources require unique Service identity, feed URL, audience, and freshness window",
            ));
        }
        Ok(Self {
            pool,
            sources,
            feed_client,
            enrichment: Arc::new(NoopFederatedStoryEnrichmentProvider),
        })
    }

    #[must_use]
    pub fn with_enrichment_provider(
        mut self,
        enrichment: Arc<dyn FederatedStoryEnrichmentProvider>,
    ) -> Self {
        self.enrichment = enrichment;
        self
    }

    pub async fn collect_once(
        &self,
        tenant_id: Option<&str>,
    ) -> AppResult<Vec<FederatedStoryCollectionResult>> {
        self.collect_once_at(tenant_id, Utc::now()).await
    }

    pub async fn collect_once_at(
        &self,
        tenant_id: Option<&str>,
        observed_at: DateTime<Utc>,
    ) -> AppResult<Vec<FederatedStoryCollectionResult>> {
        validate_tenant_id(tenant_id)?;
        let mut results = Vec::with_capacity(self.sources.len());
        for source in &self.sources {
            results.push(self.collect_source(source, tenant_id, observed_at).await?);
        }
        Ok(results)
    }

    async fn collect_source(
        &self,
        source: &FederatedStorySource,
        tenant_id: Option<&str>,
        observed_at: DateTime<Utc>,
    ) -> AppResult<FederatedStoryCollectionResult> {
        let reliability = self.feed_client.read_reliability(source).await;
        persist_reliability_evidence(
            &self.pool,
            source.service_id(),
            tenant_id,
            observed_at,
            reliability,
        )
        .await?;
        let cursor = load_source_cursor(&self.pool, source.service_id(), tenant_id).await?;
        let request = FederatedStoryFeedRequest {
            source: source.clone(),
            tenant_id: tenant_id.map(str::to_owned),
            cursor,
            limit: DEFAULT_FEED_LIMIT,
        };
        let feed = match self.feed_client.read_feed(request).await {
            Ok(feed) => feed,
            Err(error) => {
                record_gap(&self.pool, source, tenant_id, observed_at, &error).await?;
                return Ok(FederatedStoryCollectionResult {
                    source_service_id: source.service_id.clone(),
                    ingested_revisions: 0,
                    duplicate_revisions: 0,
                    gap: Some(error.kind),
                });
            }
        };
        if let Err(error) = validate_feed(source, tenant_id, &feed) {
            record_gap(&self.pool, source, tenant_id, observed_at, &error).await?;
            return Ok(FederatedStoryCollectionResult {
                source_service_id: source.service_id.clone(),
                ingested_revisions: 0,
                duplicate_revisions: 0,
                gap: Some(error.kind),
            });
        }

        let stale = source_as_of_is_stale(source, feed.as_of, observed_at)?;
        let (ingested_revisions, duplicate_revisions) =
            persist_feed(&self.pool, source, tenant_id, observed_at, &feed, !stale).await?;
        let gap = if stale {
            let error = FederatedStoryFeedError::new(
                FederatedStoryGapKind::Stale,
                format!(
                    "Story Segment Feed for Service `{}` is older than its declared freshness window",
                    source.service_id()
                ),
            );
            record_gap(&self.pool, source, tenant_id, observed_at, &error).await?;
            Some(FederatedStoryGapKind::Stale)
        } else {
            None
        };
        Ok(FederatedStoryCollectionResult {
            source_service_id: source.service_id.clone(),
            ingested_revisions,
            duplicate_revisions,
            gap,
        })
    }

    /// Discards only the observability-plane cursor. Existing collected
    /// evidence and permanent truncation/retention gaps remain visible.
    pub async fn restart_source_cursor(
        &self,
        source_service_id: &str,
        tenant_id: Option<&str>,
    ) -> AppResult<()> {
        validate_tenant_id(tenant_id)?;
        if !self
            .sources
            .iter()
            .any(|source| source.service_id() == source_service_id)
        {
            return Err(AppError::new(
                ErrorCode::NotFound,
                format!("Federated Story source `{source_service_id}` is not configured"),
            ));
        }
        sqlx::query(
            r#"
            update platform.federated_story_source_state
            set cursor = null, updated_at = now()
            where source_service_id = $1 and tenant_scope = $2
            "#,
        )
        .bind(source_service_id)
        .bind(tenant_scope(tenant_id))
        .execute(&self.pool)
        .await
        .map_err(aggregation_store_error)?;
        Ok(())
    }

    pub async fn story(
        &self,
        story_id: &str,
        tenant_id: Option<&str>,
    ) -> AppResult<FederatedRuntimeStory> {
        self.story_at(story_id, tenant_id, Utc::now()).await
    }

    pub async fn story_at(
        &self,
        story_id: &str,
        tenant_id: Option<&str>,
        assembled_at: DateTime<Utc>,
    ) -> AppResult<FederatedRuntimeStory> {
        FederatedStoryReader::new(self.pool.clone())
            .with_enrichment_provider(self.enrichment.clone())
            .story_at(story_id, tenant_id, assembled_at)
            .await
    }
}

/// Read-only projection of already-collected Federated Runtime Story evidence.
/// It has no feed client and therefore cannot acknowledge or advance execution.
#[derive(Clone)]
pub struct FederatedStoryReader {
    pool: PgPool,
    enrichment: Arc<dyn FederatedStoryEnrichmentProvider>,
}

impl fmt::Debug for FederatedStoryReader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FederatedStoryReader")
            .field("enrichment", &self.enrichment)
            .finish_non_exhaustive()
    }
}

impl FederatedStoryReader {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            enrichment: Arc::new(NoopFederatedStoryEnrichmentProvider),
        }
    }

    #[must_use]
    pub fn with_enrichment_provider(
        mut self,
        enrichment: Arc<dyn FederatedStoryEnrichmentProvider>,
    ) -> Self {
        self.enrichment = enrichment;
        self
    }

    pub async fn story(
        &self,
        story_id: &str,
        tenant_id: Option<&str>,
    ) -> AppResult<FederatedRuntimeStory> {
        self.story_at(story_id, tenant_id, Utc::now()).await
    }

    pub async fn story_at(
        &self,
        story_id: &str,
        tenant_id: Option<&str>,
        assembled_at: DateTime<Utc>,
    ) -> AppResult<FederatedRuntimeStory> {
        validate_tenant_id(tenant_id)?;
        if story_id.trim().is_empty() {
            return Err(AppError::new(
                ErrorCode::Validation,
                "Federated Runtime Story identity must not be empty",
            ));
        }
        let payloads = sqlx::query_scalar::<_, Value>(
            r#"
            select distinct on (source_service_id, segment_id) segment
            from platform.federated_story_segments
            where story_id = $1 and tenant_scope = $2
            order by source_service_id, segment_id, evidence_revision desc
            "#,
        )
        .bind(story_id)
        .bind(tenant_scope(tenant_id))
        .fetch_all(&self.pool)
        .await
        .map_err(aggregation_store_error)?;
        if payloads.is_empty() {
            return Err(AppError::new(
                ErrorCode::NotFound,
                format!("Federated Runtime Story `{story_id}` was not found"),
            ));
        }
        let mut segments = payloads
            .into_iter()
            .map(|payload| {
                let segment: StorySegment = serde_json::from_value(payload).map_err(|error| {
                    AppError::new(
                        ErrorCode::Internal,
                        "Stored Federated Runtime Story Segment is invalid",
                    )
                    .with_source(error)
                })?;
                Ok(FederatedStorySegment {
                    id: federated_segment_id(&segment),
                    segment,
                    technical_evidence: Vec::new(),
                })
            })
            .collect::<AppResult<Vec<_>>>()?;
        segments.sort_by(|left, right| {
            left.segment
                .started_at
                .cmp(&right.segment.started_at)
                .then_with(|| left.segment.recorded_at.cmp(&right.segment.recorded_at))
                .then_with(|| left.id.cmp(&right.id))
        });
        match self.enrichment.enrich(&segments).await {
            Ok(mut enrichment) => {
                for segment in &mut segments {
                    segment.technical_evidence = enrichment.remove(&segment.id).unwrap_or_default();
                }
            }
            Err(error) => {
                tracing::warn!(error = %error, story_id, "Federated Story telemetry enrichment unavailable");
            }
        }
        let gaps = load_active_gaps(&self.pool, tenant_id).await?;
        let reliability = load_reliability_evidence(&self.pool, tenant_id).await?;
        Ok(FederatedRuntimeStory {
            protocol: FEDERATED_RUNTIME_STORY_PROTOCOL.to_owned(),
            story_id: story_id.to_owned(),
            tenant_id: tenant_id.map(str::to_owned),
            assembled_at,
            segments,
            gaps,
            reliability,
        })
    }
}

fn validate_tenant_id(tenant_id: Option<&str>) -> AppResult<()> {
    if tenant_id.is_some_and(|tenant_id| tenant_id.trim().is_empty()) {
        return Err(AppError::new(
            ErrorCode::Validation,
            "Federated Story tenant identity must not be empty",
        ));
    }
    Ok(())
}

fn validate_feed(
    source: &FederatedStorySource,
    tenant_id: Option<&str>,
    feed: &StorySegmentFeed,
) -> Result<(), FederatedStoryFeedError> {
    let invalid_envelope = feed.protocol != STORY_SEGMENT_FEED_PROTOCOL
        || feed.source_service_id != source.service_id
        || feed.tenant_id.as_deref() != tenant_id
        || feed.next_cursor.trim().is_empty();
    let invalid_segment = feed.segments.iter().any(|segment| {
        segment.story_id.trim().is_empty()
            || segment.segment_id.trim().is_empty()
            || segment.evidence_revision == 0
            || segment.source.service_id != source.service_id
            || segment.tenant_id.as_deref() != tenant_id
    });
    if invalid_envelope || invalid_segment {
        return Err(FederatedStoryFeedError::new(
            FederatedStoryGapKind::Truncated,
            format!(
                "Story Segment Feed for Service `{}` violated source, tenant, cursor, or identity invariants",
                source.service_id()
            ),
        ));
    }
    Ok(())
}

fn source_as_of_is_stale(
    source: &FederatedStorySource,
    source_as_of: DateTime<Utc>,
    observed_at: DateTime<Utc>,
) -> AppResult<bool> {
    let stale_after = chrono::Duration::from_std(source.stale_after()).map_err(|error| {
        AppError::new(
            ErrorCode::Validation,
            "Federated Story source freshness window is unsupported",
        )
        .with_source(error)
    })?;
    Ok(observed_at.signed_duration_since(source_as_of) > stale_after)
}

async fn load_source_cursor(
    pool: &PgPool,
    source_service_id: &str,
    tenant_id: Option<&str>,
) -> AppResult<Option<String>> {
    sqlx::query_scalar(
        r#"
        select cursor
        from platform.federated_story_source_state
        where source_service_id = $1 and tenant_scope = $2
        "#,
    )
    .bind(source_service_id)
    .bind(tenant_scope(tenant_id))
    .fetch_optional(pool)
    .await
    .map(|row| row.flatten())
    .map_err(aggregation_store_error)
}

async fn persist_feed(
    pool: &PgPool,
    source: &FederatedStorySource,
    tenant_id: Option<&str>,
    observed_at: DateTime<Utc>,
    feed: &StorySegmentFeed,
    resolve_stale: bool,
) -> AppResult<(u64, u64)> {
    let mut transaction = pool.begin().await.map_err(aggregation_store_error)?;
    let mut ingested = 0_u64;
    let mut duplicates = 0_u64;
    for segment in &feed.segments {
        if let Some((stored_story_id, stored_tenant_scope)) = sqlx::query_as::<_, (String, String)>(
            r#"
            select story_id, tenant_scope
            from platform.federated_story_segments
            where source_service_id = $1 and segment_id = $2
            order by evidence_revision
            limit 1
            "#,
        )
        .bind(source.service_id())
        .bind(&segment.segment_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(aggregation_store_error)?
            && (stored_story_id != segment.story_id
                || stored_tenant_scope != tenant_scope(tenant_id))
        {
            return Err(AppError::new(
                ErrorCode::Conflict,
                "A late Story Segment revision attempted to change stable Story or tenant identity",
            ));
        }
        let payload = serde_json::to_value(segment).map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                "Could not serialize a Federated Runtime Story Segment",
            )
            .with_source(error)
        })?;
        let result = sqlx::query(
            r#"
            insert into platform.federated_story_segments (
                source_service_id, segment_id, evidence_revision, story_id,
                tenant_scope, segment, collected_at
            ) values ($1, $2, $3, $4, $5, $6, $7)
            on conflict (source_service_id, segment_id, evidence_revision) do nothing
            "#,
        )
        .bind(source.service_id())
        .bind(&segment.segment_id)
        .bind(i32::try_from(segment.evidence_revision).unwrap_or(i32::MAX))
        .bind(&segment.story_id)
        .bind(tenant_scope(tenant_id))
        .bind(&payload)
        .bind(observed_at)
        .execute(&mut *transaction)
        .await
        .map_err(aggregation_store_error)?;
        if result.rows_affected() == 1 {
            ingested += 1;
        } else {
            let existing = sqlx::query_scalar::<_, Value>(
                r#"
                select segment
                from platform.federated_story_segments
                where source_service_id = $1
                  and segment_id = $2
                  and evidence_revision = $3
                "#,
            )
            .bind(source.service_id())
            .bind(&segment.segment_id)
            .bind(i32::try_from(segment.evidence_revision).unwrap_or(i32::MAX))
            .fetch_one(&mut *transaction)
            .await
            .map_err(aggregation_store_error)?;
            if existing != payload {
                return Err(AppError::new(
                    ErrorCode::Conflict,
                    "A duplicate Story Segment revision contained different evidence",
                ));
            }
            duplicates += 1;
        }
    }
    sqlx::query(
        r#"
        insert into platform.federated_story_source_state (
            source_service_id, tenant_scope, cursor, last_successful_at,
            last_source_as_of, updated_at
        ) values ($1, $2, $3, $4, $5, $4)
        on conflict (source_service_id, tenant_scope) do update set
            cursor = excluded.cursor,
            last_successful_at = excluded.last_successful_at,
            last_source_as_of = excluded.last_source_as_of,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(source.service_id())
    .bind(tenant_scope(tenant_id))
    .bind(&feed.next_cursor)
    .bind(observed_at)
    .bind(feed.as_of)
    .execute(&mut *transaction)
    .await
    .map_err(aggregation_store_error)?;
    resolve_transient_gaps_in_tx(
        &mut transaction,
        source.service_id(),
        tenant_id,
        observed_at,
        resolve_stale,
    )
    .await?;
    transaction
        .commit()
        .await
        .map_err(aggregation_store_error)?;
    Ok((ingested, duplicates))
}

async fn resolve_transient_gaps_in_tx(
    transaction: &mut Transaction<'_, Postgres>,
    source_service_id: &str,
    tenant_id: Option<&str>,
    observed_at: DateTime<Utc>,
    resolve_stale: bool,
) -> AppResult<()> {
    sqlx::query(
        r#"
        update platform.federated_story_gaps
        set resolved_at = $3, last_observed_at = $3
        where source_service_id = $1
          and tenant_scope = $2
          and resolved_at is null
          and (
              kind in ('unreachable', 'unauthorized')
              or ($4 and kind = 'stale')
          )
        "#,
    )
    .bind(source_service_id)
    .bind(tenant_scope(tenant_id))
    .bind(observed_at)
    .bind(resolve_stale)
    .execute(&mut **transaction)
    .await
    .map_err(aggregation_store_error)?;
    Ok(())
}

async fn record_gap(
    pool: &PgPool,
    source: &FederatedStorySource,
    tenant_id: Option<&str>,
    observed_at: DateTime<Utc>,
    error: &FederatedStoryFeedError,
) -> AppResult<()> {
    sqlx::query(
        r#"
        insert into platform.federated_story_gaps (
            source_service_id, tenant_scope, kind, detected_at,
            last_observed_at, detail, next_action
        ) values ($1, $2, $3, $4, $4, $5, $6)
        on conflict (source_service_id, tenant_scope, kind)
            where resolved_at is null
        do update set
            last_observed_at = excluded.last_observed_at,
            detail = excluded.detail,
            next_action = excluded.next_action
        "#,
    )
    .bind(source.service_id())
    .bind(tenant_scope(tenant_id))
    .bind(error.kind.as_str())
    .bind(observed_at)
    .bind(&error.detail)
    .bind(&error.next_action)
    .execute(pool)
    .await
    .map_err(aggregation_store_error)?;
    Ok(())
}

async fn load_active_gaps(
    pool: &PgPool,
    tenant_id: Option<&str>,
) -> AppResult<Vec<FederatedStoryGap>> {
    sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            DateTime<Utc>,
            DateTime<Utc>,
            String,
            String,
        ),
    >(
        r#"
        select source_service_id, tenant_scope, kind, detected_at,
               last_observed_at, detail, next_action
        from platform.federated_story_gaps
        where tenant_scope = $1 and resolved_at is null
        order by source_service_id, kind, detected_at
        "#,
    )
    .bind(tenant_scope(tenant_id))
    .fetch_all(pool)
    .await
    .map_err(aggregation_store_error)?
    .into_iter()
    .map(
        |(
            source_service_id,
            tenant_scope,
            kind,
            detected_at,
            last_observed_at,
            detail,
            next_action,
        )| {
            Ok(FederatedStoryGap {
                source_service_id,
                tenant_id: tenant_from_scope(&tenant_scope),
                kind: FederatedStoryGapKind::from_stored(&kind)?,
                detected_at,
                last_observed_at,
                detail,
                next_action,
            })
        },
    )
    .collect()
}

async fn persist_reliability_evidence(
    pool: &PgPool,
    source_service_id: &str,
    tenant_id: Option<&str>,
    observed_at: DateTime<Utc>,
    result: Result<Option<ReliabilityReport>, FederatedStoryFeedError>,
) -> AppResult<()> {
    let (status, report, detail, next_action) = match result {
        Ok(Some(report)) => (
            FederatedStoryReliabilityStatus::Available,
            Some(serde_json::to_value(report).map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    "Could not serialize Federated Story reliability evidence",
                )
                .with_source(error)
            })?),
            None,
            None,
        ),
        Ok(None) => (
            FederatedStoryReliabilityStatus::NotDeclared,
            None,
            Some("Service does not declare a Reliability Contract".to_owned()),
            None,
        ),
        Err(error) => (
            FederatedStoryReliabilityStatus::Unavailable,
            None,
            Some(error.detail),
            Some(error.next_action),
        ),
    };
    sqlx::query(
        r#"
        insert into platform.federated_story_reliability (
            source_service_id, tenant_scope, status, report, detail,
            next_action, observed_at
        ) values ($1, $2, $3, $4, $5, $6, $7)
        on conflict (source_service_id, tenant_scope) do update set
            status = excluded.status,
            report = excluded.report,
            detail = excluded.detail,
            next_action = excluded.next_action,
            observed_at = excluded.observed_at
        "#,
    )
    .bind(source_service_id)
    .bind(tenant_scope(tenant_id))
    .bind(status.as_str())
    .bind(report)
    .bind(detail)
    .bind(next_action)
    .bind(observed_at)
    .execute(pool)
    .await
    .map_err(aggregation_store_error)?;
    Ok(())
}

async fn load_reliability_evidence(
    pool: &PgPool,
    tenant_id: Option<&str>,
) -> AppResult<Vec<FederatedStoryReliabilityEvidence>> {
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            Option<Value>,
            Option<String>,
            Option<String>,
            DateTime<Utc>,
        ),
    >(
        r#"
        select source_service_id, status, report, detail, next_action, observed_at
        from platform.federated_story_reliability
        where tenant_scope = $1
        order by source_service_id
        "#,
    )
    .bind(tenant_scope(tenant_id))
    .fetch_all(pool)
    .await
    .map_err(aggregation_store_error)?;

    rows.into_iter()
        .map(
            |(source_service_id, status, report, detail, next_action, observed_at)| {
                let status = FederatedStoryReliabilityStatus::from_stored(&status)?;
                let report = report
                    .map(|report| {
                        serde_json::from_value::<ReliabilityReport>(report).map_err(|error| {
                            AppError::new(
                                ErrorCode::Internal,
                                "Stored Federated Story reliability report is invalid",
                            )
                            .with_source(error)
                        })
                    })
                    .transpose()?;
                if (status == FederatedStoryReliabilityStatus::Available) != report.is_some() {
                    return Err(AppError::new(
                        ErrorCode::Internal,
                        "Stored Federated Story reliability evidence is inconsistent",
                    ));
                }
                Ok(FederatedStoryReliabilityEvidence {
                    source_service_id,
                    observed_at,
                    status,
                    report,
                    detail,
                    next_action,
                })
            },
        )
        .collect()
}

fn tenant_scope(tenant_id: Option<&str>) -> &str {
    tenant_id.unwrap_or("")
}

fn tenant_from_scope(tenant_scope: &str) -> Option<String> {
    (!tenant_scope.is_empty()).then(|| tenant_scope.to_owned())
}

fn federated_segment_id(segment: &StorySegment) -> String {
    format!(
        "service[{}]:{};segment[{}]:{}",
        segment.source.service_id.len(),
        segment.source.service_id,
        segment.segment_id.len(),
        segment.segment_id
    )
}

fn telemetry_attribute<'a>(attributes: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| attributes.get(*key).and_then(Value::as_str))
}

fn safe_telemetry_attributes(attributes: &Value) -> Value {
    const SAFE_KEYS: &[&str] = &[
        "lenso.story_id",
        "lenso.story_segment_id",
        "lenso.segment_id",
        "lenso.service_id",
        "lenso.workflow_instance_id",
        "lenso.workflow_step_id",
        "otel.trace_id",
        "lenso.trace_id",
        "trace_id",
        "trace.trace_id",
    ];
    let mut safe = serde_json::Map::new();
    for key in SAFE_KEYS {
        if let Some(value) = attributes.get(*key) {
            safe.insert((*key).to_owned(), value.clone());
        }
    }
    Value::Object(safe)
}

fn aggregation_store_error(error: sqlx::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        "Federated Runtime Story aggregation store failed",
    )
    .with_source(error)
}
