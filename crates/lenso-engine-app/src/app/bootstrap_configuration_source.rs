//! A source Plugin runs in its own exact, short-lived Plan before the business App exists.
//! The operator policy, not the Plugin response, owns identity, release and field grants.

use std::{collections::BTreeMap, fs, path::Path, time::Duration};

use anyhow::{Context as _, bail, ensure};
use lenso_app_authoring::{
    PluginConfigurationAuthoritySource, VersionedPluginConfigurationSnapshot,
};
use lenso_app_plan::{
    CapabilityBinding, CapabilityRequirementPlan, ExecutionClassId, PluginInstancePlan,
    ResolvedAppPlan, authoring::PluginDescriptor,
};
use lenso_capability_configuration_source::host::{
    CAPABILITY_ID, DESCRIPTOR_VERSION, FETCH_OPERATION, FetchRequest, FetchResponse,
    Source as SourceCapability, SourceJsonCodec,
};
use lenso_kernel::{
    ExecutionAdapter, ExecutionAdapterCatalog, Kernel, NoopPluginLifecycle, PreparedNativeApp,
    PreparedNativePlugin, RuntimeFailure, ShutdownOutcome,
};
use lenso_plugin_bundle::{
    BundleVerificationLimits, ImplementationPolicy, read_verified_bundle_with_limits,
    resolve_implementation,
};
use lenso_process_adapter::{
    EXECUTION_CLASS as PROCESS_CLASS, ProcessAdapter, ProcessLimits, RUNTIME_PROFILE_V2,
};
use lenso_runner::TokioDriver;
use lenso_runtime_codec::{ArtifactCatalog, ArtifactHandle};
use serde_json::{Value, json};

const SOURCE_INSTANCE: &str = "bootstrap-source";
const CLIENT_INSTANCE: &str = "bootstrap-host";
const CLIENT_PACKAGE: &str = "lenso.bootstrap-host";
const CLIENT_CLASS: &str = "lenso.bootstrap-host@1";
// Process Authoring V2 currently admits at most a 1 MiB frame, including its
// envelope. This bootstrap path intentionally inherits that tighter bound.
const MAX_FRAME_BYTES: usize = 1_048_576;

/// Exact release pins are Host policy. This value is never sourced from a
/// Plugin response or the business App's Plugin Root.
pub(super) struct BootstrapSourceSelection<'a> {
    pub bundle: &'a Path,
    pub plugin_id: &'a str,
    pub release_version: &'a str,
    pub manifest_digest: &'a str,
    pub artifact_digest: &'a str,
    pub configuration: &'a Value,
}

