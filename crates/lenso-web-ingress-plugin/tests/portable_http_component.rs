//! Real native loopback HTTP and Wasmtime Component requests over one business handler.
//! This is a Native Environment receipt, not workerd or deployed Workers evidence.

use std::{any::Any, process::Command, rc::Rc, time::Duration};

use bytes::Bytes;
use http::{Request, Response};
use http_body_util::{BodyExt as _, Full};
use hyper_util::rt::TokioIo;
use lenso_app_plan::{
    AppComposition, CapabilityBinding, CapabilityEndpointPlan, CapabilityRequirementPlan,
    ExecutionClassId, PluginInstancePlan, ResolvedAppPlan,
};
use lenso_capability_http_endpoint::{
    CAPABILITY_ID, DESCRIBE_OPERATION, DESCRIPTOR_VERSION, DescribeError, DescribeRequest,
    DescribeResponse, DescribeResponseRoutesItem, EndpointDescribe, EndpointEndpoint,
    EndpointHandle, EndpointProvider, HANDLE_OPERATION, HandleError, HandleRequest, HandleResponse,
};
use lenso_kernel::{
    ExecutionAdapterCatalog, InvocationContext, Kernel, NativeRequestFuture, RuntimeFailure,
    ShutdownOutcome,
};
use lenso_native_adapter::{
    NativePluginFactory, NativePluginFactoryContext, NativePluginInstance, NativePluginRegistry,
};
use lenso_portable_http_endpoint_fixture as handler;
use lenso_runner::TokioDriver;
use lenso_runtime_codec::{ArtifactCatalog, ArtifactHandle, JsonCapabilityCodec};
use lenso_wasm_component_adapter::{EXECUTION_CLASS, WasmComponentAdapter};
use lenso_web_ingress_plugin::{WebIngressConfig, WebIngressFactory};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::{net::TcpStream, task::LocalSet};

const PACKAGE_ID: &str = "fixture.portable-http";

#[derive(Debug)]
struct NativeEndpoint;

impl EndpointProvider for NativeEndpoint {
    fn describe(
        &self,
        _: InvocationContext,
        _: DescribeRequest,
    ) -> NativeRequestFuture<EndpointDescribe> {
        let routes = handler::ROUTES
            .into_iter()
            .map(|(route_id, method, path)| DescribeResponseRoutesItem {
                route_id: route_id.to_owned(),
                method: method.to_owned(),
                path: path.to_owned(),
                openapi: None,
            })
            .collect();
        Box::pin(async move { Ok(Ok(DescribeResponse { routes })) })
    }

    fn handle(
        &self,
        _: InvocationContext,
        request: HandleRequest,
    ) -> NativeRequestFuture<EndpointHandle> {
        Box::pin(async move {
            match handler::handle(
                &request.route_id,
                &request.method,
                &request.path,
                &request.body,
            ) {
                handler::Reply::Bytes(body) => Ok(Ok(HandleResponse {
                    status: 200,
                    headers: Vec::new(),
                    body: body.into(),
                })),
                handler::Reply::DomainError => Ok(Err(HandleError::Rejected)),
                handler::Reply::RuntimeFailure => Err(RuntimeFailure::PluginFailure {
                    detail: "portable HTTP fixture failure".to_owned(),
                }),
            }
        })
    }
}

#[derive(Debug)]
struct NativeEndpointFactory;

impl NativePluginFactory for NativeEndpointFactory {
    fn package_id(&self) -> &'static str {
        PACKAGE_ID
    }

    fn package_version(&self) -> &'static str {
        "0.0.0"
    }

    fn instantiate(
        &self,
        _: NativePluginFactoryContext<'_>,
    ) -> Result<NativePluginInstance, RuntimeFailure> {
        Ok(NativePluginInstance::new(vec![Rc::new(
            EndpointEndpoint::new(NativeEndpoint),
        )]))
    }
}

#[derive(Debug)]
struct HttpEndpointCodec;

