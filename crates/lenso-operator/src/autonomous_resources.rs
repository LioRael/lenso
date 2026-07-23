use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, ensure};
use k8s_openapi::{
    api::{
        apps::v1::{Deployment, DeploymentSpec},
        autoscaling::v2::{
            CrossVersionObjectReference, HorizontalPodAutoscaler, HorizontalPodAutoscalerSpec,
            MetricSpec, MetricTarget, ResourceMetricSource,
        },
        batch::v1::{Job, JobSpec},
        core::v1::{
            ConfigMapEnvSource, Container, ContainerPort, EnvFromSource, HTTPGetAction, PodSpec,
            PodTemplateSpec, Probe, SecretEnvSource, Service, ServicePort, ServiceSpec,
        },
        networking::v1::{
            NetworkPolicy, NetworkPolicyIngressRule, NetworkPolicyPort, NetworkPolicySpec,
        },
        policy::v1::{PodDisruptionBudget, PodDisruptionBudgetSpec},
    },
    apimachinery::pkg::{
        apis::meta::v1::{LabelSelector, ObjectMeta, OwnerReference},
        util::intstr::IntOrString,
    },
};
use kube::ResourceExt;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{
    LensoAutonomousService, LensoAutonomousWorkload, OperatorSecretReference, OperatorWorkloadRole,
};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousMigrationGate {
    Pending,
    Complete,
    Failed,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutonomousServiceResources {
    pub migration_jobs: Vec<Job>,
    pub deployments: Vec<Deployment>,
    pub services: Vec<Service>,
    pub horizontal_pod_autoscalers: Vec<HorizontalPodAutoscaler>,
    pub pod_disruption_budgets: Vec<PodDisruptionBudget>,
    pub network_policies: Vec<NetworkPolicy>,
}

pub fn build_autonomous_service_resources(
    service: &LensoAutonomousService,
    migration_gate: AutonomousMigrationGate,
) -> Result<AutonomousServiceResources> {
    validate_service(service)?;
    let secrets = service
        .spec
        .secret_references
        .iter()
        .map(|reference| (reference.reference_id.as_str(), reference))
        .collect::<BTreeMap<_, _>>();
    let mut resources = AutonomousServiceResources::default();
    for workload in &service.spec.workloads {
        if workload.role == OperatorWorkloadRole::Migration {
            resources
                .migration_jobs
                .push(build_migration_job(service, workload, &secrets)?);
            continue;
        }
        if migration_gate != AutonomousMigrationGate::Complete {
            continue;
        }
        resources
            .deployments
            .push(build_workload_deployment(service, workload, &secrets)?);
        if workload.role == OperatorWorkloadRole::Api {
            resources
                .services
                .push(build_workload_service(service, workload)?);
        }
        if workload.scaling.max_replicas > workload.scaling.min_replicas {
            resources
                .horizontal_pod_autoscalers
                .push(build_workload_hpa(service, workload));
        }
        if let Some(min_available) = workload.disruption_min_available {
            resources.pod_disruption_budgets.push(build_workload_pdb(
                service,
                workload,
                min_available,
            ));
        }
        if workload.network_policy_enabled {
            resources
                .network_policies
                .push(build_workload_network_policy(service, workload));
        }
    }
    resources
        .migration_jobs
        .sort_by(|left, right| left.metadata.name.cmp(&right.metadata.name));
    resources
        .deployments
        .sort_by(|left, right| left.metadata.name.cmp(&right.metadata.name));
    resources
        .services
        .sort_by(|left, right| left.metadata.name.cmp(&right.metadata.name));
    Ok(resources)
}

fn validate_service(service: &LensoAutonomousService) -> Result<()> {
    ensure!(
        !service.spec.service_id.trim().is_empty(),
        "serviceId must not be empty"
    );
    ensure!(
        !service.spec.environment.trim().is_empty(),
        "environment must not be empty"
    );
    ensure!(
        valid_digest(&service.spec.release_digest),
        "releaseDigest must be sha256-pinned"
    );
    ensure!(
        service
            .spec
            .release_id
            .ends_with(&service.spec.release_digest),
        "releaseId must bind releaseDigest"
    );
    ensure!(
        !service.spec.workloads.is_empty(),
        "workloads must not be empty"
    );
    ensure!(
        service
            .spec
            .workloads
            .iter()
            .any(|workload| workload.role == OperatorWorkloadRole::Migration),
        "an Autonomous Service must declare a Migration Workload"
    );
    let secret_ids = service
        .spec
        .secret_references
        .iter()
        .map(|reference| reference.reference_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut workload_ids = BTreeSet::new();
    for workload in &service.spec.workloads {
        ensure!(
            workload_ids.insert(workload.workload_id.as_str()),
            "workloadId values must be unique"
        );
        ensure!(
            valid_image(&workload.image),
            "Workload images must be digest-pinned"
        );
        ensure!(workload.replicas >= 0, "replicas must not be negative");
        ensure!(
            workload.scaling.min_replicas >= 0
                && workload.scaling.max_replicas >= workload.scaling.min_replicas,
            "scaling bounds are invalid"
        );
        ensure!(
            workload
                .secret_reference_ids
                .iter()
                .all(|reference| secret_ids.contains(reference.as_str())),
            "Workload Secret References must be declared by the resource"
        );
        if workload.role == OperatorWorkloadRole::Api {
            ensure!(workload.port.is_some(), "API Workloads require a port");
        }
    }
    Ok(())
}

fn build_migration_job(
    service: &LensoAutonomousService,
    workload: &LensoAutonomousWorkload,
    secrets: &BTreeMap<&str, &OperatorSecretReference>,
) -> Result<Job> {
    let labels = workload_labels(service, workload);
    let mut job = Job {
        metadata: resource_metadata(service, workload, labels.clone()),
        spec: Some(JobSpec {
            backoff_limit: Some(1),
            template: PodTemplateSpec {
                metadata: Some(ObjectMeta {
                    labels: Some(labels),
                    annotations: Some(workload_annotations(service, workload)),
                    ..ObjectMeta::default()
                }),
                spec: Some(PodSpec {
                    restart_policy: Some("Never".to_owned()),
                    node_selector: non_empty_map(&workload.placement.node_selector),
                    containers: vec![workload_container(workload, secrets)?],
                    ..PodSpec::default()
                }),
            },
            ..JobSpec::default()
        }),
        ..Job::default()
    };
    let execution_digest = migration_job_execution_digest(&job);
    job.metadata
        .annotations
        .get_or_insert_with(BTreeMap::new)
        .insert(
            "lenso.dev/migration-execution-digest".to_owned(),
            execution_digest,
        );
    Ok(job)
}

/// Identifies the immutable, release-bound migration execution. Runtime
/// configuration and Secret rotations are deliberately excluded: a completed
/// migration is a receipt for the release and must not rerun during a
/// config-only rollout.
pub(crate) fn migration_job_execution_digest(job: &Job) -> String {
    let spec = job.spec.as_ref();
    let template = spec.map(|spec| &spec.template);
    let pod = template.and_then(|template| template.spec.as_ref());
    let annotations = template
        .and_then(|template| template.metadata.as_ref())
        .and_then(|metadata| metadata.annotations.as_ref());
    let labels = template
        .and_then(|template| template.metadata.as_ref())
        .and_then(|metadata| metadata.labels.as_ref());
    let containers = pod
        .map(|pod| {
            pod.containers
                .iter()
                .map(|container| {
                    json!({
                        "name": container.name,
                        "image": container.image,
                        "command": container.command,
                        "args": container.args,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let content = json!({
        "backoffLimit": spec.and_then(|spec| spec.backoff_limit),
        "releaseId": annotations.and_then(|values| values.get("lenso.dev/release-id")),
        "releaseDigest": annotations.and_then(|values| values.get("lenso.dev/release-digest")),
        "workloadId": labels.and_then(|values| values.get("lenso.dev/workload")),
        "pod": {
            "restartPolicy": pod.and_then(|pod| pod.restart_policy.as_ref()),
            "containers": containers,
        },
    });
    let digest =
        Sha256::digest(serde_json::to_vec(&content).expect("Migration template must serialize"));
    format!(
        "sha256:{}",
        digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn build_workload_deployment(
    service: &LensoAutonomousService,
    workload: &LensoAutonomousWorkload,
    secrets: &BTreeMap<&str, &OperatorSecretReference>,
) -> Result<Deployment> {
    let labels = workload_labels(service, workload);
    Ok(Deployment {
        metadata: resource_metadata(service, workload, labels.clone()),
        spec: Some(DeploymentSpec {
            // Once an HPA exists it owns the replicas field. Omitting it keeps
            // reconciliation from fighting the autoscaler on every apply.
            replicas: (workload.scaling.max_replicas == workload.scaling.min_replicas)
                .then_some(workload.replicas),
            selector: LabelSelector {
                match_labels: Some(selector_labels(service, workload)),
                ..LabelSelector::default()
            },
            template: PodTemplateSpec {
                metadata: Some(ObjectMeta {
                    labels: Some(labels),
                    annotations: Some(workload_annotations(service, workload)),
                    ..ObjectMeta::default()
                }),
                spec: Some(PodSpec {
                    node_selector: non_empty_map(&workload.placement.node_selector),
                    containers: vec![workload_container(workload, secrets)?],
                    ..PodSpec::default()
                }),
            },
            ..DeploymentSpec::default()
        }),
        ..Deployment::default()
    })
}

fn workload_container(
    workload: &LensoAutonomousWorkload,
    secrets: &BTreeMap<&str, &OperatorSecretReference>,
) -> Result<Container> {
    let port = workload.port.map(|port| {
        vec![ContainerPort {
            container_port: port,
            name: Some("http".to_owned()),
            protocol: Some("TCP".to_owned()),
            ..ContainerPort::default()
        }]
    });
    Ok(Container {
        name: safe_name(&workload.workload_id),
        image: Some(workload.image.clone()),
        command: (!workload.command.is_empty()).then(|| workload.command.clone()),
        ports: port,
        env_from: env_from(workload, secrets)?,
        readiness_probe: probe(workload.readiness_path.as_deref(), workload.port),
        liveness_probe: probe(workload.liveness_path.as_deref(), workload.port),
        ..Container::default()
    })
}

fn env_from(
    workload: &LensoAutonomousWorkload,
    secrets: &BTreeMap<&str, &OperatorSecretReference>,
) -> Result<Option<Vec<EnvFromSource>>> {
    let mut sources = Vec::new();
    if let Some(config_map) = &workload.config_map_name {
        sources.push(EnvFromSource {
            config_map_ref: Some(ConfigMapEnvSource {
                name: config_map.clone(),
                ..ConfigMapEnvSource::default()
            }),
            ..EnvFromSource::default()
        });
    }
    for reference_id in &workload.secret_reference_ids {
        let reference = secrets
            .get(reference_id.as_str())
            .ok_or_else(|| anyhow::anyhow!("undeclared Secret Reference `{reference_id}`"))?;
        sources.push(EnvFromSource {
            secret_ref: Some(SecretEnvSource {
                name: reference.target_name.clone(),
                ..SecretEnvSource::default()
            }),
            ..EnvFromSource::default()
        });
    }
    Ok((!sources.is_empty()).then_some(sources))
}

fn build_workload_service(
    service: &LensoAutonomousService,
    workload: &LensoAutonomousWorkload,
) -> Result<Service> {
    let port = workload
        .port
        .ok_or_else(|| anyhow::anyhow!("API Workload port is required"))?;
    Ok(Service {
        metadata: resource_metadata(service, workload, workload_labels(service, workload)),
        spec: Some(ServiceSpec {
            selector: Some(selector_labels(service, workload)),
            ports: Some(vec![ServicePort {
                name: Some("http".to_owned()),
                port,
                target_port: Some(IntOrString::String("http".to_owned())),
                ..ServicePort::default()
            }]),
            ..ServiceSpec::default()
        }),
        ..Service::default()
    })
}

fn build_workload_hpa(
    service: &LensoAutonomousService,
    workload: &LensoAutonomousWorkload,
) -> HorizontalPodAutoscaler {
    HorizontalPodAutoscaler {
        metadata: resource_metadata(service, workload, workload_labels(service, workload)),
        spec: HorizontalPodAutoscalerSpec {
            max_replicas: workload.scaling.max_replicas,
            min_replicas: Some(workload.scaling.min_replicas),
            scale_target_ref: CrossVersionObjectReference {
                api_version: Some("apps/v1".to_owned()),
                kind: "Deployment".to_owned(),
                name: resource_name(service, workload),
            },
            metrics: Some(vec![MetricSpec {
                type_: "Resource".to_owned(),
                resource: Some(ResourceMetricSource {
                    name: "cpu".to_owned(),
                    target: MetricTarget {
                        type_: "Utilization".to_owned(),
                        average_utilization: Some(workload.scaling.target_cpu_utilization),
                        ..MetricTarget::default()
                    },
                }),
                ..MetricSpec::default()
            }]),
            ..HorizontalPodAutoscalerSpec::default()
        },
        ..HorizontalPodAutoscaler::default()
    }
}

fn build_workload_pdb(
    service: &LensoAutonomousService,
    workload: &LensoAutonomousWorkload,
    min_available: i32,
) -> PodDisruptionBudget {
    PodDisruptionBudget {
        metadata: resource_metadata(service, workload, workload_labels(service, workload)),
        spec: Some(PodDisruptionBudgetSpec {
            min_available: Some(IntOrString::Int(min_available)),
            selector: Some(LabelSelector {
                match_labels: Some(selector_labels(service, workload)),
                ..LabelSelector::default()
            }),
            ..PodDisruptionBudgetSpec::default()
        }),
        ..PodDisruptionBudget::default()
    }
}

fn build_workload_network_policy(
    service: &LensoAutonomousService,
    workload: &LensoAutonomousWorkload,
) -> NetworkPolicy {
    let ports = workload.port.map(|port| {
        vec![NetworkPolicyPort {
            port: Some(IntOrString::Int(port)),
            protocol: Some("TCP".to_owned()),
            ..NetworkPolicyPort::default()
        }]
    });
    NetworkPolicy {
        metadata: resource_metadata(service, workload, workload_labels(service, workload)),
        spec: Some(NetworkPolicySpec {
            pod_selector: Some(LabelSelector {
                match_labels: Some(selector_labels(service, workload)),
                ..LabelSelector::default()
            }),
            ingress: Some(vec![NetworkPolicyIngressRule {
                ports,
                ..NetworkPolicyIngressRule::default()
            }]),
            policy_types: Some(vec!["Ingress".to_owned()]),
            ..NetworkPolicySpec::default()
        }),
        ..NetworkPolicy::default()
    }
}

fn probe(path: Option<&str>, port: Option<i32>) -> Option<Probe> {
    Some(Probe {
        http_get: Some(HTTPGetAction {
            path: Some(path?.to_owned()),
            port: IntOrString::Int(port?),
            scheme: Some("HTTP".to_owned()),
            ..HTTPGetAction::default()
        }),
        ..Probe::default()
    })
}

fn resource_metadata(
    service: &LensoAutonomousService,
    workload: &LensoAutonomousWorkload,
    labels: BTreeMap<String, String>,
) -> ObjectMeta {
    ObjectMeta {
        name: Some(resource_name(service, workload)),
        namespace: service.namespace(),
        labels: Some(labels),
        annotations: Some(workload_annotations(service, workload)),
        owner_references: owner_reference(service),
        ..ObjectMeta::default()
    }
}

fn owner_reference(service: &LensoAutonomousService) -> Option<Vec<OwnerReference>> {
    Some(vec![OwnerReference {
        api_version: "lenso.dev/v1alpha1".to_owned(),
        kind: "LensoAutonomousService".to_owned(),
        name: service.name_any(),
        uid: service.metadata.uid.clone()?,
        controller: Some(true),
        block_owner_deletion: Some(true),
    }])
}

fn workload_labels(
    service: &LensoAutonomousService,
    workload: &LensoAutonomousWorkload,
) -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "app.kubernetes.io/name".to_owned(),
            safe_name(&service.spec.service_id),
        ),
        ("app.kubernetes.io/part-of".to_owned(), "lenso".to_owned()),
        (
            "lenso.dev/autonomous-service".to_owned(),
            service.name_any(),
        ),
        (
            "lenso.dev/workload".to_owned(),
            workload.workload_id.clone(),
        ),
        (
            "lenso.dev/workload-role".to_owned(),
            role_name(workload.role).to_owned(),
        ),
        (
            "lenso.dev/environment".to_owned(),
            service.spec.environment.clone(),
        ),
    ])
}

fn selector_labels(
    service: &LensoAutonomousService,
    workload: &LensoAutonomousWorkload,
) -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "lenso.dev/autonomous-service".to_owned(),
            service.name_any(),
        ),
        (
            "lenso.dev/workload".to_owned(),
            workload.workload_id.clone(),
        ),
    ])
}

fn workload_annotations(
    service: &LensoAutonomousService,
    workload: &LensoAutonomousWorkload,
) -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "lenso.dev/release-id".to_owned(),
            service.spec.release_id.clone(),
        ),
        (
            "lenso.dev/release-digest".to_owned(),
            service.spec.release_digest.clone(),
        ),
        (
            "lenso.dev/config-revision-id".to_owned(),
            service.spec.config_revision_id.clone(),
        ),
        (
            "lenso.dev/secret-reference-ids".to_owned(),
            workload.secret_reference_ids.join(","),
        ),
    ])
}