pub(super) fn fetch(
    selected: BootstrapSourceSelection<'_>,
    identity: PluginConfigurationAuthoritySource,
) -> anyhow::Result<VersionedPluginConfigurationSnapshot> {
    ensure!(
        selected.bundle.is_absolute(),
        "bootstrap source bundle path must be absolute"
    );
    ensure!(
        fs::symlink_metadata(selected.bundle)?.file_type().is_dir(),
        "bootstrap source bundle must be a real directory"
    );
    let (verified, manifest) =
        read_verified_bundle_with_limits(selected.bundle, &BundleVerificationLimits::default())
            .map_err(|error| anyhow::anyhow!("verify bootstrap source bundle: {error}"))?;
    ensure!(
        verified.plugin_id == selected.plugin_id
            && verified.release_version == selected.release_version
            && verified.manifest_digest == selected.manifest_digest,
        "bootstrap source bundle differs from Host policy"
    );
    let implementation = resolve_implementation(
        &manifest,
        &ImplementationPolicy {
            host_target: lenso_app_authoring::native_host_target().to_owned(),
            runtimes: vec![
                super::super::target_profile::request_native_process_admission(
                    ExecutionClassId::new(PROCESS_CLASS),
                    RUNTIME_PROFILE_V2,
                ),
            ],
        },
    )
    .map_err(|error| anyhow::anyhow!("select bootstrap source implementation: {error}"))?;
    ensure!(
        implementation.artifact.digest == selected.artifact_digest,
        "bootstrap source Artifact differs from Host policy"
    );
    let descriptor = &implementation.descriptor;
    ensure!(
        descriptor.plugin_id() == selected.plugin_id
            && descriptor.release_version() == selected.release_version
            && descriptor.authoring_version() == 2
            && descriptor.runtime_profile() == RUNTIME_PROFILE_V2
            && descriptor.entrypoint() == "plugin"
            && descriptor.required_capabilities().is_empty(),
        "bootstrap source Plugin has an unsupported contract or dependencies"
    );
    let [capability] = descriptor.provided_capabilities() else {
        bail!("bootstrap source Plugin must provide exactly one Capability");
    };
    ensure!(
        capability.capability_id() == CAPABILITY_ID
            && capability.descriptor_version() == DESCRIPTOR_VERSION
            && capability.request_operations() == [FETCH_OPERATION]
            && capability.stream_operations().is_empty()
            && capability.event_operations().is_empty(),
        "bootstrap source Plugin does not provide the exact Configuration Source Capability"
    );
    let configuration = source_configuration(descriptor, selected.configuration)?;

    let artifact = ArtifactHandle::open(
        selected.bundle.join(&implementation.artifact.path),
        selected.artifact_digest,
        implementation.artifact.size,
    )
    .map_err(|error| anyhow::anyhow!("open bootstrap source Artifact: {error:?}"))?;
    let artifacts = ArtifactCatalog::new()
        .with_artifact(SOURCE_INSTANCE, artifact)
        .map_err(|error| anyhow::anyhow!("register bootstrap source Artifact: {error:?}"))?;
    let plan = ResolvedAppPlan::new(
        vec![
            PluginInstancePlan::new(SOURCE_INSTANCE, selected.plugin_id)
                .with_package_revision(selected.artifact_digest)
                .with_authoring(descriptor.authoring_version(), descriptor.runtime_profile())
                .with_entrypoint(descriptor.entrypoint())
                .with_configuration(configuration)
                .with_execution_class(ExecutionClassId::new(PROCESS_CLASS))
                .with_required_target_capabilities(
                    descriptor.required_target_capabilities().iter().copied(),
                )
                .with_capability(capability.clone()),
            PluginInstancePlan::new(CLIENT_INSTANCE, CLIENT_PACKAGE)
                .with_authoring(1, CLIENT_CLASS)
                .with_entrypoint("host")
                .with_execution_class(ExecutionClassId::new(CLIENT_CLASS))
                .with_requirement(CapabilityRequirementPlan::one(
                    CAPABILITY_ID,
                    DESCRIPTOR_VERSION,
                )),
        ],
        vec![CapabilityBinding::new(
            CLIENT_INSTANCE,
            CAPABILITY_ID,
            DESCRIPTOR_VERSION,
            SOURCE_INSTANCE,
        )],
    );
    plan.validate()
        .map_err(|error| anyhow::anyhow!("bootstrap source Plan is invalid: {error}"))?;

    // `config-sync` can itself be called from Tokio. Keep the bootstrap
    // generation on a separate thread, then close it before App resolution.
    let response = std::thread::spawn(move || invoke(plan, artifacts))
        .join()
        .map_err(|_| anyhow::anyhow!("bootstrap source runtime thread panicked"))??;
    let revision = response
        .revision
        .parse::<u64>()
        .context("bootstrap source returned an invalid revision")?;
    ensure!(
        revision.to_string() == response.revision,
        "bootstrap source revision is not canonical"
    );
    let document = json!({
        "schema": "lenso.plugin-configuration-snapshot.v1",
        "revision": revision,
        "configurations": response.configurations,
    });
    let bytes = serde_json::to_vec(&document)?;
    ensure!(
        bytes.len() <= MAX_FRAME_BYTES,
        "bootstrap source snapshot exceeds 1 MiB"
    );
    VersionedPluginConfigurationSnapshot::from_host_bound_json(identity, &bytes)
}

fn source_configuration(descriptor: &PluginDescriptor, overlay: &Value) -> anyhow::Result<String> {
    ensure!(
        overlay.is_object(),
        "bootstrap source configuration must be an object"
    );
    descriptor
        .resolve_configuration_json(&[overlay], SOURCE_INSTANCE)
        .context("bootstrap source configuration does not match its package schema")
}

