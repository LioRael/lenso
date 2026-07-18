use axum::body::Body;
use http::{Request, StatusCode, header};
use http_body_util::BodyExt as _;
use lenso_autonomous_service::{
    ServiceRuntimeConfig, StorySegmentFeedConfig, StorySegmentRecord, StorySegmentTenantAccess,
    StorySegmentWorkflow, StorySegmentWriteDisposition, append_story_segment, prepare_runtime,
    service_router,
};
use lenso_service::{
    AuthenticatedTransportBinding, AutonomousServiceContract, AutonomousServiceStore,
    AutonomousServiceWorkload, ServiceTenancyMode, SystemSandboxWorkloadIdentityProvider,
    WorkloadCredentialRequest, WorkloadIdentityProvider, WorkloadRole,
};
use platform_testing::TestDatabase;
use std::{sync::Arc, time::Duration};
use tower::ServiceExt as _;
use utoipa_axum::router::OpenApiRouter;

const FEED_AUDIENCE: &str = "service:support/story-segment-feed";
const CURSOR_KEY: &[u8] = b"story-segment-test-cursor-key-32-bytes-minimum";
const BINDING: &str = "spiffe://lenso.test/service/aggregator";

fn service() -> AutonomousServiceContract {
    let mut service = AutonomousServiceContract::new(
        "support",
        vec![
            AutonomousServiceWorkload::new("support-api", "support", WorkloadRole::API),
            AutonomousServiceWorkload::new("support-migrate", "support", WorkloadRole::MIGRATION),
            AutonomousServiceWorkload::new("support-worker", "support", WorkloadRole::WORKER),
        ],
        ServiceTenancyMode::Required,
        vec!["local".to_owned()],
    );
    service.stores = vec![AutonomousServiceStore::new("primary", "support")];
    service
}

fn provider() -> Arc<SystemSandboxWorkloadIdentityProvider> {
    Arc::new(
        SystemSandboxWorkloadIdentityProvider::new(
            "test",
            "story-segment-workload-identity-test-secret",
        )
        .unwrap(),
    )
}

fn runtime_config(provider: Arc<SystemSandboxWorkloadIdentityProvider>) -> ServiceRuntimeConfig {
    ServiceRuntimeConfig::new("support", "primary", "support").with_story_segment_feed(
        StorySegmentFeedConfig::new(
            provider,
            FEED_AUDIENCE,
            Duration::from_secs(24 * 60 * 60),
            CURSOR_KEY,
        )
        .with_reader(
            "service:aggregator",
            StorySegmentTenantAccess::Tenants(vec!["tenant_a".to_owned()]),
        ),
    )
}

fn credential(
    provider: &SystemSandboxWorkloadIdentityProvider,
    principal: &str,
    audience: &str,
) -> String {
    provider
        .issue(WorkloadCredentialRequest::new(
            principal,
            audience,
            BINDING,
            now_ms(),
            60_000,
        ))
        .unwrap()
        .token
}

fn feed_request(path: &str, credential: Option<&str>, with_binding: bool) -> Request<Body> {
    let mut request = Request::get(path);
    if let Some(credential) = credential {
        request = request.header(header::AUTHORIZATION, format!("Bearer {credential}"));
    }
    let mut request = request.body(Body::empty()).unwrap();
    if with_binding {
        request
            .extensions_mut()
            .insert(AuthenticatedTransportBinding::new(BINDING));
    }
    request
}

async fn json_body(response: axum::response::Response) -> serde_json::Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

fn segment(story_id: &str, segment_id: &str, tenant_id: &str, status: &str) -> StorySegmentRecord {
    let now = chrono::Utc::now();
    let recorded_at = chrono::DateTime::from_timestamp(
        now.timestamp(),
        (now.timestamp_subsec_nanos() / 1_000 * 1_000) + 789,
    )
    .unwrap();
    StorySegmentRecord::new(
        story_id,
        segment_id,
        "event_contract",
        "support.ticket.opened",
        "ticket-opened",
        "v1",
        status,
        recorded_at,
    )
    .with_tenant(tenant_id)
}

