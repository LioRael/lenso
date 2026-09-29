use super::*;
use lenso_app_plan::{AppComposition, CapabilityBinding, CapabilityRequirementPlan, RestartPolicy};
use lenso_kernel::{
    DeterministicDriver, ExecutionAdapterCatalog, Kernel, RequestCapability, RuntimeDriver,
    ShutdownOutcome,
};
use lenso_native_adapter::{
    NativePluginFactory, NativePluginFactoryContext, NativePluginInstance, NativePluginRegistry,
};

#[derive(Debug)]
struct Echo;

impl RequestCapability for Echo {
    type Request = Value;
    type Response = Value;
    type DomainError = Value;
    const ID: &'static str = "example.echo@1";
    const DESCRIPTOR_VERSION: &'static str = "1.0.0";
}

#[derive(Debug)]
struct Consumer;

impl NativePluginFactory for Consumer {
    fn package_id(&self) -> &'static str {
        "test.consumer"
    }

    fn instantiate(
        &self,
        _: NativePluginFactoryContext<'_>,
    ) -> Result<NativePluginInstance, RuntimeFailure> {
        Ok(NativePluginInstance::default())
    }
}

fn start_app() -> (
    DeterministicDriver,
    lenso_kernel::NativeApp,
    lenso_process_adapter::ShutdownEvidence,
) {
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_lenso-process-test-fixture"));
    let bytes = fs::read(executable).unwrap();
    let digest = format!("sha256:{}", hex::encode(Sha256::digest(&bytes)));
    let artifact = ArtifactHandle::open(executable, &digest, bytes.len() as u64).unwrap();
    let adapter = ProcessAdapter::new(
        ArtifactCatalog::new()
            .with_artifact("plugin", artifact)
            .unwrap(),
    )
    .with_codec(EchoCodec);
    let evidence = adapter.shutdown_evidence();
    let plan = AppComposition::new(
        vec![
            PluginInstancePlan::new("plugin", "example.process")
                .with_entrypoint("plugin")
                .with_execution_class(ExecutionClassId::new(EXECUTION_CLASS))
                .with_restart_policy(RestartPolicy::on_failure(
                    1,
                    Duration::from_secs(5),
                    Duration::ZERO,
                    Duration::ZERO,
                    Duration::ZERO,
                ))
                .with_capability(CapabilityEndpointPlan::new(Echo::ID, "1.0.0", ["echo"])),
            PluginInstancePlan::new("consumer", "test.consumer")
                .with_requirement(CapabilityRequirementPlan::one(Echo::ID, "1.0.0")),
        ],
        vec![CapabilityBinding::new(
            "consumer",
            Echo::ID,
            "1.0.0",
            "plugin",
        )],
    )
    .resolve()
    .unwrap();
    let adapters = ExecutionAdapterCatalog::single(adapter)
        .with_adapter(NativePluginRegistry::new().with_factory(Consumer))
        .unwrap();
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start(plan, driver.clone(), adapters))
        .unwrap();
    (driver, app, evidence)
}

#[cfg(unix)]
#[test]
fn malformed_child_result_poisons_evidence_but_domain_rejection_does_not() {
    for malformed in [false, true] {
        let (driver, app, evidence) = start_app();
        let result = driver.run(app.handle::<Echo>("consumer").unwrap().invoke(
            "echo",
            json!({"malformed_result": malformed, "domain_error": !malformed}),
        ));
        if malformed {
            assert!(matches!(
                result,
                Err(RuntimeFailure::ProtocolViolation { .. })
            ));
        } else {
            assert_eq!(result.unwrap().unwrap_err(), json!({"kind": "rejected"}));
        }
        assert_eq!(
            driver.run(app.shutdown(Duration::from_secs(1))),
            ShutdownOutcome::Clean
        );
        drop(app);
        assert_eq!(evidence.is_clean(), !malformed);
    }
}

#[test]
fn crashed_process_restarts_without_replay_and_keeps_shutdown_evidence_unclean() {
    let (driver, app, evidence) = start_app();
    let handle = app.handle::<Echo>("consumer").unwrap();
    let directory = tempfile::tempdir().unwrap();
    let log = directory.path().join("calls");
    let result = driver.run(handle.invoke("echo", json!({"crash_log": log})));
    assert!(matches!(result, Err(RuntimeFailure::PluginFailure { .. })));
    for _ in 0..100 {
        driver.run(driver.yield_now());
        if app
            .plugin_generation("plugin")
            .is_some_and(|value| value >= 2)
        {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(app.plugin_generation("plugin"), Some(2));
    assert_eq!(
        driver
            .run(handle.invoke("echo", json!({"message": "recovered"})))
            .unwrap()
            .unwrap(),
        json!({"message": "recovered"})
    );
    assert_eq!(fs::read_to_string(log).unwrap(), "invoked\n");
    assert_eq!(
        driver.run(app.shutdown(Duration::from_secs(1))),
        ShutdownOutcome::Clean
    );
    drop((handle, app));
    assert!(!evidence.is_clean());
}
