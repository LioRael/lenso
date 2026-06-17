use app_api::build_router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use chrono::{DateTime, Utc};
use platform_core::{
    AppConfig, AppContext, DatabaseConfig, InMemoryTelemetrySpanProvider, LoggingEventPublisher,
    PLATFORM_MIGRATIONS, TelemetrySpan, apply_migrations,
};
use platform_runtime::RUNTIME_MIGRATIONS;
use platform_testing::TestDatabase;
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

#[tokio::test]
async fn admin_runtime_summary_requires_authentication() {
    let app = auth_only_app();

    let response = app
        .oneshot(admin_get("/admin/runtime/summary"))
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_runtime_summary_rejects_user_actor() {
    let app = auth_only_app();

    let response = app
        .clone()
        .oneshot(
            admin_get("/admin/runtime/summary")
                .with_header("authorization", "Bearer dev-user:user_123"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_runtime_summary_rejects_dev_bearer_outside_local_environment() {
    let app = auth_only_app_for_environment("production");

    let response = app
        .oneshot(
            admin_get("/admin/runtime/summary")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_runtime_outbox_requires_authentication() {
    let app = auth_only_app();

    let response = app
        .oneshot(admin_get("/admin/runtime/outbox"))
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_runtime_outbox_rejects_user_actor() {
    let app = auth_only_app();

    let response = app
        .oneshot(
            admin_get("/admin/runtime/outbox")
                .with_header("authorization", "Bearer dev-user:user_123"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_runtime_outbox_detail_rejects_user_actor() {
    let app = auth_only_app();

    let response = app
        .oneshot(
            admin_get("/admin/runtime/outbox/evt_1")
                .with_header("authorization", "Bearer dev-user:user_123"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_runtime_outbox_retry_requires_authentication() {
    let app = auth_only_app();

    let response = app
        .oneshot(admin_post("/admin/runtime/outbox/evt_1/retry"))
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_runtime_stories_requires_authentication() {
    let app = auth_only_app();

    let response = app
        .oneshot(admin_get("/admin/runtime/stories"))
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_runtime_stories_rejects_user_actor() {
    let app = auth_only_app();

    let response = app
        .oneshot(
            admin_get("/admin/runtime/stories")
                .with_header("authorization", "Bearer dev-user:user_123"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_runtime_story_detail_rejects_user_actor() {
    let app = auth_only_app();

    let response = app
        .oneshot(
            admin_get("/admin/runtime/stories/corr_1")
                .with_header("authorization", "Bearer dev-user:user_123"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_runtime_heatmap_requires_authentication() {
    let app = auth_only_app();

    let response = app
        .oneshot(admin_get("/admin/runtime/heatmap"))
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_runtime_heatmap_rejects_user_actor() {
    let app = auth_only_app();

    let response = app
        .oneshot(
            admin_get("/admin/runtime/heatmap")
                .with_header("authorization", "Bearer dev-user:user_123"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_runtime_function_retry_rejects_user_actor() {
    let app = auth_only_app();

    let response = app
        .oneshot(
            admin_post("/admin/runtime/functions/fnrun_1/retry")
                .with_header("authorization", "Bearer dev-user:user_123"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn service_actor_can_get_runtime_summary() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_outbox_event(&db.pool).await;
    insert_function_run(&db.pool).await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/summary")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["status"], "healthy");
    assert_eq!(body["outbox"]["pending"], 1);
    assert_eq!(body["outbox"]["processing"], 0);
    assert_eq!(body["outbox"]["published"], 0);
    assert_eq!(body["outbox"]["failed"], 0);
    assert_eq!(body["outbox"]["dead"], 0);
    assert!(body["outbox"]["oldest_pending_age_seconds"].is_number());
    assert_eq!(body["functions"]["pending"], 1);
    assert_eq!(body["functions"]["running"], 0);
    assert_eq!(body["functions"]["completed"], 0);
    assert_eq!(body["functions"]["failed"], 0);
    assert_eq!(body["functions"]["dead"], 0);
    assert!(body["functions"]["oldest_pending_age_seconds"].is_number());
    assert_eq!(body["recent_activity"].as_array().unwrap().len(), 2);
    assert_eq!(body["recent_failures"].as_array().unwrap().len(), 0);
    assert!(body["recent_activity"][0].get("payload").is_none());
    assert!(body["recent_activity"][0].get("input_json").is_none());

    db.cleanup().await;
}

#[tokio::test]
async fn runtime_summary_dead_item_makes_status_failing() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_outbox_event_with_status(&db.pool, "evt_dead", "dead", 3, Some("exhausted")).await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/summary")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["status"], "failing");
    assert_eq!(body["outbox"]["dead"], 1);
    assert_eq!(body["recent_failures"].as_array().unwrap().len(), 1);
    assert_eq!(body["recent_failures"][0]["type"], "outbox_event");
    assert_eq!(body["recent_failures"][0]["id"], "evt_dead");
    assert_eq!(body["recent_failures"][0]["last_error"], "exhausted");

    db.cleanup().await;
}

#[tokio::test]
async fn runtime_summary_failed_item_makes_status_degraded() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_function_run_with_status(&db.pool, "fnrun_failed", "failed", 2, Some("timeout")).await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/summary")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["status"], "degraded");
    assert_eq!(body["functions"]["failed"], 1);
    assert_eq!(body["functions"]["dead"], 0);
    assert_eq!(body["recent_failures"].as_array().unwrap().len(), 1);
    assert_eq!(body["recent_failures"][0]["type"], "function_run");
    assert_eq!(body["recent_failures"][0]["id"], "fnrun_failed");
    assert_eq!(body["recent_failures"][0]["last_error"], "timeout");

    db.cleanup().await;
}

#[tokio::test]
async fn runtime_summary_without_failed_or_dead_items_is_healthy() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_outbox_event_with_status(&db.pool, "evt_published", "published", 1, None).await;
    insert_function_run_with_status(&db.pool, "fnrun_completed", "completed", 1, None).await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/summary")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["status"], "healthy");
    assert_eq!(body["outbox"]["published"], 1);
    assert_eq!(body["functions"]["completed"], 1);
    assert_eq!(body["recent_failures"].as_array().unwrap().len(), 0);

    db.cleanup().await;
}

#[tokio::test]
async fn service_actor_can_list_runtime_stories() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_story_outbox_event(&db.pool).await;
    insert_story_function_run(&db.pool).await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/stories?limit=10")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"][0]["title"], "User Registration");
    assert_eq!(body["data"][0]["correlation_id"], "corr_story");
    assert_eq!(body["data"][0]["status"], "dead");
    assert_eq!(body["data"][0]["node_count"], 2);
    assert_eq!(body["data"][0]["error_count"], 1);
    assert_eq!(body["data"][0]["services"][0], "identity");
    assert_eq!(body["data"][0]["services"][1], "notifications");
    assert_eq!(body["data"][0]["pattern"][0], "event");
    assert_eq!(body["data"][0]["pattern"][1], "function");
    assert_eq!(
        body["data"][0]["root_error"],
        "notifications.send_welcome_email.v1: smtp timeout"
    );
    assert_eq!(body["page"]["limit"], 10);
    assert!(body["data"][0].get("payload").is_none());
    assert!(body["data"][0].get("input_json").is_none());

    db.cleanup().await;
}

#[tokio::test]
async fn failed_http_request_creates_request_level_story() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;

    let failed_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/dev/sessions")
                .header("content-type", "application/json")
                .header("x-request-id", "req_validation_story")
                .header("x-correlation-id", "corr_validation_story")
                .body(Body::from(r#"{"user_id":""}"#))
                .expect("request should build"),
        )
        .await
        .expect("request should complete");

    assert_eq!(failed_response.status(), StatusCode::BAD_REQUEST);
    wait_for_story_event(&db.pool, "corr_validation_story").await;

    let list_response = app
        .clone()
        .oneshot(
            admin_get("/admin/runtime/stories?limit=10")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(list_response.status(), StatusCode::OK);
    let list_body = json_body(list_response).await;
    assert_eq!(
        list_body["data"][0]["correlation_id"],
        "corr_validation_story"
    );
    assert_eq!(list_body["data"][0]["title"], "Development Auth Session");
    assert_eq!(list_body["data"][0]["pattern"][0], "http_request");
    assert_eq!(list_body["data"][0]["status"], "failed");
    assert_eq!(list_body["data"][0]["error_count"], 1);

    let detail_response = app
        .oneshot(
            admin_get("/admin/runtime/stories/corr_validation_story")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(detail_response.status(), StatusCode::OK);
    let detail_body = json_body(detail_response).await;
    assert_eq!(detail_body["data"]["nodes"][0]["type"], "http_request");
    assert_eq!(detail_body["data"]["nodes"][0]["status"], "failed");
    assert_eq!(
        detail_body["data"]["nodes"][0]["metadata"]["source_metadata"]["request_id"],
        "req_validation_story"
    );
    assert_eq!(detail_body["data"]["timeline_items"][0]["type"], "failure");
    assert_eq!(
        detail_body["data"]["timeline_items"][0]["related_node_id"],
        detail_body["data"]["nodes"][0]["id"]
    );
    assert_eq!(detail_body["data"]["edges"].as_array().unwrap().len(), 0);

    db.cleanup().await;
}

#[tokio::test]
async fn runtime_story_pagination_uses_story_updated_at_cursor() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_runtime_outbox_fixture(
        &db.pool,
        OutboxFixture {
            id: "evt_old_a",
            aggregate_id: "usr_old_a",
            correlation_id: "corr_old",
            created_at: "2026-05-31T00:00:00Z",
            locked_at: Some("2026-05-31T00:00:01Z"),
            published_at: Some("2026-05-31T00:00:02Z"),
            ..OutboxFixture::default()
        },
    )
    .await;
    insert_runtime_outbox_fixture(
        &db.pool,
        OutboxFixture {
            id: "evt_old_b",
            aggregate_id: "usr_old_b",
            correlation_id: "corr_old",
            created_at: "2026-05-31T00:05:00Z",
            locked_at: Some("2026-05-31T00:05:01Z"),
            published_at: Some("2026-05-31T00:05:02Z"),
            ..OutboxFixture::default()
        },
    )
    .await;
    insert_runtime_outbox_fixture(
        &db.pool,
        OutboxFixture {
            id: "evt_new",
            aggregate_id: "usr_new",
            correlation_id: "corr_new",
            created_at: "2026-05-31T00:10:00Z",
            locked_at: Some("2026-05-31T00:10:01Z"),
            published_at: Some("2026-05-31T00:10:02Z"),
            ..OutboxFixture::default()
        },
    )
    .await;

    let first_response = app
        .clone()
        .oneshot(
            admin_get("/admin/runtime/stories?limit=1")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(first_response.status(), StatusCode::OK);
    let first_body = json_body(first_response).await;
    assert_eq!(first_body["data"][0]["correlation_id"], "corr_new");
    let cursor = first_body["page"]["next_created_before"]
        .as_str()
        .expect("cursor should be present");

    let second_response = app
        .oneshot(
            admin_get(&format!(
                "/admin/runtime/stories?limit=1&created_before={cursor}"
            ))
            .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(second_response.status(), StatusCode::OK);
    let second_body = json_body(second_response).await;
    assert_eq!(second_body["data"][0]["correlation_id"], "corr_old");
    assert_eq!(second_body["data"][0]["node_count"], 2);

    db.cleanup().await;
}

#[tokio::test]
async fn service_actor_can_fetch_runtime_story_detail() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_story_outbox_event(&db.pool).await;
    insert_story_function_run(&db.pool).await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/stories/corr_story")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"]["summary"]["correlation_id"], "corr_story");
    assert_eq!(body["data"]["summary"]["status"], "dead");
    assert_eq!(body["data"]["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(body["data"]["nodes"][0]["id"], "evt_story");
    assert_eq!(body["data"]["nodes"][0]["type"], "event");
    assert_eq!(
        body["data"]["nodes"][0]["name"],
        "identity.user_registered.v1"
    );
    assert_eq!(body["data"]["nodes"][0]["display_name"], "User Registered");
    assert_eq!(body["data"]["nodes"][0]["service"], "identity");
    assert_eq!(body["data"]["nodes"][0]["metadata"]["attempts"], 1);
    assert_eq!(body["data"]["nodes"][1]["id"], "fnrun_story");
    assert_eq!(body["data"]["nodes"][1]["type"], "function");
    assert_eq!(
        body["data"]["nodes"][1]["name"],
        "notifications.send_welcome_email.v1"
    );
    assert_eq!(
        body["data"]["nodes"][1]["display_name"],
        "Send Welcome Email"
    );
    assert_eq!(body["data"]["nodes"][1]["status"], "dead");
    assert_eq!(body["data"]["nodes"][1]["duration_ms"], 80_000);
    assert_eq!(body["data"]["nodes"][1]["error"], "smtp timeout");
    assert_eq!(body["data"]["edges"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["edges"][0]["source"], "evt_story");
    assert_eq!(body["data"]["edges"][0]["target"], "fnrun_story");
    assert_eq!(body["data"]["edges"][0]["type"], "causation");
    assert_eq!(body["data"]["timeline_items"].as_array().unwrap().len(), 2);
    assert_eq!(
        body["data"]["timeline_items"][0]["related_node_id"],
        "evt_story"
    );
    assert_eq!(
        body["data"]["timeline_items"][1]["related_node_id"],
        "fnrun_story"
    );
    assert_eq!(
        body["data"]["nodes"][0]["metadata"]["component"],
        "connected"
    );
    assert_eq!(
        body["data"]["nodes"][1]["metadata"]["component"],
        "connected"
    );
    assert!(body["data"]["nodes"][0].get("payload").is_none());
    assert!(body["data"]["nodes"][1].get("input_json").is_none());

    db.cleanup().await;
}

#[tokio::test]
async fn service_actor_can_fetch_story_technical_operations() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app_with_telemetry(
        &db,
        vec![
            telemetry_span(
                "span_story_function_db",
                "SELECT identity.users",
                json!({
                    "lenso.correlation_id": "corr_story",
                    "lenso.story_id": "corr_story",
                    "lenso.function_run_id": "fnrun_story",
                    "db.system": "postgresql",
                    "db.statement": "select * from identity.users where email = 'a@example.test'",
                }),
            ),
            telemetry_span(
                "span_story_unlinked_http",
                "GET https://api.example.test/resources",
                json!({
                    "lenso.correlation_id": "corr_story",
                    "http.request.method": "GET",
                    "http.request.header.authorization": "Bearer secret",
                }),
            ),
            telemetry_span_at(
                "span_remote_proxy_trace_fallback",
                "remote proxy remote-crm",
                "2026-05-31T00:00:02Z",
                "2026-05-31T00:00:03Z",
                json!({
                    "lenso.correlation_id": "corr_story",
                    "lenso.story_id": "corr_story",
                    "lenso.function_run_id": "fnrun_story",
                    "otel.trace_id": "trace_story_remote_proxy",
                    "http.request.method": "GET",
                }),
            ),
        ],
    )
    .await;
    insert_story_outbox_event(&db.pool).await;
    insert_story_function_run(&db.pool).await;
    insert_remote_proxy_call(
        &db.pool,
        RemoteProxyCallFixture {
            id: "rproxy_story_external",
            correlation_id: "corr_story",
            module_name: "remote-crm",
            success: false,
            occurred_at: "2026-05-31T00:00:02Z",
            error_code: Some("external_dependency_failure"),
            trace_id: "trace_story_remote_proxy",
            span_id: "span_without_matching_telemetry",
        },
    )
    .await;
    insert_execution_log(
        &db.pool,
        "elog_remote_runtime_story",
        "fnrun_story",
        "function_run",
        "remote_crm.sync_contact.v1",
        "2026-05-31T00:00:04Z",
        "info",
        "Function handler operation completed",
        json!({
            "source": "remote_runtime",
            "module_name": "remote-crm",
            "function_name": "remote_crm.sync_contact.v1",
            "remote_path": "/runtime/functions/remote_crm.sync_contact.v1/invoke",
            "request_id": "fnrun_story",
            "trace_id": "trace_remote_runtime",
            "span_id": "span_remote_runtime",
            "duration_ms": 42,
            "success": true
        }),
    )
    .await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/stories/corr_story/technical-operations")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["order"], "started_at_asc");
    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 5);
    let function_db = data
        .iter()
        .find(|item| item["id"] == "span_story_function_db")
        .expect("function db span should be present");
    assert_eq!(function_db["source"], "otel");
    assert_eq!(function_db["category"], "db");
    assert_eq!(function_db["related_node_id"], "fnrun_story");
    assert_eq!(
        function_db["attributes"]["lenso.function_run_id"],
        "fnrun_story"
    );
    assert!(function_db["attributes"].get("db.statement").is_none());
    let unlinked_http = data
        .iter()
        .find(|item| item["id"] == "span_story_unlinked_http")
        .expect("unlinked http span should be present");
    assert_eq!(unlinked_http["category"], "http");
    assert!(unlinked_http["related_node_id"].is_null());
    assert!(
        unlinked_http["attributes"]
            .get("http.request.header.authorization")
            .is_none()
    );
    let remote_proxy = data
        .iter()
        .find(|item| item["id"] == "remote_proxy:rproxy_story_external")
        .expect("remote proxy operation should be present");
    assert_eq!(remote_proxy["source"], "remote_proxy");
    assert_eq!(remote_proxy["category"], "external");
    assert_eq!(remote_proxy["status"], "error");
    assert_eq!(remote_proxy["name"], "remote-crm GET /contacts/{id}");
    assert_eq!(remote_proxy["attributes"]["module_name"], "remote-crm");
    assert_eq!(
        remote_proxy["attributes"]["error_code"],
        "external_dependency_failure"
    );
    assert_eq!(remote_proxy["related_node_id"], "fnrun_story");
    let remote_runtime = data
        .iter()
        .find(|item| item["id"] == "remote_runtime:elog_remote_runtime_story")
        .expect("remote runtime operation should be present");
    assert_eq!(remote_runtime["source"], "remote_runtime");
    assert_eq!(remote_runtime["category"], "external");
    assert_eq!(remote_runtime["status"], "ok");
    assert_eq!(
        remote_runtime["name"],
        "remote-crm remote_crm.sync_contact.v1"
    );
    assert_eq!(remote_runtime["duration_ms"], 42);
    assert_eq!(remote_runtime["related_node_id"], "fnrun_story");
    assert_eq!(remote_runtime["attributes"]["request_id"], "fnrun_story");

    db.cleanup().await;
}

#[tokio::test]
async fn service_actor_can_fetch_execution_technical_operations() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app_with_telemetry(
        &db,
        vec![
            telemetry_span(
                "span_execution_function",
                "function_run",
                json!({
                    "lenso.correlation_id": "corr_story",
                    "lenso.function_run_id": "fnrun_story",
                    "lenso.execution.kind": "function_run",
                }),
            ),
            telemetry_span(
                "span_other_function",
                "function_run",
                json!({
                    "lenso.correlation_id": "corr_story",
                    "lenso.function_run_id": "fnrun_other",
                    "lenso.execution.kind": "function_run",
                }),
            ),
        ],
    )
    .await;
    insert_story_outbox_event(&db.pool).await;
    insert_story_function_run(&db.pool).await;
    insert_execution_log(
        &db.pool,
        "elog_remote_runtime_execution",
        "fnrun_story",
        "function_run",
        "remote_crm.sync_contact.v1",
        "2026-05-31T00:00:04Z",
        "error",
        "Function handler operation failed",
        json!({
            "source": "remote_runtime",
            "module_name": "remote-crm",
            "function_name": "remote_crm.sync_contact.v1",
            "remote_path": "/runtime/functions/remote_crm.sync_contact.v1/invoke",
            "request_id": "fnrun_story",
            "duration_ms": 77,
            "success": false,
            "error_code": "external_dependency_failure",
            "retryable": true
        }),
    )
    .await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/executions/fnrun_story/technical-operations")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 2);
    let span = data
        .iter()
        .find(|item| item["id"] == "span_execution_function")
        .expect("execution span should be present");
    assert_eq!(span["related_node_id"], "fnrun_story");
    assert_eq!(span["category"], "runtime");
    let remote_runtime = data
        .iter()
        .find(|item| item["id"] == "remote_runtime:elog_remote_runtime_execution")
        .expect("remote runtime operation should be present");
    assert_eq!(remote_runtime["source"], "remote_runtime");
    assert_eq!(remote_runtime["status"], "error");
    assert_eq!(remote_runtime["category"], "external");
    assert_eq!(remote_runtime["duration_ms"], 77);
    assert_eq!(
        remote_runtime["attributes"]["error_code"],
        "external_dependency_failure"
    );

    db.cleanup().await;
}

#[tokio::test]
async fn service_actor_can_fetch_execution_logs() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_story_function_run(&db.pool).await;
    insert_execution_log(
        &db.pool,
        "elog_started",
        "fnrun_story",
        "function_run",
        "notifications.send_welcome_email.v1",
        "2026-05-31T00:00:01Z",
        "info",
        "Function run started",
        json!({ "attempt": 1, "worker_id": "worker-a" }),
    )
    .await;
    insert_execution_log(
        &db.pool,
        "elog_completed",
        "fnrun_story",
        "function_run",
        "notifications.send_welcome_email.v1",
        "2026-05-31T00:00:03Z",
        "info",
        "Function run completed",
        json!({ "attempt": 1 }),
    )
    .await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/executions/fnrun_story/logs")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["order"], "occurred_at_asc");
    assert_eq!(body["data"].as_array().unwrap().len(), 2);
    assert_eq!(body["data"][0]["id"], "elog_started");
    assert_eq!(body["data"][0]["node_id"], "fnrun_story");
    assert_eq!(body["data"][0]["node_type"], "function_run");
    assert_eq!(body["data"][0]["body"], "Function run started");
    assert_eq!(body["data"][0]["attributes"]["worker_id"], "worker-a");
    assert_eq!(body["data"][0]["trace_id"], "trace_1");
    assert_eq!(body["data"][0]["span_id"], "span_1");
    assert_eq!(body["data"][1]["id"], "elog_completed");

    db.cleanup().await;
}

#[tokio::test]
async fn outbox_execution_logs_include_enqueued_projection() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_story_outbox_event(&db.pool).await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/executions/evt_story/logs")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"][0]["id"], "elog_outbox_enqueued_evt_story");
    assert_eq!(body["data"][0]["body"], "Outbox event enqueued");
    assert_eq!(body["data"][0]["node_type"], "outbox_event");
    assert_eq!(
        body["data"][0]["attributes"]["event_name"],
        "identity.user_registered.v1"
    );

    db.cleanup().await;
}

#[tokio::test]
async fn execution_logs_for_unknown_node_return_not_found() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/executions/missing/logs")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    db.cleanup().await;
}

#[tokio::test]
async fn service_actor_can_fetch_function_execution_payload() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_runtime_function_fixture(
        &db.pool,
        FunctionFixture {
            id: "fnrun_payload",
            correlation_id: "corr_payload",
            input_json: json!({
                "user_id": "usr_1",
                "email": "ada@example.com",
                "nested": {
                    "access_token": "secret-token",
                    "safe": true
                }
            }),
            ..FunctionFixture::default()
        },
    )
    .await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/executions/fnrun_payload/payload")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"]["node_id"], "fnrun_payload");
    assert_eq!(body["data"]["node_type"], "function");
    assert_eq!(body["data"]["input"]["user_id"], "usr_1");
    assert_eq!(body["data"]["input"]["email"], "[redacted]");
    assert_eq!(
        body["data"]["input"]["nested"]["access_token"],
        "[redacted]"
    );
    assert_eq!(body["data"]["input"]["nested"]["safe"], true);
    assert!(body["data"]["output"].is_null());
    assert_eq!(
        body["data"]["metadata"]["function_name"],
        "notifications.send_welcome_email.v1"
    );
    let redacted_fields = body["data"]["redacted_fields"].as_array().unwrap();
    assert!(redacted_fields.contains(&json!("input.email")));
    assert!(redacted_fields.contains(&json!("input.nested.access_token")));

    db.cleanup().await;
}

#[tokio::test]
async fn service_actor_can_fetch_outbox_execution_payload() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_runtime_outbox_fixture(
        &db.pool,
        OutboxFixture {
            id: "evt_payload",
            aggregate_id: "usr_payload",
            correlation_id: "corr_payload",
            headers: json!({
                "actor": {
                    "kind": "service",
                    "email": "ops@example.com"
                },
                "authorization": "Bearer secret"
            }),
            ..OutboxFixture::default()
        },
    )
    .await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/executions/evt_payload/payload")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"]["node_id"], "evt_payload");
    assert_eq!(body["data"]["node_type"], "event");
    assert_eq!(body["data"]["input"]["aggregate_id"], "usr_payload");
    assert_eq!(
        body["data"]["metadata"]["headers"]["authorization"],
        "[redacted]"
    );
    assert_eq!(
        body["data"]["metadata"]["headers"]["actor"]["email"],
        "[redacted]"
    );
    let redacted_fields = body["data"]["redacted_fields"].as_array().unwrap();
    assert!(redacted_fields.contains(&json!("metadata.actor.email")));
    assert!(redacted_fields.contains(&json!("metadata.headers.actor.email")));
    assert!(redacted_fields.contains(&json!("metadata.headers.authorization")));

    db.cleanup().await;
}

#[tokio::test]
async fn unknown_execution_payload_returns_not_found() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/executions/missing/payload")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    db.cleanup().await;
}

#[tokio::test]
async fn runtime_story_and_technical_operations_round_trip_through_admin_apis() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app_with_telemetry(
        &db,
        vec![
            telemetry_span_at(
                "span_e2e_outbox_publish",
                "outbox publish ResourceVersionPublished",
                "2026-05-31T10:00:00.050Z",
                "2026-05-31T10:00:00.250Z",
                json!({
                    "lenso.correlation_id": "corr_e2e_runtime_telemetry",
                    "lenso.story_id": "corr_e2e_runtime_telemetry",
                    "lenso.outbox_event_id": "evt_e2e_resource_published",
                    "lenso.execution.kind": "outbox_event",
                    "lenso.execution.name": "resources.resource_version_published.v1",
                }),
            ),
            telemetry_span_at(
                "span_e2e_function_run",
                "function GenerateSearchIndex",
                "2026-05-31T10:00:01.000Z",
                "2026-05-31T10:00:04.000Z",
                json!({
                    "lenso.correlation_id": "corr_e2e_runtime_telemetry",
                    "lenso.story_id": "corr_e2e_runtime_telemetry",
                    "lenso.function_run_id": "fnrun_e2e_generate_search_index",
                    "lenso.execution.kind": "function_run",
                    "lenso.execution.name": "search.generate_index.v1",
                    "db.system": "postgresql",
                    "db.statement": "insert into search.index_entries values (...)",
                }),
            ),
            telemetry_span_at(
                "span_e2e_story_level_http",
                "POST external webhook",
                "2026-05-31T10:00:02.000Z",
                "2026-05-31T10:00:02.500Z",
                json!({
                    "lenso.correlation_id": "corr_e2e_runtime_telemetry",
                    "lenso.story_id": "corr_e2e_runtime_telemetry",
                    "http.request.method": "POST",
                    "http.request.header.authorization": "Bearer secret",
                }),
            ),
        ],
    )
    .await;
    insert_runtime_outbox_fixture(
        &db.pool,
        OutboxFixture {
            id: "evt_e2e_resource_published",
            event_name: "resources.resource_version_published.v1",
            source_module: "resources",
            aggregate_id: "res_1",
            correlation_id: "corr_e2e_runtime_telemetry",
            causation_id: Some("req_e2e_publish_resource"),
            created_at: "2026-05-31T10:00:00Z",
            locked_at: Some("2026-05-31T10:00:00.050Z"),
            published_at: Some("2026-05-31T10:00:00.250Z"),
            headers: json!({
                "_lenso_runtime": {
                    "correlation_id": "corr_e2e_runtime_telemetry",
                    "traceparent": "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
                }
            }),
            ..OutboxFixture::default()
        },
    )
    .await;
    insert_runtime_function_fixture(
        &db.pool,
        FunctionFixture {
            id: "fnrun_e2e_generate_search_index",
            function_name: "search.generate_index.v1",
            correlation_id: "corr_e2e_runtime_telemetry",
            created_at: "2026-05-31T10:00:01Z",
            started_at: Some("2026-05-31T10:00:01Z"),
            completed_at: Some("2026-05-31T10:00:04Z"),
            input_json: json!({
                "resource_id": "res_1",
                "outbox_event_id": "evt_e2e_resource_published",
                "_lenso_runtime": {
                    "correlation_id": "corr_e2e_runtime_telemetry",
                    "traceparent": "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
                }
            }),
            ..FunctionFixture::default()
        },
    )
    .await;

    let list_response = app
        .clone()
        .oneshot(
            admin_get("/admin/runtime/stories?limit=10")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(list_response.status(), StatusCode::OK);
    let list_body = json_body(list_response).await;
    assert_eq!(
        list_body["data"][0]["correlation_id"],
        "corr_e2e_runtime_telemetry"
    );
    assert_eq!(list_body["data"][0]["node_count"], 2);
    assert_eq!(list_body["data"][0]["status"], "completed");

    let detail_response = app
        .clone()
        .oneshot(
            admin_get("/admin/runtime/stories/corr_e2e_runtime_telemetry")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(detail_response.status(), StatusCode::OK);
    let detail_body = json_body(detail_response).await;
    assert_eq!(
        detail_body["data"]["summary"]["correlation_id"],
        "corr_e2e_runtime_telemetry"
    );
    assert_eq!(detail_body["data"]["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(
        detail_body["data"]["edges"][0]["source"],
        "evt_e2e_resource_published"
    );
    assert_eq!(
        detail_body["data"]["edges"][0]["target"],
        "fnrun_e2e_generate_search_index"
    );
    assert_eq!(
        detail_body["data"]["timeline_items"][0]["related_node_id"],
        "evt_e2e_resource_published"
    );
    assert_eq!(
        detail_body["data"]["timeline_items"][1]["related_node_id"],
        "fnrun_e2e_generate_search_index"
    );

    let story_ops_response = app
        .clone()
        .oneshot(
            admin_get("/admin/runtime/stories/corr_e2e_runtime_telemetry/technical-operations")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(story_ops_response.status(), StatusCode::OK);
    let story_ops_body = json_body(story_ops_response).await;
    let story_ops = story_ops_body["data"].as_array().unwrap();
    assert_eq!(story_ops.len(), 3);
    assert!(story_ops.iter().any(|operation| {
        operation["id"] == "span_e2e_outbox_publish"
            && operation["related_node_id"] == "evt_e2e_resource_published"
    }));
    assert!(story_ops.iter().any(|operation| {
        operation["id"] == "span_e2e_function_run"
            && operation["related_node_id"] == "fnrun_e2e_generate_search_index"
    }));
    assert!(story_ops.iter().any(|operation| {
        operation["id"] == "span_e2e_story_level_http"
            && operation["related_node_id"].is_null()
            && operation["attributes"]
                .get("http.request.header.authorization")
                .is_none()
    }));

    let execution_ops_response = app
        .oneshot(
            admin_get(
                "/admin/runtime/executions/fnrun_e2e_generate_search_index/technical-operations",
            )
            .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(execution_ops_response.status(), StatusCode::OK);
    let execution_ops_body = json_body(execution_ops_response).await;
    assert_eq!(execution_ops_body["data"].as_array().unwrap().len(), 1);
    assert_eq!(execution_ops_body["data"][0]["id"], "span_e2e_function_run");
    assert_eq!(
        execution_ops_body["data"][0]["related_node_id"],
        "fnrun_e2e_generate_search_index"
    );
    assert!(
        execution_ops_body["data"][0]["attributes"]
            .get("db.statement")
            .is_none()
    );

    db.cleanup().await;
}

#[tokio::test]
async fn runtime_story_does_not_guess_edges_for_disconnected_work() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_runtime_outbox_fixture(
        &db.pool,
        OutboxFixture {
            id: "evt_unlinked",
            correlation_id: "corr_disconnected",
            created_at: "2026-05-31T00:00:00Z",
            causation_id: None,
            ..OutboxFixture::default()
        },
    )
    .await;
    insert_runtime_function_fixture(
        &db.pool,
        FunctionFixture {
            id: "fnrun_unlinked",
            correlation_id: "corr_disconnected",
            created_at: "2026-05-31T00:00:30Z",
            ..FunctionFixture::default()
        },
    )
    .await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/stories/corr_disconnected")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"]["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(body["data"]["edges"].as_array().unwrap().len(), 0);
    assert_eq!(body["data"]["nodes"][0]["metadata"]["component"], "orphan");
    assert_eq!(body["data"]["nodes"][1]["metadata"]["component"], "orphan");

    db.cleanup().await;
}

#[tokio::test]
async fn runtime_story_can_contain_only_outbox_events() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_runtime_outbox_fixture(
        &db.pool,
        OutboxFixture {
            id: "evt_only_a",
            correlation_id: "corr_outbox_only",
            created_at: "2026-05-31T00:00:00Z",
            ..OutboxFixture::default()
        },
    )
    .await;
    insert_runtime_outbox_fixture(
        &db.pool,
        OutboxFixture {
            id: "evt_only_b",
            aggregate_id: "usr_fixture_2",
            correlation_id: "corr_outbox_only",
            causation_id: Some("evt_only_a"),
            created_at: "2026-05-31T00:00:20Z",
            ..OutboxFixture::default()
        },
    )
    .await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/stories/corr_outbox_only")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"]["summary"]["pattern"][0], "event");
    assert_eq!(body["data"]["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(body["data"]["edges"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["edges"][0]["source"], "evt_only_a");
    assert_eq!(body["data"]["edges"][0]["target"], "evt_only_b");

    db.cleanup().await;
}

#[tokio::test]
async fn runtime_story_can_contain_only_function_runs() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_runtime_function_fixture(
        &db.pool,
        FunctionFixture {
            id: "fnrun_only_a",
            correlation_id: "corr_functions_only",
            created_at: "2026-05-31T00:00:00Z",
            ..FunctionFixture::default()
        },
    )
    .await;
    insert_runtime_function_fixture(
        &db.pool,
        FunctionFixture {
            id: "fnrun_only_b",
            correlation_id: "corr_functions_only",
            created_at: "2026-05-31T00:00:10Z",
            input_json: json!({ "function_run_id": "fnrun_only_a" }),
            ..FunctionFixture::default()
        },
    )
    .await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/stories/corr_functions_only")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"]["summary"]["pattern"][0], "function");
    assert_eq!(body["data"]["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(body["data"]["edges"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["edges"][0]["source"], "fnrun_only_a");
    assert_eq!(body["data"]["edges"][0]["target"], "fnrun_only_b");

    db.cleanup().await;
}

#[tokio::test]
async fn unknown_runtime_story_returns_not_found() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/stories/missing")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = json_body(response).await;
    assert_eq!(body["error"]["code"], "not_found");

    db.cleanup().await;
}

#[tokio::test]
async fn service_actor_can_fetch_runtime_heatmap() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_heatmap_outbox_events(&db.pool).await;
    insert_heatmap_function_runs(&db.pool).await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/heatmap?bucket_seconds=60&limit=20")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["bucket_seconds"], 60);
    assert_eq!(body["order"], "bucket_start_desc");
    assert_eq!(body["data"].as_array().unwrap().len(), 3);
    assert_eq!(body["data"][0]["service"], "notifications");
    assert_eq!(body["data"][0]["node_type"], "function");
    assert_eq!(body["data"][0]["total_count"], 1);
    assert_eq!(body["data"][0]["max_duration_ms"], 10_000);
    assert_eq!(body["data"][1]["service"], "identity");
    assert_eq!(body["data"][1]["node_type"], "event");
    assert_eq!(body["data"][1]["total_count"], 2);
    assert_eq!(body["data"][1]["error_count"], 1);
    assert_eq!(body["data"][1]["retry_count"], 1);
    assert_eq!(body["data"][1]["dead_count"], 0);
    assert_eq!(body["data"][2]["service"], "notifications");
    assert_eq!(body["data"][2]["node_type"], "function");
    assert_eq!(body["data"][2]["total_count"], 2);
    assert_eq!(body["data"][2]["error_count"], 1);
    assert_eq!(body["data"][2]["retry_count"], 1);
    assert_eq!(body["data"][2]["dead_count"], 1);
    assert_eq!(body["data"][2]["max_duration_ms"], 80_000);
    assert!(body["data"][0]["bucket_start"].is_string());
    assert!(body["data"][0]["bucket_end"].is_string());
    assert!(body["data"][0].get("payload").is_none());
    assert!(body["data"][0].get("input_json").is_none());

    db.cleanup().await;
}

#[tokio::test]
async fn runtime_heatmap_filters_by_time_status_and_names() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_heatmap_outbox_events(&db.pool).await;
    insert_heatmap_function_runs(&db.pool).await;

    let response = app
        .clone()
        .oneshot(
            admin_get(
                "/admin/runtime/heatmap?from=2026-05-31T00:00:00Z&to=2026-05-31T00:01:00Z&bucket_seconds=60&status=failed&event_name=identity.user_registered.v1&limit=20",
            )
            .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"][0]["service"], "identity");
    assert_eq!(body["data"][0]["node_type"], "event");
    assert_eq!(body["data"][0]["total_count"], 1);
    assert_eq!(body["data"][0]["error_count"], 1);
    assert_eq!(body["data"][0]["retry_count"], 1);

    let function_response = app
        .oneshot(
            admin_get(
                "/admin/runtime/heatmap?from=2026-05-31T00:00:00Z&to=2026-05-31T00:01:00Z&bucket_seconds=60&function_name=notifications.send_welcome_email.v1&limit=20",
            )
            .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(function_response.status(), StatusCode::OK);
    let function_body = json_body(function_response).await;
    assert_eq!(function_body["data"].as_array().unwrap().len(), 1);
    assert_eq!(function_body["data"][0]["service"], "notifications");
    assert_eq!(function_body["data"][0]["node_type"], "function");
    assert_eq!(function_body["data"][0]["total_count"], 2);
    assert_eq!(function_body["data"][0]["retry_count"], 1);

    db.cleanup().await;
}

#[tokio::test]
async fn runtime_heatmap_without_runtime_work_returns_empty_data() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/heatmap")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 0);
    assert_eq!(body["bucket_seconds"], 300);

    db.cleanup().await;
}

#[tokio::test]
async fn service_actor_can_fetch_story_runtime_heatmap() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_heatmap_story_events(&db.pool).await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/stories/corr_heatmap_1/heatmap?bucket_seconds=60&limit=20")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["bucket_seconds"], 60);
    assert_eq!(body["data"].as_array().unwrap().len(), 2);
    assert_eq!(body["data"][0]["service"], "api");
    assert_eq!(body["data"][0]["node_type"], "http_request");
    assert_eq!(body["data"][0]["total_count"], 1);
    assert_eq!(body["data"][0]["error_count"], 0);
    assert_eq!(body["data"][0]["max_duration_ms"], 120);
    assert_eq!(body["data"][1]["service"], "notifications");
    assert_eq!(body["data"][1]["node_type"], "function");
    assert_eq!(body["data"][1]["total_count"], 1);
    assert_eq!(body["data"][1]["error_count"], 1);
    assert_eq!(body["data"][1]["dead_count"], 1);
    assert_eq!(body["data"][1]["max_duration_ms"], 80_000);

    db.cleanup().await;
}

#[tokio::test]
async fn unknown_runtime_story_heatmap_returns_not_found() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/stories/missing/heatmap")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = json_body(response).await;
    assert_eq!(body["error"]["code"], "not_found");

    db.cleanup().await;
}

#[tokio::test]
async fn service_actor_can_list_outbox() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_outbox_event(&db.pool).await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/outbox?status=pending&event_name=identity.user_registered.v1&limit=10")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"][0]["id"], "evt_1");
    assert_eq!(body["data"][0]["event_name"], "identity.user_registered.v1");
    assert_eq!(body["data"][0]["status"], "pending");
    assert_eq!(body["data"][0]["attempts"], 0);
    assert_eq!(body["data"][0]["max_attempts"], 3);
    assert_eq!(body["data"][0]["locked_by"], Value::Null);
    assert_eq!(body["data"][0]["published_at"], Value::Null);
    assert_eq!(body["data"][0]["last_error"], Value::Null);
    assert_eq!(body["data"][0]["correlation_id"], "corr_1");
    assert!(body["data"][0].get("payload").is_none());
    assert!(body["data"][0].get("headers").is_none());
    assert!(body["data"][0]["available_at"].is_string());
    assert!(body["data"][0]["created_at"].is_string());
    assert_eq!(body["page"]["limit"], 10);
    assert!(body["page"]["next_created_before"].is_string());

    db.cleanup().await;
}

#[tokio::test]
async fn service_actor_can_list_remote_proxy_calls() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_remote_proxy_call(
        &db.pool,
        remote_proxy_fixture(
            "rproxy_old_success",
            "corr_remote_proxy",
            "remote-crm",
            true,
            "2026-05-31T00:00:00Z",
            None,
        ),
    )
    .await;
    insert_remote_proxy_call(
        &db.pool,
        remote_proxy_fixture(
            "rproxy_recent_failure",
            "corr_remote_proxy",
            "remote-crm",
            false,
            "2026-05-31T00:01:00Z",
            Some("external_dependency_failure"),
        ),
    )
    .await;
    insert_remote_proxy_call(
        &db.pool,
        remote_proxy_fixture(
            "rproxy_other_failure",
            "corr_other_remote_proxy",
            "billing-remote",
            false,
            "2026-05-31T00:02:00Z",
            Some("not_found"),
        ),
    )
    .await;

    let response = app
        .clone()
        .oneshot(
            admin_get(
                "/admin/runtime/remote-proxy-calls?module_name=remote-crm&success=false&limit=10",
            )
            .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"][0]["id"], "rproxy_recent_failure");
    assert_eq!(body["data"][0]["module_name"], "remote-crm");
    assert_eq!(body["data"][0]["method"], "GET");
    assert_eq!(body["data"][0]["declared_path"], "/contacts/{id}");
    assert_eq!(body["data"][0]["remote_path"], "/contacts/contact_1");
    assert_eq!(body["data"][0]["capability"], "remote_crm.contacts.read");
    assert_eq!(body["data"][0]["remote_status"], 502);
    assert_eq!(body["data"][0]["duration_ms"], 125);
    assert_eq!(body["data"][0]["success"], false);
    assert_eq!(body["data"][0]["error_code"], "external_dependency_failure");
    assert_eq!(body["data"][0]["retryable"], true);
    assert_eq!(body["data"][0]["request_id"], "req_rproxy_recent_failure");
    assert_eq!(body["data"][0]["correlation_id"], "corr_remote_proxy");
    assert_eq!(body["data"][0]["trace_id"], "trace_remote_proxy");
    assert_eq!(body["data"][0]["span_id"], "span_remote_proxy");
    assert_eq!(body["data"][0]["path_params"]["id"], "contact_1");
    assert_eq!(
        body["data"][0]["error_details"][0]["field"],
        "remote_module"
    );
    assert_eq!(body["page"]["limit"], 10);
    assert!(body["page"]["next_created_before"].is_string());

    let paged_response = app
        .clone()
        .oneshot(
            admin_get(
                "/admin/runtime/remote-proxy-calls?correlation_id=corr_remote_proxy&limit=10&created_before=2026-05-31T00:01:30Z",
            )
            .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("paged request should complete");
    assert_eq!(paged_response.status(), StatusCode::OK);
    let paged = json_body(paged_response).await;
    assert_eq!(paged["data"].as_array().unwrap().len(), 2);
    assert_eq!(paged["data"][0]["id"], "rproxy_recent_failure");
    assert_eq!(paged["data"][1]["id"], "rproxy_old_success");

    let story_response = app
        .oneshot(
            admin_get(
                "/admin/runtime/remote-proxy-calls?correlation_id=corr_remote_proxy&limit=10",
            )
            .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("story-scoped request should complete");
    assert_eq!(story_response.status(), StatusCode::OK);
    let story_calls = json_body(story_response).await;
    assert_eq!(story_calls["data"].as_array().unwrap().len(), 2);
    assert_eq!(story_calls["data"][0]["id"], "rproxy_recent_failure");
    assert_eq!(story_calls["data"][1]["id"], "rproxy_old_success");
    assert_eq!(
        story_calls["data"][0]["correlation_id"],
        "corr_remote_proxy"
    );

    db.cleanup().await;
}

#[tokio::test]
async fn service_actor_can_fetch_outbox_detail() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_outbox_event(&db.pool).await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/outbox/evt_1")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"]["id"], "evt_1");
    assert_eq!(body["data"]["event_name"], "identity.user_registered.v1");
    assert_eq!(body["data"]["payload"]["user_id"], "usr_1");
    assert_eq!(body["data"]["actor"]["kind"], "service");
    assert_eq!(body["data"]["actor"]["service_id"], "api");
    assert_eq!(body["data"]["trace"]["trace_id"], "trace_1");
    assert_eq!(body["data"]["correlation_id"], "corr_1");
    assert_eq!(body["data"]["causation_id"], "req_1");
    assert_eq!(body["data"]["status"], "pending");
    assert_eq!(body["data"]["attempts"], 0);
    assert_eq!(body["data"]["max_attempts"], 3);
    assert!(body["data"]["occurred_at"].is_string());
    assert!(body["data"]["created_at"].is_string());

    db.cleanup().await;
}

#[tokio::test]
async fn unknown_outbox_detail_returns_not_found() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/outbox/missing")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = json_body(response).await;
    assert_eq!(body["error"]["code"], "not_found");

    db.cleanup().await;
}

#[tokio::test]
async fn service_actor_can_retry_failed_outbox_event() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_outbox_event_with_status(&db.pool, "evt_failed", "failed", 2, Some("boom")).await;

    let response = app
        .oneshot(
            admin_post("/admin/runtime/outbox/evt_failed/retry")
                .with_header("authorization", "Bearer dev-service:admin")
                .with_header("x-correlation-id", "corr-admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"]["id"], "evt_failed");
    assert_eq!(body["data"]["status"], "pending");
    assert_eq!(body["data"]["attempts"], 2);
    assert_eq!(body["data"]["locked_by"], Value::Null);
    assert_eq!(body["data"]["last_error"], Value::Null);

    let row = outbox_retry_state(&db.pool, "evt_failed").await;
    assert_eq!(row.status, "pending");
    assert_eq!(row.attempts, 2);
    assert!(row.locked_at.is_none());
    assert!(row.locked_by.is_none());
    assert!(row.last_error.is_none());

    db.cleanup().await;
}

