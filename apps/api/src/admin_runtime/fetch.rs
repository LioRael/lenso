#[allow(clippy::wildcard_imports)]
use super::*;
use chrono::{DateTime, Utc};
use platform_core::{AppContext, AppError, ErrorCode};
use platform_http::ApiErrorResponse;
use serde_json::Value;
use sqlx::Row;

pub(crate) async fn fetch_outbox_event(
    ctx: &AppContext,
    request_ctx: &platform_core::RequestContext,
    id: &str,
) -> Result<AdminOutboxEvent, ApiErrorResponse> {
    let row = sqlx::query_as::<_, OutboxAdminRow>(
        r#"
        select
            id,
            event_name,
            status,
            attempts,
            max_attempts,
            available_at,
            locked_by,
            published_at,
            last_error,
            correlation_id,
            created_at
        from platform.outbox
        where id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&ctx.db)
    .await
    .map_err(|source| query_error(source, request_ctx))?
    .ok_or_else(|| {
        ApiErrorResponse::with_context(
            AppError::new(
                ErrorCode::NotFound,
                format!("Outbox event {id} was not found"),
            ),
            request_ctx,
        )
    })?;

    Ok(row.into())
}

pub(crate) async fn fetch_outbox_event_detail(
    ctx: &AppContext,
    request_ctx: &platform_core::RequestContext,
    id: &str,
) -> Result<AdminOutboxEventDetail, ApiErrorResponse> {
    let row = sqlx::query(
        r#"
        select
            id,
            event_name,
            event_version,
            source_module,
            aggregate_type,
            aggregate_id,
            status,
            attempts,
            max_attempts,
            available_at,
            locked_by,
            published_at,
            last_error,
            correlation_id,
            causation_id,
            occurred_at,
            created_at,
            payload,
            headers
        from platform.outbox
        where id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&ctx.db)
    .await
    .map_err(|source| query_error(source, request_ctx))?
    .ok_or_else(|| {
        ApiErrorResponse::with_context(
            AppError::new(
                ErrorCode::NotFound,
                format!("Outbox event {id} was not found"),
            ),
            request_ctx,
        )
    })?;

    outbox_detail_from_row(&row).map_err(|source| query_error(source, request_ctx))
}

pub(crate) fn outbox_detail_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<AdminOutboxEventDetail, sqlx::Error> {
    let headers: Value = row.try_get("headers")?;
    let actor = headers
        .get("actor")
        .cloned()
        .unwrap_or_else(|| Value::Object(Default::default()));
    let trace = headers
        .get("trace")
        .cloned()
        .unwrap_or_else(|| Value::Object(Default::default()));

    Ok(AdminOutboxEventDetail {
        id: row.try_get("id")?,
        event_name: row.try_get("event_name")?,
        event_version: row.try_get("event_version")?,
        source_module: row.try_get("source_module")?,
        aggregate_type: row.try_get("aggregate_type")?,
        aggregate_id: row.try_get("aggregate_id")?,
        status: row.try_get("status")?,
        attempts: row.try_get("attempts")?,
        max_attempts: row.try_get("max_attempts")?,
        available_at: row.try_get("available_at")?,
        locked_by: row.try_get("locked_by")?,
        published_at: row.try_get("published_at")?,
        last_error: row.try_get("last_error")?,
        correlation_id: row.try_get("correlation_id")?,
        causation_id: row.try_get("causation_id")?,
        occurred_at: row.try_get("occurred_at")?,
        created_at: row.try_get("created_at")?,
        payload: row.try_get("payload")?,
        actor,
        trace,
        headers,
    })
}

pub(crate) async fn fetch_function_run(
    ctx: &AppContext,
    request_ctx: &platform_core::RequestContext,
    id: &str,
) -> Result<AdminFunctionRun, ApiErrorResponse> {
    let row = sqlx::query_as::<_, FunctionRunAdminRow>(
        r#"
        select
            id,
            function_name,
            status,
            attempts,
            max_attempts,
            available_at,
            locked_by,
            started_at,
            completed_at,
            last_error,
            correlation_id,
            created_at
        from runtime.function_runs
        where id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&ctx.db)
    .await
    .map_err(|source| query_error(source, request_ctx))?
    .ok_or_else(|| {
        ApiErrorResponse::with_context(
            AppError::new(
                ErrorCode::NotFound,
                format!("Function run {id} was not found"),
            ),
            request_ctx,
        )
    })?;

    Ok(row.into())
}

