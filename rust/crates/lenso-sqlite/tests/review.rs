//! Typed publication, native transactions and execution history regression checks.

use std::{
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
    time::Duration,
};

use lenso_app::{ActionError, DurableEvent, ErrorCode, Task, TaskContext};
use lenso_sqlite::{AttemptOutcome, EnqueueOptions, JobState, SqliteQueue, rusqlite::Connection};

struct Mail;
impl Task<()> for Mail {
    type Input = u32;
    type Output = u32;
    fn id(&self) -> &str {
        "typed.mail"
    }
    async fn run(&self, context: &TaskContext<'_, ()>, input: u32) -> Result<u32, ActionError> {
        if context.attempt == 1 {
            return Err(ActionError::new(
                ErrorCode::Internal,
                "Transient mail failure",
            ));
        }
        Ok(input)
    }
}

struct Registered {
    number: u32,
}

struct Cleanup;
impl Task<()> for Cleanup {
    type Input = ();
    type Output = ();
    fn id(&self) -> &str {
        "cleanup"
    }
    async fn run(&self, _: &TaskContext<'_, ()>, (): ()) -> Result<(), ActionError> {
        Ok(())
    }
}

#[tokio::test]
async fn no_argument_tasks_use_json_unit_for_payload_and_result() {
    let queue = SqliteQueue::new(Connection::open_in_memory().unwrap()).unwrap();
    let receipt = queue
        .enqueue(&Cleanup, &(), &EnqueueOptions::default())
        .unwrap();
    assert_eq!(receipt.payload, "null");
    let result = queue
        .run_one(&Cleanup, &(), Duration::from_secs(30), 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.state, JobState::Succeeded);
    assert_eq!(result.result.as_deref(), Some("null"));
}

#[tokio::test]
async fn typed_subscriptions_join_domain_transactions_and_preserve_failed_attempts() {
    let now = Arc::new(AtomicI64::new(1000));
    let clock = now.clone();
    let queue = SqliteQueue::with_clock(Connection::open_in_memory().unwrap(), move || {
        clock.load(Ordering::SeqCst)
    })
    .unwrap();
    let mut event = DurableEvent::new("typed.registered");
    event
        .subscribe("welcome", &Mail, |user: &Registered| user.number)
        .unwrap();
    let input = Registered { number: 7 };
    let options = EnqueueOptions {
        retry: Duration::from_millis(10),
        ..EnqueueOptions::default()
    };
    queue
        .with_connection(|db| {
            db.execute("CREATE TABLE users (id INTEGER)", []).unwrap();
            let transaction = db.transaction().unwrap();
            transaction
                .execute("INSERT INTO users VALUES (1)", [])
                .unwrap();
            SqliteQueue::publish_on(&transaction, &event, "user-1", &input, &options, 1000)?;
            transaction.rollback().unwrap();
            assert_eq!(
                db.query_row("SELECT count(*) FROM lenso_jobs", [], |row| row
                    .get::<_, u32>(0))
                    .unwrap(),
                0
            );
            Ok(())
        })
        .unwrap();
    let jobs = queue.publish(&event, "user-1", &input, &options).unwrap();
    let id = &jobs[0].id;
    assert_eq!(
        queue
            .run_one(&Mail, &(), Duration::from_secs(30), 1)
            .await
            .unwrap()
            .unwrap()
            .state,
        JobState::Queued
    );
    now.store(1010, Ordering::SeqCst);
    assert_eq!(
        queue
            .run_one(&Mail, &(), Duration::from_secs(30), 1)
            .await
            .unwrap()
            .unwrap()
            .state,
        JobState::Succeeded
    );
    let history = queue.attempts(id).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].outcome, AttemptOutcome::Failed);
    assert_eq!(history[0].started_ms, 1000);
    assert_eq!(history[0].finished_ms, Some(1000));
    assert_eq!(history[0].error.as_deref(), Some("Transient mail failure"));
    assert_eq!(history[1].outcome, AttemptOutcome::Succeeded);
    assert_eq!(history[1].started_ms, 1010);
    assert_eq!(history[1].finished_ms, Some(1010));
    assert!(history[1].error.is_none());
    assert_eq!(
        event
            .subscribe("welcome", &Mail, |_: &Registered| 8)
            .unwrap_err()
            .code(),
        ErrorCode::Conflict
    );
}

#[tokio::test]
async fn scheduling_overflow_is_rejected_before_claiming_work() {
    let queue = SqliteQueue::with_clock(Connection::open_in_memory().unwrap(), || {
        9_007_199_254_740_991
    })
    .unwrap();
    let options = EnqueueOptions {
        id: Some("overflow".into()),
        retry: Duration::ZERO,
        ..EnqueueOptions::default()
    };
    assert_eq!(
        queue
            .enqueue(
                &Mail,
                &1,
                &EnqueueOptions {
                    delay: Duration::from_micros(500),
                    ..options.clone()
                }
            )
            .unwrap_err()
            .code(),
        ErrorCode::InvalidInput
    );
    assert_eq!(
        queue
            .enqueue(
                &Mail,
                &1,
                &EnqueueOptions {
                    delay: Duration::from_millis(1),
                    ..options.clone()
                }
            )
            .unwrap_err()
            .code(),
        ErrorCode::InvalidInput
    );
    queue.enqueue(&Mail, &1, &options).unwrap();
    assert_eq!(
        queue
            .run_one(&Mail, &(), Duration::from_millis(3), 1)
            .await
            .unwrap_err()
            .code(),
        ErrorCode::InvalidInput
    );
    assert_eq!(queue.get("overflow").unwrap().attempts, 0);
}

