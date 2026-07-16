use crate::EnqueueFunctionRequest;
use crate::functions::{RuntimeClient, map_runtime_error};
use chrono::Utc;
use lenso_contracts::CronSchedule;
use platform_core::{
    ActorContext, AppError, AppResult, CorrelationId, DbPool, ErrorCode, TraceContext,
};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ScheduledFunctionDefinition {
    pub schedule_key: String,
    pub module_name: String,
    pub schedule_name: String,
    pub function_name: String,
    pub cron: String,
    pub schedule: CronSchedule,
    pub input_json: Value,
    pub max_attempts: i32,
}

#[derive(Debug, Clone)]
pub struct RuntimeScheduler {
    pool: DbPool,
    worker_id: String,
}

impl RuntimeScheduler {
    #[must_use]
    pub fn new(pool: DbPool, worker_id: impl Into<String>) -> Self {
        Self {
            pool,
            worker_id: worker_id.into(),
        }
    }

    pub async fn enqueue_due(
        &self,
        schedules: &[ScheduledFunctionDefinition],
    ) -> AppResult<Vec<String>> {
        let mut run_ids = Vec::new();
        let client = RuntimeClient::new(self.pool.clone());

        for schedule in schedules {
            if let Some(run_id) = self.enqueue_due_schedule(&client, schedule).await? {
                run_ids.push(run_id);
            }
        }

        Ok(run_ids)
    }

    async fn enqueue_due_schedule(
        &self,
        client: &RuntimeClient,
        schedule: &ScheduledFunctionDefinition,
    ) -> AppResult<Option<String>> {
        let next_run_at = schedule.schedule.next_after(Utc::now()).ok_or_else(|| {
            AppError::new(
                ErrorCode::Validation,
                "Scheduled runtime function has no run within the lookahead window",
            )
        })?;

        let mut tx = self.pool.begin().await.map_err(map_runtime_error)?;
        let existing_cron: Option<String> = sqlx::query_scalar(
            r#"
            select cron_expression
            from runtime.scheduled_functions
            where schedule_key = $1
            for update
            "#,
        )
        .bind(&schedule.schedule_key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_runtime_error)?;

        let reset_next_run = existing_cron.as_deref() != Some(schedule.cron.as_str());
        if existing_cron.is_none() {
            sqlx::query(
                r#"
                insert into runtime.scheduled_functions (
                    schedule_key,
                    module_name,
                    schedule_name,
                    function_name,
                    cron_expression,
                    input_json,
                    max_attempts,
                    next_run_at
                )
                values ($1, $2, $3, $4, $5, $6, $7, $8)
                "#,
            )
            .bind(&schedule.schedule_key)
            .bind(&schedule.module_name)
            .bind(&schedule.schedule_name)
            .bind(&schedule.function_name)
            .bind(&schedule.cron)
            .bind(&schedule.input_json)
            .bind(schedule.max_attempts)
            .bind(next_run_at)
            .execute(&mut *tx)
            .await
            .map_err(map_runtime_error)?;
        } else {
            sqlx::query(
                r#"
                update runtime.scheduled_functions
                set module_name = $2,
                    schedule_name = $3,
                    function_name = $4,
                    cron_expression = $5,
                    input_json = $6,
                    max_attempts = $7,
                    next_run_at = case when $8 then $9 else next_run_at end,
                    updated_at = now()
                where schedule_key = $1
                "#,
            )
            .bind(&schedule.schedule_key)
            .bind(&schedule.module_name)
            .bind(&schedule.schedule_name)
            .bind(&schedule.function_name)
            .bind(&schedule.cron)
            .bind(&schedule.input_json)
            .bind(schedule.max_attempts)
            .bind(reset_next_run)
            .bind(next_run_at)
            .execute(&mut *tx)
            .await
            .map_err(map_runtime_error)?;
        }

        let due: Option<String> = sqlx::query_scalar(
            r#"
            select schedule_key
            from runtime.scheduled_functions
            where schedule_key = $1
                and next_run_at <= now()
            for update skip locked
            "#,
        )
        .bind(&schedule.schedule_key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_runtime_error)?;

        if due.is_none() {
            tx.commit().await.map_err(map_runtime_error)?;
            return Ok(None);
        }

        let run = client
            .enqueue_function_in_tx(
                &mut tx,
                EnqueueFunctionRequest {
                    function_name: schedule.function_name.clone(),
                    input_json: schedule.input_json.clone(),
                    correlation_id: CorrelationId::new(format!("corr_schedule_{}", Uuid::now_v7())),
                    actor: ActorContext::Service {
                        service_id: self.worker_id.clone(),
                        scopes: vec!["runtime.functions.enqueue".to_owned()],
                    },
                    tenant_id: None,
                    tenancy_mode: crate::FunctionTenancyMode::None,
                    trace: TraceContext::default(),
                    causation_id: Some(format!("runtime_schedule:{}", schedule.schedule_key)),
                    max_attempts: Some(schedule.max_attempts),
                },
            )
            .await?;

        sqlx::query(
            r#"
            update runtime.scheduled_functions
            set next_run_at = $2,
                last_enqueued_at = now(),
                updated_at = now()
            where schedule_key = $1
            "#,
        )
        .bind(&schedule.schedule_key)
        .bind(next_run_at)
        .execute(&mut *tx)
        .await
        .map_err(map_runtime_error)?;

        tx.commit().await.map_err(map_runtime_error)?;
        client.record_function_enqueued(&run).await;
        Ok(Some(run.id))
    }
}
