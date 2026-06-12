#[allow(clippy::wildcard_imports)]
use super::*;

pub(super) async fn fetch_heatmap_rows(
    ctx: &AppContext,
    request_ctx: &RequestContext,
    query: &HeatmapQuery,
    limit: i64,
    bucket_seconds: i64,
    correlation_id: &str,
) -> Result<Vec<HeatmapRow>, ApiErrorResponse> {
    sqlx::query_as::<_, HeatmapRow>(
        r#"
        with runtime_items as (
            select
                created_at,
                source_module as service,
                'event'::text as node_type,
                status,
                attempts,
                case
                    when locked_at is not null and published_at is not null then
                        greatest(0, extract(epoch from published_at - locked_at)::bigint * 1000)
                    else null::bigint
                end as duration_ms
            from platform.outbox
            where ($1::timestamptz is null or created_at < $1)
              and ($4::timestamptz is null or created_at >= $4)
              and ($5::timestamptz is null or created_at < $5)
              and ($6::text is null or status = $6)
              and ($7::text is null or event_name = $7)
              and ($8::text is null)
              and correlation_id = $9

            union all

            select
                created_at,
                split_part(function_name, '.', 1) as service,
                'function'::text as node_type,
                status,
                attempts,
                case
                    when coalesce(started_at, locked_at) is not null and completed_at is not null then
                        greatest(0, extract(epoch from completed_at - coalesce(started_at, locked_at))::bigint * 1000)
                    else null::bigint
                end as duration_ms
            from runtime.function_runs
            where ($1::timestamptz is null or created_at < $1)
              and ($4::timestamptz is null or created_at >= $4)
              and ($5::timestamptz is null or created_at < $5)
              and ($6::text is null or status = $6)
              and ($7::text is null)
              and ($8::text is null or function_name = $8)
              and correlation_id = $9

            union all

            select
                started_at as created_at,
                service,
                node_type,
                status,
                1 as attempts,
                duration_ms
            from platform.story_events
            where ($1::timestamptz is null or started_at < $1)
              and ($4::timestamptz is null or started_at >= $4)
              and ($5::timestamptz is null or started_at < $5)
              and ($6::text is null or status = $6)
              and ($7::text is null)
              and ($8::text is null)
              and correlation_id = $9
        ),
        heatmap as (
            select
                to_timestamp(
                    floor(extract(epoch from created_at) / $2::double precision) * $2
                )::timestamptz as bucket_start,
                service,
                node_type,
                count(*)::bigint as total_count,
                count(*) filter (where status in ('failed', 'dead'))::bigint as error_count,
                count(*) filter (where attempts > 1)::bigint as retry_count,
                count(*) filter (where status = 'dead')::bigint as dead_count,
                avg(duration_ms)::bigint as avg_duration_ms,
                max(duration_ms)::bigint as max_duration_ms
            from runtime_items
            group by bucket_start, service, node_type
        )
        select
            bucket_start,
            bucket_start + ($2::bigint * interval '1 second') as bucket_end,
            service,
            node_type,
            total_count,
            error_count,
            retry_count,
            dead_count,
            avg_duration_ms,
            max_duration_ms
        from heatmap
        order by bucket_start desc, service asc, node_type asc
        limit $3
        "#,
    )
    .bind(query.created_before)
    .bind(bucket_seconds)
    .bind(limit)
    .bind(query.from)
    .bind(query.to)
    .bind(query.status.as_deref())
    .bind(query.event_name.as_deref())
    .bind(query.function_name.as_deref())
    .bind(correlation_id)
    .fetch_all(&ctx.db)
    .await
    .map_err(|source| query_error(source, request_ctx))
}

pub(super) async fn runtime_story_exists(
    ctx: &AppContext,
    request_ctx: &RequestContext,
    correlation_id: &str,
) -> Result<bool, ApiErrorResponse> {
    sqlx::query_scalar::<_, bool>(
        r#"
        select exists (
            select 1
            from platform.outbox
            where correlation_id = $1

            union all

            select 1
            from runtime.function_runs
            where correlation_id = $1

            union all

            select 1
            from platform.story_events
            where correlation_id = $1
        )
        "#,
    )
    .bind(correlation_id)
    .fetch_one(&ctx.db)
    .await
    .map_err(|source| query_error(source, request_ctx))
}
