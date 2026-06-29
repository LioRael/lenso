use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;
use kube::CustomResourceExt;
use lenso_operator::{
    LensoServiceProvider, LensoServiceProviderCondition, LensoServiceProviderEnvFrom,
    LensoServiceProviderSpec, LensoServiceProviderState, LensoServiceProviderStatus,
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
