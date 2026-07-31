use axum::body::Body;
use http::{Request, StatusCode};
use platform_core::{ActorContext, TraceContext};
use platform_provider::{
    PROVIDER_PROTOCOL, ProviderDescriptor, ProviderInvocation, ProviderInvocationAcknowledgement,
    ProviderInvocationMode, ProviderOperationKind, ProviderOutcome,
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tower::ServiceExt;

#[derive(Clone, PartialEq, prost::Message)]
struct JsonEnvelope {
    #[prost(string, tag = "1")]
    payload_json: String,
}

#[tokio::test]
async fn descriptor_exposes_exact_provider_v1_contract() {
    let descriptor: ProviderDescriptor = get("/lenso/provider/v1").await;
    assert_eq!(descriptor.protocol, PROVIDER_PROTOCOL);
    assert_eq!(descriptor.service_id, "fixture-provider");
    assert_eq!(descriptor.exports.len(), 2);
    assert_eq!(descriptor.exports[0].export_key, "contacts");
    assert!(descriptor.exports[0].ready);
    assert_eq!(descriptor.transports.len(), 2);
}

#[tokio::test]
async fn durable_invocation_replays_and_rejects_identity_rebinding() {
    let app = provider_fixture::app();
    let invocation = invocation("invoke-1", json!({ "contactId": "contact-1" }));
    let first: ProviderOutcome = post_on(
        app.clone(),
        "/lenso/provider/v1/exports/contacts/runtime:invoke",
        &invocation,
        StatusCode::OK,
    )
    .await;
    let repeated: ProviderOutcome = post_on(
        app.clone(),
        "/lenso/provider/v1/exports/contacts/runtime:invoke",
        &invocation,
        StatusCode::OK,
    )
    .await;
    assert_eq!(first.outcome_digest, repeated.outcome_digest);

    let conflict = ProviderInvocation {
        payload: json!({ "contactId": "different" }),
        ..invocation
    };
    let _: Value = post_on(
        app.clone(),
        "/lenso/provider/v1/exports/contacts/runtime:invoke",
        &conflict,
        StatusCode::CONFLICT,
    )
    .await;

    let stored: ProviderOutcome =
        get_on(app.clone(), "/lenso/provider/v1/invocations/invoke-1").await;
    assert_eq!(stored.outcome_digest, first.outcome_digest);
    let ack = ProviderInvocationAcknowledgement {
        invocation_id: "invoke-1".to_owned(),
        outcome_digest: first.outcome_digest,
    };
    let _: Value = post_on(
        app,
        "/lenso/provider/v1/invocations/invoke-1:ack",
        &ack,
        StatusCode::OK,
    )
    .await;
}

#[tokio::test]
async fn locked_contract_digest_is_fail_closed() {
    let mut invocation = invocation("invoke-2", json!({}));
    invocation.input_contract_digest =
        "sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".to_owned();
    let problem: Value = post(
        "/lenso/provider/v1/exports/contacts/runtime:invoke",
        &invocation,
        StatusCode::PRECONDITION_FAILED,
    )
    .await;
    assert_eq!(problem["error"]["code"], "provider_contract_mismatch");
}

#[tokio::test]
async fn http_binding_supports_every_provider_operation() {
    let operations = [
        (
            "http:invoke",
            ProviderOperationKind::HttpRoute,
            "status_code",
        ),
        ("admin:list", ProviderOperationKind::AdminList, "records"),
        ("admin:get", ProviderOperationKind::AdminGet, "record"),
        ("admin:query", ProviderOperationKind::AdminQuery, "data"),
        ("admin:act", ProviderOperationKind::AdminAction, "result"),
        (
            "runtime:invoke",
            ProviderOperationKind::RuntimeFunction,
            "output",
        ),
        (
            "events:handle",
            ProviderOperationKind::EventHandler,
            "actions",
        ),
    ];

    for (index, (binding, kind, result_field)) in operations.into_iter().enumerate() {
        let mut request = invocation(&format!("http-operation-{index}"), json!({}));
        request.operation_kind = kind;
        let outcome: ProviderOutcome = post(
            &format!("/lenso/provider/v1/exports/contacts/{binding}"),
            &request,
            StatusCode::OK,
        )
        .await;
        assert!(outcome.result.unwrap().get(result_field).is_some());
    }
}

#[tokio::test]
async fn grpc_binding_has_descriptor_and_durable_invocation_parity() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    tokio::spawn(async move {
        provider_fixture::grpc::serve_grpc(address).await.unwrap();
    });
    let endpoint = format!("http://{address}");
    let channel = loop {
        match tonic::transport::Endpoint::new(endpoint.clone())
            .unwrap()
            .connect()
            .await
        {
            Ok(channel) => break channel,
            Err(_) => tokio::time::sleep(std::time::Duration::from_millis(10)).await,
        }
    };
    let mut client = tonic::client::Grpc::new(channel);
    let descriptor: ProviderDescriptor = grpc_call(
        &mut client,
        "/lenso.provider.v1.Provider/DescribeProvider",
        &json!({}),
    )
    .await;
    assert_eq!(descriptor.exports.len(), 2);

    let request = invocation("grpc-invoke-1", json!({ "contactId": "contact-1" }));
    let first: ProviderOutcome = grpc_call(
        &mut client,
        "/lenso.provider.v1.Provider/InvokeRuntimeFunction",
        &request,
    )
    .await;
    let replay: ProviderOutcome = grpc_call(
        &mut client,
        "/lenso.provider.v1.Provider/InvokeRuntimeFunction",
        &request,
    )
    .await;
    assert_eq!(first.outcome_digest, replay.outcome_digest);
}

