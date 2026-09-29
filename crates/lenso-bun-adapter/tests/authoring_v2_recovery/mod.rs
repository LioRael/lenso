use super::*;
use lenso_app_plan::RestartPolicy;
use lenso_kernel::RuntimeDriver;

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "keep the configured restart, stable-handle and retirement assertions together"
)]
fn crashed_provider_recovers_without_replay_or_a_clean_retirement_witness() {
    let bundle = tempfile::tempdir().unwrap();
    let source = bundle.path().join("crash.ts");
    let entrypoint = bundle.path().join("plugin.js");
    let sdk = fixture("v2-echo-child.ts")
        .parent()
        .unwrap()
        .join("../../src/index.ts");
    fs::write(
        &source,
        format!(
            r#"
import {{ definePlugin, provider, servePluginV2 }} from {sdk:?};
const descriptor = {{
  capability_id: "{ECHO_ID}",
  descriptor_version: "{VERSION}",
  descriptor_digest: "{ECHO_DIGEST}",
  operations: ["echo"], stream_operations: [], event_operations: [],
}};
let calls = 0;
await servePluginV2(definePlugin({{
  providers: [provider(descriptor, () => ({{
    descriptor,
    async invokeRequest(_operation, _context, payload) {{
      calls++;
      if (payload.crash) process.exit(17);
      return {{ kind: "success", value: {{ calls }} }};
    }},
  }}))],
}}));
"#,
        ),
    )
    .unwrap();
    assert!(
        Command::new(bun_binary())
            .args(["build", "--target", "bun", "--outfile"])
            .arg(&entrypoint)
            .arg(&source)
            .status()
            .unwrap()
            .success()
    );
    let bytes = fs::read(&entrypoint).unwrap();
    let digest = format!("sha256:{}", hex::encode(Sha256::digest(&bytes)));
    let artifact = ArtifactHandle::open(&entrypoint, &digest, bytes.len() as u64).unwrap();
    let adapter = BunAdapter::production(bun_binary())
        .with_artifacts(
            ArtifactCatalog::new()
                .with_artifact("echo", artifact)
                .unwrap(),
        )
        .with_authoring_codec(EchoCodec);
    let evidence = adapter.shutdown_evidence();
    let adapters = ExecutionAdapterCatalog::new()
        .with_adapter(NativePluginRegistry::new().with_factory(EmptyConsumerFactory))
        .unwrap()
        .with_adapter(adapter)
        .unwrap();
    let plan = AppComposition::new(
        vec![
            PluginInstancePlan::new("echo", "test.bun-echo")
                .with_authoring(2, BUN_AUTHORING_RUNTIME_PROFILE)
                .with_entrypoint("plugin")
                .with_execution_class(ExecutionClassId::bun_child_process())
                .with_restart_policy(RestartPolicy::on_failure(
                    2,
                    Duration::from_secs(5),
                    Duration::ZERO,
                    Duration::ZERO,
                    Duration::ZERO,
                ))
                .with_capability(CapabilityEndpointPlan::new(ECHO_ID, VERSION, ["echo"])),
            PluginInstancePlan::new("consumer", "test.echo-consumer")
                .with_requirement(CapabilityRequirementPlan::one(ECHO_ID, VERSION)),
        ],
        vec![CapabilityBinding::new("consumer", ECHO_ID, VERSION, "echo")],
    )
    .resolve()
    .unwrap();
    let driver = DeterministicDriver::new();
    let app = driver
        .run(Kernel::start(plan, driver.clone(), adapters))
        .unwrap();
    let handle = app.handle::<Echo>("consumer").unwrap();
    assert!(
        driver
            .run(handle.invoke("echo", json!({"crash": true})))
            .is_err()
    );
    for _ in 0..100 {
        driver.run(driver.yield_now());
        if app.plugin_generation("echo") == Some(2) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        app.plugin_generation("echo"),
        Some(2),
        "crashed provider should recover: {:?}",
        app.terminal_failure()
    );
    assert_eq!(
        driver
            .run(handle.invoke("echo", json!({})))
            .unwrap()
            .unwrap(),
        json!({"calls": 1}),
        "the stable handle must not replay the crashed invocation"
    );
    assert!(matches!(
        driver.run(app.shutdown(Duration::from_secs(2))),
        lenso_kernel::ShutdownOutcome::Clean
    ));
    drop((handle, app));
    assert!(!evidence.is_clean());
}