#[tokio::test]
async fn retry_pending_outbox_event_returns_conflict() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_outbox_event_with_status(&db.pool, "evt_pending", "pending", 0, None).await;

    let response = app
        .oneshot(
            admin_post("/admin/runtime/outbox/evt_pending/retry")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = json_body(response).await;
    assert_eq!(body["error"]["code"], "conflict");

    db.cleanup().await;
}

#[tokio::test]
async fn service_actor_can_list_function_runs() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_function_run(&db.pool).await;

    let response = app
        .oneshot(
            admin_get(
                "/admin/runtime/functions?status=pending&function_name=notifications.send_welcome_email.v1&limit=10",
            )
            .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"][0]["id"], "fnrun_1");
    assert_eq!(
        body["data"][0]["function_name"],
        "notifications.send_welcome_email.v1"
    );
    assert_eq!(body["data"][0]["status"], "pending");
    assert_eq!(body["data"][0]["attempts"], 0);
    assert_eq!(body["data"][0]["max_attempts"], 3);
    assert_eq!(body["data"][0]["locked_by"], Value::Null);
    assert_eq!(body["data"][0]["started_at"], Value::Null);
    assert_eq!(body["data"][0]["completed_at"], Value::Null);
    assert_eq!(body["data"][0]["last_error"], Value::Null);
    assert_eq!(body["data"][0]["correlation_id"], "corr_1");
    assert_welcome_email_runtime_declaration(&body["data"][0]["runtime_declaration"]);
    assert!(body["data"][0].get("input_json").is_none());
    assert!(body["data"][0].get("actor").is_none());
    assert!(body["data"][0]["available_at"].is_string());
    assert!(body["data"][0]["created_at"].is_string());
    assert_eq!(body["page"]["limit"], 10);
    assert!(body["page"]["next_created_before"].is_string());

    db.cleanup().await;
}

