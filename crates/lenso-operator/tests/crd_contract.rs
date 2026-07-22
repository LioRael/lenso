use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;
use kube::CustomResourceExt;
use lenso_operator::{
    AutonomousMigrationGate, AutonomousServiceObservation, AutonomousWorkloadObservation,
    LensoAutonomousService, LensoAutonomousServiceSpec, LensoAutonomousServiceState,
    LensoAutonomousWorkload, LensoServiceProvider, LensoServiceProviderCondition,
    LensoServiceProviderEnvFrom, LensoServiceProviderSpec, LensoServiceProviderState,
    LensoServiceProviderStatus, OperatorPlacement, OperatorScaling, OperatorSecretReference,
    OperatorWorkloadRole, build_autonomous_service_resources, observed_autonomous_service_status,
};

#[test]
fn crd_has_expected_contract() {
    let crd = LensoServiceProvider::crd();

    assert_eq!(
        crd.metadata.name.as_deref(),
        Some("lensoserviceproviders.lenso.dev")
    );
    assert_eq!(crd.spec.group, "lenso.dev");
    assert_eq!(crd.spec.names.kind, "LensoServiceProvider");
    assert_eq!(crd.spec.names.plural, "lensoserviceproviders");
    assert_eq!(crd.spec.scope, "Namespaced");
}

#[test]
fn spec_serializes_camel_case() {
    let value = serde_json::to_value(LensoServiceProvider::new("payments", spec())).unwrap();
    let spec = &value["spec"];

    assert_eq!(spec["serviceName"], "payments");
    assert_eq!(spec["releaseId"], "rel-1");
    assert_eq!(spec["manifestReference"], "oci://example/payments:v1");
    assert_eq!(spec["envFrom"]["configMap"], "payments-config");
    assert!(spec.get("service_name").is_none());
    assert!(spec.get("release_id").is_none());
}

#[test]
fn status_serializes_ready_state_and_condition_type() {
    let status = LensoServiceProviderStatus {
        state: LensoServiceProviderState::Ready,
        observed_generation: Some(7),
        observed_release_id: Some("rel-1".to_owned()),
        observed_image: Some("ghcr.io/lenso/payments:v1".to_owned()),
        ready_replicas: Some(2),
        desired_replicas: Some(2),
        available_replicas: Some(2),
        manifest_reference: Some("oci://example/payments:v1".to_owned()),
        conditions: vec![LensoServiceProviderCondition {
            type_: "Available".to_owned(),
            status: "True".to_owned(),
            reason: "DeploymentReady".to_owned(),
            message: "ready".to_owned(),
            last_transition_time: Time(k8s_openapi::jiff::Timestamp::now()),
        }],
    };

    let value = serde_json::to_value(status).unwrap();
    assert_eq!(value["state"], "ready");
    assert_eq!(value["observedGeneration"], 7);
    assert_eq!(value["conditions"][0]["type"], "Available");
    assert!(value["conditions"][0].get("type_").is_none());
}

