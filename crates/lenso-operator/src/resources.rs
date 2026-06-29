use std::collections::BTreeMap;

use anyhow::{Result, ensure};
use k8s_openapi::{
    api::{
        apps::v1::{Deployment, DeploymentSpec},
        autoscaling::v2::{
            CrossVersionObjectReference, HorizontalPodAutoscaler, HorizontalPodAutoscalerSpec,
            MetricSpec, MetricTarget, ResourceMetricSource,
        },
        core::v1::{
            ConfigMapEnvSource, Container, ContainerPort, EnvFromSource, HTTPGetAction, PodSpec,
            PodTemplateSpec, Probe, SecretEnvSource, Service, ServicePort, ServiceSpec,
        },
        networking::v1::{
            HTTPIngressPath, HTTPIngressRuleValue, Ingress, IngressBackend, IngressRule,
            IngressServiceBackend, IngressSpec, NetworkPolicy, NetworkPolicySpec,
            ServiceBackendPort,
        },
        policy::v1::{PodDisruptionBudget, PodDisruptionBudgetSpec},
    },
    apimachinery::pkg::{
        apis::meta::v1::{LabelSelector, ObjectMeta},
        util::intstr::IntOrString,
    },
};
use kube::ResourceExt;

use crate::crd::LensoServiceProvider;

const HTTP_PORT_NAME: &str = "http";
const STATUS_PATH: &str = "/lenso/service/v1/status";

pub fn build_deployment(provider: &LensoServiceProvider) -> Result<Deployment> {
    validate_spec(provider)?;
    let selector = selector_labels(provider);
    let labels = stable_labels(provider);
    let annotations = annotations(provider);

    Ok(Deployment {
        metadata: metadata(provider, labels.clone(), annotations.clone()),
        spec: Some(DeploymentSpec {
            replicas: Some(provider.spec.replicas),
            selector: LabelSelector {
                match_labels: Some(selector),
                ..LabelSelector::default()
            },
            template: PodTemplateSpec {
                metadata: Some(ObjectMeta {
                    labels: Some(labels),
                    annotations: Some(annotations),
                    ..ObjectMeta::default()
                }),
                spec: Some(PodSpec {
                    containers: vec![Container {
                        name: "service-provider".to_owned(),
                        image: Some(provider.spec.image.clone()),
                        ports: Some(vec![ContainerPort {
                            container_port: provider.spec.port,
                            name: Some(HTTP_PORT_NAME.to_owned()),
                            protocol: Some("TCP".to_owned()),
                            ..ContainerPort::default()
                        }]),
                        env_from: env_from_sources(provider),
                        readiness_probe: Some(http_probe(provider.spec.port)),
                        liveness_probe: Some(http_probe(provider.spec.port)),
                        ..Container::default()
                    }],
                    ..PodSpec::default()
                }),
            },
            ..DeploymentSpec::default()
        }),
        ..Deployment::default()
    })
}

pub fn build_service(provider: &LensoServiceProvider) -> Result<Service> {
    validate_spec(provider)?;

    Ok(Service {
        metadata: metadata(provider, stable_labels(provider), annotations(provider)),
        spec: Some(ServiceSpec {
            selector: Some(selector_labels(provider)),
            ports: Some(vec![ServicePort {
                name: Some(HTTP_PORT_NAME.to_owned()),
                port: provider.spec.port,
                protocol: Some("TCP".to_owned()),
                target_port: Some(IntOrString::String(HTTP_PORT_NAME.to_owned())),
                ..ServicePort::default()
            }]),
            ..ServiceSpec::default()
        }),
        ..Service::default()
    })
}

