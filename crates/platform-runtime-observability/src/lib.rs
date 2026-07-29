//! Service-owned Runtime Observability Capability Provider.

use axum::{Extension, Json, extract::Query};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use lenso_service::system_plane::{
    CapabilityAdvertisement, RUNTIME_OBSERVABILITY_FEATURE_QUEUE_SUMMARY,
    RUNTIME_OBSERVABILITY_FEATURE_RECOVERY_FEED, RUNTIME_OBSERVABILITY_PATH,
    RUNTIME_OBSERVABILITY_PROTOCOL, RuntimeObservabilitySnapshot, RuntimeObservabilityStatus,
    RuntimeObservationChange, RuntimeObservationChangeKind, RuntimeObservationContinuity,
    RuntimeObservationEvidenceGap, RuntimeObservationFeed, RuntimeObservationGapReason,
    RuntimeQueueKind, RuntimeQueueSummary, runtime_observability_schema_digest,
};
use platform_core::Migration;
use platform_system_plane::{
    AuthorizedSystemPlaneCaller, SystemPlaneErrorBody, SystemPlaneRejection,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use sqlx::PgPool;
use std::{collections::BTreeSet, sync::Arc};
use utoipa_axum::{router::OpenApiRouter, routes};

type QueueRow = (
    i64,
    i64,
    i64,
    i64,
    i64,
    Option<i64>,
    Option<i64>,
    Option<DateTime<Utc>>,
);

type ChangeRow = (i64, String, String, String, DateTime<Utc>);

pub const RUNTIME_OBSERVABILITY_MIGRATIONS: &[Migration] = &[Migration {
    name: "runtime-observability/0001_record_runtime_observation_changes",
    sql: include_str!("../migrations/0001_record_runtime_observation_changes.sql"),
}];

const DEFAULT_FEED_LIMIT: u16 = 100;
const MAX_FEED_LIMIT: u16 = 500;

#[derive(Debug, Deserialize)]
struct FeedQuery {
    cursor: String,
    #[serde(default = "default_feed_limit")]
    limit: u16,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ObservationCursor {
    version: u8,
    service_id: String,
    service_revision: String,
    schema_digest: String,
    sequence: u64,
    checksum: String,
}

#[derive(Debug, Clone)]
pub struct RuntimeObservabilityProvider {
    pool: PgPool,
    service_id: String,
    service_revision: String,
}

impl RuntimeObservabilityProvider {
    #[must_use]
    pub fn new(
        pool: PgPool,
        service_id: impl Into<String>,
        service_revision: impl Into<String>,
    ) -> Self {
        Self {
            pool,
            service_id: service_id.into(),
            service_revision: service_revision.into(),
        }
    }

    #[must_use]
    pub fn advertisement() -> CapabilityAdvertisement {
        CapabilityAdvertisement {
            contract_id: RUNTIME_OBSERVABILITY_PROTOCOL.to_owned(),
            major_version: 1,
            feature_ids: BTreeSet::from([
                RUNTIME_OBSERVABILITY_FEATURE_QUEUE_SUMMARY.to_owned(),
                RUNTIME_OBSERVABILITY_FEATURE_RECOVERY_FEED.to_owned(),
            ]),
            schema_digest: runtime_observability_schema_digest(),
            endpoint: RUNTIME_OBSERVABILITY_PATH.to_owned(),
        }
    }

    #[must_use]
    pub fn service_id(&self) -> &str {
        &self.service_id
    }

    #[must_use]
    pub fn service_revision(&self) -> &str {
        &self.service_revision
    }

    #[must_use]
    pub fn validation_error(&self) -> Option<&'static str> {
        if self.service_id.trim().is_empty() {
            return Some("Runtime Observability provider Service identity must not be empty");
        }
        if self.service_revision.trim().is_empty() {
            return Some("Runtime Observability provider Service revision must not be empty");
        }
        None
    }

    pub async fn snapshot(&self) -> Result<RuntimeObservabilitySnapshot, sqlx::Error> {
        let mut transaction = self.pool.begin().await?;
        sqlx::query("set transaction isolation level repeatable read read only")
            .execute(&mut *transaction)
            .await?;
        let outbox = sqlx::query_as::<_, QueueRow>(
            r#"
            select
                count(*) filter (where status = 'pending')::bigint,
                count(*) filter (where status = 'processing')::bigint,
                count(*) filter (where status = 'published')::bigint,
                count(*) filter (where status = 'failed')::bigint,
                count(*) filter (where status = 'dead')::bigint,
                extract(epoch from now() - min(created_at) filter (where status = 'pending'))::bigint,
                extract(epoch from now() - min(created_at) filter (where status in ('failed', 'dead')))::bigint,
                max(greatest(
                    created_at,
                    available_at,
                    coalesce(locked_at, created_at),
                    coalesce(published_at, created_at)
                ))
            from platform.outbox
            "#,
        )
        .fetch_one(&mut *transaction)
        .await?;
        let functions = sqlx::query_as::<_, QueueRow>(
            r#"
            select
                count(*) filter (where status = 'pending')::bigint,
                count(*) filter (where status in ('processing', 'running'))::bigint,
                count(*) filter (where status = 'completed')::bigint,
                count(*) filter (where status = 'failed')::bigint,
                count(*) filter (where status = 'dead')::bigint,
                extract(epoch from now() - min(created_at) filter (where status = 'pending'))::bigint,
                extract(epoch from now() - min(created_at) filter (where status in ('failed', 'dead')))::bigint,
                max(updated_at)
            from runtime.function_runs
            "#,
        )
        .fetch_one(&mut *transaction)
        .await?;
        let sequence = sqlx::query_scalar::<_, i64>(
            "select coalesce(max(sequence), 0)::bigint from platform.runtime_observation_changes",
        )
        .fetch_one(&mut *transaction)
        .await?;
        transaction.commit().await?;
        let status = runtime_status(&outbox, &functions);
        let snapshot_revision = snapshot_revision(&outbox, &functions);
        let schema_digest = runtime_observability_schema_digest();
        let next_cursor = self.cursor(count(sequence), &schema_digest);
        Ok(RuntimeObservabilitySnapshot {
            protocol: RUNTIME_OBSERVABILITY_PROTOCOL.to_owned(),
            service_id: self.service_id.clone(),
            service_revision: self.service_revision.clone(),
            snapshot_revision,
            schema_digest,
            next_cursor,
            observed_at: Utc::now(),
            status,
            queues: vec![
                queue_summary(RuntimeQueueKind::Outbox, &outbox),
                queue_summary(RuntimeQueueKind::Functions, &functions),
            ],
        })
    }

    pub async fn feed(
        &self,
        cursor: &str,
        limit: u16,
    ) -> Result<RuntimeObservationFeed, sqlx::Error> {
        let schema_digest = runtime_observability_schema_digest();
        let decoded = self.decode_cursor(cursor, &schema_digest);
        let sequence = match decoded {
            Ok(sequence) => sequence,
            Err(reason) => return Ok(self.gap_feed(reason, &schema_digest)),
        };
        let mut transaction = self.pool.begin().await?;
        sqlx::query("set transaction isolation level repeatable read read only")
            .execute(&mut *transaction)
            .await?;
        let minimum = sqlx::query_scalar::<_, Option<i64>>(
            "select min(sequence) from platform.runtime_observation_changes",
        )
        .fetch_one(&mut *transaction)
        .await?;
        if retention_lost(sequence, minimum) {
            transaction.commit().await?;
            return Ok(self.gap_feed(RuntimeObservationGapReason::RetentionLost, &schema_digest));
        }
        let limit = limit.clamp(1, MAX_FEED_LIMIT);
        let rows = sqlx::query_as::<_, ChangeRow>(
            r#"
            select sequence, queue_kind, resource_id, change_kind, recorded_at
            from platform.runtime_observation_changes
            where sequence > $1
            order by sequence
            limit $2
            "#,
        )
        .bind(i64::try_from(sequence).unwrap_or(i64::MAX))
        .bind(i64::from(limit) + 1)
        .fetch_all(&mut *transaction)
        .await?;
        transaction.commit().await?;
        let has_more = rows.len() > usize::from(limit);
        let changes = rows
            .into_iter()
            .take(usize::from(limit))
            .map(change)
            .collect::<Vec<_>>();
        let next_sequence = changes.last().map_or(sequence, |item| item.sequence);
        Ok(RuntimeObservationFeed {
            protocol: RUNTIME_OBSERVABILITY_PROTOCOL.to_owned(),
            service_id: self.service_id.clone(),
            service_revision: self.service_revision.clone(),
            schema_digest: schema_digest.clone(),
            collected_at: Utc::now(),
            continuity: RuntimeObservationContinuity::Continuous,
            evidence_gap: None,
            changes,
            next_cursor: self.cursor(next_sequence, &schema_digest),
            has_more,
        })
    }

    fn cursor(&self, sequence: u64, schema_digest: &str) -> String {
        let mut cursor = ObservationCursor {
            version: 1,
            service_id: self.service_id.clone(),
            service_revision: self.service_revision.clone(),
            schema_digest: schema_digest.to_owned(),
            sequence,
            checksum: String::new(),
        };
        cursor.checksum = cursor_checksum(&cursor);
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&cursor).expect("observation cursor serializes"))
    }

    fn decode_cursor(
        &self,
        encoded: &str,
        schema_digest: &str,
    ) -> Result<u64, RuntimeObservationGapReason> {
        let bytes = URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| RuntimeObservationGapReason::InvalidCursor)?;
        let cursor: ObservationCursor = serde_json::from_slice(&bytes)
            .map_err(|_| RuntimeObservationGapReason::InvalidCursor)?;
        if cursor.version != 1
            || cursor.service_id != self.service_id
            || cursor.checksum != cursor_checksum(&cursor)
        {
            return Err(RuntimeObservationGapReason::InvalidCursor);
        }
        if cursor.service_revision != self.service_revision {
            return Err(RuntimeObservationGapReason::ServiceRevisionChanged);
        }
        if cursor.schema_digest != schema_digest {
            return Err(RuntimeObservationGapReason::SchemaChanged);
        }
        Ok(cursor.sequence)
    }

    fn gap_feed(
        &self,
        reason: RuntimeObservationGapReason,
        schema_digest: &str,
    ) -> RuntimeObservationFeed {
        let message = match reason {
            RuntimeObservationGapReason::InvalidCursor => {
                "The observation cursor is invalid or belongs to another Service."
            }
            RuntimeObservationGapReason::ServiceRevisionChanged => {
                "The Service revision changed after the observation cursor was issued."
            }
            RuntimeObservationGapReason::SchemaChanged => {
                "The Runtime Observability schema changed after the cursor was issued."
            }
            RuntimeObservationGapReason::RetentionLost => {
                "Required observation changes are no longer retained by the Service."
            }
        };
        RuntimeObservationFeed {
            protocol: RUNTIME_OBSERVABILITY_PROTOCOL.to_owned(),
            service_id: self.service_id.clone(),
            service_revision: self.service_revision.clone(),
            schema_digest: schema_digest.to_owned(),
            collected_at: Utc::now(),
            continuity: RuntimeObservationContinuity::ResetRequired,
            evidence_gap: Some(RuntimeObservationEvidenceGap {
                reason,
                message: message.to_owned(),
                required_action: "fetch_fresh_runtime_observability_snapshot".to_owned(),
            }),
            changes: Vec::new(),
            next_cursor: String::new(),
            has_more: false,
        }
    }
}

