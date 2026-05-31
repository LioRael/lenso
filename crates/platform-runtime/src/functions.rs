use crate::retries::RetryPolicy;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use platform_core::{
    ActorContext, AppError, AppResult, CorrelationId, DbPool, ErrorCode, ExecutionContext,
    ExecutionId, TenantId, TraceContext,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::sync::Arc;
use uuid::Uuid;

#[async_trait]
pub trait FunctionHandler: Debug + Send + Sync {
    async fn call(&self, ctx: ExecutionContext, input: Value) -> AppResult<Value>;
}

pub use FunctionHandler as RuntimeFunction;

#[derive(Debug, Clone)]
pub struct FunctionDefinition {
    pub name: &'static str,
    pub version: u16,
    pub queue: &'static str,
    pub retry_policy: RetryPolicy,
    pub handler: Arc<dyn FunctionHandler>,
}

#[derive(Debug, Default, Clone)]
pub struct FunctionRegistry {
    functions: BTreeMap<String, FunctionDefinition>,
}

impl FunctionRegistry {
    pub fn register(&mut self, function: FunctionDefinition) {
        self.functions.insert(function.name.to_owned(), function);
    }

    pub fn get(&self, name: &str) -> Option<&FunctionDefinition> {
        self.functions.get(name)
    }

    pub fn all(&self) -> impl Iterator<Item = &FunctionDefinition> {
        self.functions.values()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FunctionRunStatus {
    Pending,
    Processing,
    Completed,
    Failed,
    Dead,
}

impl FunctionRunStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Dead => "dead",
        }
    }
}

#[derive(Debug, Clone)]
pub struct EnqueueFunctionRequest {
    pub function_name: String,
    pub input_json: Value,
    pub correlation_id: CorrelationId,
    pub actor: ActorContext,
    pub max_attempts: Option<i32>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClaimedFunctionRun {
    pub id: String,
    pub function_name: String,
    pub input_json: Value,
    pub attempts: i32,
    pub max_attempts: i32,
    pub correlation_id: String,
    pub actor: ActorContext,
}

#[derive(Debug, Clone)]
pub struct RuntimeClient {
    pool: DbPool,
}

impl RuntimeClient {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn enqueue_function(&self, request: EnqueueFunctionRequest) -> AppResult<String> {
        let id = format!("fnrun_{}", Uuid::now_v7());
        let max_attempts = request.max_attempts.unwrap_or(3);

