use std::{sync::Arc, time::Duration};

use futures::TryStreamExt;
use k8s_openapi::api::{
    apps::v1::Deployment,
    autoscaling::v2::HorizontalPodAutoscaler,
    core::v1::Service,
    networking::v1::{Ingress, NetworkPolicy},
    policy::v1::PodDisruptionBudget,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;
use kube::{
    Api, Client, Resource, ResourceExt,
    api::{Patch, PatchParams},
    runtime::{Controller, controller::Action, watcher},
};
use serde_json::json;
use thiserror::Error;
use tracing::{error, info};

use crate::{
    crd::{
        LensoServiceProvider, LensoServiceProviderCondition, LensoServiceProviderState,
        LensoServiceProviderStatus,
    },
    resources::{
        build_deployment, build_horizontal_pod_autoscaler, build_ingress, build_network_policy,
        build_pod_disruption_budget, build_service,
    },
};

const FIELD_MANAGER: &str = "lenso-operator";

#[derive(Clone)]
pub struct ReconcileContext {
    pub client: Client,
}

impl std::fmt::Debug for ReconcileContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("ReconcileContext").finish()
    }
}

#[derive(Debug, Error)]
pub enum ReconcileError {
    #[error(transparent)]
    Kube(#[from] kube::Error),
    #[error(transparent)]
    Build(#[from] anyhow::Error),
}

#[derive(Debug)]
struct DesiredResources {
    deployment: Deployment,
    service: Service,
    ingress: Option<Ingress>,
    hpa: Option<HorizontalPodAutoscaler>,
    pdb: Option<PodDisruptionBudget>,
    network_policy: Option<NetworkPolicy>,
}

pub async fn run(client: Client, namespace: Option<String>) -> anyhow::Result<()> {
    let providers = match namespace {
        Some(namespace) => Api::<LensoServiceProvider>::namespaced(client.clone(), &namespace),
        None => Api::<LensoServiceProvider>::all(client.clone()),
    };

    Controller::new(providers, watcher::Config::default())
        .shutdown_on_signal()
        .run(
            reconcile,
            error_policy,
            Arc::new(ReconcileContext { client }),
        )
        .try_for_each(|result| async move {
            let (object_ref, action) = result;
            info!(?object_ref, ?action, "reconciled LensoServiceProvider");
            Ok(())
        })
        .await?;

    Ok(())
}

pub fn deployment_status_to_provider_status(
    provider: &LensoServiceProvider,
    deployment: Option<&Deployment>,
) -> LensoServiceProviderStatus {
    let desired_replicas = provider.spec.replicas;
    let observed_generation = provider.meta().generation;
    let base = LensoServiceProviderStatus {
        state: LensoServiceProviderState::Unknown,
        observed_generation,
        observed_release_id: provider.spec.release_id.clone(),
        observed_image: Some(provider.spec.image.clone()),
        ready_replicas: Some(0),
        desired_replicas: Some(desired_replicas),
        available_replicas: Some(0),
        manifest_reference: provider.spec.manifest_reference.clone(),
        conditions: Vec::new(),
    };

    let Some(deployment) = deployment else {
        return with_condition(
            base,
            LensoServiceProviderState::Unknown,
            "Available",
            "False",
            "DeploymentMissing",
            "Deployment has not been observed",
        );
    };

    let status = deployment.status.as_ref();
    let ready = status.and_then(|status| status.ready_replicas).unwrap_or(0);
    let available = status
        .and_then(|status| status.available_replicas)
        .unwrap_or(0);
    let base = LensoServiceProviderStatus {
        ready_replicas: Some(ready),
        available_replicas: Some(available),
        ..base
    };

    if desired_replicas > 0 && ready == desired_replicas {
        with_condition(
            base,
            LensoServiceProviderState::Ready,
            "Available",
            "True",
            "DeploymentReady",
            "Deployment has the desired ready replicas",
        )
    } else {
        with_condition(
            base,
            LensoServiceProviderState::Progressing,
            "Available",
            "False",
            "DeploymentProgressing",
            "Deployment is not fully ready",
        )
    }
}

pub fn invalid_spec_status(
    provider: &LensoServiceProvider,
    message: &str,
) -> LensoServiceProviderStatus {
    with_condition(
        LensoServiceProviderStatus {
            state: LensoServiceProviderState::Failed,
            observed_generation: provider.meta().generation,
            observed_release_id: provider.spec.release_id.clone(),
            observed_image: Some(provider.spec.image.clone()),
            ready_replicas: Some(0),
            desired_replicas: Some(provider.spec.replicas),
            available_replicas: Some(0),
            manifest_reference: provider.spec.manifest_reference.clone(),
            conditions: Vec::new(),
        },
        LensoServiceProviderState::Failed,
        "SpecValid",
        "False",
        "SpecInvalid",
        message,
    )
}

async fn reconcile(
    provider: Arc<LensoServiceProvider>,
    context: Arc<ReconcileContext>,
) -> Result<Action, ReconcileError> {
    let desired = match DesiredResources::build(&provider) {
        Ok(desired) => desired,
        Err(error) => {
            patch_status(
                &context.client,
                &provider,
                invalid_spec_status(&provider, &error.to_string()),
            )
            .await?;
            return Ok(Action::await_change());
        }
    };

    apply_desired(&context.client, &provider, desired).await?;
    let deployment = deployment_api(&context.client, &provider)?
        .get_opt(&provider.name_any())
        .await?;
    let status = deployment_status_to_provider_status(&provider, deployment.as_ref());
    patch_status(&context.client, &provider, status).await?;

    Ok(Action::requeue(Duration::from_secs(300)))
}

fn error_policy(
    provider: Arc<LensoServiceProvider>,
    error: &ReconcileError,
    _context: Arc<ReconcileContext>,
) -> Action {
    error!(
        name = %provider.name_any(),
        error = %error,
        "failed to reconcile LensoServiceProvider"
    );
    Action::requeue(Duration::from_secs(60))
}

impl DesiredResources {
    fn build(provider: &LensoServiceProvider) -> anyhow::Result<Self> {
        Ok(Self {
            deployment: build_deployment(provider)?,
            service: build_service(provider)?,
            ingress: build_ingress(provider)?,
            hpa: build_horizontal_pod_autoscaler(provider)?,
            pdb: build_pod_disruption_budget(provider)?,
            network_policy: build_network_policy(provider)?,
        })
    }
}

async fn apply_desired(
    client: &Client,
    provider: &LensoServiceProvider,
    desired: DesiredResources,
) -> Result<(), ReconcileError> {
    let name = provider.name_any();
    let params = PatchParams::apply(FIELD_MANAGER).force();

    deployment_api(client, provider)?
        .patch(&name, &params, &Patch::Apply(&desired.deployment))
        .await?;
    service_api(client, provider)?
        .patch(&name, &params, &Patch::Apply(&desired.service))
        .await?;

    if let Some(ingress) = desired.ingress {
        ingress_api(client, provider)?
            .patch(&name, &params, &Patch::Apply(&ingress))
            .await?;
    }
    if let Some(hpa) = desired.hpa {
        hpa_api(client, provider)?
            .patch(&name, &params, &Patch::Apply(&hpa))
            .await?;
    }
    if let Some(pdb) = desired.pdb {
        pdb_api(client, provider)?
            .patch(&name, &params, &Patch::Apply(&pdb))
            .await?;
    }
    if let Some(network_policy) = desired.network_policy {
        network_policy_api(client, provider)?
            .patch(&name, &params, &Patch::Apply(&network_policy))
            .await?;
    }

    Ok(())
}

async fn patch_status(
    client: &Client,
    provider: &LensoServiceProvider,
    status: LensoServiceProviderStatus,
) -> Result<(), ReconcileError> {
    let patch = json!({ "status": status });
    provider_api(client, provider)?
        .patch_status(
            &provider.name_any(),
            &PatchParams::default(),
            &Patch::Merge(&patch),
        )
        .await?;
    Ok(())
}

fn provider_api(
    client: &Client,
    provider: &LensoServiceProvider,
) -> anyhow::Result<Api<LensoServiceProvider>> {
    Ok(Api::namespaced(client.clone(), &namespace(provider)?))
}

fn deployment_api(
    client: &Client,
    provider: &LensoServiceProvider,
) -> anyhow::Result<Api<Deployment>> {
    Ok(Api::namespaced(client.clone(), &namespace(provider)?))
}

fn service_api(client: &Client, provider: &LensoServiceProvider) -> anyhow::Result<Api<Service>> {
    Ok(Api::namespaced(client.clone(), &namespace(provider)?))
}

fn ingress_api(client: &Client, provider: &LensoServiceProvider) -> anyhow::Result<Api<Ingress>> {
    Ok(Api::namespaced(client.clone(), &namespace(provider)?))
}

fn hpa_api(
    client: &Client,
    provider: &LensoServiceProvider,
) -> anyhow::Result<Api<HorizontalPodAutoscaler>> {
    Ok(Api::namespaced(client.clone(), &namespace(provider)?))
}

fn pdb_api(
    client: &Client,
    provider: &LensoServiceProvider,
) -> anyhow::Result<Api<PodDisruptionBudget>> {
    Ok(Api::namespaced(client.clone(), &namespace(provider)?))
}

fn network_policy_api(
    client: &Client,
    provider: &LensoServiceProvider,
) -> anyhow::Result<Api<NetworkPolicy>> {
    Ok(Api::namespaced(client.clone(), &namespace(provider)?))
}

fn namespace(provider: &LensoServiceProvider) -> anyhow::Result<String> {
    provider
        .namespace()
        .ok_or_else(|| anyhow::anyhow!("LensoServiceProvider namespace is required"))
}

fn with_condition(
    mut status: LensoServiceProviderStatus,
    state: LensoServiceProviderState,
    type_: &str,
    condition_status: &str,
    reason: &str,
    message: &str,
) -> LensoServiceProviderStatus {
    status.state = state;
    status.conditions = vec![LensoServiceProviderCondition {
        type_: type_.to_owned(),
        status: condition_status.to_owned(),
        reason: reason.to_owned(),
        message: message.to_owned(),
        last_transition_time: Time(k8s_openapi::jiff::Timestamp::now()),
    }];
    status
}
