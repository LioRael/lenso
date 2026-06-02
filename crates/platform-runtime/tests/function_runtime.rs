use platform_core::{
    ActorContext, AppError, AppResult, CorrelationId, ErrorCode, ExecutionContext,
    PLATFORM_MIGRATIONS, TraceContext, apply_migrations,
};
use platform_runtime::{
    EnqueueFunctionRequest, FunctionDefinition, FunctionRegistry, RUNTIME_MIGRATIONS, RetryPolicy,
    RuntimeClient, RuntimeFunction, RuntimeWorker,
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

    let worker = RuntimeWorker::new(db.pool.clone(), Arc::new(registry), "worker-a", 10);
    let count = worker
        .claim_and_run_batch()
        .await
        .expect("runtime worker should run");

    assert_eq!(count, 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(run_status(&db.pool, "test.echo.v1").await, "completed");

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

    let worker = RuntimeWorker::new(db.pool.clone(), Arc::new(registry), "worker-a", 10);
    worker
        .claim_and_run_batch()
        .await
        .expect("runtime worker should handle function failure");

    let (status, attempts) = run_status_and_attempts(&db.pool, "test.fail.v1").await;
    assert_eq!(status, "failed");
    assert_eq!(attempts, 1);

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

    let worker = RuntimeWorker::new(db.pool.clone(), Arc::new(registry), "worker-a", 10);
    worker
        .claim_and_run_batch()
        .await
        .expect("runtime worker should handle exhausted function failure");

    let (status, attempts) = run_status_and_attempts(&db.pool, "test.dead.v1").await;
    assert_eq!(status, "dead");
    assert_eq!(attempts, 1);

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

fn test_function(name: &'static str, handler: Arc<dyn RuntimeFunction>) -> FunctionDefinition {
    FunctionDefinition {
        name,
        version: 1,
        queue: "default",
        retry_policy: RetryPolicy::fixed(3, Duration::ZERO),
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