#[test]
fn autonomous_service_crd_is_distinct_and_migration_gates_dependents() {
    let crd = LensoAutonomousService::crd();
    assert_eq!(
        crd.metadata.name.as_deref(),
        Some("lensoautonomousservices.lenso.dev")
    );
    assert_eq!(crd.spec.names.kind, "LensoAutonomousService");
    assert_ne!(crd.spec.names.kind, "LensoServiceProvider");

    let service = autonomous_service();
    let migration_only =
        build_autonomous_service_resources(&service, AutonomousMigrationGate::Pending)
            .expect("pending resources should build");
    assert_eq!(migration_only.migration_jobs.len(), 1);
    assert!(migration_only.deployments.is_empty());
    assert!(migration_only.services.is_empty());

    let converging =
        build_autonomous_service_resources(&service, AutonomousMigrationGate::Complete)
            .expect("complete migration should unlock dependents");
    assert_eq!(converging.migration_jobs.len(), 1);
    assert_eq!(converging.deployments.len(), 2);
    assert_eq!(converging.services.len(), 1);
    assert!(
        converging
            .deployments
            .iter()
            .all(|deployment| { deployment.spec.as_ref().unwrap().replicas.is_none() }),
        "HPA-managed Workloads must leave replicas ownership to the autoscaler"
    );
    assert!(converging.deployments.iter().all(|deployment| {
        deployment
            .spec
            .as_ref()
            .unwrap()
            .template
            .spec
            .as_ref()
            .unwrap()
            .containers[0]
            .image
            .as_deref()
            .unwrap()
            .contains("@sha256:")
    }));
    let rendered = serde_json::to_string(&converging).expect("resources should serialize");
    assert!(!rendered.contains("secretValue"));
    assert!(!rendered.contains("database-password"));

    let mut previous_release = service.clone();
    previous_release.spec.release_digest = digest("previous-release");
    previous_release.spec.release_id =
        format!("service-release:{}", previous_release.spec.release_digest);
    let previous_resources =
        build_autonomous_service_resources(&previous_release, AutonomousMigrationGate::Pending)
            .expect("rollback resources should build");
    assert_ne!(
        migration_only.migration_jobs[0].metadata.name,
        previous_resources.migration_jobs[0].metadata.name,
        "each immutable release needs an independent migration Job"
    );

    let migrating = observed_autonomous_service_status(
        &service,
        &AutonomousServiceObservation {
            migrations: vec![workload_observation(
                &service,
                "support-migration",
                false,
                Some(digest("support-migration")),
            )],
            workloads: Vec::new(),
            fresh: true,
        },
    );
    assert_eq!(migrating.state, LensoAutonomousServiceState::Migrating);
    assert_eq!(migrating.conditions[0].reason, "MigrationIncomplete");
    assert_eq!(migrating.issues[0].code, "migration_incomplete");
    assert!(!migrating.next_actions.is_empty());

    let ready = observed_autonomous_service_status(
        &service,
        &AutonomousServiceObservation {
            migrations: vec![workload_observation(
                &service,
                "support-migration",
                true,
                Some(digest("support-migration")),
            )],
            workloads: vec![
                workload_observation(&service, "support-api", true, Some(digest("support-api"))),
                workload_observation(
                    &service,
                    "support-worker",
                    true,
                    Some(digest("support-worker")),
                ),
            ],
            fresh: true,
        },
    );
    assert_eq!(ready.state, LensoAutonomousServiceState::Ready);
    assert_eq!(ready.observed_release_id, service.spec.release_id);
    assert_eq!(ready.config_revision_id, service.spec.config_revision_id);
    assert!(!ready.drifted);
    assert!(ready.issues.is_empty());

    let mut completed_migration = workload_observation(
        &service,
        "support-migration",
        true,
        Some(digest("support-migration")),
    );
    completed_migration.observed_config_revision_id = Some("config-revision:previous".to_owned());
    let config_only_rollout = observed_autonomous_service_status(
        &service,
        &AutonomousServiceObservation {
            migrations: vec![completed_migration],
            workloads: vec![
                workload_observation(&service, "support-api", true, Some(digest("support-api"))),
                workload_observation(
                    &service,
                    "support-worker",
                    true,
                    Some(digest("support-worker")),
                ),
            ],
            fresh: true,
        },
    );
    assert_eq!(
        config_only_rollout.state,
        LensoAutonomousServiceState::Ready
    );
    assert_eq!(
        config_only_rollout.config_revision_id,
        service.spec.config_revision_id
    );

    let drifted = observed_autonomous_service_status(
        &service,
        &AutonomousServiceObservation {
            migrations: vec![workload_observation(
                &service,
                "support-migration",
                true,
                None,
            )],
            workloads: Vec::new(),
            fresh: false,
        },
    );
    assert_eq!(drifted.state, LensoAutonomousServiceState::Progressing);
    assert!(drifted.drifted);
    assert_eq!(drifted.conditions[0].reason, "ObservationStaleOrDrifted");
}

