use axum::body::Body;
use http::{Request, StatusCode, header};
use http_body_util::BodyExt as _;
use lenso_autonomous_service::{ServiceRuntimeConfig, prepare_runtime, service_router};
use lenso_contracts::{
    ModuleManifest, RuntimeSurface, WorkflowDataContract, WorkflowDefinition,
    WorkflowStepDeclaration,
};
use lenso_service::{
    AutonomousServiceContract, AutonomousServiceStore, AutonomousServiceWorkload,
    ServiceTenancyMode, WorkloadRole,
};
use platform_testing::TestDatabase;
use tower::ServiceExt as _;
use utoipa_axum::router::OpenApiRouter;

fn service() -> AutonomousServiceContract {
    let mut service = AutonomousServiceContract::new(
        "support-sla",
        vec![
            AutonomousServiceWorkload::new("support-sla-api", "support-sla", WorkloadRole::API),
            AutonomousServiceWorkload::new(
                "support-sla-migrate",
                "support-sla",
                WorkloadRole::MIGRATION,
            ),
            AutonomousServiceWorkload::new(
                "support-sla-worker",
                "support-sla",
                WorkloadRole::WORKER,
            ),
        ],
        ServiceTenancyMode::Optional,
        vec!["local".to_owned()],
    );
    service.modules = vec!["support-sla".to_owned()];
    service.stores = vec![AutonomousServiceStore::new("primary", "support-sla")];
    service
}

fn manifest() -> ModuleManifest {
    ModuleManifest::builder("support-sla")
        .runtime(RuntimeSurface {
            functions: vec![],
            schedules: vec![],
            workflows: vec![workflow("v1"), workflow("v2")],
        })
        .build()
}

fn workflow(version: &str) -> WorkflowDefinition {
    WorkflowDefinition::new(
        "support-sla",
        "ticket_sla",
        version,
        WorkflowDataContract::new("support.sla.start", "v1"),
        WorkflowDataContract::new("support.sla.result", "v1"),
        vec![
            WorkflowStepDeclaration::new("acknowledge_ticket"),
            WorkflowStepDeclaration::new("await_resolution"),
        ],
    )
}

fn runtime_config(manifest: &ModuleManifest) -> ServiceRuntimeConfig {
    ServiceRuntimeConfig::new("support-sla", "primary", "support-sla")
        .with_module_manifests(std::slice::from_ref(manifest))
}

fn start_request(version: &str) -> Request<Body> {
    Request::post("/runtime/workflows/support-sla/ticket_sla/instances")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({
                "definitionVersion": version,
                "input": {"ticketId": "ticket_01"},
                "storyContext": {
                    "storyId": "story_support_01",
                    "segmentId": "segment_start_01"
                },
                "tenantScope": {"tenantId": "tenant_01"}
            })
            .to_string(),
        ))
        .unwrap()
}

async fn json_body(response: axum::response::Response) -> serde_json::Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn versioned_workflow_start_and_inspection_survive_runtime_restart() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let service = service();
    let manifest = manifest();
    let state = prepare_runtime(&service, &runtime_config(&manifest), db.pool.clone(), &[])
        .await
        .expect("Service runtime should prepare workflow storage");
    let app = service_router(OpenApiRouter::new(), state);

    let started_response = app.clone().oneshot(start_request("v1")).await.unwrap();
    assert_eq!(started_response.status(), StatusCode::CREATED);
    let started = json_body(started_response).await;
    assert_eq!(started["protocol"], "lenso.workflow-start-result.v1");
    assert_eq!(started["instance"]["definition"]["version"], "v1");
    assert_eq!(started["instance"]["state"], "running");
    assert_eq!(
        started["instance"]["steps"][0]["definitionStepName"],
        "acknowledge_ticket"
    );
    let instance_id = started["instance"]["instanceId"]
        .as_str()
        .unwrap()
        .to_owned();
    let initial_step_id = started["instance"]["initialStepId"]
        .as_str()
        .unwrap()
        .to_owned();
    let created_at = started["instance"]["createdAt"].clone();
    drop(app);

    let restarted_state =
        prepare_runtime(&service, &runtime_config(&manifest), db.pool.clone(), &[])
            .await
            .expect("restarted Service runtime should reuse its owned Store");
    let restarted_app = service_router(OpenApiRouter::new(), restarted_state);
    let inspected_response = restarted_app
        .clone()
        .oneshot(
            Request::get(format!("/runtime/workflows/instances/{instance_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(inspected_response.status(), StatusCode::OK);
    let inspected = json_body(inspected_response).await;

    assert_eq!(inspected["protocol"], "lenso.workflow-inspection.v1");
    assert_eq!(inspected["instance"]["instanceId"], instance_id);
    assert_eq!(inspected["instance"]["serviceId"], "support-sla");
    assert_eq!(inspected["instance"]["definition"]["owner"], "support-sla");
    assert_eq!(inspected["instance"]["definition"]["name"], "ticket_sla");
    assert_eq!(inspected["instance"]["definition"]["version"], "v1");
    assert_eq!(inspected["instance"]["state"], "running");
    assert_eq!(
        inspected["instance"]["storyContext"]["storyId"],
        "story_support_01"
    );
    assert_eq!(
        inspected["instance"]["tenantScope"]["tenantId"],
        "tenant_01"
    );
    assert_eq!(inspected["instance"]["initialStepId"], initial_step_id);
    assert_eq!(inspected["instance"]["steps"][0]["stepId"], initial_step_id);
    assert_eq!(inspected["instance"]["createdAt"], created_at);
    assert_eq!(inspected["instance"]["updatedAt"], created_at);

    let newer_response = restarted_app
        .clone()
        .oneshot(start_request("v2"))
        .await
        .unwrap();
    assert_eq!(newer_response.status(), StatusCode::CREATED);
    let newer = json_body(newer_response).await;
    assert_eq!(newer["instance"]["definition"]["version"], "v2");
    assert_ne!(newer["instance"]["instanceId"], instance_id);
    assert_eq!(inspected["instance"]["definition"]["version"], "v1");

    let unknown_response = restarted_app.oneshot(start_request("v3")).await.unwrap();
    assert_eq!(unknown_response.status(), StatusCode::NOT_FOUND);
    let unknown = json_body(unknown_response).await;
    assert_eq!(unknown["code"], "workflow_definition_version_not_found");
    assert_eq!(
        unknown["next_actions"][0],
        "select_registered_workflow_version"
    );

    let persisted_instances: i64 = sqlx::query_scalar(
        "select count(*) from platform.service_workflow_instances where service_id = 'support-sla'",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    let persisted_steps: i64 =
        sqlx::query_scalar("select count(*) from platform.service_workflow_steps")
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(persisted_instances, 2);
    assert_eq!(persisted_steps, 2);

    db.cleanup().await;
}
