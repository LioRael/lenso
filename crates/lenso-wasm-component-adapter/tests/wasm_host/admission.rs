//! Three real calls share the production Component worker and its bounded queue.
use std::cell::RefCell;

use futures::{channel::oneshot, select};

use super::*;

#[derive(Debug)]
struct ObservedEchoCodec {
    encoded_b: RefCell<Option<oneshot::Sender<()>>>,
}

impl JsonCapabilityCodec for ObservedEchoCodec {
    fn capability_id(&self) -> &'static str {
        NarrowCodec.capability_id()
    }

    fn descriptor_version(&self) -> &'static str {
        NarrowCodec.descriptor_version()
    }

    fn request_operations(&self) -> &'static [&'static str] {
        NarrowCodec.request_operations()
    }

    fn encode_request(&self, operation: &str, request: &dyn Any) -> Result<Value, RuntimeFailure> {
        let encoded = NarrowCodec.encode_request(operation, request)?;
        if encoded == 202 {
            self.encoded_b
                .borrow_mut()
                .take()
                .unwrap()
                .send(())
                .unwrap();
        }
        Ok(encoded)
    }

    fn decode_response(
        &self,
        operation: &str,
        value: Value,
    ) -> Result<Box<dyn Any>, RuntimeFailure> {
        NarrowCodec.decode_response(operation, value)
    }

    fn decode_domain_error(
        &self,
        operation: &str,
        value: Value,
    ) -> Result<Box<dyn Any>, RuntimeFailure> {
        NarrowCodec.decode_domain_error(operation, value)
    }
}

#[derive(Debug)]
struct GatedProbeCodec {
    entered_a: RefCell<Option<oneshot::Sender<()>>>,
    release_a: RefCell<Option<oneshot::Receiver<()>>>,
}

impl JsonCapabilityCodec for GatedProbeCodec {
    fn capability_id(&self) -> &'static str {
        ProbeCodec.capability_id()
    }

    fn descriptor_version(&self) -> &'static str {
        ProbeCodec.descriptor_version()
    }

    fn request_operations(&self) -> &'static [&'static str] {
        ProbeCodec.request_operations()
    }

    fn encode_request(&self, operation: &str, request: &dyn Any) -> Result<Value, RuntimeFailure> {
        ProbeCodec.encode_request(operation, request)
    }

    fn decode_response(
        &self,
        operation: &str,
        value: Value,
    ) -> Result<Box<dyn Any>, RuntimeFailure> {
        ProbeCodec.decode_response(operation, value)
    }

    fn decode_domain_error(
        &self,
        operation: &str,
        value: Value,
    ) -> Result<Box<dyn Any>, RuntimeFailure> {
        ProbeCodec.decode_domain_error(operation, value)
    }

    fn invoke_host_request(
        &self,
        dependency: PluginDependencyHandle,
        operation: String,
        request: Value,
        context: InvocationContext,
    ) -> JsonHostRequestFuture {
        let gate = (request["value"] == "wasm-101").then(|| {
            (
                self.entered_a.borrow_mut().take().unwrap(),
                self.release_a.borrow_mut().take().unwrap(),
            )
        });
        Box::pin(async move {
            if let Some((entered, release)) = gate {
                entered.send(()).unwrap();
                release.await.unwrap();
            }
            ProbeCodec
                .invoke_host_request(dependency, operation, request, context)
                .await
        })
    }
}

fn concurrent_import_plan() -> ResolvedAppPlan {
    let template = wasm_guest_import_plan();
    let mut plugin = PluginInstancePlan::new("plugin", "test.component")
        .with_entrypoint("plugin")
        .with_execution_class(ExecutionClassId::new(EXECUTION_CLASS))
        .with_capability(
            CapabilityEndpointPlan::new(
                EchoCapability::ID,
                EchoCapability::DESCRIPTOR_VERSION,
                ["echo"],
            )
            .with_limits(0, 3),
        );
    for requirement in template
        .plugin_instance("plugin")
        .unwrap()
        .required_capabilities()
    {
        plugin = plugin.with_requirement(requirement.clone());
    }
    let instances = template
        .plugin_instances()
        .iter()
        .map(|instance| {
            if instance.instance_key() == "plugin" {
                plugin.clone()
            } else {
                instance.clone()
            }
        })
        .collect();
    let bindings = template
        .capability_bindings()
        .iter()
        .map(|binding| {
            if binding.capability_id() == EchoCapability::ID {
                binding.clone().with_limits(0, 3)
            } else {
                binding.clone()
            }
        })
        .collect();
    // The default single-operation concurrency is one, which would keep B/C
    // in Kernel admission. This bounded test Plan admits exactly three calls.
    AppComposition::new(instances, bindings).resolve().unwrap()
}