fn resource_name(service: &LensoAutonomousService, workload: &LensoAutonomousWorkload) -> String {
    let base = format!(
        "{}-{}",
        service.name_any(),
        safe_name(&workload.workload_id)
    );
    if workload.role == OperatorWorkloadRole::Migration {
        let release_suffix = service
            .spec
            .release_digest
            .strip_prefix("sha256:")
            .unwrap_or(&service.spec.release_digest)
            .chars()
            .take(12)
            .collect::<String>();
        migration_job_name(&base, &release_suffix)
    } else {
        base
    }
}

fn migration_job_name(base: &str, release_suffix: &str) -> String {
    const LABEL_VALUE_LIMIT: usize = 63;
    const BASE_DIGEST_LENGTH: usize = 8;
    let unbounded = format!("{base}-{release_suffix}");
    if unbounded.len() <= LABEL_VALUE_LIMIT {
        return unbounded;
    }
    let base_digest = Sha256::digest(base.as_bytes())
        .into_iter()
        .take(BASE_DIGEST_LENGTH / 2)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let base_limit = LABEL_VALUE_LIMIT
        .saturating_sub(release_suffix.len())
        .saturating_sub(base_digest.len())
        .saturating_sub(2);
    let bounded_base = base
        .chars()
        .take(base_limit)
        .collect::<String>()
        .trim_end_matches('-')
        .to_owned();
    format!("{bounded_base}-{base_digest}-{release_suffix}")
}

fn safe_name(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

fn role_name(role: OperatorWorkloadRole) -> &'static str {
    match role {
        OperatorWorkloadRole::Api => "api",
        OperatorWorkloadRole::Worker => "worker",
        OperatorWorkloadRole::Migration => "migration",
        OperatorWorkloadRole::Extension => "extension",
    }
}

fn valid_image(image: &str) -> bool {
    image
        .rsplit_once("@")
        .is_some_and(|(reference, digest)| !reference.trim().is_empty() && valid_digest(digest))
}

fn valid_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn non_empty_map(map: &BTreeMap<String, String>) -> Option<BTreeMap<String, String>> {
    (!map.is_empty()).then(|| map.clone())
}
