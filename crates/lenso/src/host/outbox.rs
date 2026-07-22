//! Host-owned transactional Outbox delivery.
//!
//! [`OutboxRelay`] claims events published through
//! [`crate::host::transaction::LinkedTransaction`] and passes each claimed
//! event to an [`EventDispatcher`]. Delivery is at least once: a retryable
//! dispatcher error retains the event for a later attempt, so consumers must
//! make effects idempotent using [`ClaimedOutboxEvent::id`]. Retry timing,
//! attempt exhaustion, and dead-letter behavior remain owned by the host
//! relay.
//!
//! This facade intentionally exposes no Outbox table names, SQL, or direct
//! status mutation API.

pub use platform_core::{
    AppError, AppResult, ClaimedOutboxEvent, ErrorCode, EventDispatcher, OutboxRelay,
};
