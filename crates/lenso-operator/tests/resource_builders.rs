use k8s_openapi::{
    api::apps::v1::{Deployment, DeploymentStatus},
    apimachinery::pkg::util::intstr::IntOrString,
};
use kube::ResourceExt;
use lenso_operator::{
    LensoServiceProvider, LensoServiceProviderAutoscaling, LensoServiceProviderDisruptionBudget,
    LensoServiceProviderEnvFrom, LensoServiceProviderIngress, LensoServiceProviderNetworkPolicy,
    LensoServiceProviderSpec, LensoServiceProviderState, build_deployment,
    build_horizontal_pod_autoscaler, build_ingress, build_network_policy,
    build_pod_disruption_budget, build_service, deployment_status_to_provider_status,
    invalid_spec_status,
};

#[test]
fn deployment_has_labels_annotations_probes_and_env_from() {
    let provider = provider();
    let deployment = build_deployment(&provider).unwrap();
    let labels = deployment.metadata.labels.as_ref().unwrap();
    let annotations = deployment.metadata.annotations.as_ref().unwrap();

    assert_eq!(labels["app.kubernetes.io/name"], "payments");
    assert_eq!(labels["app.kubernetes.io/part-of"], "lenso");
    assert_eq!(labels["app.kubernetes.io/component"], "service-provider");
    assert_eq!(labels["lenso.dev/service-provider"], "payments");
    assert_eq!(labels["lenso.dev/environment"], "prod");
    assert_eq!(annotations["lenso.dev/modules"], "auth,story");
    assert_eq!(annotations["lenso.dev/release-id"], "rel-1");
    assert_eq!(
        annotations["lenso.dev/manifest-reference"],
        "oci://example/payments:v1"
    );
    let owner = &deployment.metadata.owner_references.as_ref().unwrap()[0];
    assert_eq!(owner.kind, "LensoServiceProvider");
    assert_eq!(owner.name, "payments");
    assert_eq!(owner.uid, "provider-uid");
    assert_eq!(owner.controller, Some(true));

    let template_spec = deployment.spec.unwrap().template.spec.unwrap();
    let container = &template_spec.containers[0];
    assert_eq!(
        container.image.as_deref(),
        Some("ghcr.io/lenso/payments:v1")
    );
    assert_eq!(
        container
            .readiness_probe
            .as_ref()
            .unwrap()
            .http_get
            .as_ref()
            .unwrap()
            .path
            .as_deref(),
        Some("/lenso/service/v1/status")
    );
    assert_eq!(
        container
            .liveness_probe
            .as_ref()
            .unwrap()
            .http_get
            .as_ref()
            .unwrap()
            .port,
        IntOrString::Int(8080)
    );

    let env_from = container.env_from.as_ref().unwrap();
    assert_eq!(
        env_from[0]
            .config_map_ref
            .as_ref()
            .map(|source| source.name.as_str()),
        Some("payments-config")
    );
    assert_eq!(
        env_from[1]
            .secret_ref
            .as_ref()
            .map(|source| source.name.as_str()),
        Some("payments-secret")
    );
}

#[test]
fn service_selects_name_label_and_exposes_http_port() {
    let service = build_service(&provider()).unwrap();
    let spec = service.spec.unwrap();

    assert_eq!(spec.selector.unwrap()["app.kubernetes.io/name"], "payments");
    assert_eq!(
        spec.ports.as_ref().unwrap()[0].name.as_deref(),
        Some("http")
    );
    assert_eq!(spec.ports.as_ref().unwrap()[0].port, 8080);
    assert_eq!(
        spec.ports.as_ref().unwrap()[0].target_port,
        Some(IntOrString::String("http".to_owned()))
    );
}

