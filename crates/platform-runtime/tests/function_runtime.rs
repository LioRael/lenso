use lenso_contracts::CronSchedule;
use platform_core::{
    ActorContext, AppError, AppResult, CorrelationId, ErrorCode, ExecutionContext, ExecutionId,
    ExecutionLogProvider, ExecutionLogQuery, ExecutionLogRecord, ExecutionLogWriter,
    PLATFORM_MIGRATIONS, PostgresExecutionLogProvider, TenantId, TraceContext, apply_migrations,
};
use platform_runtime::{
    EnqueueFunctionRequest, FunctionDefinition, FunctionRegistry, FunctionTerminalObservation,
    RUNTIME_MIGRATIONS, RetryPolicy, RuntimeClient, RuntimeFunction, RuntimeScheduler,
    RuntimeWorker, ScheduledFunctionDefinition,
};
use platform_testing::TestDatabase;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[test]
fn can_register_function() {
    let mut registry = FunctionRegistry::default();
    registry.register(test_function("test.echo.v1", Arc::new(Succeeds)));

    assert!(registry.get("test.echo.v1").is_some());
    assert_eq!(registry.all().count(), 1);
}

#[test]
fn can_register_runtime_loaded_function_names() {
    let mut registry = FunctionRegistry::default();
    let function_name = format!("{}.{}", "remote_crm", "sync_contact.v1");

    registry.register(FunctionDefinition {
        name: function_name.clone(),
        version: 1,
        queue: "remote-crm".to_owned(),
        retry_policy: RetryPolicy::default(),
        handler: Arc::new(Succeeds),
    });

    let definition = registry
        .get("remote_crm.sync_contact.v1")
        .expect("function should register");
    assert_eq!(definition.name, function_name);
    assert_eq!(definition.queue, "remote-crm");
}

#[tokio::test]
async fn enqueue_creates_function_run_row() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_runtime_stack_migrations(&db).await;

    let client = RuntimeClient::new(db.pool.clone());
    let run_id = client
        .enqueue_function(EnqueueFunctionRequest {
            function_name: "test.echo.v1".to_owned(),
            input_json: json!({ "hello": "world" }),
            correlation_id: CorrelationId::new("corr_1"),
            actor: ActorContext::User {
                user_id: "usr_1".to_owned(),
                scopes: vec!["test:run".to_owned()],
            },
            tenant_id: Some(TenantId("tenant_01".to_owned())),
            tenancy_mode: platform_runtime::FunctionTenancyMode::Required,
            trace: trace_context(),
            causation_id: Some("evt_1".to_owned()),
            max_attempts: Some(5),
        })
        .await
        .expect("function should enqueue");

    let row = sqlx::query_as::<_, (String, Value, String, i32, String, Value)>(
        r#"
        select function_name, input_json, status, max_attempts, correlation_id, actor
        from runtime.function_runs
        where id = $1
        "#,
    )
    .bind(&run_id)
    .fetch_one(&db.pool)
    .await
    .expect("function run should exist");

    assert_eq!(row.0, "test.echo.v1");
    assert_eq!(row.1["hello"], "world");
    assert_eq!(row.1["_lenso_runtime"]["correlation_id"], "corr_1");
    assert_eq!(row.1["_lenso_runtime"]["causation_id"], "evt_1");
    assert_eq!(row.1["_lenso_runtime"]["trace"]["trace_id"], "trace_1");
    assert_eq!(row.1["_lenso_runtime"]["trace"]["span_id"], "span_1");
    assert_eq!(row.2, "pending");
    assert_eq!(row.3, 5);
    assert_eq!(row.4, "corr_1");
    assert_eq!(row.5["kind"], "user");

    db.cleanup().await;
}