#[tokio::test]
async fn service_actor_can_retry_dead_function_run() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_function_run_with_status(&db.pool, "fnrun_dead", "dead", 3, Some("exhausted")).await;

    let response = app
        .oneshot(
            admin_post("/admin/runtime/functions/fnrun_dead/retry")
                .with_header("authorization", "Bearer dev-service:admin")
                .with_header("x-correlation-id", "corr-admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"]["id"], "fnrun_dead");
    assert_eq!(body["data"]["status"], "pending");
    assert_eq!(body["data"]["attempts"], 3);
    assert_eq!(body["data"]["locked_by"], Value::Null);
    assert_eq!(body["data"]["last_error"], Value::Null);
    assert_welcome_email_runtime_declaration(&body["data"]["runtime_declaration"]);

    let row = function_retry_state(&db.pool, "fnrun_dead").await;
    assert_eq!(row.status, "pending");
    assert_eq!(row.attempts, 3);
    assert!(row.locked_at.is_none());
    assert!(row.locked_by.is_none());
    assert!(row.last_error.is_none());

    db.cleanup().await;
}

#[tokio::test]
async fn retry_unknown_function_run_returns_not_found() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;

    let response = app
        .oneshot(
            admin_post("/admin/runtime/functions/missing/retry")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = json_body(response).await;
    assert_eq!(body["error"]["code"], "not_found");

    db.cleanup().await;
}