pub fn build_ingress(provider: &LensoServiceProvider) -> Result<Option<Ingress>> {
    validate_spec(provider)?;
    let Some(ingress) = &provider.spec.ingress else {
        return Ok(None);
    };

    ensure!(
        !ingress.host.trim().is_empty(),
        "ingress.host must not be empty"
    );

    Ok(Some(Ingress {
        metadata: metadata(provider, stable_labels(provider), annotations(provider)),
        spec: Some(IngressSpec {
            rules: Some(vec![IngressRule {
                host: Some(ingress.host.clone()),
                http: Some(HTTPIngressRuleValue {
                    paths: vec![HTTPIngressPath {
                        backend: IngressBackend {
                            service: Some(IngressServiceBackend {
                                name: resource_name(provider),
                                port: Some(ServiceBackendPort {
                                    name: Some(HTTP_PORT_NAME.to_owned()),
                                    ..ServiceBackendPort::default()
                                }),
                            }),
                            ..IngressBackend::default()
                        },
                        path: Some("/".to_owned()),
                        path_type: "Prefix".to_owned(),
                    }],
                }),
            }]),
            ..IngressSpec::default()
        }),
        ..Ingress::default()
    }))
}

pub fn build_horizontal_pod_autoscaler(
    provider: &LensoServiceProvider,
) -> Result<Option<HorizontalPodAutoscaler>> {
    validate_spec(provider)?;
    let Some(autoscaling) = &provider.spec.autoscaling else {
        return Ok(None);
    };
    if !autoscaling.enabled {
        return Ok(None);
    }
    ensure!(
        autoscaling.min_replicas >= 1,
        "minReplicas must be at least 1"
    );
    ensure!(
        autoscaling.max_replicas >= autoscaling.min_replicas,
        "maxReplicas must be greater than or equal to minReplicas"
    );
    ensure!(
        (1..=100).contains(&autoscaling.target_cpu_utilization),
        "targetCpuUtilization must be between 1 and 100"
    );

    Ok(Some(HorizontalPodAutoscaler {
        metadata: metadata(provider, stable_labels(provider), annotations(provider)),
        spec: HorizontalPodAutoscalerSpec {
            max_replicas: autoscaling.max_replicas,
            min_replicas: Some(autoscaling.min_replicas),
            scale_target_ref: CrossVersionObjectReference {
                api_version: Some("apps/v1".to_owned()),
                kind: "Deployment".to_owned(),
                name: resource_name(provider),
            },
            metrics: Some(vec![MetricSpec {
                type_: "Resource".to_owned(),
                resource: Some(ResourceMetricSource {
                    name: "cpu".to_owned(),
                    target: MetricTarget {
                        type_: "Utilization".to_owned(),
                        average_utilization: Some(autoscaling.target_cpu_utilization),
                        ..MetricTarget::default()
                    },
                }),
                ..MetricSpec::default()
            }]),
            ..HorizontalPodAutoscalerSpec::default()
        },
        ..HorizontalPodAutoscaler::default()
    }))
}

pub fn build_pod_disruption_budget(
    provider: &LensoServiceProvider,
) -> Result<Option<PodDisruptionBudget>> {
    validate_spec(provider)?;
    let Some(disruption_budget) = &provider.spec.disruption_budget else {
        return Ok(None);
    };
    if !disruption_budget.enabled {
        return Ok(None);
    }
    ensure!(
        disruption_budget.min_available >= 0,
        "minAvailable must not be negative"
    );

    Ok(Some(PodDisruptionBudget {
        metadata: metadata(provider, stable_labels(provider), annotations(provider)),
        spec: Some(PodDisruptionBudgetSpec {
            min_available: Some(IntOrString::Int(disruption_budget.min_available)),
            selector: Some(LabelSelector {
                match_labels: Some(selector_labels(provider)),
                ..LabelSelector::default()
            }),
            ..PodDisruptionBudgetSpec::default()
        }),
        ..PodDisruptionBudget::default()
    }))
}

pub fn build_network_policy(provider: &LensoServiceProvider) -> Result<Option<NetworkPolicy>> {
    validate_spec(provider)?;
    let Some(network_policy) = &provider.spec.network_policy else {
        return Ok(None);
    };
    if !network_policy.enabled {
        return Ok(None);
    }

    Ok(Some(NetworkPolicy {
        metadata: metadata(provider, stable_labels(provider), annotations(provider)),
        spec: Some(NetworkPolicySpec {
            pod_selector: Some(LabelSelector {
                match_labels: Some(selector_labels(provider)),
                ..LabelSelector::default()
            }),
            policy_types: Some(vec!["Ingress".to_owned()]),
            ..NetworkPolicySpec::default()
        }),
        ..NetworkPolicy::default()
    }))
}