#[tokio::test]
async fn caller_owned_function_run_identity_replays_only_the_same_request() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_runtime_stack_migrations(&db).await;

    let client = RuntimeClient::new(db.pool.clone());
    let request = EnqueueFunctionRequest {
        function_name: "test.echo.v1".to_owned(),
        input_json: json!({ "hello": "world" }),
        correlation_id: CorrelationId::new("corr_stable"),
        actor: ActorContext::System,
        tenant_id: None,
        tenancy_mode: platform_runtime::FunctionTenancyMode::None,
        trace: trace_context(),
        causation_id: Some("event_stable".to_owned()),
        max_attempts: Some(5),
    };

    let mut first = db.pool.begin().await.unwrap();
    client
        .enqueue_function_with_id_in_tx(&mut first, "fnrun_stable", request.clone())
        .await
        .expect("first request should enqueue");
    first.commit().await.unwrap();

    sqlx::query("update runtime.function_runs set status = 'completed' where id = $1")
        .bind("fnrun_stable")
        .execute(&db.pool)
        .await
        .unwrap();

    let mut replay = db.pool.begin().await.unwrap();
    client
        .enqueue_function_with_id_in_tx(&mut replay, "fnrun_stable", request.clone())
        .await
        .expect("the same durable request should replay safely");
    replay.commit().await.unwrap();

    let row = sqlx::query_as::<_, (i64, String)>(
        "select count(*), max(status) from runtime.function_runs where id = $1",
    )
    .bind("fnrun_stable")
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(row, (1, "completed".to_owned()));

    let mut drifted = request;
    drifted.input_json = json!({ "hello": "different" });
    let mut conflict = db.pool.begin().await.unwrap();
    let error = client
        .enqueue_function_with_id_in_tx(&mut conflict, "fnrun_stable", drifted)
        .await
        .expect_err("a stable run identity cannot be rebound");
    assert_eq!(error.code, ErrorCode::Conflict);
    conflict.rollback().await.unwrap();

    db.cleanup().await;
}

#[tokio::test]
async fn scheduler_enqueues_due_cron_function_once_per_match() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_runtime_stack_migrations(&db).await;

    let scheduler = RuntimeScheduler::new(db.pool.clone(), "worker-a");
    let schedule = ScheduledFunctionDefinition {
        schedule_key: "support-ticket:escalate-overdue".to_owned(),
        module_name: "support-ticket".to_owned(),
        schedule_name: "escalate-overdue".to_owned(),
        function_name: "support_ticket.escalate_overdue.v1".to_owned(),
        cron: "* * * * *".to_owned(),
        schedule: CronSchedule::parse("* * * * *").expect("cron"),
        input_json: json!({ "source": "schedule" }),
        max_attempts: 3,
    };

    let not_due_yet = scheduler
        .enqueue_due(std::slice::from_ref(&schedule))
        .await
        .expect("new schedule should initialize");
    assert!(not_due_yet.is_empty());

    sqlx::query(
        r#"
        update runtime.scheduled_functions
        set next_run_at = now() - interval '1 minute'
        where schedule_key = $1
        "#,
    )
    .bind(&schedule.schedule_key)
    .execute(&db.pool)
    .await
    .expect("schedule should become due");

    let first = scheduler
        .enqueue_due(std::slice::from_ref(&schedule))
        .await
        .expect("due schedule should enqueue");
    let second = scheduler
        .enqueue_due(std::slice::from_ref(&schedule))
        .await
        .expect("same cron match should not enqueue again");

    assert_eq!(first.len(), 1);
    assert!(second.is_empty());

    let count: i64 =
        sqlx::query_scalar("select count(*) from runtime.function_runs where function_name = $1")
            .bind(&schedule.function_name)
            .fetch_one(&db.pool)
            .await
            .expect("function run count should query");
    assert_eq!(count, 1);

    let next_run_delayed: bool = sqlx::query_scalar(
        r#"
        select next_run_at > now()
        from runtime.scheduled_functions
        where schedule_key = $1
        "#,
    )
    .bind(&schedule.schedule_key)
    .fetch_one(&db.pool)
    .await
    .expect("schedule state should query");
    assert!(next_run_delayed);

    db.cleanup().await;
}

#[tokio::test]
async fn worker_executes_function_and_marks_completed() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_runtime_stack_migrations(&db).await;

    let calls = Arc::new(AtomicUsize::new(0));
    let mut registry = FunctionRegistry::default();
    registry.register(test_function(
        "test.echo.v1",
        Arc::new(CountingFunction {
            calls: calls.clone(),
        }),
    ));

    enqueue(&db.pool, "test.echo.v1", 3).await;

    let worker = RuntimeWorker::new(db.pool.clone(), Arc::new(registry), "worker-a");
    let count = worker
        .claim_and_run_batch(10)
        .await
        .expect("runtime worker should run");

    assert_eq!(count, 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(run_status(&db.pool, "test.echo.v1").await, "completed");
    assert_eq!(
        execution_log_bodies(&db.pool, "test.echo.v1").await,
        vec![
            "Function run enqueued".to_owned(),
            "Function run claimed".to_owned(),
            "Function run started".to_owned(),
            "Function run completed".to_owned()
        ]
    );

    db.cleanup().await;
}