#[tokio::test]
async fn service_actor_can_get_function_run_by_id() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let app = test_app(&db).await;
    insert_function_run(&db.pool).await;

    let response = app
        .oneshot(
            admin_get("/admin/runtime/functions/fnrun_1")
                .with_header("authorization", "Bearer dev-service:admin"),
        )
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["data"]["id"], "fnrun_1");
    assert_eq!(
        body["data"]["function_name"],
        "notifications.send_welcome_email.v1"
    );
    assert_eq!(body["data"]["status"], "pending");
    assert_eq!(body["data"]["input_json"]["user_id"], "usr_1");
    assert_eq!(body["data"]["actor"]["kind"], "system");
    assert_eq!(body["data"]["correlation_id"], "corr_1");
    assert_eq!(body["data"]["attempts"], 0);
    assert_eq!(body["data"]["max_attempts"], 3);
    assert_welcome_email_runtime_declaration(&body["data"]["runtime_declaration"]);
    assert!(body["data"]["available_at"].is_string());
    assert!(body["data"]["created_at"].is_string());

    db.cleanup().await;
}

#[tokio::test]
async fn admin_runtime_openapi_contract_is_present() {
    let document = app_api::openapi_document();
    let value = serde_json::to_value(&document).expect("OpenAPI document should serialize");

    assert_eq!(
        value["paths"]["/admin/runtime/summary"]["get"]["operationId"],
        "admin_runtime_get_summary"
    );
    assert_eq!(
        value["paths"]["/admin/runtime/outbox"]["get"]["operationId"],
        "admin_runtime_list_outbox"
    );
    assert_eq!(
        value["paths"]["/admin/runtime/outbox/{id}"]["get"]["operationId"],
        "admin_runtime_get_outbox"
    );
    assert_eq!(
        value["paths"]["/admin/runtime/functions"]["get"]["operationId"],
        "admin_runtime_list_function_runs"
    );
    assert_eq!(
        value["paths"]["/admin/runtime/functions/{id}"]["get"]["operationId"],
        "admin_runtime_get_function_run"
    );
    assert_eq!(
        value["paths"]["/admin/runtime/remote-proxy-calls"]["get"]["operationId"],
        "admin_runtime_list_remote_proxy_calls"
    );
    assert!(
        value["paths"]
            .get("/admin/runtime/timeline/{correlation_id}")
            .is_none()
    );
    assert_eq!(
        value["paths"]["/admin/runtime/stories"]["get"]["operationId"],
        "admin_runtime_list_stories"
    );
    assert_eq!(
        value["paths"]["/admin/runtime/stories/{correlation_id}"]["get"]["operationId"],
        "admin_runtime_get_story"
    );
    assert_eq!(
        value["paths"]["/admin/runtime/stories/{correlation_id}/heatmap"]["get"]["operationId"],
        "admin_runtime_get_story_heatmap"
    );
    assert_eq!(
        value["paths"]["/admin/runtime/stories/{correlation_id}/technical-operations"]["get"]["operationId"],
        "admin_runtime_get_story_technical_operations"
    );
    assert_eq!(
        value["paths"]["/admin/runtime/executions/{node_id}/technical-operations"]["get"]["operationId"],
        "admin_runtime_get_execution_technical_operations"
    );
    assert_eq!(
        value["paths"]["/admin/runtime/executions/{node_id}/payload"]["get"]["operationId"],
        "admin_runtime_get_execution_payload"
    );
    assert_eq!(
        value["paths"]["/admin/runtime/executions/{node_id}/logs"]["get"]["operationId"],
        "admin_runtime_get_execution_logs"
    );
    assert_eq!(
        value["paths"]["/admin/runtime/heatmap"]["get"]["operationId"],
        "admin_runtime_get_heatmap"
    );
    assert!(value["components"]["schemas"]["AdminRuntimeOutboxItem"].is_object());
    assert!(value["components"]["schemas"]["AdminOutboxEventDetail"].is_object());
    assert!(value["components"]["schemas"]["AdminRuntimeFunctionRunItem"].is_object());
    assert!(value["components"]["schemas"]["AdminFunctionRunDetail"].is_object());
    assert!(value["components"]["schemas"]["AdminRemoteProxyCallItem"].is_object());
    assert!(value["components"]["schemas"]["AdminRemoteProxyCallListResponse"].is_object());
    assert!(value["components"]["schemas"]["AdminRuntimeSummaryResponse"].is_object());
    assert!(value["components"]["schemas"]["AdminRuntimeSummaryItem"].is_object());
    assert!(value["components"]["schemas"]["AdminRuntimeTimelineItem"].is_object());
    assert!(value["components"]["schemas"]["AdminRuntimeStoryListItem"].is_object());
    assert!(value["components"]["schemas"]["AdminRuntimeStoryDetail"].is_object());
    assert!(value["components"]["schemas"]["AdminRuntimeHeatmapCell"].is_object());
    assert!(value["components"]["schemas"]["AdminRuntimeHeatmapResponse"].is_object());
    assert!(value["components"]["schemas"]["AdminRuntimeExecutionPayload"].is_object());
    assert!(value["components"]["schemas"]["AdminRuntimeExecutionPayloadResponse"].is_object());
    assert!(value["components"]["schemas"]["AdminRuntimeExecutionLog"].is_object());
    assert!(value["components"]["schemas"]["AdminRuntimeExecutionLogListResponse"].is_object());
    assert!(value["components"]["schemas"]["AdminRuntimeTechnicalOperation"].is_object());
    assert!(
        value["components"]["schemas"]["AdminRuntimeTechnicalOperationListResponse"].is_object()
    );
    assert_eq!(
        value["paths"]["/admin/runtime/outbox/{id}/retry"]["post"]["operationId"],
        "admin_runtime_retry_outbox"
    );
    assert_eq!(
        value["paths"]["/admin/runtime/functions/{id}/retry"]["post"]["operationId"],
        "admin_runtime_retry_function_run"
    );
}