#[test]
fn full_queue_rejection_preserves_running_component_and_queued_call() {
    // A control build may live in another worktree. Loading one prebuilt fixture
    // keeps its guest bytes identical while only the host guard changes.
    let guest = std::env::var_os("LENSO_WASM_QUALIFICATION_GUEST").map_or_else(
        || rust_host_import_guest().to_vec(),
        |path| std::fs::read(path).unwrap(),
    );
    let component = wit_component::ComponentEncoder::default()
        .module(&guest)
        .unwrap()
        .validate(true)
        .encode()
        .unwrap();
    let artifact_file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(artifact_file.path(), &component).unwrap();
    let digest = sha256_digest(&component);
    eprintln!(
        "qualification pid={} guest={} component={digest}",
        std::process::id(),
        sha256_digest(&guest)
    );
    let artifact =
        ArtifactHandle::open(artifact_file.path(), &digest, component.len() as u64).unwrap();
    let (entered_a, entered) = oneshot::channel();
    let (release, release_a) = oneshot::channel();
    let (encoded_b, encoded) = oneshot::channel();
    let publications = Arc::new(AtomicUsize::new(0));
    let wasm = WasmComponentAdapter::new(
        ArtifactCatalog::new()
            .with_artifact("plugin", artifact)
            .unwrap(),
    )
    .with_codec(ObservedEchoCodec {
        encoded_b: RefCell::new(Some(encoded_b)),
    })
    .with_codec(GatedProbeCodec {
        entered_a: RefCell::new(Some(entered_a)),
        release_a: RefCell::new(Some(release_a)),
    })
    .with_codec(NotificationsCodec)
    .with_limits(WasmComponentLimits {
        max_turn: Duration::from_secs(5),
        ..WasmComponentLimits::default()
    });
    let adapters = ExecutionAdapterCatalog::new()
        .with_adapter(
            ConformanceExecutionAdapter::new()
                .with_factory(ProbeProviderFactory)
                .with_factory(NotificationsFactory {
                    publications: publications.clone(),
                })
                .with_factory(EmptyConsumerFactory),
        )
        .unwrap()
        .with_adapter(wasm)
        .unwrap();
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start(
            concurrent_import_plan(),
            driver.clone(),
            adapters,
        ))
        .unwrap();
    let handle = app.handle::<EchoCapability>("consumer").unwrap();
    let (a, b, c) = driver.run(async {
        let a = handle.invoke("echo", 101).fuse();
        futures::pin_mut!(a);
        select! {
            result = a => panic!("A completed before its guest host import: {result:?}"),
            result = entered.fuse() => result.unwrap(),
        }
        // Only the running guest can issue this import. Its production worker has
        // set the Store epoch deadline and waits for our explicit release below.
        eprintln!("A entered guest host import; gate held");
        let b = handle.invoke("echo", 202).fuse();
        futures::pin_mut!(b);
        select! {
            result = a => panic!("A completed while waiting for B admission: {result:?}"),
            result = b => panic!("B completed while A held the worker: {result:?}"),
            result = encoded.fuse() => result.unwrap(),
        }
        // The same local task that encodes B polls call_inner through try_send
        // before yielding. No worker can consume B while A holds the import.
        eprintln!("B reached adapter and yielded; A gate still held");
        let c = handle.invoke("echo", 303).await;
        eprintln!("C result before A release: {c:?}");
        release.send(()).unwrap();
        let (a, b) = futures::join!(a, b);
        eprintln!("after release A={a:?} B={b:?}");
        (a, b, c)
    });
    let shutdown = driver.run(app.shutdown(Duration::from_secs(1)));
    eprintln!("shutdown={shutdown:?}");
    assert_eq!(shutdown, lenso_kernel::ShutdownOutcome::Clean);
    assert!(matches!(
        c,
        Err(RuntimeFailure::ResourceExhausted {
            capability: EXECUTION_CLASS,
            ref operation,
        }) if operation == "invoke"
    ));
    assert_eq!(
        a,
        Ok(Ok(101)),
        "C rejection must not interrupt A's guest turn"
    );
    assert_eq!(b, Ok(Ok(202)), "the already-admitted B must still execute");
    assert_eq!(publications.load(Ordering::Relaxed), 2);
}