#[tokio::test]
async fn a_large_valid_lease_does_not_overflow_the_runtime_timer() {
    let queue = SqliteQueue::with_clock(Connection::open_in_memory().unwrap(), || 1000).unwrap();
    let options = EnqueueOptions {
        max_attempts: 1,
        retry: Duration::ZERO,
        ..EnqueueOptions::default()
    };
    queue.enqueue(&Mail, &1, &options).unwrap();
    let lease = Duration::from_millis(9_007_199_254_740_991 - 1000);
    assert_eq!(
        queue
            .run_one(&Mail, &(), lease, 1)
            .await
            .unwrap()
            .unwrap()
            .state,
        JobState::Failed
    );
}

#[test]
fn typed_submission_joins_a_native_transaction_without_manual_serialization() {
    let mut db = Connection::open_in_memory().unwrap();
    SqliteQueue::initialize(&db).unwrap();
    let tx = db.transaction().unwrap();
    let options = EnqueueOptions {
        id: Some("native-unit".into()),
        ..EnqueueOptions::default()
    };
    let job = SqliteQueue::enqueue_on(&tx, &Cleanup, &(), &options, 1000).unwrap();
    assert_eq!(job.task, "cleanup");
    assert_eq!(job.payload, "null");
    assert_eq!(
        SqliteQueue::enqueue_on(&tx, &Cleanup, &(), &options, 1000)
            .unwrap()
            .id,
        job.id
    );
    tx.rollback().unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM lenso_jobs", [], |row| row
            .get::<_, u32>(0))
            .unwrap(),
        0
    );
}

#[test]
fn one_publication_uses_one_clock_snapshot_for_every_subscriber() {
    let now = AtomicI64::new(1000);
    let queue = SqliteQueue::with_clock(Connection::open_in_memory().unwrap(), move || {
        now.fetch_add(1, Ordering::SeqCst)
    })
    .unwrap();
    let mut event = DurableEvent::new("clock");
    event.subscribe("first", &Cleanup, |_: &()| ()).unwrap();
    event.subscribe("second", &Cleanup, |_: &()| ()).unwrap();
    let jobs = queue
        .publish(&event, "one", &(), &EnqueueOptions::default())
        .unwrap();
    assert_eq!(
        jobs.iter().map(|job| job.available_ms).collect::<Vec<_>>(),
        [1000, 1000]
    );
}

struct LeaseCleanup {
    started: Arc<tokio::sync::Notify>,
    cancelled: Arc<tokio::sync::Notify>,
    finish: Arc<tokio::sync::Notify>,
}
impl Task<()> for LeaseCleanup {
    type Input = ();
    type Output = ();
    fn id(&self) -> &str {
        "lease.cleanup"
    }
    async fn run(&self, context: &TaskContext<'_, ()>, (): ()) -> Result<(), ActionError> {
        self.started.notify_one();
        context.cancellation.cancelled().await;
        self.cancelled.notify_one();
        self.finish.notified().await;
        Err(ActionError::new(ErrorCode::Cancelled, "Cleanup complete"))
    }
}
#[tokio::test]
async fn lease_loss_signals_cancellation_and_awaits_handler_cleanup() {
    let queue = Arc::new(SqliteQueue::new(Connection::open_in_memory().unwrap()).unwrap());
    let started = Arc::new(tokio::sync::Notify::new());
    let cancelled = Arc::new(tokio::sync::Notify::new());
    let finish = Arc::new(tokio::sync::Notify::new());
    let task = LeaseCleanup {
        started: started.clone(),
        cancelled: cancelled.clone(),
        finish: finish.clone(),
    };
    let job = queue
        .enqueue(&task, &(), &EnqueueOptions::default())
        .unwrap();
    let worker = queue.clone();
    let running = tokio::spawn(async move {
        worker
            .run_one(&task, &(), Duration::from_millis(300), 1)
            .await
    });
    started.notified().await;
    queue
        .with_connection(|db| {
            db.execute("UPDATE lenso_jobs SET lease_token='replacement'", [])
                .unwrap();
            Ok(())
        })
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), cancelled.notified())
        .await
        .expect("handler observes cancellation before being dropped");
    assert!(!running.is_finished(), "worker still owns cleanup");
    finish.notify_one();
    assert_eq!(
        running.await.unwrap().unwrap_err().code(),
        ErrorCode::Conflict
    );
    assert_eq!(
        queue.attempts(&job.id).unwrap()[0].outcome,
        AttemptOutcome::Running
    );
}