async fn test_app(db: &TestDatabase) -> axum::Router {
    let migrations = PLATFORM_MIGRATIONS
        .iter()
        .chain(RUNTIME_MIGRATIONS)
        .copied()
        .collect::<Vec<_>>();
    apply_migrations(&db.pool, &migrations)
        .await
        .expect("migrations should apply");

    let mut config = AppConfig::from_env();
    config.database = DatabaseConfig {
        url: db.url.clone(),
        max_connections: 5,
    };
    let ctx = AppContext::new(config, db.pool.clone(), Arc::new(LoggingEventPublisher));
    install_runtime_function_declarations();
    build_router(ctx)
}

async fn test_app_with_telemetry(db: &TestDatabase, spans: Vec<TelemetrySpan>) -> axum::Router {
    let migrations = PLATFORM_MIGRATIONS
        .iter()
        .chain(RUNTIME_MIGRATIONS)
        .copied()
        .collect::<Vec<_>>();
    apply_migrations(&db.pool, &migrations)
        .await
        .expect("migrations should apply");

    let mut config = AppConfig::from_env();
    config.database = DatabaseConfig {
        url: db.url.clone(),
        max_connections: 5,
    };
    let ctx = AppContext::new(config, db.pool.clone(), Arc::new(LoggingEventPublisher))
        .with_telemetry_span_provider(Arc::new(InMemoryTelemetrySpanProvider::new(spans)));
    install_runtime_function_declarations();
    build_router(ctx)
}