#[tokio::test]
async fn handler_structured_logs_use_the_runtime_bound_execution_scope() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_runtime_stack_migrations(&db).await;

    let mut registry = FunctionRegistry::default();
    registry.register(test_function(
        "test.structured-log.v1",
        Arc::new(EmitsStructuredExecutionLog),
    ));
    let run_id = enqueue(&db.pool, "test.structured-log.v1", 3).await;

    let worker = RuntimeWorker::new(db.pool.clone(), Arc::new(registry), "worker-a")
        .with_service_name("inventory-service");
    worker
        .claim_and_run_batch(10)
        .await
        .expect("runtime worker should run");

    let logs = PostgresExecutionLogProvider::new(db.pool.clone())
        .query_execution_logs(ExecutionLogQuery {
            execution_id: run_id.clone(),
            occurred_before: None,
            limit: 100,
        })
        .await
        .expect("execution logs should query");
    let log = logs
        .iter()
        .find(|log| log.body == "Inventory reservation checked")
        .expect("structured handler log should be persisted");

    assert_eq!(log.execution_id, run_id);
    assert_eq!(log.story_id, "corr_1");
    assert_eq!(log.correlation_id, "corr_1");
    assert_eq!(log.execution_type, "function_run");
    assert_eq!(log.execution_name, "test.structured-log.v1");
    assert_eq!(log.trace_id.as_deref(), Some("trace_1"));
    assert_eq!(log.span_id.as_deref(), Some("span_1"));
    assert_eq!(log.service_name, "inventory-service");
    assert_eq!(log.attributes["code"], "inventory.reservation.checked");
    assert_eq!(log.attributes["nested"]["password"], "[REDACTED]");
    assert!(log.attributes.get("lenso.execution.id").is_none());
    assert!(
        log.redacted_fields
            .iter()
            .any(|field| field == "nested.password")
    );

    db.cleanup().await;
}

#[tokio::test]
async fn structured_log_writer_failure_does_not_fail_the_function_run() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_runtime_stack_migrations(&db).await;

    let mut registry = FunctionRegistry::default();
    registry.register(test_function(
        "test.structured-log-failure.v1",
        Arc::new(EmitsStructuredExecutionLog),
    ));
    enqueue(&db.pool, "test.structured-log-failure.v1", 3).await;

    let worker = RuntimeWorker::new(db.pool.clone(), Arc::new(registry), "worker-a")
        .with_execution_log_writer(Arc::new(AlwaysFailsExecutionLogWriter));
    worker
        .claim_and_run_batch(10)
        .await
        .expect("execution-log failure must not fail the runtime worker");

    assert_eq!(
        run_status(&db.pool, "test.structured-log-failure.v1").await,
        "completed"
    );

    db.cleanup().await;
}

#[tokio::test]
async fn stalled_structured_log_writer_does_not_inflate_handler_duration() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_runtime_stack_migrations(&db).await;

    let mut registry = FunctionRegistry::default();
    registry.register(test_function(
        "test.structured-log-duration.v1",
        Arc::new(EmitsStructuredExecutionLog),
    ));
    let run_id = enqueue(&db.pool, "test.structured-log-duration.v1", 3).await;

    let worker = RuntimeWorker::new(db.pool.clone(), Arc::new(registry), "worker-a")
        .with_execution_log_writer(Arc::new(StalledExecutionLogWriter));
    let wall_started = std::time::Instant::now();
    worker
        .claim_and_run_batch(10)
        .await
        .expect("runtime worker should finish despite a stalled execution-log writer");
    assert!(wall_started.elapsed() >= Duration::from_millis(90));

    let handler_duration_ms: i64 = sqlx::query_scalar(
        r#"
        select (attributes ->> 'duration_ms')::bigint
        from platform.execution_logs
        where execution_id = $1
          and body = 'Function handler operation completed'
        "#,
    )
    .bind(&run_id)
    .fetch_one(&db.pool)
    .await
    .expect("handler operation duration should be recorded");
    assert!(
        handler_duration_ms < 50,
        "handler duration must exclude the execution-log drain delay: {handler_duration_ms}ms"
    );

    db.cleanup().await;
}

