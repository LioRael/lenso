use std::{
    fmt,
    path::Path,
    sync::{Mutex, MutexGuard},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub use lenso_app::EventDelivery as Delivery;
use lenso_app::{ActionError, CancellationToken, DurableEvent, ErrorCode, Task, TaskContext};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

/// Persisted state of a job across attempts and process restarts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum JobState {
    /// Waiting for its delay, retry deadline, or an available worker slot.
    Queued,
    /// Owned by a worker holding an unexpired lease.
    Running,
    /// Completed with a serialized result.
    Succeeded,
    /// Exhausted its allowed attempts.
    Failed,
    /// Cancelled before execution or by a cooperating handler.
    Cancelled,
}

/// Persisted job status; payloads and results must be protected by application authorization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    /// Stable receipt and idempotency identifier.
    pub id: String,
    /// Task kind claimed only by compatible workers.
    pub task: String,
    /// Current durable state.
    pub state: JobState,
    /// Attempts started, including expired leases.
    pub attempts: u32,
    /// Maximum attempts including the initial execution.
    pub max_attempts: u32,
    /// Earliest claim time as Unix milliseconds.
    pub available_ms: i64,
    /// Constant delay between failed attempts in milliseconds.
    pub retry_ms: i64,
    /// Serialized task input.
    pub payload: String,
    /// Serialized output after success.
    pub result: Option<String>,
    /// Safe failure message from the last attempt.
    pub error: Option<String>,
}

/// Submission policy shared by producers, independent of worker concurrency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnqueueOptions {
    /// Optional caller-supplied idempotency identifier.
    pub id: Option<String>,
    /// Delay before the first attempt.
    pub delay: Duration,
    /// Maximum attempts including the initial execution.
    pub max_attempts: u32,
    /// Constant delay between failed attempts.
    pub retry: Duration,
}

impl Default for EnqueueOptions {
    fn default() -> Self {
        Self {
            id: None,
            delay: Duration::ZERO,
            max_attempts: 3,
            retry: Duration::from_secs(1),
        }
    }
}

/// Persisted execution record for one task attempt, independent of later retries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AttemptOutcome {
    /// The attempt still owns an active lease.
    Running,
    /// Its handler completed successfully.
    Succeeded,
    /// Its handler failed or its lease expired.
    Failed,
    /// Cancellation stopped the attempt or an expired cancelled lease was recovered.
    Cancelled,
}

/// Persisted execution record for one task attempt, independent of later retries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskAttempt {
    /// One-based execution attempt.
    pub attempt: u32,
    /// Unix milliseconds when the lease was acquired.
    pub started_ms: i64,
    /// Unix milliseconds when the attempt finished or its expired lease was recovered.
    pub finished_ms: Option<i64>,
    /// Attempt outcome. A failed attempt can leave its job queued for retry.
    pub outcome: AttemptOutcome,
    /// Safe error message, retained even after a later attempt succeeds.
    pub error: Option<String>,
}

/// A SQLite queue with application-owned scheduling; no background worker starts implicitly.
pub struct SqliteQueue {
    db: Mutex<Connection>,
    now: Box<dyn Fn() -> Result<i64, ActionError> + Send + Sync>,
}

impl fmt::Debug for SqliteQueue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SqliteQueue").finish_non_exhaustive()
    }
}

fn internal(error: impl std::error::Error + Send + Sync + 'static) -> ActionError {
    ActionError::new(ErrorCode::Internal, "Queue operation failed").with_source(error)
}
fn invalid_input(error: impl std::error::Error + Send + Sync + 'static) -> ActionError {
    ActionError::new(ErrorCode::InvalidInput, "Invalid task payload").with_source(error)
}
fn lease_lost() -> ActionError {
    ActionError::new(ErrorCode::Conflict, "Task lease lost")
}
fn milliseconds(duration: Duration) -> Result<i64, ActionError> {
    if !duration.subsec_nanos().is_multiple_of(1_000_000) {
        return Err(ActionError::new(
            ErrorCode::InvalidInput,
            "Task timing must use whole milliseconds",
        ));
    }
    let value = i64::try_from(duration.as_millis()).map_err(|cause| {
        ActionError::new(ErrorCode::InvalidInput, "Invalid task timing or limit").with_source(cause)
    })?;
    deadline(0, value)
}