fn auth_only_app() -> axum::Router {
    auth_only_app_for_environment("local")
}

fn auth_only_app_for_environment(environment: &str) -> axum::Router {
    let mut config = AppConfig::from_env();
    config.service.environment = environment.to_owned();
    let ctx = AppContext::new(
        config,
        platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build"),
        Arc::new(LoggingEventPublisher),
    );
    install_runtime_function_declarations();
    build_router(ctx)
}

fn install_runtime_function_declarations() {
    platform_admin::install_runtime_function_declarations(vec![
        platform_admin::AdminRuntimeFunctionDeclarationMetadata {
            module_name: "notifications".to_owned(),
            module_source: platform_module::ModuleSource::Linked,
            name: "notifications.send_welcome_email.v1".to_owned(),
            version: 1,
            queue: "notifications".to_owned(),
            input_schema: Some("notifications.send_welcome_email.v1".to_owned()),
            retry_policy: None,
        },
    ]);
}

fn assert_welcome_email_runtime_declaration(value: &Value) {
    assert_eq!(value["module_name"], "notifications");
    assert_eq!(value["module_source"], "linked");
    assert_eq!(value["name"], "notifications.send_welcome_email.v1");
    assert_eq!(value["version"], 1);
    assert_eq!(value["queue"], "notifications");
    assert_eq!(value["input_schema"], "notifications.send_welcome_email.v1");
    assert_eq!(value["retry_policy"], Value::Null);
}

fn telemetry_span(id: &str, name: &str, attributes: Value) -> TelemetrySpan {
    telemetry_span_at(
        id,
        name,
        "2026-05-31T00:00:00Z",
        "2026-05-31T00:00:01Z",
        attributes,
    )
}

fn telemetry_span_at(
    id: &str,
    name: &str,
    started_at: &str,
    ended_at: &str,
    attributes: Value,
) -> TelemetrySpan {
    TelemetrySpan {
        attributes,
        ended_at: ended_at.parse().expect("timestamp should parse"),
        id: id.to_owned(),
        name: name.to_owned(),
        started_at: started_at.parse().expect("timestamp should parse"),
        status: Some("ok".to_owned()),
    }
}

#[derive(Debug, Clone, Copy)]
struct RemoteProxyCallFixture {
    id: &'static str,
    correlation_id: &'static str,
    module_name: &'static str,
    success: bool,
    occurred_at: &'static str,
    error_code: Option<&'static str>,
    trace_id: &'static str,
    span_id: &'static str,
}

fn remote_proxy_fixture(
    id: &'static str,
    correlation_id: &'static str,
    module_name: &'static str,
    success: bool,
    occurred_at: &'static str,
    error_code: Option<&'static str>,
) -> RemoteProxyCallFixture {
    RemoteProxyCallFixture {
        id,
        correlation_id,
        module_name,
        success,
        occurred_at,
        error_code,
        trace_id: "trace_remote_proxy",
        span_id: "span_remote_proxy",
    }
}

