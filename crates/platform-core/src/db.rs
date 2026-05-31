use crate::config::DatabaseConfig;
use crate::error::{AppError, AppResult, ErrorCode};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Postgres, Transaction};

pub type DbPool = PgPool;
pub type DbTransaction<'a> = Transaction<'a, Postgres>;

pub async fn connect_pool(config: &DatabaseConfig) -> AppResult<DbPool> {
    PgPoolOptions::new()
        .max_connections(config.max_connections)
        .connect(&config.url)
        .await
        .map_err(|source| {
            AppError::new(ErrorCode::ExternalDependency, "Database connection failed")
                .with_source(source)
                .retryable()
        })
}

pub async fn ping(pool: &DbPool) -> AppResult<()> {
    sqlx::query("select 1")
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(|source| {
            AppError::new(
                ErrorCode::ExternalDependency,
                "Database health check failed",
            )
            .with_source(source)
            .retryable()
        })
}