#[tokio::test]
async fn failure_retries_function_run() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_runtime_stack_migrations(&db).await;

    let mut registry = FunctionRegistry::default();
    registry.register(test_function(
        "test.fail.v1",
        Arc::new(AlwaysRetryableFailure),
    ));
    enqueue(&db.pool, "test.fail.v1", 3).await;

    let worker = RuntimeWorker::new(db.pool.clone(), Arc::new(registry), "worker-a");
    worker
        .claim_and_run_batch(10)
        .await
        .expect("runtime worker should handle function failure");

    let (status, attempts) = run_status_and_attempts(&db.pool, "test.fail.v1").await;
    assert_eq!(status, "failed");
    assert_eq!(attempts, 1);
    assert!(
        execution_log_bodies(&db.pool, "test.fail.v1")
            .await
            .contains(&"Function run failed".to_owned())
    );

    db.cleanup().await;
}

#[tokio::test]
async fn retryable_failure_uses_function_retry_delay() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_runtime_stack_migrations(&db).await;

    let mut registry = FunctionRegistry::default();
    registry.register(test_function_with_retry_policy(
        "test.delay.v1",
        RetryPolicy::fixed(3, Duration::from_secs(60)),
        Arc::new(AlwaysRetryableFailure),
    ));
    enqueue(&db.pool, "test.delay.v1", 3).await;

    let worker = RuntimeWorker::new(db.pool.clone(), Arc::new(registry), "worker-a");
    worker
        .claim_and_run_batch(10)
        .await
        .expect("runtime worker should handle function failure");

    let retry_is_delayed: bool = sqlx::query_scalar(
        r#"
        select available_at > now() + interval '50 seconds'
        from runtime.function_runs
        where function_name = 'test.delay.v1'
        "#,
    )
    .fetch_one(&db.pool)
    .await
    .expect("available_at should query");

    assert!(retry_is_delayed);

    db.cleanup().await;
}

#[tokio::test]
async fn provider_retry_after_overrides_the_function_default_delay() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_runtime_stack_migrations(&db).await;

    let mut registry = FunctionRegistry::default();
    registry.register(test_function_with_retry_policy(
        "test.provider-delay.v1",
        RetryPolicy::fixed(3, Duration::ZERO),
        Arc::new(ProviderRetryAfterFailure),
    ));
    enqueue(&db.pool, "test.provider-delay.v1", 3).await;

    let worker = RuntimeWorker::new(db.pool.clone(), Arc::new(registry), "worker-a");
    worker
        .claim_and_run_batch(10)
        .await
        .expect("runtime worker should preserve Provider retry-after");

    let retry_is_delayed: bool = sqlx::query_scalar(
        r#"
        select available_at > now() + interval '50 seconds'
        from runtime.function_runs
        where function_name = 'test.provider-delay.v1'
        "#,
    )
    .fetch_one(&db.pool)
    .await
    .expect("available_at should query");

    assert!(retry_is_delayed);
    db.cleanup().await;
}

#[tokio::test]
async fn stale_processing_function_run_can_be_reclaimed() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_runtime_stack_migrations(&db).await;

    enqueue(&db.pool, "test.stale.v1", 3).await;
    sqlx::query(
        r#"
        update runtime.function_runs
        set status = 'processing',
            locked_at = now() - interval '10 minutes',
            locked_by = 'worker-dead'
        where function_name = 'test.stale.v1'
        "#,
    )
    .execute(&db.pool)
    .await
    .expect("function run should become stale");

    let worker = RuntimeWorker::new(
        db.pool.clone(),
        Arc::new(FunctionRegistry::default()),
        "worker-b",
    );
    let claimed = worker
        .claim_batch(10)
        .await
        .expect("stale function run should claim");

    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].function_name, "test.stale.v1");

    db.cleanup().await;
}

#[tokio::test]
async fn exhausted_attempts_marks_function_run_dead() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_runtime_stack_migrations(&db).await;

    let mut registry = FunctionRegistry::default();
    registry.register(test_function(
        "test.dead.v1",
        Arc::new(AlwaysRetryableFailure),
    ));
    enqueue(&db.pool, "test.dead.v1", 1).await;

    let worker = RuntimeWorker::new(db.pool.clone(), Arc::new(registry), "worker-a");
    worker
        .claim_and_run_batch(10)
        .await
        .expect("runtime worker should handle exhausted function failure");

    let (status, attempts) = run_status_and_attempts(&db.pool, "test.dead.v1").await;
    assert_eq!(status, "dead");
    assert_eq!(attempts, 1);

    db.cleanup().await;
}

