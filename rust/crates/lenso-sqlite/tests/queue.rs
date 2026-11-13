//! Durable queue conformance, native transactions and lease ownership.

use std::{
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
    time::Duration,
};

use lenso_app::{ActionError, ErrorCode, Task, TaskContext};
use lenso_sqlite::{Delivery, EnqueueOptions, JobState, SqliteQueue, rusqlite::Connection};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::Notify;

struct Identity {
    failures: u32,
}

struct FixtureTask<'a> {
    queue: &'a SqliteQueue,
    failures: u32,
    cancel_during: bool,
}
impl Task<()> for FixtureTask<'_> {
    type Input = Value;
    type Output = Value;
    fn id(&self) -> &str {
        "test.identity"
    }
    async fn run(&self, context: &TaskContext<'_, ()>, input: Value) -> Result<Value, ActionError> {
        if self.cancel_during {
            self.queue.cancel(context.job_id)?;
        }
        Identity {
            failures: self.failures,
        }
        .run(context, input)
        .await
    }
}
impl Task<()> for Identity {
    type Input = Value;
    type Output = Value;
    fn id(&self) -> &str {
        "test.identity"
    }
    async fn run(&self, context: &TaskContext<'_, ()>, input: Value) -> Result<Value, ActionError> {
        if context.attempt <= self.failures {
            Err(ActionError::new(ErrorCode::Internal, "Temporary failure"))
        } else {
            Ok(input)
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    name: String,
    max_attempts: u32,
    delay_ms: u64,
    retry_ms: u64,
    failures: u32,
    cancel: bool,
    #[serde(default)]
    cancel_during: bool,
    steps: Vec<Step>,
}
#[derive(Deserialize)]
struct Step {
    now: i64,
    state: String,
    attempts: u32,
}
const LEASE: Duration = Duration::from_secs(3);

#[test]
fn receipt_reuse_preserves_policy_and_rejects_payload_conflicts() {
    let queue = SqliteQueue::with_clock(Connection::open_in_memory().unwrap(), || 1000).unwrap();
    let task = Identity { failures: 0 };
    let first = queue
        .enqueue(
            &task,
            &json!(1),
            &EnqueueOptions {
                id: Some("same".into()),
                max_attempts: 2,
                delay: Duration::from_millis(100),
                ..EnqueueOptions::default()
            },
        )
        .unwrap();
    let repeated = queue
        .enqueue(
            &task,
            &json!(1),
            &EnqueueOptions {
                id: Some("same".into()),
                max_attempts: 9,
                ..EnqueueOptions::default()
            },
        )
        .unwrap();
    assert_eq!(repeated, first);
    assert_eq!(
        queue
            .enqueue(
                &task,
                &json!(2),
                &EnqueueOptions {
                    id: Some("same".into()),
                    ..EnqueueOptions::default()
                }
            )
            .unwrap_err()
            .code(),
        ErrorCode::Conflict
    );
    assert_eq!(
        queue
            .enqueue(
                &task,
                &json!(1),
                &EnqueueOptions {
                    max_attempts: 0,
                    ..EnqueueOptions::default()
                }
            )
            .unwrap_err()
            .code(),
        ErrorCode::InvalidInput
    );
}

#[tokio::test]
async fn shared_durable_task_semantics() {
    let fixtures: Vec<Fixture> =
        serde_json::from_str(include_str!("../../../../conformance/fixtures/task.json")).unwrap();
    for fixture in fixtures {
        let now = Arc::new(AtomicI64::new(1000));
        let clock = now.clone();
        let queue = SqliteQueue::with_clock(Connection::open_in_memory().unwrap(), move || {
            clock.load(Ordering::SeqCst)
        })
        .unwrap();
        let task = FixtureTask {
            queue: &queue,
            failures: fixture.failures,
            cancel_during: fixture.cancel_during,
        };
        queue
            .enqueue(
                &task,
                &json!({"message": "hello"}),
                &EnqueueOptions {
                    id: Some("receipt".into()),
                    delay: Duration::from_millis(fixture.delay_ms),
                    max_attempts: fixture.max_attempts,
                    retry: Duration::from_millis(fixture.retry_ms),
                },
            )
            .unwrap();
        if fixture.cancel {
            assert!(queue.cancel("receipt").unwrap());
        }
        for step in fixture.steps {
            now.store(step.now, Ordering::SeqCst);
            let job = queue.run_one(&task, &(), LEASE, 1).await.unwrap();
            let state = job
                .as_ref()
                .map(|job| {
                    serde_json::to_value(job.state)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned()
                })
                .unwrap_or_else(|| "idle".into());
            assert_eq!(state, step.state, "{}", fixture.name);
            assert_eq!(
                queue.get("receipt").unwrap().attempts,
                step.attempts,
                "{}",
                fixture.name
            );
            if let Some(job) = job.filter(|job| job.state == JobState::Succeeded) {
                assert_eq!(
                    serde_json::from_str::<Value>(&job.result.unwrap()).unwrap(),
                    json!({"message":"hello"})
                );
            }
        }
    }
}

#[tokio::test]
async fn expired_and_exhausted_leases_recover_after_reopening() {
    let directory = tempfile::tempdir().unwrap();
    let filename = directory.path().join("jobs.sqlite");
    let queue = SqliteQueue::with_clock(Connection::open(&filename).unwrap(), || 1000).unwrap();
    let task = Identity { failures: 0 };
    queue
        .enqueue(
            &task,
            &json!(7),
            &EnqueueOptions {
                id: Some("recover".into()),
                max_attempts: 2,
                ..EnqueueOptions::default()
            },
        )
        .unwrap();
    queue
        .enqueue(
            &task,
            &json!(8),
            &EnqueueOptions {
                id: Some("exhausted".into()),
                max_attempts: 1,
                ..EnqueueOptions::default()
            },
        )
        .unwrap();
    queue.with_connection(|db| {
        db.execute("UPDATE lenso_jobs SET state = 'running', attempts = 1, lease_token = 'dead-worker', lease_until = 999", []).unwrap();
        Ok(())
    }).unwrap();
    drop(queue);
    let queue = SqliteQueue::with_clock(Connection::open(filename).unwrap(), || 1000).unwrap();
    assert_eq!(
        queue
            .run_one(&task, &(), LEASE, 1)
            .await
            .unwrap()
            .unwrap()
            .state,
        JobState::Succeeded
    );
    assert_eq!(queue.get("recover").unwrap().attempts, 2);
    assert_eq!(queue.get("exhausted").unwrap().state, JobState::Failed);
    queue
        .with_connection(|db| {
            assert_eq!(
                db.execute(
                    "UPDATE lenso_jobs SET result = 'stale' WHERE lease_token = 'dead-worker'",
                    []
                )
                .unwrap(),
                0
            );
            Ok(())
        })
        .unwrap();
}

struct Waiting {
    started: Arc<Notify>,
    release: Arc<Notify>,
    cancel: bool,
}
impl Task<()> for Waiting {
    type Input = Value;
    type Output = Value;
    fn id(&self) -> &str {
        "test.identity"
    }
    async fn run(&self, context: &TaskContext<'_, ()>, _: Value) -> Result<Value, ActionError> {
        self.started.notify_one();
        if self.cancel {
            context.cancellation.cancelled().await;
            return Err(ActionError::new(ErrorCode::Cancelled, "Task cancelled"));
        }
        self.release.notified().await;
        Ok(json!(42))
    }
}

#[tokio::test]
async fn two_connections_respect_concurrency_and_heartbeat() {
    let directory = tempfile::tempdir().unwrap();
    let filename = directory.path().join("jobs.sqlite");
    let a = Arc::new(SqliteQueue::open(&filename).unwrap());
    let b = SqliteQueue::open(&filename).unwrap();
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let task = Waiting {
        started: started.clone(),
        release: release.clone(),
        cancel: false,
    };
    a.enqueue(
        &task,
        &json!(1),
        &EnqueueOptions {
            id: Some("one".into()),
            ..EnqueueOptions::default()
        },
    )
    .unwrap();
    a.enqueue(
        &task,
        &json!(2),
        &EnqueueOptions {
            id: Some("two".into()),
            ..EnqueueOptions::default()
        },
    )
    .unwrap();
    let worker = a.clone();
    let running = tokio::spawn(async move {
        worker
            .run_one(&task, &(), Duration::from_millis(90), 1)
            .await
    });
    started.notified().await;
    let identity = Identity { failures: 0 };
    assert!(b.run_one(&identity, &(), LEASE, 1).await.unwrap().is_none());
    tokio::time::sleep(Duration::from_millis(140)).await;
    assert!(
        b.run_one(&identity, &(), LEASE, 1).await.unwrap().is_none(),
        "heartbeat retains lease beyond initial deadline"
    );
    release.notify_one();
    assert_eq!(
        running.await.unwrap().unwrap().unwrap().state,
        JobState::Succeeded
    );
    assert_eq!(
        b.run_one(&identity, &(), LEASE, 1)
            .await
            .unwrap()
            .unwrap()
            .state,
        JobState::Succeeded
    );
}

#[tokio::test]
async fn running_cancellation_is_cooperative_and_persisted() {
    let queue = Arc::new(SqliteQueue::new(Connection::open_in_memory().unwrap()).unwrap());
    let started = Arc::new(Notify::new());
    let task = Waiting {
        started: started.clone(),
        release: Arc::new(Notify::new()),
        cancel: true,
    };
    queue
        .enqueue(
            &task,
            &json!(1),
            &EnqueueOptions {
                id: Some("cancel-running".into()),
                ..EnqueueOptions::default()
            },
        )
        .unwrap();
    let worker = queue.clone();
    let running = tokio::spawn(async move {
        worker
            .run_one(&task, &(), Duration::from_millis(30), 1)
            .await
    });
    started.notified().await;
    assert!(queue.cancel("cancel-running").unwrap());
    let job = tokio::time::timeout(Duration::from_secs(2), running)
        .await
        .unwrap()
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(job.state, JobState::Cancelled);
    assert!(!queue.cancel("cancel-running").unwrap());
}

struct Stale<'a>(&'a SqliteQueue);
impl Task<()> for Stale<'_> {
    type Input = Value;
    type Output = Value;
    fn id(&self) -> &str {
        "test.identity"
    }
    async fn run(&self, _: &TaskContext<'_, ()>, _: Value) -> Result<Value, ActionError> {
        self.0.with_connection(|db| {
            db.execute("UPDATE lenso_jobs SET lease_token = 'replacement'", [])
                .unwrap();
            Ok(())
        })?;
        Ok(json!(42))
    }
}

