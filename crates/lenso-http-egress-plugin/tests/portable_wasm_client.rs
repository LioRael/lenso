//! A real Wasm Guest calls a Plan-bound Native HTTP Client over loopback.
//! This proves a Native Environment path, not Workers execution.

use std::{
    any::Any,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::{
    Router,
    body::Bytes,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use lenso_app_plan::{
    AppComposition, CapabilityBinding, CapabilityEndpointPlan, CapabilityRequirementPlan,
    ExecutionClassId, PlanResolutionError, PluginInstancePlan,
};
use lenso_capability_http_client::{
    CAPABILITY_ID as HTTP_CLIENT_ID, ClientJsonCodec, DESCRIPTOR_VERSION as HTTP_CLIENT_VERSION,
    SEND_OPERATION,
};
use lenso_http_egress_plugin::{HttpEgressConfig, PACKAGE_ID as EGRESS_PACKAGE_ID};
use lenso_kernel::{
    ExecutionAdapterCatalog, Kernel, RequestCapability, RuntimeFailure, ShutdownOutcome,
};
use lenso_native_adapter::{
    NativePluginFactory, NativePluginFactoryContext, NativePluginInstance, NativePluginRegistry,
};
use lenso_runner::TokioDriver;
use lenso_runtime_codec::{ArtifactCatalog, ArtifactHandle, JsonCapabilityCodec};
use lenso_wasm_component_adapter::{EXECUTION_CLASS, WasmComponentAdapter};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::{net::TcpListener, task::LocalSet};

const FIXTURE_ID: &str = "fixture.http-client@1";
const FIXTURE_VERSION: &str = "1.0.0";
const FIXTURE_PACKAGE: &str = "fixture.portable-http-client";

#[derive(Debug)]
struct CallerFactory;

impl NativePluginFactory for CallerFactory {
    fn package_id(&self) -> &'static str {
        "fixture.http-client-caller"
    }

    fn instantiate(
        &self,
        _: NativePluginFactoryContext<'_>,
    ) -> Result<NativePluginInstance, RuntimeFailure> {
        Ok(NativePluginInstance::default())
    }
}

#[derive(Debug)]
struct FixtureCapability;

impl RequestCapability for FixtureCapability {
    type Request = Value;
    type Response = Value;
    type DomainError = Value;

    const ID: &'static str = FIXTURE_ID;
    const DESCRIPTOR_VERSION: &'static str = FIXTURE_VERSION;
}

#[derive(Debug)]
struct FixtureCodec;

impl JsonCapabilityCodec for FixtureCodec {
    fn capability_id(&self) -> &'static str {
        FIXTURE_ID
    }
    fn descriptor_version(&self) -> &'static str {
        FIXTURE_VERSION
    }
    fn request_operations(&self) -> &'static [&'static str] {
        &["run"]
    }

    fn encode_request(&self, operation: &str, request: &dyn Any) -> Result<Value, RuntimeFailure> {
        if operation == "run" {
            request
                .downcast_ref::<Value>()
                .cloned()
                .ok_or_else(fixture_protocol_error)
        } else {
            Err(fixture_protocol_error())
        }
    }

    fn decode_response(
        &self,
        operation: &str,
        value: Value,
    ) -> Result<Box<dyn Any>, RuntimeFailure> {
        if operation == "run" {
            Ok(Box::new(value))
        } else {
            Err(fixture_protocol_error())
        }
    }

    fn decode_domain_error(
        &self,
        operation: &str,
        value: Value,
    ) -> Result<Box<dyn Any>, RuntimeFailure> {
        if operation == "run" {
            Ok(Box::new(value))
        } else {
            Err(fixture_protocol_error())
        }
    }
}

fn fixture_protocol_error() -> RuntimeFailure {
    RuntimeFailure::ProtocolViolation {
        capability: FIXTURE_ID,
    }
}

fn composition(config: &HttpEgressConfig, bind_client: bool) -> AppComposition {
    let egress = PluginInstancePlan::new("http-egress", EGRESS_PACKAGE_ID)
        .with_configuration(serde_json::to_string(config).unwrap())
        .with_capability(CapabilityEndpointPlan::new(
            HTTP_CLIENT_ID,
            HTTP_CLIENT_VERSION,
            [SEND_OPERATION],
        ));
    let guest = PluginInstancePlan::new("guest", FIXTURE_PACKAGE)
        .with_entrypoint("plugin")
        .with_execution_class(ExecutionClassId::new(EXECUTION_CLASS))
        .with_requirement(
            CapabilityRequirementPlan::one(HTTP_CLIENT_ID, HTTP_CLIENT_VERSION)
                .with_requirement_id("~lenso.http.client@1"),
        )
        .with_capability(CapabilityEndpointPlan::new(
            FIXTURE_ID,
            FIXTURE_VERSION,
            ["run"],
        ));
    let caller = PluginInstancePlan::new("caller", "fixture.http-client-caller")
        .with_requirement(CapabilityRequirementPlan::one(FIXTURE_ID, FIXTURE_VERSION));
    let mut bindings = vec![CapabilityBinding::new(
        "caller",
        FIXTURE_ID,
        FIXTURE_VERSION,
        "guest",
    )];
    if bind_client {
        bindings.push(
            CapabilityBinding::new("guest", HTTP_CLIENT_ID, HTTP_CLIENT_VERSION, "http-egress")
                .with_requirement_id("~lenso.http.client@1"),
        );
    }
    AppComposition::new(vec![egress, guest, caller], bindings)
}

