//! Atomic transaction boundary for host-owned linked modules.

use platform_core::{
    OutboxPublisher, begin_transaction, claim_idempotency_key_in_tx, commit_transaction,
    rollback_transaction,
};

pub use platform_core::{
    AppError, AppResult, DbPool, DbTransaction, IdempotencyClaim, IdempotencyKey, OutboxEvent,
};

#[derive(Debug)]
pub struct LinkedTransaction<'a> {
    transaction: DbTransaction<'a>,
}

impl<'a> LinkedTransaction<'a> {
    pub async fn begin(pool: &'a DbPool) -> AppResult<Self> {
        let transaction = begin_transaction(pool).await?;
        Ok(Self { transaction })
    }

    pub async fn claim_idempotency_key(
        &mut self,
        key: &IdempotencyKey,
    ) -> AppResult<IdempotencyClaim> {
        claim_idempotency_key_in_tx(&mut self.transaction, key).await
    }

    /// Access the caller transaction for app-owned business SQL.
    ///
    /// The facade intentionally does not provide repositories or query builders.
    pub fn sql(&mut self) -> &mut DbTransaction<'a> {
        &mut self.transaction
    }

    pub async fn publish_outbox(&mut self, event: &OutboxEvent) -> AppResult<()> {
        OutboxPublisher
            .publish_in_tx(&mut self.transaction, event)
            .await
    }

    pub async fn commit(self) -> AppResult<()> {
        commit_transaction(self.transaction).await
    }

    pub async fn rollback(self) -> AppResult<()> {
        rollback_transaction(self.transaction).await
    }
}
