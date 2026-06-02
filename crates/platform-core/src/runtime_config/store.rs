use crate::db::DbPool;
use crate::error::{AppError, AppResult, ErrorCode};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::BTreeMap;
use uuid::Uuid;

/// One stored config row plus metadata, for the audit/values console views.
#[derive(Debug, Clone)]
pub struct StoredRuntimeConfig {
    pub service: String,
    pub key: String,
    pub value: Value,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<String>,
}

/// One audit-log row.
#[derive(Debug, Clone)]
pub struct RuntimeConfigAuditEntry {
    pub service: String,
    pub key: String,
    pub old_value: Option<Value>,
    pub new_value: Value,
    pub actor: Option<String>,
    pub changed_at: DateTime<Utc>,
}

/// Load every stored value into a `(service, key) -> value` map for snapshot
/// resolution.
pub async fn load_all_values(pool: &DbPool) -> AppResult<BTreeMap<(String, String), Value>> {
    let rows = sqlx::query_as::<_, (String, String, Value)>(
        "select service, key, value from config.setting_values",
    )
    .fetch_all(pool)
    .await
    .map_err(store_error)?;

    Ok(rows
        .into_iter()
        .map(|(service, key, value)| ((service, key), value))
        .collect())
}

/// Upsert a value and insert an audit row in one transaction. Returns the new
/// stored row.
pub async fn upsert_value(
    pool: &DbPool,
    service: &str,
    key: &str,
    value: &Value,
    actor: Option<&str>,
) -> AppResult<StoredRuntimeConfig> {
    let mut tx = pool.begin().await.map_err(store_error)?;

    let old_value = sqlx::query_scalar::<_, Value>(
        "select value from config.setting_values where service = $1 and key = $2",
    )
    .bind(service)
    .bind(key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(store_error)?;

    let row = sqlx::query_as::<_, (String, String, Value, DateTime<Utc>, Option<String>)>(
        r#"
        insert into config.setting_values (service, key, value, updated_at, updated_by)
        values ($1, $2, $3, now(), $4)
        on conflict (service, key)
        do update set value = excluded.value, updated_at = now(), updated_by = excluded.updated_by
        returning service, key, value, updated_at, updated_by
        "#,
    )
    .bind(service)
    .bind(key)
    .bind(value)
    .bind(actor)
    .fetch_one(&mut *tx)
    .await
    .map_err(store_error)?;

    sqlx::query(
        r#"
        insert into config.setting_audit (id, service, key, old_value, new_value, actor, changed_at)
        values ($1, $2, $3, $4, $5, $6, now())
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(service)
    .bind(key)
    .bind(&old_value)
    .bind(value)
    .bind(actor)
    .execute(&mut *tx)
    .await
    .map_err(store_error)?;

    tx.commit().await.map_err(store_error)?;

    Ok(StoredRuntimeConfig {
        service: row.0,
        key: row.1,
        value: row.2,
        updated_at: row.3,
        updated_by: row.4,
    })
}

/// Delete a stored row (reset to shared/default), recording an audit entry if a
/// row existed. Returns true if a row was deleted.
pub async fn delete_value(
    pool: &DbPool,
    service: &str,
    key: &str,
    actor: Option<&str>,
) -> AppResult<bool> {
    let mut tx = pool.begin().await.map_err(store_error)?;
    let old_value = sqlx::query_scalar::<_, Value>(
        "delete from config.setting_values where service = $1 and key = $2 returning value",
    )
    .bind(service)
    .bind(key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(store_error)?;

    let deleted = old_value.is_some();
    if let Some(old) = old_value {
        // `setting_audit.new_value` is NOT NULL, so a delete records the JSON
        // `null` literal to mean "value removed". Distinguish a delete from an
        // upsert-of-null by the presence of `old_value`.
        sqlx::query(
            r#"
            insert into config.setting_audit (id, service, key, old_value, new_value, actor, changed_at)
            values ($1, $2, $3, $4, 'null'::jsonb, $5, now())
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(service)
        .bind(key)
        .bind(&old)
        .bind(actor)
        .execute(&mut *tx)
        .await
        .map_err(store_error)?;
    }
    tx.commit().await.map_err(store_error)?;
    Ok(deleted)
}

/// Audit history for one `(service, key)`, newest first.
pub async fn load_audit(
    pool: &DbPool,
    service: &str,
    key: &str,
    limit: i64,
) -> AppResult<Vec<RuntimeConfigAuditEntry>> {
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            Option<Value>,
            Value,
            Option<String>,
            DateTime<Utc>,
        ),
    >(
        r#"
        select service, key, old_value, new_value, actor, changed_at
        from config.setting_audit
        where service = $1 and key = $2
        order by changed_at desc
        limit $3
        "#,
    )
    .bind(service)
    .bind(key)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(store_error)?;

    Ok(rows
        .into_iter()
        .map(
            |(service, key, old_value, new_value, actor, changed_at)| RuntimeConfigAuditEntry {
                service,
                key,
                old_value,
                new_value,
                actor,
                changed_at,
            },
        )
        .collect())
}

fn store_error(source: sqlx::Error) -> AppError {
    AppError::new(ErrorCode::Internal, "settings store query failed").with_source(source)
}
