use std::{collections::BTreeSet, fmt::Debug, sync::Arc, time::Duration};

use futures::TryStreamExt;
use k8s_openapi::api::{
    apps::v1::Deployment, autoscaling::v2::HorizontalPodAutoscaler, batch::v1::Job,
    core::v1::Service, networking::v1::NetworkPolicy, policy::v1::PodDisruptionBudget,
};
use kube::{
    Api, Client, Resource, ResourceExt,
    api::{DeleteParams, ListParams, Patch, PatchParams},
    runtime::{Controller, controller::Action, watcher},
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::json;
use thiserror::Error;
use tracing::{error, info};

use crate::autonomous_resources::migration_job_execution_digest;
use crate::{
    AutonomousMigrationGate, AutonomousServiceObservation, AutonomousWorkloadObservation,
    LensoAutonomousService, build_autonomous_service_resources, observed_autonomous_service_status,
};

const FIELD_MANAGER: &str = "lenso-operator-autonomous";

#[derive(Clone)]
struct AutonomousReconcileContext {
    client: Client,
}

impl Debug for AutonomousReconcileContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AutonomousReconcileContext")
            .finish()
    }
}

#[derive(Debug, Error)]
enum AutonomousReconcileError {
    #[error(transparent)]
    Kube(#[from] kube::Error),
    #[error(transparent)]
    Build(#[from] anyhow::Error),
}

pub async fn run_autonomous(client: Client, namespace: Option<String>) -> anyhow::Result<()> {
    let services = match namespace {
        Some(namespace) => Api::<LensoAutonomousService>::namespaced(client.clone(), &namespace),
        None => Api::<LensoAutonomousService>::all(client.clone()),
    };
    Controller::new(services, watcher::Config::default())
        .shutdown_on_signal()
        .run(
            reconcile,
            error_policy,
            Arc::new(AutonomousReconcileContext { client }),
        )
        .try_for_each(|result| async move {
            let (object_ref, action) = result;
            info!(?object_ref, ?action, "reconciled LensoAutonomousService");
            Ok(())
        })
        .await?;
    Ok(())
}

async fn reconcile(
    service: Arc<LensoAutonomousService>,
    context: Arc<AutonomousReconcileContext>,
) -> Result<Action, AutonomousReconcileError> {
    let namespace = service
        .namespace()
        .ok_or_else(|| anyhow::anyhow!("LensoAutonomousService namespace is required"))?;
    let pending = build_autonomous_service_resources(&service, AutonomousMigrationGate::Pending)?;
    let jobs = Api::<Job>::namespaced(context.client.clone(), &namespace);
    for job in &pending.migration_jobs {
        apply_owned_once(&jobs, &service, job).await?;
    }
    // Migration Jobs are immutable release receipts. Retain completed Jobs so rollback or
    // reconciliation can never recreate and rerun an already applied migration.
    let migrations = observe_migrations(&jobs, &pending.migration_jobs).await?;
    let migration_gate = migration_gate(&migrations);

    let desired = build_autonomous_service_resources(&service, migration_gate)?;
    if migration_gate == AutonomousMigrationGate::Complete {
        apply_dependents(&context.client, &namespace, &service, &desired).await?;
    }
    let workloads = if migration_gate == AutonomousMigrationGate::Complete {
        observe_workloads(&context.client, &namespace, &service, &desired.deployments).await?
    } else {
        Vec::new()
    };
    let fresh = migrations
        .iter()
        .chain(workloads.iter())
        .all(|observation| observation.fresh);
    let status = observed_autonomous_service_status(
        &service,
        &AutonomousServiceObservation {
            migrations,
            workloads,
            fresh,
        },
    );
    Api::<LensoAutonomousService>::namespaced(context.client.clone(), &namespace)
        .patch_status(
            &service.name_any(),
            &PatchParams::default(),
            &Patch::Merge(&json!({ "status": status })),
        )
        .await?;
    Ok(Action::requeue(Duration::from_secs(30)))
}

fn migration_gate(migrations: &[AutonomousWorkloadObservation]) -> AutonomousMigrationGate {
    if migrations
        .iter()
        .any(|item| item.failed || item.ready && !item.fresh)
    {
        AutonomousMigrationGate::Failed
    } else if migrations.iter().all(|item| item.ready && item.fresh) {
        AutonomousMigrationGate::Complete
    } else {
        AutonomousMigrationGate::Pending
    }
}

fn error_policy(
    service: Arc<LensoAutonomousService>,
    error: &AutonomousReconcileError,
    _context: Arc<AutonomousReconcileContext>,
) -> Action {
    error!(
        name = %service.name_any(),
        error = %error,
        "failed to reconcile LensoAutonomousService"
    );
    Action::requeue(Duration::from_secs(30))
}

async fn apply_dependents(
    client: &Client,
    namespace: &str,
    owner: &LensoAutonomousService,
    desired: &crate::AutonomousServiceResources,
) -> Result<(), AutonomousReconcileError> {
    let deployments = Api::<Deployment>::namespaced(client.clone(), namespace);
    for resource in &desired.deployments {
        apply_owned(&deployments, owner, resource).await?;
    }
    prune_owned(&deployments, owner, &desired.deployments).await?;
    let services = Api::<Service>::namespaced(client.clone(), namespace);
    for resource in &desired.services {
        apply_owned(&services, owner, resource).await?;
    }
    prune_owned(&services, owner, &desired.services).await?;
    let hpas = Api::<HorizontalPodAutoscaler>::namespaced(client.clone(), namespace);
    for resource in &desired.horizontal_pod_autoscalers {
        apply_owned(&hpas, owner, resource).await?;
    }
    prune_owned(&hpas, owner, &desired.horizontal_pod_autoscalers).await?;
    let pdbs = Api::<PodDisruptionBudget>::namespaced(client.clone(), namespace);
    for resource in &desired.pod_disruption_budgets {
        apply_owned(&pdbs, owner, resource).await?;
    }
    prune_owned(&pdbs, owner, &desired.pod_disruption_budgets).await?;
    let network_policies = Api::<NetworkPolicy>::namespaced(client.clone(), namespace);
    for resource in &desired.network_policies {
        apply_owned(&network_policies, owner, resource).await?;
    }
    prune_owned(&network_policies, owner, &desired.network_policies).await?;
    Ok(())
}

async fn prune_owned<K>(
    api: &Api<K>,
    owner: &LensoAutonomousService,
    desired: &[K],
) -> Result<(), AutonomousReconcileError>
where
    K: Clone + Debug + DeserializeOwned + Serialize + Resource<DynamicType = ()>,
{
    let selector = format!("lenso.dev/autonomous-service={}", owner.name_any());
    let existing = api.list(&ListParams::default().labels(&selector)).await?;
    for name in obsolete_owned_resource_names(&existing.items, desired, owner) {
        api.delete(&name, &DeleteParams::default()).await?;
    }
    Ok(())
}

fn obsolete_owned_resource_names<K>(
    existing: &[K],
    desired: &[K],
    owner: &LensoAutonomousService,
) -> Vec<String>
where
    K: Resource<DynamicType = ()>,
{
    let desired_names = desired
        .iter()
        .map(ResourceExt::name_any)
        .collect::<BTreeSet<_>>();
    existing
        .iter()
        .filter(|resource| owned_by(*resource, owner))
        .map(ResourceExt::name_any)
        .filter(|name| !desired_names.contains(name))
        .collect()
}

async fn apply_owned<K>(
    api: &Api<K>,
    owner: &LensoAutonomousService,
    desired: &K,
) -> Result<(), AutonomousReconcileError>
where
    K: Clone + Debug + DeserializeOwned + Serialize + Resource<DynamicType = ()>,
{
    let name = desired.name_any();
    if let Some(existing) = api.get_opt(&name).await?
        && !owned_by(&existing, owner)
    {
        return Err(anyhow::anyhow!(
            "resource `{name}` exists and is not owned by LensoAutonomousService `{}`",
            owner.name_any()
        )
        .into());
    }
    api.patch(
        &name,
        &PatchParams::apply(FIELD_MANAGER).force(),
        &Patch::Apply(desired),
    )
    .await?;
    Ok(())
}

async fn apply_owned_once<K>(
    api: &Api<K>,
    owner: &LensoAutonomousService,
    desired: &K,
) -> Result<(), AutonomousReconcileError>
where
    K: Clone + Debug + DeserializeOwned + Serialize + Resource<DynamicType = ()>,
{
    let name = desired.name_any();
    if let Some(existing) = api.get_opt(&name).await? {
        if !owned_by(&existing, owner) {
            return Err(anyhow::anyhow!(
                "resource `{name}` exists and is not owned by LensoAutonomousService `{}`",
                owner.name_any()
            )
            .into());
        }
        return Ok(());
    }
    apply_owned(api, owner, desired).await
}

fn owned_by<K>(resource: &K, owner: &LensoAutonomousService) -> bool
where
    K: Resource<DynamicType = ()>,
{
    let Some(owner_uid) = owner.meta().uid.as_deref() else {
        return false;
    };
    resource
        .meta()
        .owner_references
        .as_ref()
        .into_iter()
        .flatten()
        .any(|reference| {
            reference.api_version == "lenso.dev/v1alpha1"
                && reference.kind == "LensoAutonomousService"
                && reference.name == owner.name_any()
                && reference.uid == owner_uid
        })
}

async fn observe_migrations(
    api: &Api<Job>,
    desired: &[Job],
) -> Result<Vec<AutonomousWorkloadObservation>, kube::Error> {
    let mut observations = Vec::new();
    for job in desired {
        let current = api.get_opt(&job.name_any()).await?;
        let status = current.as_ref().and_then(|item| item.status.as_ref());
        let workload_id = job
            .metadata
            .labels
            .as_ref()
            .and_then(|labels| labels.get("lenso.dev/workload"))
            .cloned()
            .unwrap_or_else(|| job.name_any());
        let complete = status
            .and_then(|value| value.conditions.as_ref())
            .into_iter()
            .flatten()
            .any(|condition| condition.type_ == "Complete" && condition.status == "True");
        let failed = !complete
            && status
                .and_then(|value| value.conditions.as_ref())
                .into_iter()
                .flatten()
                .any(|condition| condition.type_ == "Failed" && condition.status == "True");
        observations.push(AutonomousWorkloadObservation {
            workload_id: workload_id.clone(),
            ready: complete,
            failed,
            observed_digest: current.as_ref().and_then(job_image_digest),
            observed_release_id: current
                .as_ref()
                .and_then(|job| job_template_annotation(job, "lenso.dev/release-id")),
            observed_release_digest: current
                .as_ref()
                .and_then(|job| job_template_annotation(job, "lenso.dev/release-digest")),
            observed_config_revision_id: current
                .as_ref()
                .and_then(|job| job_template_annotation(job, "lenso.dev/config-revision-id")),
            fresh: current
                .as_ref()
                .is_some_and(|current| migration_job_is_fresh(current, job)),
        });
    }
    Ok(observations)
}

fn migration_job_is_fresh(current: &Job, desired: &Job) -> bool {
    let expected_execution_digest = job_annotation(desired, "lenso.dev/migration-execution-digest");
    expected_execution_digest.is_some()
        && expected_execution_digest == Some(migration_job_execution_digest(desired))
        && job_annotation(current, "lenso.dev/migration-execution-digest")
            == expected_execution_digest
        && Some(migration_job_execution_digest(current)) == expected_execution_digest
        && job_template_annotation(current, "lenso.dev/release-id")
            == job_template_annotation(desired, "lenso.dev/release-id")
        && job_template_annotation(current, "lenso.dev/release-digest")
            == job_template_annotation(desired, "lenso.dev/release-digest")
}

fn job_annotation(job: &Job, key: &str) -> Option<String> {
    job.metadata.annotations.as_ref()?.get(key).cloned()
}

async fn observe_workloads(
    client: &Client,
    namespace: &str,
    service: &LensoAutonomousService,
    desired: &[Deployment],
) -> Result<Vec<AutonomousWorkloadObservation>, kube::Error> {
    let api = Api::<Deployment>::namespaced(client.clone(), namespace);
    let mut observations = Vec::new();
    for deployment in desired {
        let current = api.get_opt(&deployment.name_any()).await?;
        let status = current.as_ref().and_then(|item| item.status.as_ref());
        let workload_id = deployment
            .metadata
            .labels
            .as_ref()
            .and_then(|labels| labels.get("lenso.dev/workload"))
            .cloned()
            .unwrap_or_else(|| deployment.name_any());
        let minimum_ready = service
            .spec
            .workloads
            .iter()
            .find(|workload| workload.workload_id == workload_id)
            .map_or(1, |workload| workload.scaling.min_replicas.max(1));
        let generation_fresh = current.as_ref().is_some_and(|item| {
            item.status
                .as_ref()
                .and_then(|status| status.observed_generation)
                .zip(item.metadata.generation)
                .is_some_and(|(observed, desired)| observed >= desired)
        });
        let desired_replicas = current
            .as_ref()
            .and_then(|item| item.spec.as_ref())
            .and_then(|spec| spec.replicas)
            .unwrap_or(minimum_ready);
        let total_replicas = status.and_then(|value| value.replicas).unwrap_or(0);
        let ready_replicas = status.and_then(|value| value.ready_replicas).unwrap_or(0);
        let updated_replicas = status.and_then(|value| value.updated_replicas).unwrap_or(0);
        let available_replicas = status
            .and_then(|value| value.available_replicas)
            .unwrap_or(0);
        let unavailable_replicas = status
            .and_then(|value| value.unavailable_replicas)
            .unwrap_or(0);
        let template_matches = current.as_ref().is_some_and(|current| {
            current
                .spec
                .as_ref()
                .and_then(|spec| spec.template.metadata.as_ref())
                .map(|metadata| &metadata.annotations)
                == deployment
                    .spec
                    .as_ref()
                    .and_then(|spec| spec.template.metadata.as_ref())
                    .map(|metadata| &metadata.annotations)
                && deployment_image_digest(current) == deployment_image_digest(deployment)
        });
        observations.push(AutonomousWorkloadObservation {
            workload_id: workload_id.clone(),
            ready: generation_fresh
                && template_matches
                && replica_set_converged(
                    desired_replicas,
                    total_replicas,
                    updated_replicas,
                    ready_replicas,
                    available_replicas,
                    unavailable_replicas,
                ),
            failed: false,
            observed_digest: current.as_ref().and_then(deployment_image_digest),
            observed_release_id: current.as_ref().and_then(|deployment| {
                deployment_template_annotation(deployment, "lenso.dev/release-id")
            }),
            observed_release_digest: current.as_ref().and_then(|deployment| {
                deployment_template_annotation(deployment, "lenso.dev/release-digest")
            }),
            observed_config_revision_id: current.as_ref().and_then(|deployment| {
                deployment_template_annotation(deployment, "lenso.dev/config-revision-id")
            }),
            fresh: generation_fresh && template_matches,
        });
    }
    Ok(observations)
}

const fn replica_set_converged(
    desired_replicas: i32,
    total_replicas: i32,
    updated_replicas: i32,
    ready_replicas: i32,
    available_replicas: i32,
    unavailable_replicas: i32,
) -> bool {
    total_replicas >= desired_replicas
        && updated_replicas == total_replicas
        && ready_replicas == total_replicas
        && available_replicas == total_replicas
        && unavailable_replicas == 0
}

fn job_image_digest(job: &Job) -> Option<String> {
    job.spec
        .as_ref()?
        .template
        .spec
        .as_ref()?
        .containers
        .first()?
        .image
        .as_deref()?
        .rsplit_once('@')
        .map(|(_, digest)| digest.to_owned())
}

fn deployment_image_digest(deployment: &Deployment) -> Option<String> {
    deployment
        .spec
        .as_ref()?
        .template
        .spec
        .as_ref()?
        .containers
        .first()?
        .image
        .as_deref()?
        .rsplit_once('@')
        .map(|(_, digest)| digest.to_owned())
}

fn job_template_annotation(job: &Job, key: &str) -> Option<String> {
    job.spec
        .as_ref()?
        .template
        .metadata
        .as_ref()?
        .annotations
        .as_ref()?
        .get(key)
        .cloned()
}

fn deployment_template_annotation(deployment: &Deployment, key: &str) -> Option<String> {
    deployment
        .spec
        .as_ref()?
        .template
        .metadata
        .as_ref()?
        .annotations
        .as_ref()?
        .get(key)
        .cloned()
}

#[cfg(test)]
mod tests {
    use k8s_openapi::{
        api::{
            apps::v1::{Deployment, DeploymentSpec},
            batch::v1::{Job, JobSpec},
            core::v1::{Container, PodSpec, PodTemplateSpec},
        },
        apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference},
    };
    use serde_json::json;

    use super::*;

    fn migration(ready: bool, failed: bool, fresh: bool) -> AutonomousWorkloadObservation {
        AutonomousWorkloadObservation {
            workload_id: "migration".to_owned(),
            ready,
            failed,
            observed_digest: Some("sha256:test".to_owned()),
            observed_release_id: Some("release:test".to_owned()),
            observed_release_digest: Some("sha256:release".to_owned()),
            observed_config_revision_id: Some("config:test".to_owned()),
            fresh,
        }
    }

    #[test]
    fn stale_completed_migration_never_unlocks_dependents() {
        assert_eq!(
            migration_gate(&[migration(true, false, false)]),
            AutonomousMigrationGate::Failed
        );
        assert_eq!(
            migration_gate(&[migration(true, false, true)]),
            AutonomousMigrationGate::Complete
        );
        assert_eq!(
            migration_gate(&[migration(false, false, false)]),
            AutonomousMigrationGate::Pending
        );
    }

    #[test]
    fn completed_migration_is_fresh_for_exact_release_execution_across_config_rollout() {
        let mut desired: Job = serde_json::from_value(json!({
            "metadata": {
                "name": "support-migration-release",
                "annotations": {}
            },
            "spec": {
                "backoffLimit": 1,
                "template": {
                    "metadata": {
                        "labels": {"lenso.dev/workload": "migration"},
                        "annotations": {
                            "lenso.dev/release-id": "service-release:sha256:release",
                            "lenso.dev/release-digest": "sha256:release",
                            "lenso.dev/config-revision-id": "config:1"
                        }
                    },
                    "spec": {
                        "restartPolicy": "Never",
                        "nodeSelector": {"topology.kubernetes.io/zone": "acceptance"},
                        "containers": [{
                            "name": "migration",
                            "image": "registry.example/migration@sha256:release",
                            "command": ["migrate", "--expand"],
                            "envFrom": [
                                {"configMapRef": {"name": "support-config-v1"}},
                                {"secretRef": {"name": "support-database-v1"}}
                            ]
                        }]
                    }
                }
            }
        }))
        .expect("desired Job should deserialize");
        let digest = migration_job_execution_digest(&desired);
        desired
            .metadata
            .annotations
            .get_or_insert_with(Default::default)
            .insert("lenso.dev/migration-execution-digest".to_owned(), digest);
        assert!(migration_job_is_fresh(&desired, &desired));

        let mut changed_command = desired.clone();
        changed_command
            .spec
            .as_mut()
            .unwrap()
            .template
            .spec
            .as_mut()
            .unwrap()
            .containers[0]
            .command = Some(vec!["migrate".to_owned(), "--contract".to_owned()]);
        assert!(!migration_job_is_fresh(&changed_command, &desired));

        let mut config_only_desired = desired.clone();
        config_only_desired
            .spec
            .as_mut()
            .unwrap()
            .template
            .spec
            .as_mut()
            .unwrap()
            .containers[0]
            .env_from
            .as_mut()
            .unwrap()[0]
            .config_map_ref
            .as_mut()
            .unwrap()
            .name = "support-config-v2".to_owned();
        config_only_desired
            .spec
            .as_mut()
            .unwrap()
            .template
            .spec
            .as_mut()
            .unwrap()
            .containers[0]
            .env_from
            .as_mut()
            .unwrap()[1]
            .secret_ref
            .as_mut()
            .unwrap()
            .name = "support-database-v2".to_owned();
        config_only_desired
            .spec
            .as_mut()
            .unwrap()
            .template
            .metadata
            .as_mut()
            .unwrap()
            .annotations
            .as_mut()
            .unwrap()
            .insert(
                "lenso.dev/config-revision-id".to_owned(),
                "config:2".to_owned(),
            );
        assert!(migration_job_is_fresh(&desired, &config_only_desired));
    }

    #[test]
    fn observations_read_the_actual_pod_template_digest() {
        let job = Job {
            spec: Some(JobSpec {
                template: pod_template("registry.example/migration@sha256:observed"),
                ..JobSpec::default()
            }),
            ..Job::default()
        };
        let deployment = Deployment {
            spec: Some(DeploymentSpec {
                template: pod_template("registry.example/api@sha256:observed"),
                ..DeploymentSpec::default()
            }),
            ..Deployment::default()
        };

        assert_eq!(job_image_digest(&job).as_deref(), Some("sha256:observed"));
        assert_eq!(
            deployment_image_digest(&deployment).as_deref(),
            Some("sha256:observed")
        );
    }

    #[test]
    fn hpa_rollout_requires_every_live_replica_to_be_updated() {
        assert!(!replica_set_converged(1, 5, 1, 5, 5, 0));
        assert!(replica_set_converged(1, 5, 5, 5, 5, 0));
        assert!(!replica_set_converged(1, 5, 5, 4, 4, 1));
    }

    #[test]
    fn pruning_targets_only_obsolete_resources_owned_by_the_service() {
        let owner = autonomous_owner();
        let desired = vec![owned_deployment("keep", "owner-uid")];
        let existing = vec![
            owned_deployment("keep", "owner-uid"),
            owned_deployment("remove", "owner-uid"),
            owned_deployment("foreign", "another-uid"),
        ];

        assert_eq!(
            obsolete_owned_resource_names(&existing, &desired, &owner),
            vec!["remove"]
        );
    }

    fn pod_template(image: &str) -> PodTemplateSpec {
        PodTemplateSpec {
            spec: Some(PodSpec {
                containers: vec![Container {
                    name: "workload".to_owned(),
                    image: Some(image.to_owned()),
                    ..Container::default()
                }],
                ..PodSpec::default()
            }),
            ..PodTemplateSpec::default()
        }
    }

    fn autonomous_owner() -> LensoAutonomousService {
        serde_json::from_value(json!({
            "apiVersion": "lenso.dev/v1alpha1",
            "kind": "LensoAutonomousService",
            "metadata": {"name": "support", "uid": "owner-uid"},
            "spec": {
                "serviceId": "service:support",
                "environment": "staging",
                "releaseId": "service-release:sha256:release",
                "releaseDigest": "sha256:release",
                "configRevisionId": "config:1",
                "expectedEnvironmentRevision": 7,
                "secretReferences": [],
                "policyEvidenceReferences": [],
                "evidenceReferences": [],
                "workloads": [],
                "rolloutStrategy": "migration_first"
            }
        }))
        .expect("owner fixture should deserialize")
    }

    fn owned_deployment(name: &str, owner_uid: &str) -> Deployment {
        Deployment {
            metadata: ObjectMeta {
                name: Some(name.to_owned()),
                owner_references: Some(vec![OwnerReference {
                    api_version: "lenso.dev/v1alpha1".to_owned(),
                    kind: "LensoAutonomousService".to_owned(),
                    name: "support".to_owned(),
                    uid: owner_uid.to_owned(),
                    controller: Some(true),
                    block_owner_deletion: Some(true),
                }]),
                ..ObjectMeta::default()
            },
            ..Deployment::default()
        }
    }
}