async fn grpc_call<T: DeserializeOwned>(
    client: &mut tonic::client::Grpc<tonic::transport::Channel>,
    path: &'static str,
    value: &impl serde::Serialize,
) -> T {
    client.ready().await.unwrap();
    let request = tonic::Request::new(JsonEnvelope {
        payload_json: serde_json::to_string(value).unwrap(),
    });
    let response: tonic::Response<JsonEnvelope> = client
        .unary(
            request,
            tonic::codegen::http::uri::PathAndQuery::from_static(path),
            tonic_prost::ProstCodec::default(),
        )
        .await
        .unwrap();
    serde_json::from_str(&response.into_inner().payload_json).unwrap()
}

fn invocation(invocation_id: &str, payload: Value) -> ProviderInvocation {
    ProviderInvocation {
        protocol: PROVIDER_PROTOCOL.to_owned(),
        invocation_id: invocation_id.to_owned(),
        request_id: "request-1".to_owned(),
        attempt: 1,
        deadline: "2026-07-30T00:01:00Z".to_owned(),
        service_release_digest:
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
        export_key: "contacts".to_owned(),
        module_release_digest:
            "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned(),
        manifest_digest: "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
            .to_owned(),
        operation_kind: ProviderOperationKind::RuntimeFunction,
        operation_name: "contacts.sync.v1".to_owned(),
        operation_version: "1".to_owned(),
        mode: ProviderInvocationMode::Durable,
        input_contract_digest:
            "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd".to_owned(),
        output_contract_digest:
            "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd".to_owned(),
        tenant_id: Some("tenant-1".to_owned()),
        actor: ActorContext::System,
        delegation: None,
        locale: Some("en-US".to_owned()),
        context: Default::default(),
        correlation_id: "correlation-1".to_owned(),
        causation_id: None,
        trace: TraceContext {
            trace_id: Some("trace-1".to_owned()),
            ..TraceContext::default()
        },
        content_type: "application/json".to_owned(),
        payload,
    }
}

async fn get<T: DeserializeOwned>(uri: &str) -> T {
    get_on(provider_fixture::app(), uri).await
}

async fn get_on<T: DeserializeOwned>(app: axum::Router, uri: &str) -> T {
    let response = app
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    decode(response).await
}

async fn post<T: DeserializeOwned>(
    uri: &str,
    value: &impl serde::Serialize,
    expected: StatusCode,
) -> T {
    post_on(provider_fixture::app(), uri, value, expected).await
}

async fn post_on<T: DeserializeOwned>(
    app: axum::Router,
    uri: &str,
    value: &impl serde::Serialize,
    expected: StatusCode,
) -> T {
    let response = app
        .oneshot(
            Request::post(uri)
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(value).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), expected);
    decode(response).await
}

async fn decode<T: DeserializeOwned>(response: axum::response::Response) -> T {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}