#[test]
fn optional_resources_follow_spec_flags() {
    let provider = provider();
    assert!(build_ingress(&provider).unwrap().is_none());
    assert!(
        build_horizontal_pod_autoscaler(&provider)
            .unwrap()
            .is_none()
    );
    assert!(build_pod_disruption_budget(&provider).unwrap().is_none());
    assert!(build_network_policy(&provider).unwrap().is_none());

    let mut provider = provider;
    provider.spec.ingress = Some(LensoServiceProviderIngress {
        host: "payments.example.com".to_owned(),
    });
    provider.spec.autoscaling = Some(LensoServiceProviderAutoscaling {
        enabled: true,
        min_replicas: 2,
        max_replicas: 5,
        target_cpu_utilization: 65,
    });
    provider.spec.disruption_budget = Some(LensoServiceProviderDisruptionBudget {
        enabled: true,
        min_available: 1,
    });
    provider.spec.network_policy = Some(LensoServiceProviderNetworkPolicy { enabled: true });

    let ingress = build_ingress(&provider).unwrap().unwrap();
    assert_eq!(
        ingress.spec.unwrap().rules.unwrap()[0].host.as_deref(),
        Some("payments.example.com")
    );

    let hpa = build_horizontal_pod_autoscaler(&provider).unwrap().unwrap();
    let hpa_spec = hpa.spec;
    assert_eq!(hpa_spec.min_replicas, Some(2));
    assert_eq!(hpa_spec.max_replicas, 5);

    let pdb = build_pod_disruption_budget(&provider).unwrap().unwrap();
    assert_eq!(pdb.spec.unwrap().min_available, Some(IntOrString::Int(1)));

    let network_policy = build_network_policy(&provider).unwrap().unwrap();
    assert_eq!(
        network_policy
            .spec
            .unwrap()
            .pod_selector
            .unwrap()
            .match_labels
            .unwrap()["app.kubernetes.io/name"],
        "payments"
    );
}

#[test]
fn disabled_optional_resources_are_omitted() {
    let mut provider = provider();
    provider.spec.autoscaling = Some(LensoServiceProviderAutoscaling {
        enabled: false,
        min_replicas: 1,
        max_replicas: 3,
        target_cpu_utilization: 70,
    });
    provider.spec.disruption_budget = Some(LensoServiceProviderDisruptionBudget {
        enabled: false,
        min_available: 1,
    });
    provider.spec.network_policy = Some(LensoServiceProviderNetworkPolicy { enabled: false });

    assert!(
        build_horizontal_pod_autoscaler(&provider)
            .unwrap()
            .is_none()
    );
    assert!(build_pod_disruption_budget(&provider).unwrap().is_none());
    assert!(build_network_policy(&provider).unwrap().is_none());
}

#[test]
fn status_helpers_reflect_deployment_readiness() {
    let provider = provider();

    let missing = deployment_status_to_provider_status(&provider, None);
    assert_eq!(missing.state, LensoServiceProviderState::Unknown);
    assert_eq!(missing.conditions[0].reason, "DeploymentMissing");

    let ready_deployment = deployment_with_ready_replicas(&provider, 2);
    let ready = deployment_status_to_provider_status(&provider, Some(&ready_deployment));
    assert_eq!(ready.state, LensoServiceProviderState::Ready);
    assert_eq!(ready.ready_replicas, Some(2));

    let progressing_deployment = deployment_with_ready_replicas(&provider, 1);
    let progressing =
        deployment_status_to_provider_status(&provider, Some(&progressing_deployment));
    assert_eq!(progressing.state, LensoServiceProviderState::Progressing);
    assert_eq!(progressing.conditions[0].reason, "DeploymentProgressing");

    let failed = invalid_spec_status(&provider, "port must be valid");
    assert_eq!(failed.state, LensoServiceProviderState::Failed);
    assert_eq!(failed.conditions[0].reason, "SpecInvalid");
    assert_eq!(
        failed.observed_image.as_deref(),
        Some("ghcr.io/lenso/payments:v1")
    );
    assert_eq!(failed.observed_release_id.as_deref(), Some("rel-1"));
}

fn deployment_with_ready_replicas(
    provider: &LensoServiceProvider,
    ready_replicas: i32,
) -> Deployment {
    let mut deployment = Deployment::default();
    deployment.metadata.name = Some(provider.name_any());
    deployment.status = Some(DeploymentStatus {
        available_replicas: Some(ready_replicas),
        ready_replicas: Some(ready_replicas),
        replicas: Some(provider.spec.replicas),
        ..DeploymentStatus::default()
    });
    deployment
}

fn provider() -> LensoServiceProvider {
    let mut provider = LensoServiceProvider::new(
        "payments",
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
        },
    );
    provider.metadata.namespace = Some("lenso-system".to_owned());
    provider.metadata.uid = Some("provider-uid".to_owned());
    provider
}
