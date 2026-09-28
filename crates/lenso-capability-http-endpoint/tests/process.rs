#![cfg(feature = "process")]

use std::{cell::RefCell, rc::Rc};

use lenso_capability_http_endpoint::{
    CAPABILITY_ID, DESCRIBE_OPERATION, DESCRIPTOR_VERSION, DescribeResponse,
    EndpointHandleInvocationError, HANDLE_OPERATION, HandleError, HandleRequest, HandleResponse,
    Json, endpoint, process::ProcessEndpoint,
};
use lenso_kernel::{InvocationContext, RuntimeFailure};
use lenso_process_sdk::{ProcessOutcome, ProcessPlugin};
use lenso_runtime_codec::{JSON_REQUEST_ABI_V1, JsonCapabilityDescriptor, JsonPluginDescriptor};
use serde_json::{Value, json};

#[derive(Clone, Debug, Default)]
struct Counter {
    calls: Rc<RefCell<Vec<u64>>>,
}

#[endpoint(standalone)]
impl Counter {
    #[post("counter.increment", "/counter")]
    async fn increment(
        &self,
        context: InvocationContext,
    ) -> Result<Json<usize>, EndpointHandleInvocationError> {
        futures::future::ready(()).await;
        assert_eq!(context.caller_instance(), None);
        assert_eq!(context.deadline(), None);
        assert_eq!(context.remaining_budget(), None);
        assert_eq!(context.extensions().count(), 0);
        assert_eq!(context.sealed_extensions().count(), 0);
        assert!(!context.is_cancelled());
        self.calls.borrow_mut().push(context.request_id());
        Ok(Json(self.calls.borrow().len()))
    }

    #[post("counter.domain", "/domain")]
    async fn domain(&self) -> Result<HandleResponse, EndpointHandleInvocationError> {
        futures::future::ready(()).await;
        Err(EndpointHandleInvocationError::Domain(HandleError::Rejected))
    }

    #[post("counter.runtime", "/runtime")]
    async fn runtime(&self) -> Result<HandleResponse, EndpointHandleInvocationError> {
        futures::future::ready(()).await;
        Err(EndpointHandleInvocationError::Runtime(
            RuntimeFailure::AdmissionClosed,
        ))
    }
}

fn request(route_id: &str) -> Value {
    serde_json::to_value(HandleRequest {
        body: Vec::new().into(),
        credential: Some(lenso_capability_http_endpoint::HandleRequestCredential {
            scheme: "Bearer".to_owned(),
            value: "untrusted-http-evidence".to_owned(),
        }),
        headers: Vec::new(),
        method: "POST".to_owned(),
        path: "/counter".to_owned(),
        path_parameters: Vec::new(),
        query: None,
        request_id: "http-correlation-not-kernel-authority".to_owned(),
        route_id: route_id.to_owned(),
    })
    .unwrap()
}

#[test]
fn descriptor_uses_generated_contract_identity_and_request_operations() {
    let descriptor = ProcessEndpoint::new(Counter::default()).descriptor();
    let parsed: JsonPluginDescriptor = serde_json::from_value(descriptor.clone()).unwrap();
    assert_eq!(
        parsed,
        JsonPluginDescriptor {
            abi: JSON_REQUEST_ABI_V1.to_owned(),
            capabilities: vec![JsonCapabilityDescriptor {
                capability_id: CAPABILITY_ID.to_owned(),
                descriptor_version: DESCRIPTOR_VERSION.to_owned(),
                request_operations: vec![
                    DESCRIBE_OPERATION.to_owned(),
                    HANDLE_OPERATION.to_owned()
                ],
                stream_operations: Vec::new(),
            }],
            required_capabilities: Vec::new(),
        }
    );
    assert_eq!(
        descriptor,
        json!({
            "abi": "lenso.json-request@1",
            "capabilities": [{
                "capability_id": CAPABILITY_ID,
                "descriptor_version": DESCRIPTOR_VERSION,
                "request_operations": [DESCRIBE_OPERATION, HANDLE_OPERATION]
            }]
        })
    );
}

#[test]
fn typed_description_and_two_calls_preserve_state_with_local_contexts() {
    let counter = Counter::default();
    let process = ProcessEndpoint::new(counter.clone());
    let ProcessOutcome::Success(description) =
        process.invoke(CAPABILITY_ID, DESCRIBE_OPERATION, json!({}))
    else {
        panic!("expected successful describe");
    };
    let description: DescribeResponse = serde_json::from_value(description).unwrap();
    assert_eq!(description.routes[0].route_id, "counter.increment");
    assert_eq!(description.routes[0].method, "POST");
    assert_eq!(description.routes[0].path, "/counter");
    for count in [1, 2] {
        let ProcessOutcome::Success(response) = process.invoke(
            CAPABILITY_ID,
            HANDLE_OPERATION,
            request("counter.increment"),
        ) else {
            panic!("expected successful handle");
        };
        let response: HandleResponse = serde_json::from_value(response).unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(
            serde_json::from_slice::<usize>(&response.body).unwrap(),
            count
        );
    }
    assert_eq!(*counter.calls.borrow(), [2, 3]);
}

#[test]
fn invalid_identity_and_json_fail_before_handler_execution() {
    let counter = Counter::default();
    let process = ProcessEndpoint::new(counter.clone());
    for (capability, operation, input) in [
        ("unknown@1", HANDLE_OPERATION, request("counter.increment")),
        (CAPABILITY_ID, "unknown", request("counter.increment")),
        (CAPABILITY_ID, HANDLE_OPERATION, json!({})),
        (CAPABILITY_ID, DESCRIBE_OPERATION, json!(null)),
    ] {
        assert!(matches!(
            process.invoke(capability, operation, input),
            ProcessOutcome::Failure(_)
        ));
    }
    assert!(counter.calls.borrow().is_empty());
}

#[test]
fn unknown_route_and_wrong_method_are_domain_rejections_without_dispatch() {
    let counter = Counter::default();
    let process = ProcessEndpoint::new(counter.clone());
    let mut wrong_method = request("counter.increment");
    wrong_method["method"] = json!("GET");
    for input in [request("unknown"), wrong_method] {
        let ProcessOutcome::DomainError(error) =
            process.invoke(CAPABILITY_ID, HANDLE_OPERATION, input)
        else {
            panic!("expected domain rejection");
        };
        assert_eq!(
            serde_json::from_value::<HandleError>(error).unwrap(),
            HandleError::Rejected
        );
    }
    assert!(counter.calls.borrow().is_empty());
}

#[test]
fn provider_domain_errors_remain_distinct_from_runtime_failures() {
    let process = ProcessEndpoint::new(Counter::default());
    assert!(matches!(
        process.invoke(CAPABILITY_ID, HANDLE_OPERATION, request("counter.domain")),
        ProcessOutcome::DomainError(_)
    ));
    assert!(matches!(
        process.invoke(CAPABILITY_ID, HANDLE_OPERATION, request("counter.runtime")),
        ProcessOutcome::Failure(_)
    ));
}
