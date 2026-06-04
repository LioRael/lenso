use platform_core::{
    CorrelationId, RemoteHttpProxyCallRecord, RequestContext, RequestId, TraceContext,
    apply_migrations, insert_remote_http_proxy_call, remote_proxy_call_story_event_id,
    story_events::http_request_story_event_id,
};
use platform_testing::{SequentialIdGenerator, TestDatabase};
use serde_json::json;
use sqlx::Row;

#[tokio::test]
async fn remote_proxy_call_records_request_and_trace_context() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };

    apply_migrations(&db.pool, platform_core::PLATFORM_MIGRATIONS)
        .await
        .expect("platform migrations should apply");

    let mut request_ctx = RequestContext::new(
        RequestId::new("req_remote_trace"),
        CorrelationId::new("corr_remote_trace"),
    );
    request_ctx.trace = TraceContext {
        trace_id: Some("00000000000000000000000000000011".to_owned()),
        span_id: Some("0000000000000011".to_owned()),
        baggage: Vec::new(),
    };
    request_ctx.causation_id = Some(http_request_story_event_id(&request_ctx));
    let record = RemoteHttpProxyCallRecord {
        module_name: "remote-crm".to_owned(),
        method: "GET".to_owned(),
        declared_path: "/contacts/{id}".to_owned(),
        remote_path: "/contacts/contact_1".to_owned(),
        capability: Some("remote_crm.contacts.read".to_owned()),
        display_name: Some("Fetch Contact".to_owned()),
        story_title: Some("Fetch Contact".to_owned()),
        remote_status: Some(200),
        duration_ms: 42,
        success: true,
        error_code: None,
        retryable: false,
        path_params: json!({ "id": "contact_1" }),
        error_details: json!({ "ignored": true }),
    };

    let id = insert_remote_http_proxy_call(
        &db.pool,
        &SequentialIdGenerator::default(),
        &request_ctx,
        record,
    )
    .await
    .expect("remote proxy call should insert");

    let row = sqlx::query(
        r#"
        select
            id,
            request_id,
            correlation_id,
            trace_id,
            span_id,
            path_params,
            error_details
        from platform.remote_http_proxy_calls
        where id = $1
        "#,
    )
    .bind(&id)
    .fetch_one(&db.pool)
    .await
    .expect("remote proxy call should query");

    assert_eq!(row.try_get::<String, _>("id").expect("id"), "rproxy_1");
    assert_eq!(
        row.try_get::<String, _>("request_id").expect("request_id"),
        "req_remote_trace"
    );
    assert_eq!(
        row.try_get::<String, _>("correlation_id")
            .expect("correlation_id"),
        "corr_remote_trace"
    );
    assert_eq!(
        row.try_get::<Option<String>, _>("trace_id")
            .expect("trace_id")
            .as_deref(),
        Some("00000000000000000000000000000011")
    );
    assert_eq!(
        row.try_get::<Option<String>, _>("span_id")
            .expect("span_id")
            .as_deref(),
        Some("0000000000000011")
    );
    assert_eq!(
        row.try_get::<serde_json::Value, _>("path_params")
            .expect("path_params"),
        json!({ "id": "contact_1" })
    );
    assert_eq!(
        row.try_get::<serde_json::Value, _>("error_details")
            .expect("error_details"),
        json!([])
    );

    let story_row = sqlx::query(
        r#"
        select
            id,
            source_type,
            source_id,
            node_type,
            name,
            status,
            service,
            correlation_id,
            causation_id,
            duration_ms,
            error,
            metadata,
            trace_id,
            span_id
        from platform.story_events
        where source_type = 'remote_proxy_call'
            and source_id = $1
        "#,
    )
    .bind(&id)
    .fetch_one(&db.pool)
    .await
    .expect("remote proxy story node should query");

    assert_eq!(
        story_row.try_get::<String, _>("id").expect("story id"),
        remote_proxy_call_story_event_id("rproxy_1")
    );
    assert_eq!(
        story_row
            .try_get::<String, _>("source_type")
            .expect("source_type"),
        "remote_proxy_call"
    );
    assert_eq!(
        story_row
            .try_get::<String, _>("source_id")
            .expect("source_id"),
        "rproxy_1"
    );
    assert_eq!(
        story_row
            .try_get::<String, _>("node_type")
            .expect("node_type"),
        "remote_proxy_call"
    );
    assert_eq!(
        story_row.try_get::<String, _>("name").expect("name"),
        "Fetch Contact"
    );
    assert_eq!(
        story_row.try_get::<String, _>("status").expect("status"),
        "completed"
    );
    assert_eq!(
        story_row.try_get::<String, _>("service").expect("service"),
        "remote-crm"
    );
    assert_eq!(
        story_row
            .try_get::<String, _>("correlation_id")
            .expect("story correlation_id"),
        "corr_remote_trace"
    );
    assert_eq!(
        story_row
            .try_get::<Option<String>, _>("causation_id")
            .expect("causation_id")
            .as_deref(),
        Some("httpreq_req_remote_trace")
    );
    assert_eq!(
        story_row
            .try_get::<i64, _>("duration_ms")
            .expect("duration_ms"),
        42
    );
    assert_eq!(
        story_row
            .try_get::<Option<String>, _>("error")
            .expect("error"),
        None
    );
    assert_eq!(
        story_row
            .try_get::<Option<String>, _>("trace_id")
            .expect("story trace_id")
            .as_deref(),
        Some("00000000000000000000000000000011")
    );
    assert_eq!(
        story_row
            .try_get::<Option<String>, _>("span_id")
            .expect("story span_id")
            .as_deref(),
        Some("0000000000000011")
    );
    let metadata = story_row
        .try_get::<serde_json::Value, _>("metadata")
        .expect("metadata");
    assert_eq!(metadata["remote_proxy_call_id"], "rproxy_1");
    assert_eq!(metadata["module_name"], "remote-crm");
    assert_eq!(metadata["method"], "GET");
    assert_eq!(metadata["declared_path"], "/contacts/{id}");
    assert_eq!(metadata["display_name"], "Fetch Contact");
    assert_eq!(metadata["story_title"], "Fetch Contact");
    assert_eq!(metadata["remote_path"], "/contacts/contact_1");
    assert_eq!(metadata["remote_status"], 200);
    assert_eq!(metadata["duration_ms"], 42);
    assert_eq!(metadata["request_id"], "req_remote_trace");
    assert_eq!(metadata["trace_id"], "00000000000000000000000000000011");
    assert_eq!(metadata["span_id"], "0000000000000011");
    assert_eq!(metadata["path_params"]["id"], "contact_1");

    let http_story_count = sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from platform.story_events
        where correlation_id = $1
            and node_type = 'http_request'
        "#,
    )
    .bind("corr_remote_trace")
    .fetch_one(&db.pool)
    .await
    .expect("HTTP story count should query");
    assert_eq!(http_story_count, 0);

    db.cleanup().await;
}