fn invoke(plan: ResolvedAppPlan, artifacts: ArtifactCatalog) -> anyhow::Result<FetchResponse> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()?;
    let result = {
        let _entered = runtime.enter();
        futures::executor::block_on(tokio::task::LocalSet::new().run_until(async move {
            let adapters = ExecutionAdapterCatalog::new()
                .with_adapter(
                    ProcessAdapter::new(artifacts)
                        .with_limits(ProcessLimits {
                            max_frame_bytes: MAX_FRAME_BYTES,
                            max_pending_requests: 1,
                            startup_timeout: Duration::from_secs(5),
                            cancellation_settlement_timeout: Duration::from_secs(2),
                        })
                        .with_codec(SourceJsonCodec),
                )
                .map_err(|error| anyhow::anyhow!("register bootstrap Process Adapter: {error:?}"))?
                .with_adapter(BootstrapClientAdapter)
                .map_err(|error| anyhow::anyhow!("register bootstrap Host Adapter: {error:?}"))?;
            let app = Kernel::start(plan, TokioDriver::new(), adapters)
                .await
                .map_err(|error| anyhow::anyhow!("start bootstrap source Plugin: {error:?}"))?;
            let outcome = async {
                let handle = app
                    .handle::<SourceCapability>(CLIENT_INSTANCE)
                    .map_err(|error| anyhow::anyhow!("bind bootstrap source: {error:?}"))?;
                let result = tokio::time::timeout(
                    Duration::from_secs(15),
                    handle.invoke(FETCH_OPERATION, FetchRequest {}),
                )
                .await
                .context("bootstrap source fetch deadline exceeded")?
                .map_err(|_| anyhow::anyhow!("bootstrap source runtime failed"))?
                .map_err(|_| anyhow::anyhow!("bootstrap source rejected the fetch"))?;
                Ok::<_, anyhow::Error>(result)
            }
            .await;
            let shutdown = app.shutdown(Duration::from_secs(3)).await;
            ensure!(
                shutdown == ShutdownOutcome::Clean,
                "bootstrap source Plugin did not shut down cleanly"
            );
            outcome
        }))
    };
    runtime.shutdown_background();
    result
}

#[derive(Debug)]
struct BootstrapClientAdapter;

impl ExecutionAdapter for BootstrapClientAdapter {
    fn execution_class(&self) -> ExecutionClassId {
        ExecutionClassId::new(CLIENT_CLASS)
    }

    fn prepare(&self, plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure> {
        let instance = plan.plugin_instance(CLIENT_INSTANCE).ok_or_else(|| {
            RuntimeFailure::InvalidResolvedPlan {
                detail: "missing bootstrap Host client".to_owned(),
            }
        })?;
        if instance.package_id() != CLIENT_PACKAGE
            || instance.execution_class().as_str() != CLIENT_CLASS
        {
            return Err(RuntimeFailure::InvalidResolvedPlan {
                detail: "unexpected bootstrap Host client".to_owned(),
            });
        }
        Ok(PreparedNativeApp::new(
            Vec::new(),
            BTreeMap::from([(
                CLIENT_INSTANCE.to_owned(),
                PreparedNativePlugin::new(Vec::new(), NoopPluginLifecycle),
            )]),
        ))
    }
}

#[cfg(test)]
mod tests {
    use lenso_app_plan::authoring::PluginDescriptor;
    use serde_json::{Value, json};

    use super::source_configuration;

    fn descriptor() -> PluginDescriptor {
        PluginDescriptor::new("example.configuration-source", "1.0.0", "source")
            .with_configuration_defaults(json!({"path": "/default"}))
            .with_configuration_schema(json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "token": {"x-lenso-sensitive": true}
                },
                "required": ["path"],
                "additionalProperties": false
            }))
    }

    #[test]
    fn bootstrap_configuration_merges_defaults_and_rejects_unknown_fields() {
        let selected = source_configuration(&descriptor(), &json!({})).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&selected).unwrap(),
            json!({"path": "/default"})
        );

        let error = source_configuration(&descriptor(), &json!({"unexpected": true})).unwrap_err();
        assert!(format!("{error:#}").contains("$.unexpected"));
        assert!(source_configuration(&descriptor(), &json!("not an object")).is_err());
    }

    #[test]
    fn bootstrap_configuration_accepts_only_secret_references() {
        let valid = source_configuration(
            &descriptor(),
            &json!({"token": {"secret_ref": "operator/source-token"}}),
        )
        .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&valid).unwrap(),
            json!({"path": "/default", "token": {"secret_ref": "operator/source-token"}})
        );

        let error =
            source_configuration(&descriptor(), &json!({"token": "plaintext-secret"})).unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("$.token: sensitive value must be a secret_ref"));
        assert!(!message.contains("plaintext-secret"));
    }
}