#[must_use]
pub fn router<S>(provider: Option<Arc<RuntimeObservabilityProvider>>) -> OpenApiRouter<S>
where
    S: Clone + Send + Sync + 'static,
{
    OpenApiRouter::new()
        .routes(routes!(runtime_observability_snapshot))
        .routes(routes!(runtime_observability_feed))
        .layer(Extension(provider))
}

#[utoipa::path(
    get,
    path = "/system-plane/v1/runtime-observability/changes",
    params(("cursor" = String, Query, description = "Opaque cursor returned by a snapshot or prior feed page"), ("limit" = Option<u16>, Query)),
    responses(
        (status = 200, description = "Runtime observation changes or an explicit Evidence Gap", body = RuntimeObservationFeed),
        (status = 401, description = "Workload Identity or transport binding was not accepted", body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 403, description = "Caller has no active enrollment grant", body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 503, description = "Runtime observation is unavailable", body = SystemPlaneErrorBody, content_type = "application/problem+json")
    ),
    security(("bearer_auth" = [])),
    tag = "system-plane-runtime-observability"
)]
async fn runtime_observability_feed(
    caller: AuthorizedSystemPlaneCaller,
    Extension(provider): Extension<Option<Arc<RuntimeObservabilityProvider>>>,
    Query(query): Query<FeedQuery>,
) -> Result<Json<RuntimeObservationFeed>, SystemPlaneRejection> {
    let provider = require_provider(provider)?;
    caller.require_capability(
        RUNTIME_OBSERVABILITY_PROTOCOL,
        &runtime_observability_schema_digest(),
        [RUNTIME_OBSERVABILITY_FEATURE_RECOVERY_FEED],
    )?;
    provider
        .feed(&query.cursor, query.limit)
        .await
        .map(Json)
        .map_err(|_| observation_failed())
}