impl HttpEndpointCodec {
    fn invalid() -> RuntimeFailure {
        RuntimeFailure::ProtocolViolation {
            capability: CAPABILITY_ID,
        }
    }
}

impl JsonCapabilityCodec for HttpEndpointCodec {
    fn capability_id(&self) -> &'static str {
        CAPABILITY_ID
    }

    fn descriptor_version(&self) -> &'static str {
        DESCRIPTOR_VERSION
    }

    fn request_operations(&self) -> &'static [&'static str] {
        &[DESCRIBE_OPERATION, HANDLE_OPERATION]
    }

    fn encode_request(&self, operation: &str, request: &dyn Any) -> Result<Value, RuntimeFailure> {
        match operation {
            DESCRIBE_OPERATION => request
                .downcast_ref::<DescribeRequest>()
                .and_then(|request| serde_json::to_value(request).ok()),
            HANDLE_OPERATION => request
                .downcast_ref::<HandleRequest>()
                .and_then(|request| serde_json::to_value(request).ok()),
            _ => None,
        }
        .ok_or_else(Self::invalid)
    }

    fn decode_response(
        &self,
        operation: &str,
        value: Value,
    ) -> Result<Box<dyn Any>, RuntimeFailure> {
        match operation {
            DESCRIBE_OPERATION => serde_json::from_value::<DescribeResponse>(value)
                .map(|value| Box::new(value) as Box<dyn Any>),
            HANDLE_OPERATION => serde_json::from_value::<HandleResponse>(value)
                .map(|value| Box::new(value) as Box<dyn Any>),
            _ => return Err(Self::invalid()),
        }
        .map_err(|_| Self::invalid())
    }

    fn decode_domain_error(
        &self,
        operation: &str,
        value: Value,
    ) -> Result<Box<dyn Any>, RuntimeFailure> {
        match operation {
            DESCRIBE_OPERATION => serde_json::from_value::<DescribeError>(value)
                .map(|value| Box::new(value) as Box<dyn Any>),
            HANDLE_OPERATION => serde_json::from_value::<HandleError>(value)
                .map(|value| Box::new(value) as Box<dyn Any>),
            _ => return Err(Self::invalid()),
        }
        .map_err(|_| Self::invalid())
    }
}

fn plan(wasm: bool) -> ResolvedAppPlan {
    let mut endpoint = PluginInstancePlan::new("endpoint", PACKAGE_ID).with_capability(
        CapabilityEndpointPlan::new(
            CAPABILITY_ID,
            DESCRIPTOR_VERSION,
            [DESCRIBE_OPERATION, HANDLE_OPERATION],
        ),
    );
    if wasm {
        endpoint = endpoint
            .with_entrypoint("plugin")
            .with_execution_class(ExecutionClassId::new(EXECUTION_CLASS));
    }
    let ingress = PluginInstancePlan::new("ingress", lenso_web_ingress_plugin::PACKAGE_ID)
        .with_configuration(serde_json::to_string(&WebIngressConfig::default()).unwrap())
        .with_requirement(CapabilityRequirementPlan::many(
            CAPABILITY_ID,
            DESCRIPTOR_VERSION,
        ));
    AppComposition::new(
        vec![endpoint, ingress],
        vec![CapabilityBinding::new(
            "ingress",
            CAPABILITY_ID,
            DESCRIPTOR_VERSION,
            "endpoint",
        )],
    )
    .resolve()
    .unwrap()
}

fn component() -> (tempfile::NamedTempFile, ArtifactHandle) {
    let target = tempfile::tempdir().unwrap();
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/portable-http-endpoint/guest/Cargo.toml");
    let status = Command::new(env!("CARGO"))
        .args([
            "build",
            "--locked",
            "--release",
            "--target",
            "wasm32-unknown-unknown",
            "--manifest-path",
        ])
        .arg(&manifest)
        .arg("--target-dir")
        .arg(target.path())
        .status()
        .unwrap();
    assert!(status.success(), "portable HTTP guest did not compile");
    let module = std::fs::read(
        target
            .path()
            .join("wasm32-unknown-unknown/release/lenso_portable_http_test_guest.wasm"),
    )
    .unwrap();
    let component = wit_component::ComponentEncoder::default()
        .module(&module)
        .unwrap()
        .validate(true)
        .encode()
        .unwrap();
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), &component).unwrap();
    let digest = format!("sha256:{}", hex::encode(Sha256::digest(&component)));
    let artifact = ArtifactHandle::open(file.path(), &digest, component.len() as u64).unwrap();
    (file, artifact)
}