fn guest_artifact() -> ArtifactHandle {
    let target = tempfile::tempdir().unwrap();
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/portable-client-guest/Cargo.toml");
    let status = Command::new(env!("CARGO"))
        .args([
            "build",
            "--locked",
            "--offline",
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
    assert!(
        status.success(),
        "portable HTTP Client Guest did not compile"
    );
    let module = std::fs::read(
        target
            .path()
            .join("wasm32-unknown-unknown/release/lenso_portable_http_client_test_guest.wasm"),
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
    ArtifactHandle::open(file.path(), &digest, component.len() as u64).unwrap()
}

async fn loopback_server() -> (
    std::net::SocketAddr,
    Arc<AtomicUsize>,
    tokio::task::JoinHandle<()>,
) {
    let hits = Arc::new(AtomicUsize::new(0));
    let echo_hits = hits.clone();
    let failure_hits = hits.clone();
    let router = Router::new()
        .route(
            "/echo",
            post(move |headers: HeaderMap, body: Bytes| {
                echo_hits.fetch_add(1, Ordering::SeqCst);
                async move {
                    let observed = headers
                        .get("x-client")
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_owned();
                    (StatusCode::CREATED, [("x-observed-client", observed)], body)
                }
            }),
        )
        .route(
            "/failure",
            get(move || {
                failure_hits.fetch_add(1, Ordering::SeqCst);
                async { (StatusCode::SERVICE_UNAVAILABLE, "offline") }
            }),
        );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (address, hits, task)
}

async fn call(app: &lenso_kernel::NativeApp, request: Value) -> Value {
    app.handle::<FixtureCapability>("caller")
        .unwrap()
        .invoke("run", request)
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn portable_guest_calls_only_its_bound_native_http_client() {
    LocalSet::new()
        .run_until(async {
            let (address, hits, server) = loopback_server().await;
            let origin = format!("http://{address}");
            let config = HttpEgressConfig::new([&origin]).unwrap();
            let wasm = WasmComponentAdapter::new(
                ArtifactCatalog::new()
                    .with_artifact("guest", guest_artifact())
                    .unwrap(),
            )
            .with_codec(FixtureCodec)
            .with_codec(ClientJsonCodec);
            let adapters = ExecutionAdapterCatalog::new()
                .with_adapter(
                    NativePluginRegistry::new()
                        .with_linked_factories()
                        .with_factory(CallerFactory),
                )
                .unwrap()
                .with_adapter(wasm)
                .unwrap();
            let app = Kernel::start(
                composition(&config, true).resolve().unwrap(),
                TokioDriver::new(),
                adapters,
            )
            .await
            .unwrap();

            let response = call(
                &app,
                json!({
                    "method": "POST", "url": format!("{origin}/echo"),
                    "headers": [{"name": "x-client", "value": "portable"}],
                    "body": "AAH/",
                }),
            )
            .await;
            assert_eq!(response["provider"], "http-egress");
            assert_eq!(response["response"]["status"], 201);
            assert_eq!(response["response"]["body"], "AAH/");
            assert!(
                response["response"]["headers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|header| header["name"] == "x-observed-client"
                        && header["value"] == "portable")
            );

            let denied = call(
                &app,
                json!({
                    "method": "GET", "url": "http://127.0.0.1:1/private",
                    "headers": [], "body": "",
                }),
            )
            .await;
            assert_eq!(denied["provider"], "http-egress");
            assert_eq!(denied["domain_error"], "destination_not_allowed");
            assert_eq!(hits.load(Ordering::SeqCst), 1);

            let unavailable = call(
                &app,
                json!({
                    "method": "GET", "url": format!("{origin}/failure"),
                    "headers": [], "body": "",
                }),
            )
            .await;
            assert_eq!(unavailable["response"]["status"], 503);
            assert_eq!(unavailable["response"]["body"], "b2ZmbGluZQ==");
            assert_eq!(hits.load(Ordering::SeqCst), 2);

            assert_eq!(
                app.shutdown(Duration::from_secs(1)).await,
                ShutdownOutcome::Clean
            );
            server.abort();
        })
        .await;
}

#[test]
fn portable_guest_without_explicit_http_binding_is_not_resolved() {
    let config = HttpEgressConfig::new(["http://127.0.0.1:44001"]).unwrap();
    assert!(matches!(
        composition(&config, false).resolve(),
        Err(PlanResolutionError::MissingOneBinding {
            consumer_instance,
            capability_id,
        }) if consumer_instance == "guest" && capability_id == HTTP_CLIENT_ID
    ));
}