#[utoipa::path(
    get,
    path = "/system-plane/v1/runtime-observability",
    responses(
        (status = 200, description = "Revisioned runtime queue snapshot", body = RuntimeObservabilitySnapshot),
        (status = 401, description = "Workload Identity or transport binding was not accepted", body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 403, description = "Caller has no active enrollment grant", body = SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 503, description = "Runtime observation is unavailable", body = SystemPlaneErrorBody, content_type = "application/problem+json")
    ),
    security(("bearer_auth" = [])),
    tag = "system-plane-runtime-observability"
)]
async fn runtime_observability_snapshot(
    caller: AuthorizedSystemPlaneCaller,
    Extension(provider): Extension<Option<Arc<RuntimeObservabilityProvider>>>,
) -> Result<Json<RuntimeObservabilitySnapshot>, SystemPlaneRejection> {
    let provider = require_provider(provider)?;
    caller.require_capability(
        RUNTIME_OBSERVABILITY_PROTOCOL,
        &runtime_observability_schema_digest(),
        [RUNTIME_OBSERVABILITY_FEATURE_QUEUE_SUMMARY],
    )?;
    provider
        .snapshot()
        .await
        .map(Json)
        .map_err(|_| observation_failed())
}

fn require_provider(
    provider: Option<Arc<RuntimeObservabilityProvider>>,
) -> Result<Arc<RuntimeObservabilityProvider>, SystemPlaneRejection> {
    provider.ok_or_else(|| {
        SystemPlaneRejection::unavailable(
            "runtime_observability_unavailable",
            "Runtime Observability capability is not configured for this Service",
            "configure_runtime_observability",
        )
    })
}