#[tokio::test]
async fn opted_in_dead_run_publishes_one_bounded_terminal_observation() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_runtime_stack_migrations(&db).await;

    let mut registry = FunctionRegistry::default();
    registry.register(test_function(
        "test.provider-dead.v1",
        Arc::new(ObservedProviderFailure),
    ));
    let run_id = enqueue(&db.pool, "test.provider-dead.v1", 1).await;

    let worker = RuntimeWorker::new(db.pool.clone(), Arc::new(registry), "worker-a");
    worker
        .claim_and_run_batch(10)
        .await
        .expect("runtime worker should publish the terminal observation");

    let rows = sqlx::query_as::<_, (String, String, String, Value)>(
        r#"
        select id, event_name, correlation_id, payload
        from platform.outbox
        where event_name = 'lenso.runtime-function-terminal.v1'
        "#,
    )
    .fetch_all(&db.pool)
    .await
    .expect("terminal observation should query");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, format!("runtime:function-run:{run_id}:terminal"));
    assert_eq!(rows[0].1, "lenso.runtime-function-terminal.v1");
    assert_eq!(rows[0].2, "corr_1");
    assert_eq!(rows[0].3["functionRunId"], run_id);
    assert_eq!(rows[0].3["functionName"], "test.provider-dead.v1");
    assert_eq!(rows[0].3["ownerModule"], "test/provider");
    assert_eq!(rows[0].3["failureClassification"], "retry_exhausted");
    assert!(rows[0].3.get("input").is_none());
    assert!(rows[0].3.get("message").is_none());
    assert!(rows[0].3.get("providerResponse").is_none());

    db.cleanup().await;
}

#[derive(Debug)]
struct Succeeds;

#[async_trait::async_trait]
impl RuntimeFunction for Succeeds {
    async fn call(&self, _ctx: ExecutionContext, _input: Value) -> AppResult<Value> {
        Ok(json!({ "ok": true }))
    }
}

#[derive(Debug)]
struct CountingFunction {
    calls: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl RuntimeFunction for CountingFunction {
    async fn call(&self, ctx: ExecutionContext, input: Value) -> AppResult<Value> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(ctx.function_name, "test.echo.v1");
        assert_eq!(ctx.correlation_id.0, "corr_1");
        assert_eq!(ctx.trace.trace_id.as_deref(), Some("trace_1"));
        assert_eq!(ctx.trace.span_id.as_deref(), Some("span_1"));
        assert_eq!(ctx.causation_id.as_deref(), Some("evt_1"));
        assert_eq!(
            ctx.tenant_id.as_ref().map(|tenant| tenant.0.as_str()),
            Some("tenant_01")
        );
        assert_eq!(input["hello"], "world");
        Ok(json!({ "ok": true }))
    }
}

#[derive(Debug)]
struct AlwaysRetryableFailure;

#[async_trait::async_trait]
impl RuntimeFunction for AlwaysRetryableFailure {
    async fn call(&self, _ctx: ExecutionContext, _input: Value) -> AppResult<Value> {
        Err(AppError::new(ErrorCode::ExternalDependency, "temporary failure").retryable())
    }
}

#[derive(Debug)]
struct EmitsStructuredExecutionLog;

#[async_trait::async_trait]
impl RuntimeFunction for EmitsStructuredExecutionLog {
    async fn call(&self, mut ctx: ExecutionContext, _input: Value) -> AppResult<Value> {
        ctx.execution_id = ExecutionId("fnrun_forged".to_owned());
        let sensitive_fixture = ["redaction", "-fixture"].concat();
        tracing::info!(
            target: "lenso::execution",
            {
                code = "inventory.reservation.checked",
                attributes = %json!({
                    "nested": {
                        "password": sensitive_fixture
                    }
                }),
                lenso.execution.id = "fnrun_forged",
            },
            "Inventory reservation checked"
        );
        Ok(json!({ "ok": true }))
    }

    fn observability(&self) -> Option<platform_runtime::FunctionHandlerObservability> {
        Some(platform_runtime::FunctionHandlerObservability::new(
            "handler",
            json!({ "operation": "inventory.reserve" }),
        ))
    }
}

#[derive(Debug)]
struct ProviderRetryAfterFailure;

