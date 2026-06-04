use platform_core::{
    CorrelationId, RemoteHttpProxyCallRecord, RequestContext, RequestId, TraceContext,
    apply_migrations, insert_remote_http_proxy_call,
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
    let record = RemoteHttpProxyCallRecord {
        module_name: "remote-crm".to_owned(),
        method: "GET".to_owned(),
        declared_path: "/contacts/{id}".to_owned(),
        remote_path: "/contacts/contact_1".to_owned(),
        capability: Some("remote_crm.contacts.read".to_owned()),
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

    db.cleanup().await;
}