fn workload_observation(
    service: &LensoAutonomousService,
    workload_id: &str,
    ready: bool,
    observed_digest: Option<String>,
) -> AutonomousWorkloadObservation {
    AutonomousWorkloadObservation {
        workload_id: workload_id.to_owned(),
        ready,
        failed: false,
        observed_digest,
        observed_release_id: Some(service.spec.release_id.clone()),
        observed_release_digest: Some(service.spec.release_digest.clone()),
        observed_config_revision_id: Some(service.spec.config_revision_id.clone()),
        fresh: true,
    }
}

fn autonomous_service() -> LensoAutonomousService {
    let mut resource = LensoAutonomousService::new(
        "support",
        LensoAutonomousServiceSpec {
            service_id: "service:support".to_owned(),
            environment: "staging".to_owned(),
            release_id: format!("service-release:{}", digest("release")),
            release_digest: digest("release"),
            config_revision_id: format!("config-revision:{}", digest("config")),
            expected_environment_revision: 7,
            secret_references: vec![OperatorSecretReference {
                reference_id: "secret-ref:support-db".to_owned(),
                provider: "kubernetes".to_owned(),
                target_name: "support-db".to_owned(),
            }],
            policy_evidence_references: vec!["policy-evidence:support".to_owned()],
            evidence_references: vec!["environment-verification:staging".to_owned()],
            workloads: vec![
                autonomous_workload("support-migration", OperatorWorkloadRole::Migration, None),
                autonomous_workload("support-api", OperatorWorkloadRole::Api, Some(8080)),
                autonomous_workload("support-worker", OperatorWorkloadRole::Worker, None),
            ],
            rollout_strategy: "bounded_canary".to_owned(),
            rollback_release_id: Some(format!("service-release:{}", digest("previous"))),
        },
    );
    resource.metadata.namespace = Some("lenso-m5-staging".to_owned());
    resource.metadata.uid = Some("autonomous-service-uid".to_owned());
    resource
}

fn autonomous_workload(
    workload_id: &str,
    role: OperatorWorkloadRole,
    port: Option<i32>,
) -> LensoAutonomousWorkload {
    LensoAutonomousWorkload {
        workload_id: workload_id.to_owned(),
        role,
        image: format!("ghcr.io/liorael/support@{}", digest(workload_id)),
        replicas: 1,
        port,
        command: Vec::new(),
        config_map_name: Some("support-config".to_owned()),
        secret_reference_ids: vec!["secret-ref:support-db".to_owned()],
        placement: OperatorPlacement {
            node_selector: std::collections::BTreeMap::from([(
                "topology.kubernetes.io/region".to_owned(),
                "local-1".to_owned(),
            )]),
        },
        scaling: OperatorScaling {
            min_replicas: 1,
            max_replicas: 3,
            target_cpu_utilization: 70,
        },
        disruption_min_available: Some(1),
        network_policy_enabled: true,
        readiness_path: port.map(|_| "/health/ready".to_owned()),
        liveness_path: port.map(|_| "/health/live".to_owned()),
    }
}

fn digest(value: &str) -> String {
    let seed = value
        .bytes()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let repeated = seed.repeat(64 / seed.len() + 1);
    format!("sha256:{}", &repeated[..64])
}

fn spec() -> LensoServiceProviderSpec {
    LensoServiceProviderSpec {
        service_name: "payments".to_owned(),
        environment: "prod".to_owned(),
        image: "ghcr.io/lenso/payments:v1".to_owned(),
        release_id: Some("rel-1".to_owned()),
        manifest_reference: Some("oci://example/payments:v1".to_owned()),
        modules: vec!["auth".to_owned(), "story".to_owned()],
        replicas: 2,
        port: 8080,
        env_from: Some(LensoServiceProviderEnvFrom {
            config_map: Some("payments-config".to_owned()),
            secret: Some("payments-secret".to_owned()),
        }),
        ingress: None,
        autoscaling: None,
        disruption_budget: None,
        network_policy: None,
    }
}
