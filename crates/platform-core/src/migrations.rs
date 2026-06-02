use crate::error::{AppError, AppResult, ErrorCode};
use sqlx::PgPool;

#[derive(Debug, Clone, Copy)]
pub struct Migration {
    pub name: &'static str,
    pub sql: &'static str,
}

pub const PLATFORM_MIGRATIONS: &[Migration] = &[
    Migration {
        name: "platform/0001_create_platform_schema",
        sql: include_str!("../migrations/0001_create_platform_schema.sql"),
    },
    Migration {
        name: "platform/0002_create_outbox",
        sql: include_str!("../migrations/0002_create_outbox.sql"),
    },
    Migration {
        name: "platform/0003_extend_outbox_delivery_fields",
        sql: include_str!("../migrations/0003_extend_outbox_delivery_fields.sql"),
    },
    Migration {
        name: "platform/0004_add_outbox_summary_index",
        sql: include_str!("../migrations/0004_add_outbox_summary_index.sql"),
    },
    Migration {
        name: "platform/0005_create_execution_logs",
        sql: include_str!("../migrations/0005_create_execution_logs.sql"),
    },
];

pub async fn apply_migrations(pool: &PgPool, migrations: &[Migration]) -> AppResult<()> {
    ensure_migration_table(pool).await?;

    for migration in migrations {
        apply_migration(pool, migration).await?;
    }

    Ok(())
}

async fn ensure_migration_table(pool: &PgPool) -> AppResult<()> {
    sqlx::raw_sql(
        r#"
        create schema if not exists platform;

        create table if not exists platform.schema_migrations (
            name text primary key,
            applied_at timestamptz not null default now()
        );
        "#,
    )
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(map_migration_error)
}

async fn apply_migration(pool: &PgPool, migration: &Migration) -> AppResult<()> {
    let mut tx = pool.begin().await.map_err(map_migration_error)?;

    let already_applied: Option<String> = sqlx::query_scalar(
        r#"
        select name
        from platform.schema_migrations
        where name = $1
        "#,
    )
    .bind(migration.name)
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_migration_error)?;

    if already_applied.is_some() {
        tx.commit().await.map_err(map_migration_error)?;
        return Ok(());
    }

    sqlx::raw_sql(migration.sql)
        .execute(&mut *tx)
        .await
        .map_err(map_migration_error)?;

    sqlx::query(
        r#"
        insert into platform.schema_migrations (name)
        values ($1)
        on conflict (name) do nothing
        "#,
    )
    .bind(migration.name)
    .execute(&mut *tx)
    .await
    .map_err(map_migration_error)?;

    tx.commit().await.map_err(map_migration_error)
}

fn map_migration_error(source: sqlx::Error) -> AppError {
    AppError::new(ErrorCode::Internal, "Database migration failed").with_source(source)
}
