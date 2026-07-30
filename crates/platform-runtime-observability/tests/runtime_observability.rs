use axum::{Extension, body::Body};
use http::{Request, StatusCode, header};
use http_body_util::BodyExt as _;
use lenso_service::{
    AuthenticatedTransportBinding, SystemSandboxWorkloadIdentityProvider,
    WorkloadCredentialRequest, WorkloadIdentityProvider,
    system_plane::{
        RUNTIME_OBSERVABILITY_FEATURE_QUEUE_SUMMARY, RUNTIME_OBSERVABILITY_FEATURE_RECOVERY_FEED,
        RUNTIME_OBSERVABILITY_PROTOCOL, RuntimeObservabilityStatus, RuntimeObservationContinuity,
        RuntimeObservationGapReason, runtime_observability_schema,
        runtime_observability_schema_digest,
    },
};
use platform_runtime_observability::{
    RUNTIME_OBSERVABILITY_MIGRATIONS, RuntimeObservabilityProvider, router,
};
use platform_system_plane::{
    EnrollmentGrant, SystemPlaneAccess, SystemPlaneRegistryBuilder, SystemPlaneRuntime,
    SystemSandboxEnrollmentAuthorizer,
};
use platform_testing::TestDatabase;
use std::sync::Arc;
use tower::ServiceExt as _;

#[test]
fn advertisement_binds_the_exact_runtime_observability_schema() {
    let advertisement = RuntimeObservabilityProvider::advertisement();

    assert_eq!(advertisement.contract_id, RUNTIME_OBSERVABILITY_PROTOCOL);
    assert_eq!(
        advertisement.schema_digest,
        runtime_observability_schema_digest()
    );
    assert_eq!(
        advertisement.feature_ids,
        [
            RUNTIME_OBSERVABILITY_FEATURE_QUEUE_SUMMARY.to_owned(),
            RUNTIME_OBSERVABILITY_FEATURE_RECOVERY_FEED.to_owned(),
        ]
        .into_iter()
        .collect()
    );
    assert!(
        jsonschema::validator_for(&runtime_observability_schema()).is_ok(),
        "committed capability schema must remain valid JSON Schema"
    );
}

#[tokio::test]
async fn snapshot_is_revisioned_and_classifies_service_owned_queue_pressure() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    platform_core::apply_migrations(&db.pool, platform_core::PLATFORM_MIGRATIONS)
        .await
        .unwrap();
    platform_core::apply_migrations(&db.pool, platform_runtime::RUNTIME_MIGRATIONS)
        .await
        .unwrap();
    platform_core::apply_migrations(&db.pool, RUNTIME_OBSERVABILITY_MIGRATIONS)
        .await
        .unwrap();
    sqlx::query(
        r#"
        insert into platform.outbox (
            id, event_name, event_version, source_module, aggregate_type,
            aggregate_id, correlation_id, occurred_at, payload, status
        ) values
            ('pending-event', 'ticket.opened', 1, 'tickets', 'ticket', '1', 'story-1', now(), '{}', 'pending'),
            ('failed-event', 'ticket.failed', 1, 'tickets', 'ticket', '2', 'story-2', now(), '{}', 'failed')
        "#,
    )
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        insert into runtime.function_runs (
            id, function_name, input_json, correlation_id, actor, status
        ) values ('dead-run', 'tickets.notify', '{}', 'story-3', '{}', 'dead')
        "#,
    )
    .execute(&db.pool)
    .await
    .unwrap();
    let provider = RuntimeObservabilityProvider::new(
        db.pool.clone(),
        "support",
        "release:sha256:0123456789abcdef",
    );

    let first = provider.snapshot().await.unwrap();
    let second = provider.snapshot().await.unwrap();

    assert_eq!(first.status, RuntimeObservabilityStatus::Failing);
    assert_eq!(first.snapshot_revision, second.snapshot_revision);
    assert_ne!(first.observed_at, second.observed_at);
    assert_eq!(first.next_cursor, second.next_cursor);
    assert_eq!(first.queues[0].pending, 1);
    assert_eq!(first.queues[0].failed, 1);
    assert_eq!(first.queues[1].dead, 1);

    db.cleanup().await;
}