fn observation_failed() -> SystemPlaneRejection {
    SystemPlaneRejection::unavailable(
        "runtime_observation_failed",
        "Runtime observation query failed",
        "restore_service_store_observation",
    )
}

fn queue_summary(kind: RuntimeQueueKind, row: &QueueRow) -> RuntimeQueueSummary {
    RuntimeQueueSummary {
        queue: kind,
        pending: count(row.0),
        active: count(row.1),
        completed: count(row.2),
        failed: count(row.3),
        dead: count(row.4),
        oldest_pending_age_seconds: row.5.map(count),
        oldest_failed_age_seconds: row.6.map(count),
    }
}

fn runtime_status(outbox: &QueueRow, functions: &QueueRow) -> RuntimeObservabilityStatus {
    if outbox.4 > 0 || functions.4 > 0 {
        RuntimeObservabilityStatus::Failing
    } else if outbox.3 > 0 || functions.3 > 0 {
        RuntimeObservabilityStatus::Degraded
    } else {
        RuntimeObservabilityStatus::Healthy
    }
}

fn snapshot_revision(outbox: &QueueRow, functions: &QueueRow) -> String {
    let material = format!(
        "{:?}:{:?}",
        (outbox.0, outbox.1, outbox.2, outbox.3, outbox.4, outbox.7),
        (
            functions.0,
            functions.1,
            functions.2,
            functions.3,
            functions.4,
            functions.7
        )
    );
    format!("sha256:{}", hex(&Sha256::digest(material)))
}

fn change(row: ChangeRow) -> RuntimeObservationChange {
    RuntimeObservationChange {
        sequence: count(row.0),
        queue: match row.1.as_str() {
            "outbox" => RuntimeQueueKind::Outbox,
            "functions" => RuntimeQueueKind::Functions,
            _ => unreachable!("migration constrains runtime observation queue kinds"),
        },
        resource_id: row.2,
        change_kind: match row.3.as_str() {
            "upserted" => RuntimeObservationChangeKind::Upserted,
            "deleted" => RuntimeObservationChangeKind::Deleted,
            _ => unreachable!("migration constrains runtime observation change kinds"),
        },
        recorded_at: row.4,
    }
}

fn cursor_checksum(cursor: &ObservationCursor) -> String {
    let material = format!(
        "{}:{}:{}:{}:{}",
        cursor.version,
        cursor.service_id,
        cursor.service_revision,
        cursor.schema_digest,
        cursor.sequence
    );
    format!("sha256:{}", hex(&Sha256::digest(material)))
}

const fn default_feed_limit() -> u16 {
    DEFAULT_FEED_LIMIT
}

fn count(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn retention_lost(sequence: u64, minimum: Option<i64>) -> bool {
    minimum.is_some_and(|minimum| sequence.saturating_add(1) < count(minimum))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    fn provider(revision: &str) -> RuntimeObservabilityProvider {
        RuntimeObservabilityProvider::new(
            PgPoolOptions::new()
                .connect_lazy("postgres://lenso:lenso@127.0.0.1/lenso")
                .unwrap(),
            "support",
            revision,
        )
    }

    #[tokio::test]
    async fn cursor_is_scoped_to_service_revision_and_schema() {
        let first = provider("release:one");
        let schema = runtime_observability_schema_digest();
        let cursor = first.cursor(42, &schema);

        assert_eq!(first.decode_cursor(&cursor, &schema), Ok(42));
        assert_eq!(
            provider("release:two").decode_cursor(&cursor, &schema),
            Err(RuntimeObservationGapReason::ServiceRevisionChanged)
        );
        assert_eq!(
            first.decode_cursor(&cursor, "sha256:changed"),
            Err(RuntimeObservationGapReason::SchemaChanged)
        );
        assert_eq!(
            first.decode_cursor("invalid", &schema),
            Err(RuntimeObservationGapReason::InvalidCursor)
        );
    }

    #[test]
    fn retention_gap_requires_a_missing_sequence() {
        assert!(!retention_lost(4, Some(5)));
        assert!(retention_lost(4, Some(6)));
        assert!(!retention_lost(4, None));
    }
}
