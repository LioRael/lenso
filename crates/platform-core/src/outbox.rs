use crate::db::{DbPool, DbTransaction};
use crate::error::{AppError, AppResult, ErrorCode};
use crate::events::EventEnvelope;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboxStatus {
    Pending,
    Processing,
    Published,
    Failed,
    Dead,
}

impl OutboxStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Published => "published",
            Self::Failed => "failed",
            Self::Dead => "dead",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OutboxEvent {
    pub id: String,
    pub event_name: String,
    pub event_version: u16,
    pub source_module: String,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub correlation_id: String,
    pub causation_id: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub payload: Value,
    pub headers: Value,
}

impl OutboxEvent {
    pub fn from_envelope(aggregate_type: impl Into<String>, event: &EventEnvelope) -> Self {
        Self {
            id: event.event_id.clone(),
            event_name: event.event_name.clone(),
            event_version: event.event_version,
            source_module: event.source_module.clone(),
            aggregate_type: aggregate_type.into(),
            aggregate_id: event.subject.clone(),
            correlation_id: event.correlation_id.0.clone(),
            causation_id: event.causation_id.clone(),
            occurred_at: event.occurred_at,
            payload: event.payload.clone(),
            headers: json!({
                "actor": event.actor,
                "schema_ref": event.schema_ref,
                "trace": event.trace,
            }),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClaimedOutboxEvent {
    pub id: String,
    pub event_name: String,
    pub event_version: u16,
    pub source_module: String,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub correlation_id: String,
    pub causation_id: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub payload: Value,
    pub headers: Value,
    pub attempts: i32,
    pub max_attempts: i32,
}

#[derive(Debug, Clone, Default)]
pub struct OutboxPublisher;

impl OutboxPublisher {
    pub async fn publish_in_tx(
        &self,
        tx: &mut DbTransaction<'_>,
        event: &OutboxEvent,
    ) -> AppResult<()> {
        sqlx::query(
            r#"
            insert into platform.outbox (
                id,
                event_name,
                event_version,
                source_module,
                aggregate_type,
                aggregate_id,
                correlation_id,
                causation_id,
                occurred_at,
                payload,
                headers
            )
            values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(&event.id)
        .bind(&event.event_name)
        .bind(i32::from(event.event_version))
        .bind(&event.source_module)
        .bind(&event.aggregate_type)
        .bind(&event.aggregate_id)
        .bind(&event.correlation_id)
        .bind(&event.causation_id)
        .bind(event.occurred_at)
        .bind(&event.payload)
        .bind(&event.headers)
        .execute(&mut **tx)
        .await
        .map(|_| ())
        .map_err(map_outbox_error)
    }

    pub async fn pending_count(&self, pool: &DbPool) -> AppResult<i64> {
        sqlx::query_scalar(
            r#"
            select count(*)
            from platform.outbox
            where status = 'pending'
            "#,
        )
        .fetch_one(pool)
        .await
        .map_err(map_outbox_error)
    }
}

#[async_trait]
pub trait EventDispatcher: Debug + Send + Sync {
    async fn dispatch(&self, event: &ClaimedOutboxEvent) -> AppResult<()>;
}

#[async_trait]
pub trait EventHandler: Debug + Send + Sync {
    fn event_name(&self) -> &'static str;
    async fn handle(&self, event: &ClaimedOutboxEvent) -> AppResult<()>;
}

#[derive(Debug, Clone, Default)]
pub struct EventHandlerRegistry {
    handlers: BTreeMap<&'static str, Vec<Arc<dyn EventHandler>>>,
}

impl EventHandlerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, handler: Arc<dyn EventHandler>) {
        self.handlers
            .entry(handler.event_name())
            .or_default()
            .push(handler);
    }

    pub fn register_all(&mut self, handlers: impl IntoIterator<Item = Arc<dyn EventHandler>>) {
        for handler in handlers {
            self.register(handler);
        }
    }

    pub fn handler_count(&self, event_name: &str) -> usize {
        self.handlers.get(event_name).map_or(0, Vec::len)
    }
}

#[async_trait]
impl EventDispatcher for EventHandlerRegistry {
    async fn dispatch(&self, event: &ClaimedOutboxEvent) -> AppResult<()> {
        let Some(handlers) = self.handlers.get(event.event_name.as_str()) else {
            tracing::debug!(
                event_name = %event.event_name,
                outbox_id = %event.id,
                "no in-process event handlers registered"
            );
            return Ok(());
        };

        for handler in handlers {
            handler.handle(event).await?;
        }

        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct LoggingEventDispatcher;

#[async_trait]
impl EventDispatcher for LoggingEventDispatcher {
    async fn dispatch(&self, event: &ClaimedOutboxEvent) -> AppResult<()> {
        tracing::info!(
            outbox_id = %event.id,
            event_name = %event.event_name,
            event_version = event.event_version,
            aggregate_id = %event.aggregate_id,
            correlation_id = %event.correlation_id,
            "outbox event dispatched"
        );
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct OutboxRelay {
    pool: DbPool,
    worker_id: String,
    batch_size: i64,
}

impl OutboxRelay {
    pub fn new(pool: DbPool, worker_id: impl Into<String>, batch_size: i64) -> Self {
        Self {
            pool,
            worker_id: worker_id.into(),
            batch_size,
        }
    }

    pub async fn claim_batch(&self) -> AppResult<Vec<ClaimedOutboxEvent>> {
        sqlx::query_as::<_, OutboxRow>(
            r#"
            with claimed as (
                select id
                from platform.outbox
                where status in ('pending', 'failed')
                  and available_at <= now()
                order by available_at asc, created_at asc
                limit $1
                for update skip locked
            )
            update platform.outbox outbox
            set status = 'processing',
                locked_at = now(),
                locked_by = $2,
                last_error = null
            from claimed
            where outbox.id = claimed.id
            returning
                outbox.id,
                outbox.event_name,
                outbox.event_version,
                outbox.source_module,
                outbox.aggregate_type,
                outbox.aggregate_id,
                outbox.correlation_id,
                outbox.causation_id,
                outbox.occurred_at,
                outbox.payload,
                outbox.headers,
                outbox.attempts,
                outbox.max_attempts
            "#,
        )
        .bind(self.batch_size)
        .bind(&self.worker_id)
        .fetch_all(&self.pool)
        .await
        .map(|rows| rows.into_iter().map(Into::into).collect())
        .map_err(map_outbox_error)
    }

    pub async fn relay_once(&self, dispatcher: &dyn EventDispatcher) -> AppResult<usize> {
        let events = self.claim_batch().await?;
        let count = events.len();

        for event in events {
            match dispatcher.dispatch(&event).await {
                Ok(()) => self.mark_published(&event.id).await?,
                Err(error) => self.mark_dispatch_failed(&event, &error).await?,
            }
        }

        Ok(count)
    }

    pub async fn mark_published(&self, id: &str) -> AppResult<()> {
        sqlx::query(
            r#"
            update platform.outbox
            set status = 'published',
                published_at = now(),
                locked_at = null,
                locked_by = null,
                last_error = null
            where id = $1
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(map_outbox_error)
    }

    pub async fn mark_dispatch_failed(
        &self,
        event: &ClaimedOutboxEvent,
        error: &AppError,
    ) -> AppResult<()> {
        let next_attempt = event.attempts + 1;
        let status = if next_attempt >= event.max_attempts {
            OutboxStatus::Dead
        } else if error.retryable {
            OutboxStatus::Failed
        } else {
            OutboxStatus::Dead
        };

        sqlx::query(
            r#"
            update platform.outbox
            set status = $2,
                attempts = attempts + 1,
                available_at = case when $2 = 'failed' then now() else available_at end,
                locked_at = null,
                locked_by = null,
                last_error = $3
            where id = $1
            "#,
        )
        .bind(&event.id)
        .bind(status.as_str())
        .bind(error.public_message.as_str())
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(map_outbox_error)
    }
}

type OutboxRow = (
    String,
    String,
    i32,
    String,
    String,
    String,
    String,
    Option<String>,
    DateTime<Utc>,
    Value,
    Value,
    i32,
    i32,
);

impl From<OutboxRow> for ClaimedOutboxEvent {
    fn from(row: OutboxRow) -> Self {
        let (
            id,
            event_name,
            event_version,
            source_module,
            aggregate_type,
            aggregate_id,
            correlation_id,
            causation_id,
            occurred_at,
            payload,
            headers,
            attempts,
            max_attempts,
        ) = row;

        Self {
            id,
            event_name,
            event_version: event_version
                .try_into()
                .expect("event_version should fit into u16"),
            source_module,
            aggregate_type,
            aggregate_id,
            correlation_id,
            causation_id,
            occurred_at,
            payload,
            headers,
            attempts,
            max_attempts,
        }
    }
}

fn map_outbox_error(source: sqlx::Error) -> AppError {
    AppError::new(ErrorCode::Internal, "Outbox operation failed").with_source(source)
}
