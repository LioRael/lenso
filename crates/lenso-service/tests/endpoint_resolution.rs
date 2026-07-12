use lenso_service::{
    Endpoint, EndpointResolutionError, EndpointResolutionErrorCode, EndpointResolver,
    EndpointState, LastValidEndpointResolver, LocalProcessEndpointResolver, ServiceReference,
    StaticEndpointResolver,
};
use std::sync::{Arc, Mutex};

fn support_reference() -> ServiceReference {
    ServiceReference::new("support")
}

fn support_state(addresses: &[&str]) -> EndpointState {
    EndpointState::new(
        support_reference(),
        addresses
            .iter()
            .map(|address| Endpoint::new(*address))
            .collect(),
    )
}

#[test]
fn static_resolver_returns_explicit_deterministic_endpoint_state() {
    let resolver = StaticEndpointResolver::new([support_state(&[
        "http://127.0.0.1:4102",
        "http://127.0.0.1:4101",
    ])])
    .unwrap();

    let first = resolver.resolve(&support_reference()).unwrap();
    let second = resolver.resolve(&support_reference()).unwrap();

    assert_eq!(first, second);
    assert_eq!(first.endpoints[0].address, "http://127.0.0.1:4102");
    assert_eq!(first.endpoints[1].address, "http://127.0.0.1:4101");
}

#[test]
fn local_process_resolver_publishes_and_removes_developer_endpoints() {
    let resolver = LocalProcessEndpointResolver::new();
    let workload_publisher = resolver.clone();
    let reference = support_reference();

    workload_publisher
        .publish(support_state(&["http://127.0.0.1:4101"]))
        .unwrap();
    assert_eq!(
        resolver.resolve(&reference).unwrap().endpoints[0].address,
        "http://127.0.0.1:4101"
    );

    resolver.remove(&reference);
    let error = resolver.resolve(&reference).unwrap_err();
    assert_eq!(
        error.code,
        EndpointResolutionErrorCode::NoUsableEndpointState
    );
}

#[derive(Clone)]
struct MutableResolver {
    result: Arc<Mutex<Result<EndpointState, EndpointResolutionError>>>,
}

impl EndpointResolver for MutableResolver {
    fn resolve(
        &self,
        _service: &ServiceReference,
    ) -> Result<EndpointState, EndpointResolutionError> {
        self.result.lock().unwrap().clone()
    }
}

#[test]
fn client_resolution_retains_last_valid_state_during_source_unavailability() {
    let current = Arc::new(Mutex::new(Ok(support_state(&["http://127.0.0.1:4101"]))));
    let resolver = LastValidEndpointResolver::new(MutableResolver {
        result: current.clone(),
    });

    let valid = resolver.resolve(&support_reference()).unwrap();
    *current.lock().unwrap() = Err(EndpointResolutionError::source_unavailable(
        &support_reference(),
        "System Plane is unavailable",
    ));

    assert_eq!(resolver.resolve(&support_reference()).unwrap(), valid);
}

#[test]
fn client_resolution_has_stable_failure_when_no_usable_state_exists() {
    let resolver = LastValidEndpointResolver::new(MutableResolver {
        result: Arc::new(Mutex::new(Err(
            EndpointResolutionError::source_unavailable(
                &support_reference(),
                "resolver is unavailable",
            ),
        ))),
    });

    let error = resolver.resolve(&support_reference()).unwrap_err();
    assert_eq!(
        error.code,
        EndpointResolutionErrorCode::NoUsableEndpointState
    );
    assert_eq!(
        error.next_action,
        "Configure or publish at least one endpoint for Service Reference `support`, then retry."
    );
}

#[test]
fn endpoint_address_changes_do_not_change_service_identity() {
    let first = support_state(&["https://old-support.internal"]);
    let second = support_state(&["https://new-support.internal"]);

    assert_eq!(first.service, second.service);
    assert_ne!(first.endpoints, second.endpoints);
}