#[async_trait::async_trait]
impl RuntimeFunction for ProviderRetryAfterFailure {
    async fn call(&self, _ctx: ExecutionContext, _input: Value) -> AppResult<Value> {
        Err(
            AppError::new(ErrorCode::ExternalDependency, "Provider throttled")
                .retryable()
                .with_retry_after_ms(Some(60_000)),
        )
    }
}

#[derive(Debug)]
struct AlwaysFailsExecutionLogWriter;

#[async_trait::async_trait]
impl ExecutionLogWriter for AlwaysFailsExecutionLogWriter {
    async fn write_execution_log(&self, _record: ExecutionLogRecord) -> AppResult<String> {
        Err(AppError::new(
            ErrorCode::ExternalDependency,
            "execution log store unavailable",
        ))
    }
}

#[derive(Debug)]
struct StalledExecutionLogWriter;

#[async_trait::async_trait]
impl ExecutionLogWriter for StalledExecutionLogWriter {
    async fn write_execution_log(&self, _record: ExecutionLogRecord) -> AppResult<String> {
        tokio::time::sleep(Duration::from_secs(1)).await;
        Ok("elog_stalled".to_owned())
    }
}

#[derive(Debug)]
struct ObservedProviderFailure;

#[async_trait::async_trait]
impl RuntimeFunction for ObservedProviderFailure {
    async fn call(&self, _ctx: ExecutionContext, _input: Value) -> AppResult<Value> {
        Err(AppError::new(ErrorCode::ExternalDependency, "sensitive provider response").retryable())
    }

    fn terminal_observation(&self) -> Option<FunctionTerminalObservation> {
        Some(FunctionTerminalObservation::new("test/provider"))
    }
}

fn test_function(name: &str, handler: Arc<dyn RuntimeFunction>) -> FunctionDefinition {
    test_function_with_retry_policy(name, RetryPolicy::fixed(3, Duration::ZERO), handler)
}

fn test_function_with_retry_policy(
    name: &str,
    retry_policy: RetryPolicy,
    handler: Arc<dyn RuntimeFunction>,
) -> FunctionDefinition {
    FunctionDefinition {
        name: name.to_owned(),
        version: 1,
        queue: "default".to_owned(),
        retry_policy,
        handler,
    }
}

async fn enqueue(pool: &platform_core::DbPool, function_name: &str, max_attempts: i32) -> String {
    RuntimeClient::new(pool.clone())
        .enqueue_function(EnqueueFunctionRequest {
            function_name: function_name.to_owned(),
            input_json: json!({ "hello": "world" }),
            correlation_id: CorrelationId::new("corr_1"),
            actor: ActorContext::System,
            tenant_id: Some(TenantId("tenant_01".to_owned())),
            tenancy_mode: platform_runtime::FunctionTenancyMode::Required,
            trace: trace_context(),
            causation_id: Some("evt_1".to_owned()),
            max_attempts: Some(max_attempts),
        })
        .await
        .expect("function should enqueue")
}

fn trace_context() -> TraceContext {
    TraceContext {
        trace_id: Some("trace_1".to_owned()),
        span_id: Some("span_1".to_owned()),
        baggage: Vec::new(),
    }
}

async fn apply_runtime_stack_migrations(db: &TestDatabase) {
    let migrations = PLATFORM_MIGRATIONS
        .iter()
        .chain(RUNTIME_MIGRATIONS)
        .copied()
        .collect::<Vec<_>>();
    apply_migrations(&db.pool, &migrations)
        .await
        .expect("runtime migrations should apply");
}

async fn run_status(pool: &platform_core::DbPool, function_name: &str) -> String {
    sqlx::query_scalar("select status from runtime.function_runs where function_name = $1")
        .bind(function_name)
        .fetch_one(pool)
        .await
        .expect("status should query")
}

async fn run_status_and_attempts(
    pool: &platform_core::DbPool,
    function_name: &str,
) -> (String, i32) {
    sqlx::query_as("select status, attempts from runtime.function_runs where function_name = $1")
        .bind(function_name)
        .fetch_one(pool)
        .await
        .expect("status and attempts should query")
}

async fn execution_log_bodies(pool: &platform_core::DbPool, function_name: &str) -> Vec<String> {
    sqlx::query_scalar(
        r#"
        select log.body
        from platform.execution_logs log
        join runtime.function_runs run on run.id = log.execution_id
        where run.function_name = $1
        order by log.occurred_at asc, log.id asc
        "#,
    )
    .bind(function_name)
    .fetch_all(pool)
    .await
    .expect("execution log bodies should query")
}