fn validate_spec(provider: &LensoServiceProvider) -> Result<()> {
    ensure!(
        !provider.spec.service_name.trim().is_empty(),
        "serviceName must not be empty"
    );
    ensure!(
        !provider.spec.environment.trim().is_empty(),
        "environment must not be empty"
    );
    ensure!(
        !provider.spec.image.trim().is_empty(),
        "image must not be empty"
    );
    ensure!(
        (1..=65_535).contains(&provider.spec.port),
        "port must be between 1 and 65535"
    );
    ensure!(provider.spec.replicas >= 0, "replicas must not be negative");
    Ok(())
}

fn resource_name(provider: &LensoServiceProvider) -> String {
    provider.name_any()
}

fn metadata(
    provider: &LensoServiceProvider,
    labels: BTreeMap<String, String>,
    annotations: BTreeMap<String, String>,
) -> ObjectMeta {
    ObjectMeta {
        name: Some(resource_name(provider)),
        namespace: provider.namespace(),
        labels: Some(labels),
        annotations: Some(annotations),
        ..ObjectMeta::default()
    }
}

fn stable_labels(provider: &LensoServiceProvider) -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "app.kubernetes.io/name".to_owned(),
            provider.spec.service_name.clone(),
        ),
        ("app.kubernetes.io/part-of".to_owned(), "lenso".to_owned()),
        (
            "app.kubernetes.io/component".to_owned(),
            "service-provider".to_owned(),
        ),
        (
            "lenso.dev/service-provider".to_owned(),
            resource_name(provider),
        ),
        (
            "lenso.dev/environment".to_owned(),
            provider.spec.environment.clone(),
        ),
    ])
}

fn selector_labels(provider: &LensoServiceProvider) -> BTreeMap<String, String> {
    BTreeMap::from([(
        "app.kubernetes.io/name".to_owned(),
        provider.spec.service_name.clone(),
    )])
}

fn annotations(provider: &LensoServiceProvider) -> BTreeMap<String, String> {
    let mut annotations = BTreeMap::from([(
        "lenso.dev/modules".to_owned(),
        provider.spec.modules.join(","),
    )]);
    if let Some(release_id) = &provider.spec.release_id {
        annotations.insert("lenso.dev/release-id".to_owned(), release_id.clone());
    }
    if let Some(manifest_reference) = &provider.spec.manifest_reference {
        annotations.insert(
            "lenso.dev/manifest-reference".to_owned(),
            manifest_reference.clone(),
        );
    }
    annotations
}

fn env_from_sources(provider: &LensoServiceProvider) -> Option<Vec<EnvFromSource>> {
    let env_from = provider.spec.env_from.as_ref()?;
    let mut sources = Vec::new();
    if let Some(config_map) = &env_from.config_map {
        sources.push(EnvFromSource {
            config_map_ref: Some(ConfigMapEnvSource {
                name: config_map.clone(),
                ..ConfigMapEnvSource::default()
            }),
            ..EnvFromSource::default()
        });
    }
    if let Some(secret) = &env_from.secret {
        sources.push(EnvFromSource {
            secret_ref: Some(SecretEnvSource {
                name: secret.clone(),
                ..SecretEnvSource::default()
            }),
            ..EnvFromSource::default()
        });
    }
    (!sources.is_empty()).then_some(sources)
}

fn http_probe(port: i32) -> Probe {
    Probe {
        http_get: Some(HTTPGetAction {
            path: Some(STATUS_PATH.to_owned()),
            port: IntOrString::Int(port),
            scheme: Some("HTTP".to_owned()),
            ..HTTPGetAction::default()
        }),
        ..Probe::default()
    }
}