async fn request(
    address: std::net::SocketAddr,
    method: &str,
    uri: &str,
    body: &[u8],
) -> Response<Bytes> {
    let stream = TcpStream::connect(address).await.unwrap();
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .unwrap();
    tokio::task::spawn_local(async move { connection.await.unwrap() });
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .body(Full::new(Bytes::copy_from_slice(body)))
        .unwrap();
    let response = sender.send_request(request).await.unwrap();
    let (parts, body) = response.into_parts();
    Response::from_parts(parts, body.collect().await.unwrap().to_bytes())
}

#[tokio::test(flavor = "current_thread")]
async fn one_http_endpoint_runs_through_native_and_real_wasm_component() {
    LocalSet::new()
        .run_until(async {
            let native_ingress = WebIngressFactory::new();
            let native = Kernel::start_native(
                plan(false),
                TokioDriver::new(),
                NativePluginRegistry::new()
                    .with_factory(NativeEndpointFactory)
                    .with_factory(native_ingress.clone()),
            )
            .await
            .unwrap();

            let (_component_file, artifact) = component();
            let wasm_ingress = WebIngressFactory::new();
            let wasm = WasmComponentAdapter::new(
                ArtifactCatalog::new()
                    .with_artifact("endpoint", artifact)
                    .unwrap(),
            )
            .with_codec(HttpEndpointCodec);
            let adapters = ExecutionAdapterCatalog::new()
                .with_adapter(NativePluginRegistry::new().with_factory(wasm_ingress.clone()))
                .unwrap()
                .with_adapter(wasm)
                .unwrap();
            let wasm_app = Kernel::start(plan(true), TokioDriver::new(), adapters)
                .await
                .unwrap();

            let cases: [(&str, &str, &[u8], u16, &[u8]); 6] = [
                ("GET", "/method/42", b"", 200, b"GET /method/42"),
                (
                    "POST",
                    "/bytes",
                    &[0, 255, 128, 13, 10, 1],
                    200,
                    &[0, 255, 128, 13, 10, 1],
                ),
                (
                    "POST",
                    "/method/42",
                    b"",
                    405,
                    br#"{"error":"method_not_allowed"}"#,
                ),
                ("GET", "/absent", b"", 404, br#"{"error":"not_found"}"#),
                (
                    "GET",
                    "/reject",
                    b"",
                    502,
                    br#"{"error":"endpoint_rejected"}"#,
                ),
                (
                    "GET",
                    "/failure",
                    b"",
                    503,
                    br#"{"error":"endpoint_unavailable"}"#,
                ),
            ];
            for (method, uri, body, status, expected_body) in cases {
                let native =
                    request(native_ingress.local_address().unwrap(), method, uri, body).await;
                let wasm = request(wasm_ingress.local_address().unwrap(), method, uri, body).await;
                assert_eq!(native.status().as_u16(), status, "native {method} {uri}");
                assert_eq!(wasm.status().as_u16(), status, "wasm {method} {uri}");
                assert_eq!(
                    native.body().as_ref(),
                    expected_body,
                    "native {method} {uri}"
                );
                assert_eq!(wasm.body().as_ref(), expected_body, "wasm {method} {uri}");
            }
            assert_eq!(
                native.shutdown(Duration::from_secs(2)).await,
                ShutdownOutcome::Clean
            );
            assert_eq!(
                wasm_app.shutdown(Duration::from_secs(2)).await,
                ShutdownOutcome::Clean
            );
        })
        .await;
}