        sqlx::query(
            r#"
            insert into runtime.function_runs (
                id,
                function_name,
                input_json,
                max_attempts,
                correlation_id,
                actor
            )
            values ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(&id)
        .bind(&request.function_name)
        .bind(&request.input_json)
        .bind(max_attempts)
        .bind(&request.correlation_id.0)
        .bind(serde_json::to_value(&request.actor).map_err(map_serde_error)?)
        .execute(&self.pool)
        .await
        .map_err(map_runtime_error)?;

        Ok(id)
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeWorker {
    pool: DbPool,
    registry: Arc<FunctionRegistry>,
    worker_id: String,
    batch_size: i64,
}

impl RuntimeWorker {
    pub fn new(
        pool: DbPool,
        registry: Arc<FunctionRegistry>,
        worker_id: impl Into<String>,
        batch_size: i64,
    ) -> Self {
        Self {
            pool,
            registry,
            worker_id: worker_id.into(),
            batch_size,
        }
    }

    pub async fn claim_batch(&self) -> AppResult<Vec<ClaimedFunctionRun>> {
        sqlx::query_as::<_, FunctionRunRow>(
            r#"
            with claimed as (
                select id
                from runtime.function_runs
                where status in ('pending', 'failed')
                  and available_at <= now()
                order by available_at asc, created_at asc
                limit $1
                for update skip locked
            )
            update runtime.function_runs function_run
            set status = 'processing',
                locked_at = now(),
                locked_by = $2,
                started_at = coalesce(started_at, now()),
                last_error = null,
                updated_at = now()
            from claimed
            where function_run.id = claimed.id
            returning
                function_run.id,
                function_run.function_name,
                function_run.input_json,
                function_run.attempts,
                function_run.max_attempts,
                function_run.correlation_id,
                function_run.actor
            "#,
        )
        .bind(self.batch_size)
        .bind(&self.worker_id)
        .fetch_all(&self.pool)
        .await
        .map(|rows| {
            rows.into_iter()
                .map(TryInto::try_into)
                .collect::<AppResult<Vec<_>>>()
        })
        .map_err(map_runtime_error)?
    }

    pub async fn claim_and_run_batch(&self) -> AppResult<usize> {
        let runs = self.claim_batch().await?;
        let count = runs.len();

        for run in runs {
            self.run_claimed(run).await?;
        }

        Ok(count)
    }

    async fn run_claimed(&self, run: ClaimedFunctionRun) -> AppResult<()> {
        let Some(definition) = self.registry.get(&run.function_name) else {
            let error = AppError::new(
                ErrorCode::Internal,
                format!("Runtime function {} is not registered", run.function_name),
            )
            .retryable();
            self.mark_failed(&run, &error).await?;
            return Ok(());
        };

        let attempt = u32::try_from(run.attempts + 1).unwrap_or(u32::MAX);
        let ctx = ExecutionContext {
            execution_id: ExecutionId(run.id.clone()),
            function_name: run.function_name.clone(),
            attempt,
            queue: definition.queue.to_owned(),
            correlation_id: CorrelationId::new(run.correlation_id.clone()),
            causation_id: None,
            actor: run.actor.clone(),
            tenant_id: None::<TenantId>,
            trace: TraceContext::default(),
            deadline: None::<DateTime<Utc>>,
        };

        match definition.handler.call(ctx, run.input_json.clone()).await {
            Ok(_output) => self.mark_completed(&run.id).await,
            Err(error) => self.mark_failed(&run, &error).await,
        }
    }

    pub async fn mark_completed(&self, id: &str) -> AppResult<()> {
        sqlx::query(
            r#"
            update runtime.function_runs
            set status = 'completed',
                completed_at = now(),
                locked_at = null,
                locked_by = null,
                last_error = null,
                updated_at = now()
            where id = $1
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(map_runtime_error)
    }

    pub async fn mark_failed(&self, run: &ClaimedFunctionRun, error: &AppError) -> AppResult<()> {
        let next_attempt = run.attempts + 1;
        let status = if next_attempt >= run.max_attempts {
            FunctionRunStatus::Dead
        } else if error.retryable {
            FunctionRunStatus::Failed
        } else {
            FunctionRunStatus::Dead
        };

        sqlx::query(
            r#"
            update runtime.function_runs
            set status = $2,
                attempts = attempts + 1,
                available_at = case when $2 = 'failed' then now() else available_at end,
                locked_at = null,
                locked_by = null,
                last_error = $3,
                updated_at = now()
            where id = $1
            "#,
        )
        .bind(&run.id)
        .bind(status.as_str())
        .bind(error.public_message.as_str())
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(map_runtime_error)
    }
}

type FunctionRunRow = (String, String, Value, i32, i32, String, Value);

impl TryFrom<FunctionRunRow> for ClaimedFunctionRun {
    type Error = AppError;

    fn try_from(row: FunctionRunRow) -> Result<Self, Self::Error> {
        let (id, function_name, input_json, attempts, max_attempts, correlation_id, actor) = row;
        Ok(Self {
            id,
            function_name,
            input_json,
            attempts,
            max_attempts,
            correlation_id,
            actor: serde_json::from_value(actor).map_err(map_serde_error)?,
        })
    }
}

fn map_runtime_error(source: sqlx::Error) -> AppError {
    AppError::new(ErrorCode::Internal, "Runtime operation failed").with_source(source)
}

fn map_serde_error(source: serde_json::Error) -> AppError {
    AppError::new(ErrorCode::Internal, "Runtime payload serialization failed").with_source(source)
}