fn deadline(now: i64, delay: i64) -> Result<i64, ActionError> {
    const MAX_SAFE_MILLISECONDS: i64 = 9_007_199_254_740_991;
    now.checked_add(delay)
        .filter(|value| now >= 0 && delay >= 0 && *value <= MAX_SAFE_MILLISECONDS)
        .ok_or_else(|| ActionError::new(ErrorCode::InvalidInput, "Invalid task timing or limit"))
}
fn random_token() -> Result<String, ActionError> {
    use std::fmt::Write;
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes)
        .map_err(|_| ActionError::new(ErrorCode::Internal, "Entropy unavailable"))?;
    let mut token = String::with_capacity(32);
    for byte in bytes {
        write!(&mut token, "{byte:02x}").expect("writing to String cannot fail");
    }
    Ok(token)
}

fn decode_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<Job> {
    let state: String = row.get("state")?;
    let state = match state.as_str() {
        "queued" => JobState::Queued,
        "running" => JobState::Running,
        "succeeded" => JobState::Succeeded,
        "failed" => JobState::Failed,
        "cancelled" => JobState::Cancelled,
        _ => {
            return Err(rusqlite::Error::InvalidParameterName(
                "Invalid stored job state".into(),
            ));
        }
    };
    Ok(Job {
        id: row.get("id")?,
        task: row.get("task")?,
        state,
        attempts: row.get("attempts")?,
        max_attempts: row.get("max_attempts")?,
        available_ms: row.get("available_ms")?,
        retry_ms: row.get("retry_ms")?,
        payload: row.get("payload")?,
        result: row.get("result")?,
        error: row.get("error")?,
    })
}

impl SqliteQueue {
    /// Opens a persistent queue with a five-second SQLite busy timeout.
    ///
    /// # Errors
    /// Returns an error if the connection or schema cannot be initialized.
    pub fn open(filename: impl AsRef<Path>) -> Result<Self, ActionError> {
        let db = Connection::open(filename).map_err(internal)?;
        Self::new(db)
    }

    /// Takes ownership of a native connection, preserving direct SQL access through [`Self::with_connection`].
    ///
    /// # Errors
    /// Returns an error if the schema cannot be initialized.
    pub fn new(db: Connection) -> Result<Self, ActionError> {
        Self::with_fallible_clock(db, || {
            let elapsed = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(internal)?;
            i64::try_from(elapsed.as_millis()).map_err(internal)
        })
    }

    /// Supplies an application clock for deterministic scheduling tests.
    ///
    /// # Errors
    /// Returns an error if connection configuration or schema initialization fails.
    pub fn with_clock(
        db: Connection,
        now: impl Fn() -> i64 + Send + Sync + 'static,
    ) -> Result<Self, ActionError> {
        Self::with_fallible_clock(db, move || Ok(now()))
    }

    fn with_fallible_clock(
        db: Connection,
        now: impl Fn() -> Result<i64, ActionError> + Send + Sync + 'static,
    ) -> Result<Self, ActionError> {
        db.busy_timeout(Duration::from_secs(5)).map_err(internal)?;
        Self::initialize(&db)?;
        Ok(Self {
            db: Mutex::new(db),
            now: Box::new(now),
        })
    }

