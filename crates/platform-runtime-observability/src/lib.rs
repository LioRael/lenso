//! Service-owned Runtime Observability Capability Provider.

use axum::{Extension, Json};
use chrono::{DateTime, Utc};
use lenso_service::system_plane::{
    CapabilityAdvertisement, RUNTIME_OBSERVABILITY_FEATURE_QUEUE_SUMMARY,
    RUNTIME_OBSERVABILITY_PATH, RUNTIME_OBSERVABILITY_PROTOCOL, RuntimeObservabilitySnapshot,
    RuntimeObservabilityStatus, RuntimeQueueKind, RuntimeQueueSummary,
    runtime_observability_schema_digest,
};
use platform_system_plane::{
    AuthorizedSystemPlaneCaller, SystemPlaneErrorBody, SystemPlaneRejection,
};
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
            feature_ids: BTreeSet::from([RUNTIME_OBSERVABILITY_FEATURE_QUEUE_SUMMARY.to_owned()]),
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
        .fetch_one(&self.pool)
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
        .fetch_one(&self.pool)
        .await?;
        let status = runtime_status(&outbox, &functions);
        let snapshot_revision = snapshot_revision(&outbox, &functions);
        Ok(RuntimeObservabilitySnapshot {
            protocol: RUNTIME_OBSERVABILITY_PROTOCOL.to_owned(),
            service_id: self.service_id.clone(),
            service_revision: self.service_revision.clone(),
            snapshot_revision,
            observed_at: Utc::now(),
            status,
            queues: vec![
                queue_summary(RuntimeQueueKind::Outbox, &outbox),
                queue_summary(RuntimeQueueKind::Functions, &functions),
            ],
        })
    }
}

#[must_use]
pub fn router<S>(provider: Option<Arc<RuntimeObservabilityProvider>>) -> OpenApiRouter<S>
where
    S: Clone + Send + Sync + 'static,
{
    OpenApiRouter::new()
        .routes(routes!(runtime_observability_snapshot))
        .layer(Extension(provider))
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
    _caller: AuthorizedSystemPlaneCaller,
    Extension(provider): Extension<Option<Arc<RuntimeObservabilityProvider>>>,
) -> Result<Json<RuntimeObservabilitySnapshot>, SystemPlaneRejection> {
    let provider = provider.ok_or_else(|| {
        SystemPlaneRejection::unavailable(
            "runtime_observability_unavailable",
            "Runtime Observability capability is not configured for this Service",
            "configure_runtime_observability",
        )
    })?;
    provider.snapshot().await.map(Json).map_err(|_error| {
        SystemPlaneRejection::unavailable(
            "runtime_observation_failed",
            "Runtime observation query failed",
            "restore_service_store_observation",
        )
    })
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
    let material = format!("{outbox:?}:{functions:?}");
    format!("sha256:{}", hex(&Sha256::digest(material)))
}

fn count(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