#[tokio::test]
async fn feed_resumes_after_snapshot_watermark_and_reports_invalid_cursor_as_a_gap() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    platform_core::apply_migrations(&db.pool, platform_core::PLATFORM_MIGRATIONS)
        .await
        .unwrap();
    platform_core::apply_migrations(&db.pool, platform_runtime::RUNTIME_MIGRATIONS)
        .await
        .unwrap();
    platform_core::apply_migrations(&db.pool, RUNTIME_OBSERVABILITY_MIGRATIONS)
        .await
        .unwrap();
    let provider = RuntimeObservabilityProvider::new(
        db.pool.clone(),
        "support",
        "release:sha256:0123456789abcdef",
    );
    let snapshot = provider.snapshot().await.unwrap();
    sqlx::query(
        r#"
        insert into runtime.function_runs (
            id, function_name, input_json, correlation_id, actor, status
        ) values ('new-run', 'tickets.notify', '{}', 'story-4', '{}', 'pending')
        "#,
    )
    .execute(&db.pool)
    .await
    .unwrap();

    let feed = provider.feed(&snapshot.next_cursor, 100).await.unwrap();
    assert_eq!(feed.continuity, RuntimeObservationContinuity::Continuous);
    assert_eq!(feed.changes.len(), 1);
    assert_eq!(feed.changes[0].resource_id, "new-run");
    assert_ne!(feed.next_cursor, snapshot.next_cursor);

    let gap = provider.feed("not-a-cursor", 100).await.unwrap();
    assert_eq!(gap.continuity, RuntimeObservationContinuity::ResetRequired);
    assert_eq!(
        gap.evidence_gap.unwrap().reason,
        RuntimeObservationGapReason::InvalidCursor
    );
    assert!(gap.next_cursor.is_empty());
    db.cleanup().await;
}

#[tokio::test]
async fn capability_route_uses_the_same_enrollment_and_workload_identity_seam_as_core() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    platform_core::apply_migrations(&db.pool, platform_core::PLATFORM_MIGRATIONS)
        .await
        .unwrap();
    platform_core::apply_migrations(&db.pool, platform_runtime::RUNTIME_MIGRATIONS)
        .await
        .unwrap();
    platform_core::apply_migrations(&db.pool, RUNTIME_OBSERVABILITY_MIGRATIONS)
        .await
        .unwrap();
    let provider = Arc::new(RuntimeObservabilityProvider::new(
        db.pool.clone(),
        "support",
        "release:sha256:0123456789abcdef",
    ));
    let identity = Arc::new(
        SystemSandboxWorkloadIdentityProvider::new("test", "runtime-observability-secret").unwrap(),
    );
    let enrollment = Arc::new(
        SystemSandboxEnrollmentAuthorizer::new(
            "test",
            EnrollmentGrant::system_sandbox("support", "service:console", 7, 4_000_000_000_000),
        )
        .unwrap(),
    );
    let registry = SystemPlaneRegistryBuilder::new(
        "support",
        "service:support",
        "release:sha256:0123456789abcdef",
    )
    .register(RuntimeObservabilityProvider::advertisement())
    .build()
    .unwrap();
    let system_plane = Arc::new(SystemPlaneRuntime::new(
        registry,
        SystemPlaneAccess::new(identity.clone(), "service:support", enrollment),
    ));
    let system_plane = Some(system_plane);
    let app = platform_system_plane::router::<()>(system_plane.clone())
        .merge(router(Some(provider)))
        .layer(Extension(system_plane))
        .split_for_parts()
        .0
        .layer(Extension(AuthenticatedTransportBinding::new(
            "tls:console-peer",
        )));
    let credential = identity
        .issue(WorkloadCredentialRequest::new(
            "service:console",
            "service:support",
            "tls:console-peer",
            now_unix_ms(),
            30_000,
        ))
        .unwrap();

    let response = app
        .clone()
        .oneshot(
            Request::get("/system-plane/v1/runtime-observability")
                .header(
                    header::AUTHORIZATION,
                    format!("Bearer {}", credential.token),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["protocol"], RUNTIME_OBSERVABILITY_PROTOCOL);
    assert_eq!(body["serviceId"], "support");
    let response = app
        .oneshot(
            Request::get(format!(
                "/system-plane/v1/runtime-observability/changes?cursor={}",
                body["nextCursor"].as_str().unwrap()
            ))
            .header(
                header::AUTHORIZATION,
                format!("Bearer {}", credential.token),
            )
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["continuity"], "continuous");
    assert_eq!(body["changes"], serde_json::json!([]));
    db.cleanup().await;
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