    /// Initializes queue tables on an application-owned connection without taking ownership.
    ///
    /// # Errors
    /// Returns database schema initialization errors.
    pub fn initialize(db: &Connection) -> Result<(), ActionError> {
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS lenso_jobs (
            id TEXT PRIMARY KEY, task TEXT NOT NULL, payload TEXT NOT NULL,
            state TEXT NOT NULL, attempts INTEGER NOT NULL DEFAULT 0, max_attempts INTEGER NOT NULL,
            available_ms INTEGER NOT NULL, retry_ms INTEGER NOT NULL,
            lease_token TEXT, lease_until INTEGER, cancel_requested INTEGER NOT NULL DEFAULT 0,
            result TEXT, error TEXT
        ); CREATE INDEX IF NOT EXISTS lenso_jobs_ready ON lenso_jobs(task, state, available_ms);
        CREATE TABLE IF NOT EXISTS lenso_task_attempts (
            job_id TEXT NOT NULL, attempt INTEGER NOT NULL, started_ms INTEGER NOT NULL,
            finished_ms INTEGER, outcome TEXT NOT NULL, error TEXT, PRIMARY KEY(job_id, attempt)
        );",
        )
        .map_err(internal)?;
        Ok(())
    }

    fn db(&self) -> Result<MutexGuard<'_, Connection>, ActionError> {
        self.db
            .lock()
            .map_err(|_| ActionError::new(ErrorCode::Internal, "Queue lock poisoned"))
    }

    /// Runs synchronous native SQL, including transactions that also enqueue jobs.
    /// The closure holds this queue's connection lock. Use `enqueue_on` or `publish_on`
    /// with the borrowed connection; do not reenter owning methods on the same queue.
    ///
    /// # Errors
    /// Propagates the closure's error or a poisoned connection lock.
    pub fn with_connection<T>(
        &self,
        run: impl FnOnce(&mut Connection) -> Result<T, ActionError>,
    ) -> Result<T, ActionError> {
        let mut db = self.db()?;
        run(&mut db)
    }

    /// Submits a typed task. Submission authorization belongs to the calling action.
    ///
    /// # Errors
    /// Returns input, persistence or idempotency conflict failures.
    pub fn enqueue<R, T: Task<R>>(
        &self,
        task: &T,
        input: &T::Input,
        options: &EnqueueOptions,
    ) -> Result<Job, ActionError> {
        let payload = serde_json::to_string(input).map_err(invalid_input)?;
        let db = self.db()?;
        Self::enqueue_serialized_on(&db, task.id(), &payload, options, (self.now)()?)
    }

    /// Submits a typed task on a native connection or transaction without starting another transaction.
    ///
    /// # Errors
    /// Returns input encoding, invalid options, database or idempotency conflict failures.
    pub fn enqueue_on<R, T: Task<R>>(
        db: &Connection,
        task: &T,
        input: &T::Input,
        options: &EnqueueOptions,
        now_ms: i64,
    ) -> Result<Job, ActionError> {
        let payload = serde_json::to_string(input).map_err(invalid_input)?;
        Self::enqueue_serialized_on(db, task.id(), &payload, options, now_ms)
    }

    /// Submits an already encoded payload on a native connection or transaction.
    ///
    /// # Errors
    /// Returns invalid options, database failures or an ID reused with a different task or payload.
    pub fn enqueue_serialized_on(
        db: &Connection,
        task: &str,
        payload: &str,
        options: &EnqueueOptions,
        now_ms: i64,
    ) -> Result<Job, ActionError> {
        if options.max_attempts == 0 {
            return Err(ActionError::new(
                ErrorCode::InvalidInput,
                "Maximum attempts must be positive",
            ));
        }
        let id = options.id.clone().map(Ok).unwrap_or_else(random_token)?;
        let available = deadline(now_ms, milliseconds(options.delay)?)?;
        let retry_ms = milliseconds(options.retry)?;
        deadline(now_ms, retry_ms)?;
        db.execute("INSERT INTO lenso_jobs (id, task, payload, state, max_attempts, available_ms, retry_ms)
            VALUES (?, ?, ?, 'queued', ?, ?, ?) ON CONFLICT(id) DO NOTHING",
            params![id, task, payload, options.max_attempts, available, retry_ms]).map_err(internal)?;
        let job = db
            .query_row("SELECT * FROM lenso_jobs WHERE id = ?", [&id], decode_job)
            .map_err(internal)?;
        if job.task != task || job.payload != payload {
            return Err(ActionError::new(
                ErrorCode::Conflict,
                "Job ID already has another payload or task",
            ));
        }
        Ok(job)
    }

    /// Atomically creates independent subscriber jobs; repeat publication IDs deduplicate each subscriber.
    ///
    /// # Errors
    /// Rolls back all new deliveries on duplicate subscriptions, conflicting payloads or database errors.
    pub fn publish<T>(
        &self,
        event: &DurableEvent<T>,
        event_id: &str,
        value: &T,
        options: &EnqueueOptions,
    ) -> Result<Vec<Job>, ActionError> {
        let deliveries = event.deliveries(value)?;
        self.publish_serialized(event.id(), event_id, &deliveries, options)
    }

    /// Persists prepared deliveries atomically, including inside an application-owned transaction.
    ///
    /// # Errors
    /// Returns duplicate subscription, payload conflict or database errors.
    pub fn publish_serialized(
        &self,
        event: &str,
        event_id: &str,
        deliveries: &[Delivery],
        options: &EnqueueOptions,
    ) -> Result<Vec<Job>, ActionError> {
        let db = self.db()?;
        Self::publish_serialized_on(&db, event, event_id, deliveries, options, (self.now)()?)
    }

    /// Publishes a typed event on the caller's native connection or transaction.
    ///
    /// # Errors
    /// Returns subscriber mapping, idempotency conflict or database failures.
    pub fn publish_on<T>(
        db: &Connection,
        event: &DurableEvent<T>,
        event_id: &str,
        value: &T,
        options: &EnqueueOptions,
        now_ms: i64,
    ) -> Result<Vec<Job>, ActionError> {
        Self::publish_serialized_on(
            db,
            event.id(),
            event_id,
            &event.deliveries(value)?,
            options,
            now_ms,
        )
    }

    /// Atomically persists prepared subscriber jobs without committing the caller's transaction.
    ///
    /// # Errors
    /// Rolls back new deliveries on conflict while retaining the caller's surrounding transaction.
    pub fn publish_serialized_on(
        db: &Connection,
        event: &str,
        event_id: &str,
        deliveries: &[Delivery],
        options: &EnqueueOptions,
        now_ms: i64,
    ) -> Result<Vec<Job>, ActionError> {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        if deliveries
            .iter()
            .any(|delivery| !seen.insert(&delivery.subscription))
        {
            return Err(ActionError::new(
                ErrorCode::Conflict,
                "Duplicate event subscription",
            ));
        }
        db.execute_batch("SAVEPOINT lenso_publish")
            .map_err(internal)?;
        let jobs = deliveries
            .iter()
            .map(|delivery| {
                let id = serde_json::to_string(&(event, event_id, &delivery.subscription))
                    .map_err(invalid_input)?;
                Self::enqueue_serialized_on(
                    db,
                    &delivery.task,
                    &delivery.payload,
                    &EnqueueOptions {
                        id: Some(id),
                        ..options.clone()
                    },
                    now_ms,
                )
            })
            .collect::<Result<Vec<_>, _>>();
        match jobs {
            Ok(jobs) => {
                db.execute_batch("RELEASE SAVEPOINT lenso_publish")
                    .map_err(internal)?;
                Ok(jobs)
            }
            Err(error) => {
                db.execute_batch(
                    "ROLLBACK TO SAVEPOINT lenso_publish; RELEASE SAVEPOINT lenso_publish",
                )
                .map_err(internal)?;
                Err(error)
            }
        }
    }

    /// Reads the history of attempts without discarding failures after a successful retry.
    ///
    /// # Errors
    /// Returns `not_found` for an unknown job or database decoding failures.
    pub fn attempts(&self, id: &str) -> Result<Vec<TaskAttempt>, ActionError> {
        self.get(id)?;
        self.db()?
            .prepare("SELECT * FROM lenso_task_attempts WHERE job_id = ? ORDER BY attempt")
            .map_err(internal)?
            .query_map([id], |row| {
                let state: String = row.get("outcome")?;
                let outcome = match state.as_str() {
                    "running" => AttemptOutcome::Running,
                    "succeeded" => AttemptOutcome::Succeeded,
                    "failed" => AttemptOutcome::Failed,
                    "cancelled" => AttemptOutcome::Cancelled,
                    _ => {
                        return Err(rusqlite::Error::InvalidParameterName(
                            "Invalid stored attempt outcome".into(),
                        ));
                    }
                };
                Ok(TaskAttempt {
                    attempt: row.get("attempt")?,
                    started_ms: row.get("started_ms")?,
                    finished_ms: row.get("finished_ms")?,
                    outcome,
                    error: row.get("error")?,
                })
            })
            .map_err(internal)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(internal)
    }

    /// Reads persisted status; callers must apply their own ownership and management authorization.
    ///
    /// # Errors
    /// Returns `not_found` for an unknown receipt or database failures.
    pub fn get(&self, id: &str) -> Result<Job, ActionError> {
        self.db()?
            .query_row("SELECT * FROM lenso_jobs WHERE id = ?", [id], decode_job)
            .optional()
            .map_err(internal)?
            .ok_or_else(|| ActionError::new(ErrorCode::NotFound, "Job not found"))
    }

    /// Cancels queued work immediately and requests cooperative cancellation of running work.
    ///
    /// # Errors
    /// Returns database failures; terminal or missing jobs return `false`.
    pub fn cancel(&self, id: &str) -> Result<bool, ActionError> {
        Ok(self
            .db()?
            .execute(
                "UPDATE lenso_jobs SET cancel_requested = 1,
            state = CASE WHEN state = 'queued' THEN 'cancelled' ELSE state END
            WHERE id = ? AND state IN ('queued', 'running')",
                [id],
            )
            .map_err(internal)?
            > 0)
    }

    fn claim(
        &self,
        task: &str,
        lease_ms: i64,
        concurrency: u32,
    ) -> Result<Option<(Job, String)>, ActionError> {
        let now = (self.now)()?;
        let until = deadline(now, lease_ms)?;
        let mut db = self.db()?;
        let transaction = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(internal)?;
        transaction.execute("UPDATE lenso_task_attempts SET finished_ms = ?, outcome = CASE WHEN (
            SELECT cancel_requested FROM lenso_jobs WHERE id = job_id) = 1 THEN 'cancelled' ELSE 'failed' END,
            error = 'Task lease expired' WHERE outcome = 'running' AND job_id IN (
                SELECT id FROM lenso_jobs WHERE task = ? AND state = 'running' AND lease_until <= ?
            )", params![now, task, now]).map_err(internal)?;
        transaction
            .execute(
                "UPDATE lenso_jobs SET state = CASE WHEN cancel_requested = 1 THEN 'cancelled'
            WHEN attempts >= max_attempts THEN 'failed' ELSE 'queued' END,
            lease_token = NULL, lease_until = NULL, error = 'Task lease expired'
            WHERE task = ? AND state = 'running' AND lease_until <= ?",
                params![task, now],
            )
            .map_err(internal)?;
        let token = random_token()?;
        let job = transaction
            .query_row(
                "UPDATE lenso_jobs SET state = 'running', attempts = attempts + 1,
            lease_token = ?, lease_until = ? WHERE id = (
                SELECT id FROM lenso_jobs WHERE task = ? AND state = 'queued' AND available_ms <= ?
                AND (SELECT count(*) FROM lenso_jobs WHERE task = ? AND state = 'running') < ?
                ORDER BY available_ms, id LIMIT 1
            ) RETURNING *",
                params![token, until, task, now, task, concurrency],
                decode_job,
            )
            .optional()
            .map_err(internal)?;
        if let Some(job) = &job {
            transaction.execute("INSERT INTO lenso_task_attempts (job_id, attempt, started_ms, outcome) VALUES (?, ?, ?, 'running')", params![job.id, job.attempts, now]).map_err(internal)?;
        }
        transaction.commit().map_err(internal)?;
        Ok(job.map(|job| (job, token)))
    }

    fn renew(&self, id: &str, token: &str, lease_ms: i64) -> Result<bool, ActionError> {
        let now = (self.now)()?;
        self.db()?.query_row("UPDATE lenso_jobs SET lease_until = ?
            WHERE id = ? AND state = 'running' AND lease_token = ? AND lease_until > ? RETURNING cancel_requested",
            params![deadline(now, lease_ms)?, id, token, now], |row| row.get(0)).optional().map_err(internal)?.ok_or_else(lease_lost)
    }

    /// Claims one job, renews its lease and persists its result, retry, failure or cancellation.
    /// All workers for a task kind must agree on its global concurrency limit.
    ///
    /// # Errors
    /// Returns invalid limits, database errors or `conflict` if the worker loses its lease.
    pub async fn run_one<R: Sync, T: Task<R>>(
        &self,
        task: &T,
        resources: &R,
        lease: Duration,
        concurrency: u32,
    ) -> Result<Option<Job>, ActionError> {
        let lease_ms = milliseconds(lease)?;
        if lease_ms < 3 || concurrency == 0 {
            return Err(ActionError::new(
                ErrorCode::InvalidInput,
                "Invalid lease or concurrency",
            ));
        }
        let Some((job, token)) = self.claim(task.id(), lease_ms, concurrency)? else {
            return Ok(None);
        };
        let context = TaskContext {
            job_id: &job.id,
            attempt: job.attempts,
            resources,
            cancellation: CancellationToken::new(),
        };
        let mut lease_error = None;
        let result = async {
            let input = serde_json::from_str(&job.payload).map_err(invalid_input)?;
            let future = task.run(&context, input);
            tokio::pin!(future);
            let heartbeat_ms = (lease_ms / 3).min(2_147_483_647);
            let mut heartbeat = tokio::time::interval(Duration::from_millis(u64::try_from(heartbeat_ms).map_err(invalid_input)?));
            heartbeat.tick().await;
            loop {
                tokio::select! {
                    result = &mut future => break serde_json::to_string(&result?).map_err(|error| ActionError::new(ErrorCode::Internal, "Invalid output").with_source(error)),
                    _ = heartbeat.tick() => {
                        match self.renew(&job.id, &token, lease_ms) {
                            Ok(true) => context.cancellation.cancel(),
                            Ok(false) => {},
                            Err(error) => {
                                context.cancellation.cancel();
                                lease_error = Some(error);
                                let _ = future.await;
                                return Err(lease_lost());
                            }
                        }
                    }
                }
            }
        }.await;
        if let Some(error) = lease_error {
            return Err(error);
        }
        let state = match &result {
            Ok(_) => "succeeded",
            Err(error) if error.code() == ErrorCode::Cancelled => "cancelled",
            Err(_) if job.attempts >= job.max_attempts => "failed",
            Err(_) => "queued",
        };
        let now = (self.now)()?;
        let available = if state == "queued" {
            deadline(now, job.retry_ms)?
        } else {
            job.available_ms
        };
        let mut db = self.db()?;
        let transaction = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(internal)?;
        let completed = transaction.query_row("UPDATE lenso_jobs SET state = CASE WHEN cancel_requested = 1 AND ? <> 'succeeded' THEN 'cancelled' ELSE ? END, result = ?, error = ?,
            available_ms = ?, lease_token = NULL, lease_until = NULL
            WHERE id = ? AND state = 'running' AND lease_token = ? AND lease_until > ? RETURNING *",
            params![state, state, result.as_ref().ok(), result.as_ref().err().map(ActionError::message), available, job.id, token, now], decode_job,
        ).optional().map_err(internal)?.ok_or_else(lease_lost)?;
        let outcome = match completed.state {
            JobState::Queued | JobState::Failed => "failed",
            JobState::Succeeded => "succeeded",
            JobState::Cancelled => "cancelled",
            _ => return Err(lease_lost()),
        };
        transaction.execute("UPDATE lenso_task_attempts SET finished_ms = ?, outcome = ?, error = ? WHERE job_id = ? AND attempt = ?", params![now, outcome, result.as_ref().err().map(ActionError::message), job.id, job.attempts]).map_err(internal)?;
        transaction.commit().map_err(internal)?;
        Ok(Some(completed))
    }
}