async fn insert_remote_proxy_call(pool: &platform_core::DbPool, fixture: RemoteProxyCallFixture) {
    sqlx::query(
        r#"
        insert into platform.remote_http_proxy_calls (
            id,
            module_name,
            method,
            declared_path,
            remote_path,
            capability,
            remote_status,
            duration_ms,
            success,
            error_code,
            retryable,
            request_id,
            correlation_id,
            trace_id,
            span_id,
            path_params,
            error_details,
            occurred_at
        )
        values ($1, $2, 'GET', '/contacts/{id}', '/contacts/contact_1', 'remote_crm.contacts.read', $3, 125, $4, $5, true, $6, $7, $8, $9, $10, $11, $12)
        "#,
    )
    .bind(fixture.id)
    .bind(fixture.module_name)
    .bind(if fixture.success { 200 } else { 502 })
    .bind(fixture.success)
    .bind(fixture.error_code)
    .bind(format!("req_{}", fixture.id))
    .bind(fixture.correlation_id)
    .bind(fixture.trace_id)
    .bind(fixture.span_id)
    .bind(json!({ "id": "contact_1" }))
    .bind(if fixture.success {
        json!([])
    } else {
        json!([{ "field": "remote_module", "reason": fixture.module_name }])
    })
    .bind(parse_time(fixture.occurred_at))
    .execute(pool)
    .await
    .expect("remote proxy call fixture should insert");
}

async fn insert_outbox_event(pool: &platform_core::DbPool) {
    sqlx::query(
        r#"
        insert into platform.outbox (
            id,
            event_name,
            event_version,
            source_module,
            aggregate_type,
            aggregate_id,
            correlation_id,
            causation_id,
            occurred_at,
            payload,
            headers
        )
        values (
            'evt_1',
            'identity.user_registered.v1',
            1,
            'identity',
            'user',
            'usr_1',
            'corr_1',
            'req_1',
            now(),
            $1,
            $2
        )
        "#,
    )
    .bind(json!({ "user_id": "usr_1" }))
    .bind(json!({
        "actor": {
            "kind": "service",
            "service_id": "api",
            "scopes": []
        },
        "trace": {
            "trace_id": "trace_1",
            "span_id": "span_1"
        },
        "schema_ref": "contracts/events/identity/identity.user_registered.v1.schema.json"
    }))
    .execute(pool)
    .await
    .expect("outbox event should insert");
}

async fn insert_outbox_event_with_status(
    pool: &platform_core::DbPool,
    id: &str,
    status: &str,
    attempts: i32,
    last_error: Option<&str>,
) {
    sqlx::query(
        r#"
        insert into platform.outbox (
            id,
            event_name,
            event_version,
            source_module,
            aggregate_type,
            aggregate_id,
            correlation_id,
            occurred_at,
            payload,
            headers,
            status,
            attempts,
            locked_at,
            locked_by,
            last_error
        )
        values (
            $1,
            'identity.user_registered.v1',
            1,
            'identity',
            'user',
            'usr_1',
            'corr_1',
            now(),
            $2,
            '{}'::jsonb,
            $3,
            $4,
            now(),
            'worker-a',
            $5
        )
        "#,
    )
    .bind(id)
    .bind(json!({ "user_id": "usr_1" }))
    .bind(status)
    .bind(attempts)
    .bind(last_error)
    .execute(pool)
    .await
    .expect("outbox event should insert");
}

async fn insert_function_run(pool: &platform_core::DbPool) {
    sqlx::query(
        r#"
        insert into runtime.function_runs (
            id,
            function_name,
            input_json,
            correlation_id,
            actor
        )
        values (
            'fnrun_1',
            'notifications.send_welcome_email.v1',
            $1,
            'corr_1',
            '{"kind":"system"}'::jsonb
        )
        "#,
    )
    .bind(json!({ "user_id": "usr_1" }))
    .execute(pool)
    .await
    .expect("function run should insert");
}

async fn insert_function_run_with_status(
    pool: &platform_core::DbPool,
    id: &str,
    status: &str,
    attempts: i32,
    last_error: Option<&str>,
) {
    sqlx::query(
        r#"
        insert into runtime.function_runs (
            id,
            function_name,
            input_json,
            status,
            attempts,
            locked_at,
            locked_by,
            last_error,
            correlation_id,
            actor
        )
        values (
            $1,
            'notifications.send_welcome_email.v1',
            $2,
            $3,
            $4,
            now(),
            'worker-a',
            $5,
            'corr_1',
            '{"kind":"system"}'::jsonb
        )
        "#,
    )
    .bind(id)
    .bind(json!({ "user_id": "usr_1" }))
    .bind(status)
    .bind(attempts)
    .bind(last_error)
    .execute(pool)
    .await
    .expect("function run should insert");
}

async fn insert_story_outbox_event(pool: &platform_core::DbPool) {
    sqlx::query(
        r#"
        insert into platform.outbox (
            id,
            event_name,
            event_version,
            source_module,
            aggregate_type,
            aggregate_id,
            correlation_id,
            causation_id,
            occurred_at,
            payload,
            headers,
            status,
            attempts,
            max_attempts,
            locked_at,
            published_at,
            created_at
        )
        values (
            'evt_story',
            'identity.user_registered.v1',
            1,
            'identity',
            'user',
            'usr_1',
            'corr_story',
            'req_story',
            '2026-05-31T00:00:00Z',
            $1,
            '{}'::jsonb,
            'published',
            1,
            3,
            '2026-05-31T00:00:05Z',
            '2026-05-31T00:00:20Z',
            '2026-05-31T00:00:00Z'
        )
        "#,
    )
    .bind(json!({ "user_id": "usr_1" }))
    .execute(pool)
    .await
    .expect("story outbox event should insert");
}

async fn insert_story_function_run(pool: &platform_core::DbPool) {
    sqlx::query(
        r#"
        insert into runtime.function_runs (
            id,
            function_name,
            input_json,
            status,
            attempts,
            max_attempts,
            locked_at,
            started_at,
            completed_at,
            last_error,
            correlation_id,
            actor,
            created_at,
            updated_at
        )
        values (
            'fnrun_story',
            'notifications.send_welcome_email.v1',
            $1,
            'dead',
            3,
            3,
            '2026-05-31T00:00:30Z',
            '2026-05-31T00:00:40Z',
            '2026-05-31T00:02:00Z',
            'smtp timeout',
            'corr_story',
            '{"kind":"system"}'::jsonb,
            '2026-05-31T00:00:30Z',
            '2026-05-31T00:02:00Z'
        )
        "#,
    )
    .bind(json!({
        "user_id": "usr_1",
        "_lenso_runtime": {
            "causation_id": "evt_story"
        }
    }))
    .execute(pool)
    .await
    .expect("story function run should insert");
}

#[allow(clippy::too_many_arguments)]
async fn insert_execution_log(
    pool: &platform_core::DbPool,
    id: &str,
    execution_id: &str,
    execution_type: &str,
    execution_name: &str,
    occurred_at: &str,
    severity: &str,
    body: &str,
    attributes: Value,
) {
    sqlx::query(
        r#"
        insert into platform.execution_logs (
            id,
            correlation_id,
            story_id,
            execution_id,
            execution_type,
            execution_name,
            occurred_at,
            severity,
            body,
            attributes,
            trace_id,
            span_id,
            service_name,
            redacted_fields
        )
        values (
            $1,
            'corr_story',
            'corr_story',
            $2,
            $3,
            $4,
            $5,
            $6,
            $7,
            $8,
            'trace_1',
            'span_1',
            'notifications',
            array[]::text[]
        )
        "#,
    )
    .bind(id)
    .bind(execution_id)
    .bind(execution_type)
    .bind(execution_name)
    .bind(parse_time(occurred_at))
    .bind(severity)
    .bind(body)
    .bind(attributes)
    .execute(pool)
    .await
    .expect("execution log should insert");
}

