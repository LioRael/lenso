#[allow(clippy::wildcard_imports)]
use super::*;

pub(super) async fn fetch_story_rows(
    ctx: &AppContext,
    request_ctx: &RequestContext,
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

                union all

                select
                    correlation_id,
                    occurred_at as updated_at
                from platform.remote_http_proxy_calls
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
                0::integer as attempts,
                1::integer as max_attempts,
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

            union all

            select
                'remote_proxy_call'::text as item_type,
                'remoteproxy_' || id as id,
                module_name || ' ' || method || ' ' || declared_path as name,
                case when success then 'completed' else 'failed' end as status,
                0::integer as attempts,
                1::integer as max_attempts,
                correlation_id,
                request_id as causation_id,
                module_name as service,
                occurred_at as created_at,
                occurred_at as started_at,
                occurred_at + (duration_ms * interval '1 millisecond') as completed_at,
                case
                    when success then null
                    when error_code is null then 'remote proxy call failed'
                    else 'remote proxy call failed with ' || error_code
                end as last_error,
                jsonb_build_object(
                    'remote_proxy_call_id', id,
                    'module_name', module_name,
                    'method', method,
                    'declared_path', declared_path,
                    'remote_path', remote_path,
                    'capability', capability,
                    'display_name', null,
                    'story_title', null,
                    'remote_status', remote_status,
                    'duration_ms', duration_ms,
                    'request_id', request_id,
                    'trace_id', trace_id,
                    'span_id', span_id,
                    'success', success,
                    'error_code', error_code,
                    'retryable', retryable,
                    'path_params', path_params,
                    'error_details', error_details
                ) as metadata
            from platform.remote_http_proxy_calls proxy_calls
            where correlation_id in (select correlation_id from story_keys)
                and not exists (
                    select 1
                    from platform.story_events story_events
                    where story_events.source_type = 'remote_proxy_call'
                        and story_events.source_id = proxy_calls.id
                )
        ) story_work
        order by correlation_id asc, created_at asc, id asc
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