#[tokio::test]
async fn feed_cursor_resumes_after_restart_and_duplicate_reads_are_deterministic() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let provider = provider();
    let config = runtime_config(provider.clone());
    let state = prepare_runtime(&service(), &config, db.pool.clone(), &[])
        .await
        .unwrap();
    let first = segment("story_01", "segment_01", "tenant_a", "started").with_workflow(
        StorySegmentWorkflow {
            instance_id: "workflow_01".to_owned(),
            definition_owner: "support".to_owned(),
            definition_name: "ticket_sla".to_owned(),
            definition_version: "v1".to_owned(),
            step_id: Some("step_01".to_owned()),
            parent_instance_id: Some("workflow_parent".to_owned()),
            compensation_id: None,
            intervention_id: None,
        },
    );
    let second = segment("story_01", "segment_02", "tenant_a", "completed");
    assert_eq!(
        append_story_segment(&state, &first).await.unwrap(),
        StorySegmentWriteDisposition::Appended
    );
    assert_eq!(
        append_story_segment(&state, &second).await.unwrap(),
        StorySegmentWriteDisposition::Appended
    );
    let token = credential(&provider, "service:aggregator", FEED_AUDIENCE);
    let app = service_router(OpenApiRouter::new(), state);
    let page_one = app
        .oneshot(feed_request(
            "/runtime/story-segments?tenantId=tenant_a&limit=1",
            Some(&token),
            true,
        ))
        .await
        .unwrap();
    assert_eq!(page_one.status(), StatusCode::OK);
    let page_one = json_body(page_one).await;
    assert_eq!(page_one["protocol"], "lenso.story-segment-feed.v1");
    assert_eq!(page_one["segments"][0]["segmentId"], "segment_01");
    assert_eq!(
        page_one["segments"][0]["workflow"]["instanceId"],
        "workflow_01"
    );
    assert_eq!(
        page_one["segments"][0]["workflow"]["parentInstanceId"],
        "workflow_parent"
    );
    let cursor = page_one["nextCursor"].as_str().unwrap().to_owned();

    // Rebuilding runtime state over the same Store simulates the API Workload
    // restarting while the cursor remains entirely durable.
    let restarted = prepare_runtime(&service(), &config, db.pool.clone(), &[])
        .await
        .unwrap();
    let app = service_router(OpenApiRouter::new(), restarted.clone());
    let path = format!("/runtime/story-segments?tenantId=tenant_a&limit=1&cursor={cursor}");
    let resumed = app
        .clone()
        .oneshot(feed_request(&path, Some(&token), true))
        .await
        .unwrap();
    assert_eq!(resumed.status(), StatusCode::OK);
    let resumed = json_body(resumed).await;
    assert_eq!(resumed["segments"][0]["segmentId"], "segment_02");
    let resumed_cursor = resumed["nextCursor"].as_str().unwrap().to_owned();

    let retried = app
        .clone()
        .oneshot(feed_request(&path, Some(&token), true))
        .await
        .unwrap();
    let retried = json_body(retried).await;
    assert_eq!(retried["segments"], resumed["segments"]);
    assert_eq!(retried["nextCursor"], resumed["nextCursor"]);

    let late_revision = segment("story_01", "segment_01", "tenant_a", "completed")
        .with_revision(2)
        .with_attempt(2);
    assert_eq!(
        append_story_segment(&restarted, &late_revision)
            .await
            .unwrap(),
        StorySegmentWriteDisposition::Appended
    );
    assert_eq!(
        append_story_segment(&restarted, &late_revision)
            .await
            .unwrap(),
        StorySegmentWriteDisposition::Duplicate
    );
    let late_page = app
        .oneshot(feed_request(
            &format!("/runtime/story-segments?tenantId=tenant_a&cursor={resumed_cursor}"),
            Some(&token),
            true,
        ))
        .await
        .unwrap();
    let late_page = json_body(late_page).await;
    assert_eq!(late_page["segments"][0]["segmentId"], "segment_01");
    assert_eq!(late_page["segments"][0]["evidenceRevision"], 2);

    let count: i64 = sqlx::query_scalar(
        "select count(*) from platform.service_story_segments where service_id = 'support'",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(count, 3);
    let update = sqlx::query(
        "update platform.service_story_segments set status = 'rewritten' where segment_id = 'segment_01'",
    )
    .execute(&db.pool)
    .await;
    assert!(update.is_err(), "the Store must reject Feed rewrites");

    drop(restarted);
    db.cleanup().await;
}

#[tokio::test]
async fn workload_identity_audience_and_tenant_policy_isolate_feed_reads() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let provider = provider();
    let config = runtime_config(provider.clone());
    let state = prepare_runtime(&service(), &config, db.pool.clone(), &[])
        .await
        .unwrap();
    append_story_segment(
        &state,
        &segment("story_a", "segment_a", "tenant_a", "completed"),
    )
    .await
    .unwrap();
    append_story_segment(
        &state,
        &segment("story_b", "segment_b", "tenant_b", "completed"),
    )
    .await
    .unwrap();
    let app = service_router(OpenApiRouter::new(), state);

    let missing = app
        .clone()
        .oneshot(feed_request(
            "/runtime/story-segments?tenantId=tenant_a",
            None,
            true,
        ))
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);

    let wrong_audience = credential(&provider, "service:aggregator", "service:other");
    let rejected = app
        .clone()
        .oneshot(feed_request(
            "/runtime/story-segments?tenantId=tenant_a",
            Some(&wrong_audience),
            true,
        ))
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::UNAUTHORIZED);

    let token = credential(&provider, "service:aggregator", FEED_AUDIENCE);
    let forbidden = app
        .clone()
        .oneshot(feed_request(
            "/runtime/story-segments?tenantId=tenant_b",
            Some(&token),
            true,
        ))
        .await
        .unwrap();
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);

    let allowed = app
        .oneshot(feed_request(
            "/runtime/story-segments?tenantId=tenant_a",
            Some(&token),
            true,
        ))
        .await
        .unwrap();
    assert_eq!(allowed.status(), StatusCode::OK);
    let allowed_bytes = allowed.into_body().collect().await.unwrap().to_bytes();
    let allowed: serde_json::Value = serde_json::from_slice(&allowed_bytes).unwrap();
    assert_eq!(allowed["segments"].as_array().unwrap().len(), 1);
    assert_eq!(allowed["segments"][0]["tenantId"], "tenant_a");
    assert_eq!(allowed["segments"][0]["segmentId"], "segment_a");
    assert!(
        !String::from_utf8_lossy(&allowed_bytes).contains(&token),
        "credential material must never be exposed in feed evidence"
    );

    db.cleanup().await;
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis()
        .try_into()
        .unwrap()
}