#[tokio::test]
async fn stale_worker_cannot_overwrite_a_replacement() {
    let queue = SqliteQueue::new(Connection::open_in_memory().unwrap()).unwrap();
    let task = Stale(&queue);
    queue
        .enqueue(
            &task,
            &json!(1),
            &EnqueueOptions {
                id: Some("stale".into()),
                ..EnqueueOptions::default()
            },
        )
        .unwrap();
    assert_eq!(
        queue
            .run_one(&task, &(), LEASE, 1)
            .await
            .unwrap_err()
            .code(),
        ErrorCode::Conflict
    );
    assert_eq!(queue.get("stale").unwrap().state, JobState::Running);
    assert!(queue.get("stale").unwrap().result.is_none());
    let attempts = queue.attempts("stale").unwrap();
    assert_eq!(attempts[0].outcome, lenso_sqlite::AttemptOutcome::Running);
    assert!(attempts[0].finished_ms.is_none());
}

#[test]
fn event_fanout_is_idempotent_and_atomic() {
    let queue = SqliteQueue::with_clock(Connection::open_in_memory().unwrap(), || 1000).unwrap();
    let task = Identity { failures: 0 };
    let deliveries = [
        Delivery::new("welcome", &task, &json!(1)).unwrap(),
        Delivery::new("audit", &task, &json!(2)).unwrap(),
    ];
    let first = queue
        .publish_serialized(
            "user.created",
            "event-1",
            &deliveries,
            &EnqueueOptions::default(),
        )
        .unwrap();
    assert_eq!(
        queue
            .publish_serialized(
                "user.created",
                "event-1",
                &deliveries,
                &EnqueueOptions::default()
            )
            .unwrap(),
        first
    );
    let conflict = [
        Delivery::new("new", &task, &json!(3)).unwrap(),
        Delivery::new("welcome", &task, &json!("different")).unwrap(),
    ];
    assert_eq!(
        queue
            .publish_serialized(
                "user.created",
                "event-1",
                &conflict,
                &EnqueueOptions::default()
            )
            .unwrap_err()
            .code(),
        ErrorCode::Conflict
    );
    queue
        .with_connection(|db| {
            assert_eq!(
                db.query_row("SELECT count(*) FROM lenso_jobs", [], |row| row
                    .get::<_, u32>(0))
                    .unwrap(),
                2
            );
            Ok(())
        })
        .unwrap();
    let duplicate = [deliveries[0].clone(), deliveries[0].clone()];
    assert_eq!(
        queue
            .publish_serialized(
                "user.created",
                "event-2",
                &duplicate,
                &EnqueueOptions::default()
            )
            .unwrap_err()
            .code(),
        ErrorCode::Conflict
    );
}

#[test]
fn enqueue_joins_native_database_transactions() {
    let queue = SqliteQueue::new(Connection::open_in_memory().unwrap()).unwrap();
    queue
        .with_connection(|db| {
            db.execute("CREATE TABLE invitations (id TEXT PRIMARY KEY)", [])
                .unwrap();
            let transaction = db.transaction().unwrap();
            transaction
                .execute("INSERT INTO invitations VALUES ('invite-1')", [])
                .unwrap();
            SqliteQueue::enqueue_serialized_on(
                &transaction,
                "test.identity",
                "{\"invitation\":\"invite-1\"}",
                &EnqueueOptions {
                    id: Some("invite-mail".into()),
                    ..EnqueueOptions::default()
                },
                1000,
            )?;
            transaction.rollback().unwrap();
            assert_eq!(
                db.query_row("SELECT count(*) FROM invitations", [], |row| row
                    .get::<_, u32>(0))
                    .unwrap(),
                0
            );
            Ok(())
        })
        .unwrap();
    assert_eq!(
        queue.get("invite-mail").unwrap_err().code(),
        ErrorCode::NotFound
    );
}
