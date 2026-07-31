use crate::error::{AppError, AppResult, ErrorCode};
use sha2::{Digest as _, Sha256};
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
    Migration {
        name: "platform/0006_create_story_events",
        sql: include_str!("../migrations/0006_create_story_events.sql"),
    },
    Migration {
        name: "platform/0007_create_config_schema",
        sql: include_str!("../migrations/0007_create_config_schema.sql"),
    },
    Migration {
        name: "platform/0008_create_remote_http_proxy_calls",
        sql: include_str!("../migrations/0008_create_remote_http_proxy_calls.sql"),
    },
    Migration {
        name: "platform/0009_add_story_query_indexes",
        sql: include_str!("../migrations/0009_add_story_query_indexes.sql"),
    },
    Migration {
        name: "platform/0010_create_idempotency_claims",
        sql: include_str!("../migrations/0010_create_idempotency_claims.sql"),
    },
    Migration {
        name: "platform/0011_create_extraction_artifacts",
        sql: include_str!("../migrations/0011_create_extraction_artifacts.sql"),
    },
    Migration {
        name: "platform/0012_create_delivery_artifacts",
        sql: include_str!("../migrations/0012_create_delivery_artifacts.sql"),
    },
];

pub async fn apply_migrations(pool: &PgPool, migrations: &[Migration]) -> AppResult<()> {
    ensure_migration_table(pool).await?;

    for migration in migrations {
        apply_migration(pool, migration).await?;
    }

    Ok(())
}

pub async fn apply_module_migration(
    pool: &PgPool,
    name: &str,
    artifact_digest: &str,
    sql: &str,
) -> AppResult<()> {
    let observed_digest = {
        use std::fmt::Write as _;
        let mut value = String::from("sha256:");
        for byte in Sha256::digest(sql.as_bytes()) {
            write!(&mut value, "{byte:02x}").expect("writing to a String cannot fail");
        }
        value
    };
    if name.trim().is_empty() || observed_digest != artifact_digest {
        return Err(AppError::new(
            ErrorCode::Internal,
            "Module migration identity or artifact digest is invalid",
        ));
    }
    let mut tx = pool.begin().await.map_err(map_migration_error)?;
    sqlx::raw_sql(
        r#"
        create schema if not exists platform;
        create table if not exists platform.module_schema_migrations (
            name text primary key,
            artifact_digest text not null,
            applied_at timestamptz not null default now()
        );
        "#,
    )
    .execute(&mut *tx)
    .await
    .map_err(map_migration_error)?;
    let existing: Option<String> = sqlx::query_scalar(
        "select artifact_digest from platform.module_schema_migrations where name = $1",
    )
    .bind(name)
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_migration_error)?;
    if let Some(existing) = existing {
        if existing != artifact_digest {
            return Err(AppError::new(
                ErrorCode::Conflict,
                "Applied Module migration digest differs from the reviewed artifact",
            ));
        }
        tx.commit().await.map_err(map_migration_error)?;
        return Ok(());
    }
    sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()))
        .execute(&mut *tx)
        .await
        .map_err(map_migration_error)?;
    sqlx::query(
        "insert into platform.module_schema_migrations (name, artifact_digest) values ($1, $2)",
    )
    .bind(name)
    .bind(artifact_digest)
    .execute(&mut *tx)
    .await
    .map_err(map_migration_error)?;
    tx.commit().await.map_err(map_migration_error)
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
