//! Real native loopback HTTP and Wasmtime Component requests over one business handler.
//! This is a Native Environment receipt, not workerd or deployed Workers evidence.

use std::{process::Command, time::Duration};

use bytes::Bytes;
use http::{Request, Response};
use http_body_util::{BodyExt as _, Full};
use hyper_util::rt::TokioIo;
use lenso_app_plan::{
    AppComposition, CapabilityBinding, CapabilityEndpointPlan, CapabilityRequirementPlan,
    ExecutionClassId, PluginInstancePlan, ResolvedAppPlan,
};
use lenso_capability_http_endpoint::{
    CAPABILITY_ID, DESCRIBE_OPERATION, DESCRIPTOR_VERSION, EndpointJsonCodec, HANDLE_OPERATION,
};
use lenso_kernel::{ExecutionAdapterCatalog, Kernel, ShutdownOutcome};
use lenso_native_adapter::NativePluginRegistry;
use lenso_portable_http_endpoint_fixture::linked::{NativeEndpointFactory, PACKAGE_ID};
use lenso_runner::TokioDriver;
use lenso_runtime_codec::{ArtifactCatalog, ArtifactHandle};
use lenso_wasm_component_adapter::{EXECUTION_CLASS, WasmComponentAdapter};
use lenso_web_ingress_plugin::{WebIngressConfig, WebIngressFactory};
use sha2::{Digest, Sha256};
use tokio::{net::TcpStream, task::LocalSet};

type HttpCase<'a> = (&'a str, &'a str, &'a [u8], u16, &'a [u8]);

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
    let temporary_target = tempfile::tempdir().unwrap();
    let target = match std::env::var_os("LENSO_WASM_FIXTURE_CACHE_DIR") {
        Some(root) => {
            let root = std::path::PathBuf::from(root);
            assert!(root.is_absolute(), "fixture cache root must be absolute");
            // Keep nested Cargo's lock separate from the outer workspace build.
            root.join("wasm-guests")
        }
        None => temporary_target.path().to_owned(),
    };
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
        .arg(&target)
        .status()
        .unwrap();
    assert!(status.success(), "portable HTTP guest did not compile");
    let module = std::fs::read(
        target.join("wasm32-unknown-unknown/release/lenso_portable_http_test_guest.wasm"),
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
    headers: &[(&str, &str)],
) -> Response<Bytes> {
    let stream = TcpStream::connect(address).await.unwrap();
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .unwrap();
    tokio::task::spawn_local(async move { connection.await.unwrap() });
    let mut builder = Request::builder().method(method).uri(uri);
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let request = builder
        .body(Full::new(Bytes::copy_from_slice(body)))
        .unwrap();
    let response = sender.send_request(request).await.unwrap();
    let (parts, body) = response.into_parts();
    Response::from_parts(parts, body.collect().await.unwrap().to_bytes())
}

async fn assert_authorization_behavior(address: std::net::SocketAddr, label: &str) {
    let evidence = request(
        address,
        "GET",
        "/evidence",
        b"",
        &[("x-test", "alpha"), ("authorization", "Bearer test-token")],
    )
    .await;
    assert_eq!(evidence.status().as_u16(), 200, "{label} evidence");
    assert_eq!(
        evidence.body().as_ref(),
        b"bearer:alpha",
        "{label} evidence"
    );
    let ambiguous = request(
        address,
        "GET",
        "/evidence",
        b"",
        &[
            ("authorization", "Bearer first"),
            ("authorization", "Bearer second"),
        ],
    )
    .await;
    assert_eq!(ambiguous.status().as_u16(), 400, "{label} ambiguous bearer");
}

async fn assert_native_and_wasm_http_cases(
    native_address: std::net::SocketAddr,
    wasm_address: std::net::SocketAddr,
) {
    let cases: [HttpCase<'_>; 6] = [
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
        let native = request(native_address, method, uri, body, &[]).await;
        let wasm = request(wasm_address, method, uri, body, &[]).await;
        assert_eq!(native.status().as_u16(), status, "native {method} {uri}");
        assert_eq!(wasm.status().as_u16(), status, "wasm {method} {uri}");
        assert_eq!(
            native.body().as_ref(),
            expected_body,
            "native {method} {uri}"
        );
        assert_eq!(wasm.body().as_ref(), expected_body, "wasm {method} {uri}");
    }
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
            .with_codec(EndpointJsonCodec);
            let adapters = ExecutionAdapterCatalog::new()
                .with_adapter(NativePluginRegistry::new().with_factory(wasm_ingress.clone()))
                .unwrap()
                .with_adapter(wasm)
                .unwrap();
            let wasm_app = Kernel::start(plan(true), TokioDriver::new(), adapters)
                .await
                .unwrap();

            for (label, address) in [
                ("native", native_ingress.local_address().unwrap()),
                ("wasm", wasm_ingress.local_address().unwrap()),
            ] {
                assert_authorization_behavior(address, label).await;
            }

            assert_native_and_wasm_http_cases(
                native_ingress.local_address().unwrap(),
                wasm_ingress.local_address().unwrap(),
            )
            .await;
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
