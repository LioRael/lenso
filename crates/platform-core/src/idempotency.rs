use crate::db::DbTransaction;
use crate::error::{AppError, AppResult, ErrorCode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdempotencyKey {
    scope: String,
    value: String,
}

impl IdempotencyKey {
    pub fn parse(scope: impl Into<String>, value: impl Into<String>) -> AppResult<Self> {
        let scope = scope.into();
        let value = value.into();
        if scope.trim().is_empty() || value.trim().is_empty() {
            return Err(AppError::new(
                ErrorCode::Validation,
                "Idempotency scope and key must not be empty",
            ));
        }
        Ok(Self { scope, value })
    }

    pub fn scope(&self) -> &str {
        &self.scope
    }

    pub fn value(&self) -> &str {
        &self.value
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdempotencyClaim {
    Acquired,
    Existing,
}

pub async fn claim_idempotency_key_in_tx(
    transaction: &mut DbTransaction<'_>,
    key: &IdempotencyKey,
) -> AppResult<IdempotencyClaim> {
    let inserted = sqlx::query_scalar::<_, i32>(
        r#"
        insert into platform.idempotency_claims (scope, key)
        values ($1, $2)
        on conflict (scope, key) do nothing
        returning 1
        "#,
    )
    .bind(key.scope())
    .bind(key.value())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(map_idempotency_error)?;
    Ok(if inserted.is_some() {
        IdempotencyClaim::Acquired
    } else {
        IdempotencyClaim::Existing
    })
}

fn map_idempotency_error(source: sqlx::Error) -> AppError {
    AppError::new(ErrorCode::Internal, "Idempotency claim failed").with_source(source)
}