pub(crate) async fn fetch_summary_items(
    ctx: &AppContext,
    request_ctx: &platform_core::RequestContext,
    failures_only: bool,
) -> Result<Vec<AdminRuntimeSummaryItem>, ApiErrorResponse> {
    let rows = sqlx::query_as::<_, SummaryItemRow>(
        r#"
        select *
        from (
            select
                'outbox_event'::text as item_type,
                id,
                event_name as name,
                status,
                attempts,
                max_attempts,
                correlation_id,
                created_at,
                last_error
            from platform.outbox
            where (not $1 or status in ('failed', 'dead'))

            union all

            select
                'function_run'::text as item_type,
                id,
                function_name as name,
                status,
                attempts,
                max_attempts,
                correlation_id,
                created_at,
                last_error
            from runtime.function_runs
            where (not $1 or status in ('failed', 'dead'))

            union all

            select
                node_type as item_type,
                id,
                name,
                status,
                1 as attempts,
                1 as max_attempts,
                correlation_id,
                started_at as created_at,
                error as last_error
            from platform.story_events
            where (not $1 or status in ('failed', 'dead'))
        ) summary_items
        order by created_at desc, item_type asc, id desc
        limit 10
        "#,
    )
    .bind(failures_only)
    .fetch_all(&ctx.db)
    .await
    .map_err(|source| query_error(source, request_ctx))?;

    Ok(rows.into_iter().map(Into::into).collect())
}

pub(crate) async fn fetch_runtime_node_ref(
    ctx: &AppContext,
    request_ctx: &platform_core::RequestContext,
    node_id: &str,
) -> Result<RuntimeNodeRef, ApiErrorResponse> {
    let row = sqlx::query_as::<_, RuntimeNodeRefTuple>(
        r#"
        select id, item_type, correlation_id
        from (
            select
                id,
                'event'::text as item_type,
                correlation_id
            from platform.outbox
            where id = $1

            union all

            select
                id,
                'function'::text as item_type,
                correlation_id
            from runtime.function_runs
            where id = $1

            union all

            select
                id,
                node_type as item_type,
                correlation_id
            from platform.story_events
            where id = $1
        ) runtime_nodes
        order by item_type asc
        limit 1
        "#,
    )
    .bind(node_id)
    .fetch_optional(&ctx.db)
    .await
    .map_err(|source| query_error(source, request_ctx))?
    .ok_or_else(|| {
        ApiErrorResponse::with_context(
            AppError::new(
                ErrorCode::NotFound,
                format!("Runtime execution node {node_id} was not found"),
            ),
            request_ctx,
        )
    })?;

    Ok(row.into())
}

pub(crate) async fn fetch_story_rows(
    ctx: &AppContext,
    request_ctx: &platform_core::RequestContext,
    correlation_id: Option<&str>,
    created_before: Option<DateTime<Utc>>,
    limit: i64,
) -> Result<Vec<StoryWorkRow>, ApiErrorResponse> {
    let rows = sqlx::query_as::<_, StoryWorkTuple>(
        r#"
        with story_keys as (
            select correlation_id, max(updated_at) as updated_at
            from (
                select
                    correlation_id,
                    coalesce(published_at, locked_at, created_at) as updated_at
                from platform.outbox
                where ($1::text is null or correlation_id = $1)

                union all

                select
                    correlation_id,
                    coalesce(completed_at, started_at, locked_at, created_at) as updated_at
                from runtime.function_runs
                where ($1::text is null or correlation_id = $1)

                union all

                select
                    correlation_id,
                    updated_at
                from platform.story_events
                where ($1::text is null or correlation_id = $1)
            ) story_items
            group by correlation_id
            having ($2::timestamptz is null or max(updated_at) < $2)
            order by updated_at desc, correlation_id asc
            limit $3
        )
        select *
        from (
            select
                'event'::text as item_type,
                id,
                event_name as name,
                status,
                attempts,
                max_attempts,
                correlation_id,
                causation_id,
                source_module as service,
                created_at,
                locked_at as started_at,
                published_at as completed_at,
                last_error,
                headers as metadata
            from platform.outbox
            where correlation_id in (select correlation_id from story_keys)

            union all

            select
                'function'::text as item_type,
                id,
                function_name as name,
                status,
                attempts,
                max_attempts,
                correlation_id,
                null::text as causation_id,
                split_part(function_name, '.', 1) as service,
                created_at,
                coalesce(started_at, locked_at) as started_at,
                completed_at,
                last_error,
                input_json as metadata
            from runtime.function_runs
            where correlation_id in (select correlation_id from story_keys)

            union all

            select
                node_type as item_type,
                id,
                name,
                status,
                1 as attempts,
                1 as max_attempts,
                correlation_id,
                causation_id,
                service,
                started_at as created_at,
                started_at,
                completed_at,
                error as last_error,
                metadata
            from platform.story_events
            where correlation_id in (select correlation_id from story_keys)
        ) story_rows
        order by correlation_id asc, created_at asc, item_type asc, id asc
        "#,
    )
    .bind(correlation_id)
    .bind(created_before)
    .bind(limit)
    .fetch_all(&ctx.db)
    .await
    .map_err(|source| query_error(source, request_ctx))?;

    Ok(rows.into_iter().map(Into::into).collect())
}
