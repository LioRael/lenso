//! SQLite-backed durable tasks with leases, retries and atomic event fanout.
#![doc = include_str!("../README.md")]

mod queue;

pub use queue::{
    AttemptOutcome, Delivery, EnqueueOptions, Job, JobState, SqliteQueue, TaskAttempt,
};
pub use rusqlite;