async fn wait_for_story_event(pool: &platform_core::DbPool, correlation_id: &str) {
    for _ in 0..100 {
        let count: i64 = sqlx::query_scalar(
            r#"
            select count(*)::bigint
            from platform.story_events
            where correlation_id = $1
            "#,
        )
        .bind(correlation_id)
        .fetch_one(pool)
        .await
        .expect("story event count should query");

        if count > 0 {
            return;
        }

        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    panic!("story event for {correlation_id} was not projected");
}

async fn insert_heatmap_outbox_events(pool: &platform_core::DbPool) {
    sqlx::query(
        r#"
        insert into platform.outbox (
            id,
            event_name,
            event_version,
            source_module,
            aggregate_type,
            aggregate_id,
            correlation_id,
            occurred_at,
            payload,
            headers,
            status,
            attempts,
            max_attempts,
            locked_at,
            published_at,
            last_error,
            created_at
        )
        values
            (
                'evt_heatmap_ok',
                'identity.user_registered.v1',
                1,
                'identity',
                'user',
                'usr_1',
                'corr_heatmap_1',
                '2026-05-31T00:00:10Z',
                $1,
                '{}'::jsonb,
                'published',
                1,
                3,
                '2026-05-31T00:00:12Z',
                '2026-05-31T00:00:20Z',
                null,
                '2026-05-31T00:00:10Z'
            ),
            (
                'evt_heatmap_failed',
                'identity.user_registered.v1',
                1,
                'identity',
                'user',
                'usr_2',
                'corr_heatmap_2',
                '2026-05-31T00:00:30Z',
                $2,
                '{}'::jsonb,
                'failed',
                2,
                3,
                '2026-05-31T00:00:35Z',
                null,
                'handler timeout',
                '2026-05-31T00:00:30Z'
            )
        "#,
    )
    .bind(json!({ "user_id": "usr_1" }))
    .bind(json!({ "user_id": "usr_2" }))
    .execute(pool)
    .await
    .expect("heatmap outbox events should insert");
}

async fn insert_heatmap_function_runs(pool: &platform_core::DbPool) {
    sqlx::query(
        r#"
        insert into runtime.function_runs (
            id,
            function_name,
            input_json,
            status,
            attempts,
            max_attempts,
            locked_at,
            started_at,
            completed_at,
            last_error,
            correlation_id,
            actor,
            created_at,
            updated_at
        )
        values
            (
                'fnrun_heatmap_dead',
                'notifications.send_welcome_email.v1',
                $1,
                'dead',
                3,
                3,
                '2026-05-31T00:00:40Z',
                '2026-05-31T00:00:40Z',
                '2026-05-31T00:02:00Z',
                'smtp timeout',
                'corr_heatmap_1',
                '{"kind":"system"}'::jsonb,
                '2026-05-31T00:00:40Z',
                '2026-05-31T00:02:00Z'
            ),
            (
                'fnrun_heatmap_completed',
                'notifications.send_welcome_email.v1',
                $2,
                'completed',
                1,
                3,
                '2026-05-31T00:00:42Z',
                '2026-05-31T00:00:42Z',
                '2026-05-31T00:00:50Z',
                null,
                'corr_heatmap_2',
                '{"kind":"system"}'::jsonb,
                '2026-05-31T00:00:42Z',
                '2026-05-31T00:00:50Z'
            ),
            (
                'fnrun_heatmap_later',
                'notifications.cleanup_expired_sessions.v1',
                $3,
                'completed',
                1,
                3,
                '2026-05-31T00:02:10Z',
                '2026-05-31T00:02:10Z',
                '2026-05-31T00:02:20Z',
                null,
                'corr_heatmap_3',
                '{"kind":"system"}'::jsonb,
                '2026-05-31T00:02:10Z',
                '2026-05-31T00:02:20Z'
            )
        "#,
    )
    .bind(json!({ "user_id": "usr_1" }))
    .bind(json!({ "user_id": "usr_2" }))
    .bind(json!({ "job": "cleanup" }))
    .execute(pool)
    .await
    .expect("heatmap function runs should insert");
}

async fn insert_heatmap_story_events(pool: &platform_core::DbPool) {
    sqlx::query(
        r#"
        insert into platform.story_events (
            id,
            source_type,
            source_id,
            node_type,
            name,
            status,
            service,
            correlation_id,
            causation_id,
            started_at,
            completed_at,
            duration_ms,
            error,
            metadata,
            trace_id,
            span_id,
            created_at,
            updated_at
        )
        values
            (
                'story_heatmap_http',
                'http_request',
                'req_heatmap_1',
                'http_request',
                'POST /identity/users',
                'completed',
                'api',
                'corr_heatmap_1',
                null,
                '2026-05-31T00:00:10Z',
                '2026-05-31T00:00:10.120Z',
                120,
                null,
                '{}'::jsonb,
                'trace_heatmap_1',
                'span_heatmap_1',
                '2026-05-31T00:00:10Z',
                '2026-05-31T00:00:10.120Z'
            ),
            (
                'story_heatmap_fn_dead',
                'function_run',
                'fnrun_heatmap_story_dead',
                'function',
                'notifications.send_welcome_email.v1',
                'dead',
                'notifications',
                'corr_heatmap_1',
                'story_heatmap_http',
                '2026-05-31T00:00:40Z',
                '2026-05-31T00:02:00Z',
                80000,
                'smtp timeout',
                '{}'::jsonb,
                'trace_heatmap_1',
                'span_heatmap_2',
                '2026-05-31T00:00:40Z',
                '2026-05-31T00:02:00Z'
            ),
            (
                'story_heatmap_other',
                'function_run',
                'fnrun_heatmap_story_other',
                'function',
                'notifications.cleanup_expired_sessions.v1',
                'completed',
                'notifications',
                'corr_heatmap_2',
                null,
                '2026-05-31T00:00:45Z',
                '2026-05-31T00:00:50Z',
                5000,
                null,
                '{}'::jsonb,
                'trace_heatmap_2',
                'span_heatmap_3',
                '2026-05-31T00:00:45Z',
                '2026-05-31T00:00:50Z'
            )
        "#,
    )
    .execute(pool)
    .await
    .expect("heatmap story events should insert");
}

#[derive(Clone)]
struct OutboxFixture {
    id: &'static str,
    event_name: &'static str,
    source_module: &'static str,
    aggregate_id: &'static str,
    correlation_id: &'static str,
    causation_id: Option<&'static str>,
    status: &'static str,
    attempts: i32,
    max_attempts: i32,
    locked_at: Option<&'static str>,
    published_at: Option<&'static str>,
    last_error: Option<&'static str>,
    created_at: &'static str,
    headers: Value,
}

impl Default for OutboxFixture {
    fn default() -> Self {
        Self {
            id: "evt_fixture",
            event_name: "identity.user_registered.v1",
            source_module: "identity",
            aggregate_id: "usr_fixture",
            correlation_id: "corr_fixture",
            causation_id: None,
            status: "published",
            attempts: 1,
            max_attempts: 3,
            locked_at: Some("2026-05-31T00:00:01Z"),
            published_at: Some("2026-05-31T00:00:02Z"),
            last_error: None,
            created_at: "2026-05-31T00:00:00Z",
            headers: Value::Object(Default::default()),
        }
    }
}

#[derive(Clone)]
struct FunctionFixture {
    id: &'static str,
    function_name: &'static str,
    correlation_id: &'static str,
    status: &'static str,
    attempts: i32,
    max_attempts: i32,
    locked_at: Option<&'static str>,
    started_at: Option<&'static str>,
    completed_at: Option<&'static str>,
    last_error: Option<&'static str>,
    created_at: &'static str,
    input_json: Value,
}

impl Default for FunctionFixture {
    fn default() -> Self {
        Self {
            id: "fnrun_fixture",
            function_name: "notifications.send_welcome_email.v1",
            correlation_id: "corr_fixture",
            status: "completed",
            attempts: 1,
            max_attempts: 3,
            locked_at: Some("2026-05-31T00:00:03Z"),
            started_at: Some("2026-05-31T00:00:04Z"),
            completed_at: Some("2026-05-31T00:00:05Z"),
            last_error: None,
            created_at: "2026-05-31T00:00:03Z",
            input_json: Value::Object(Default::default()),
        }
    }
}

async fn insert_runtime_outbox_fixture(pool: &platform_core::DbPool, fixture: OutboxFixture) {
    sqlx::query(
        r#"
        insert into platform.outbox (
            id,
            event_name,
            event_version,
            source_module,
            aggregate_type,
            aggregate_id,
            correlation_id,
            causation_id,
            occurred_at,
            payload,
            headers,
            status,
            attempts,
            max_attempts,
            locked_at,
            published_at,
            last_error,
            created_at
        )
        values ($1, $2, 1, $3, 'user', $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $7)
        "#,
    )
    .bind(fixture.id)
    .bind(fixture.event_name)
    .bind(fixture.source_module)
    .bind(fixture.aggregate_id)
    .bind(fixture.correlation_id)
    .bind(fixture.causation_id)
    .bind(parse_time(fixture.created_at))
    .bind(json!({ "aggregate_id": fixture.aggregate_id }))
    .bind(fixture.headers)
    .bind(fixture.status)
    .bind(fixture.attempts)
    .bind(fixture.max_attempts)
    .bind(fixture.locked_at.map(parse_time))
    .bind(fixture.published_at.map(parse_time))
    .bind(fixture.last_error)
    .execute(pool)
    .await
    .expect("runtime outbox fixture should insert");
}

async fn insert_runtime_function_fixture(pool: &platform_core::DbPool, fixture: FunctionFixture) {
    sqlx::query(
        r#"
        insert into runtime.function_runs (
            id,
            function_name,
            input_json,
            status,
            attempts,
            max_attempts,
            locked_at,
            started_at,
            completed_at,
            last_error,
            correlation_id,
            actor,
            created_at,
            updated_at
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, '{"kind":"system"}'::jsonb, $12, coalesce($9, $12))
        "#,
    )
    .bind(fixture.id)
    .bind(fixture.function_name)
    .bind(fixture.input_json)
    .bind(fixture.status)
    .bind(fixture.attempts)
    .bind(fixture.max_attempts)
    .bind(fixture.locked_at.map(parse_time))
    .bind(fixture.started_at.map(parse_time))
    .bind(fixture.completed_at.map(parse_time))
    .bind(fixture.last_error)
    .bind(fixture.correlation_id)
    .bind(parse_time(fixture.created_at))
    .execute(pool)
    .await
    .expect("runtime function fixture should insert");
}

fn parse_time(value: &str) -> DateTime<Utc> {
    value.parse().expect("fixture timestamp should parse")
}

fn admin_get(uri: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .expect("request should build")
}

fn admin_post(uri: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .body(Body::empty())
        .expect("request should build")
}

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body should read");
    serde_json::from_slice(&bytes).expect("body should be json")
}

trait RequestExt {
    fn with_header(self, name: &'static str, value: &'static str) -> Self;
}

impl RequestExt for Request<Body> {
    fn with_header(mut self, name: &'static str, value: &'static str) -> Self {
        self.headers_mut().insert(name, value.parse().unwrap());
        self
    }
}

#[derive(Debug)]
struct RetryState {
    status: String,
    attempts: i32,
    locked_at: Option<chrono::DateTime<chrono::Utc>>,
    locked_by: Option<String>,
    last_error: Option<String>,
}

async fn outbox_retry_state(pool: &platform_core::DbPool, id: &str) -> RetryState {
    let (status, attempts, locked_at, locked_by, last_error) =
        sqlx::query_as::<_, (String, i32, Option<_>, Option<String>, Option<String>)>(
            "select status, attempts, locked_at, locked_by, last_error from platform.outbox where id = $1",
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("outbox retry state should query");

    RetryState {
        status,
        attempts,
        locked_at,
        locked_by,
        last_error,
    }
}

async fn function_retry_state(pool: &platform_core::DbPool, id: &str) -> RetryState {
    let (status, attempts, locked_at, locked_by, last_error) =
        sqlx::query_as::<_, (String, i32, Option<_>, Option<String>, Option<String>)>(
            "select status, attempts, locked_at, locked_by, last_error from runtime.function_runs where id = $1",
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("function retry state should query");

    RetryState {
        status,
        attempts,
        locked_at,
        locked_by,
        last_error,
    }
}
