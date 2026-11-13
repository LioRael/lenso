# lenso-sqlite

SQLite-backed durable tasks and atomic event fanout. The application owns the worker loop and decides when to poll or stop.

```rust
use std::time::Duration;
use lenso_app::{ActionError, Task, TaskContext};
use lenso_sqlite::{EnqueueOptions, JobState, SqliteQueue, rusqlite::Connection};

struct Echo;
impl Task<()> for Echo {
    type Input = String;
    type Output = String;
    fn id(&self) -> &str { "example.echo" }
    async fn run(&self, _: &TaskContext<'_, ()>, input: String) -> Result<String, ActionError> {
        Ok(input)
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut db = Connection::open_in_memory()?;
    SqliteQueue::initialize(&db)?;
    let transaction = db.transaction()?;
    let receipt = SqliteQueue::enqueue_on(&transaction, &Echo, &"hello".to_owned(), &EnqueueOptions::default(), 1000)?;
    transaction.commit()?;
    let queue = SqliteQueue::new(db)?;
    queue.run_one(&Echo, &(), Duration::from_secs(30), 1).await?;
    assert_eq!(queue.get(&receipt.id)?.state, JobState::Succeeded);
    Ok(())
}
```

Input and output are persisted as JSON. Retries and expired leases allow repeated execution: downstream effects must use the job id for idempotency. All workers for a task kind must agree on its concurrency limit. Submission, status and cancellation require application authorization.

Use `queue.enqueue(&task, &input, &options)` for ordinary typed submission. Use `enqueue_on(db, &task, &input, &options, now_ms)` when it must commit with a native domain transaction; neither path requires manually naming the task or serializing its input. Already encoded payloads use `enqueue_serialized_on`. This unreleased baseline renames the previous string/payload `enqueue_on` to that explicit low-level name.

`publish` accepts a typed `DurableEvent`; subscribers own their mapping to task inputs. `publish_on` uses a savepoint inside the supplied connection or transaction, so the caller's domain write and subscriber jobs can commit or roll back together. `publish_serialized` is the low-level driver interface for already prepared deliveries. A publication never makes an earlier, separately committed write atomic. Newly created jobs in a publication use one clock snapshot.

Lease loss signals cancellation and awaits handler cleanup before returning the lease error. A handler that ignores cancellation can delay that return; the driver does not promise forced termination. Stale completion is still fenced out.

`attempts(id)` retains start, finish, outcome and safe errors for every recorded attempt, including failures before a successful retry. Running cancellation is cooperative. The crate re-exports its `rusqlite` version so callers can use matching native connection and transaction types.
