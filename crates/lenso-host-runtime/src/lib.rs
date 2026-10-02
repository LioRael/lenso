mod codecs;
use codecs::{PortableJsonCodec, intern_operations, intern_string};

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    env,
    path::{Path, PathBuf},
    process::Stdio,
    rc::Rc,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use lenso::host::{self, FileControlStateStore, HostBuilder, KernelGenerationRuntime};
use lenso_bun_adapter::{BunAdapter, BunAdapterConfig, BunWire};
use lenso_host_distribution::{PreparedHostGeneration, VerifiedDistribution};
use lenso_kernel::ExecutionAdapterCatalog;
use lenso_plugin_control_plane::{
    AppGenerationTransitionSpec, CanonicalDocument, CatalogFactory, ControlLifecycle,
    ControlPlaneError, ControlStateStore, ReplacementMode, ResolvedGeneration, RolloutPolicy,
};
use lenso_process_adapter::ProcessAdapter;
use lenso_runtime_codec::JsonCapabilityCodec;

const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const STOP_TIMEOUT: Duration = Duration::from_secs(10);

/// Runs a prepared TypeScript Host with its explicitly linked generated codecs.
///
/// Contracts crossing a Bun Authoring V2 boundary require exact typed codecs;
/// the stock binary keeps its legacy Request-only JSON fallback. No codec grants
/// a Plugin a binding or permission: the verified distribution and Host policy
/// remain the only composition and resource authority.
pub fn run_with_codecs(
    codecs: Vec<Rc<dyn JsonCapabilityCodec>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let arguments = parse_arguments()?;
    run(&arguments, codecs)
}

fn run(
    arguments: &Arguments,
    codecs: Vec<Rc<dyn JsonCapabilityCodec>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let distribution_root = arguments
        .distribution_lock
        .parent()
        .and_then(Path::parent)
        .ok_or("distribution lock needs a distribution root")?;
    let expected_lock =
        std::fs::canonicalize(distribution_root.join(".lenso/distribution.lock.json"))?;
    let supplied_lock = std::fs::canonicalize(&arguments.distribution_lock)?;
    if supplied_lock != expected_lock {
        return Err("--distribution must name the distribution's canonical lock".into());
    }
    let distribution = VerifiedDistribution::open(distribution_root)?;
    let identity = distribution.identity().to_owned();
    let app_root = std::fs::canonicalize(&arguments.app_root)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(tokio::task::LocalSet::new().run_until(async move {
        let options = host::control::ControlOptions {
            distribution: identity,
            startup_timeout: arguments.startup_timeout,
            stop_timeout: arguments.stop_timeout,
        };
        let reconcile_distribution = distribution.clone();
        let reconcile_root = app_root.clone();
        let reconcile_policy = arguments.configuration_policy.clone();
        let receipt_distribution = distribution.clone();
        let receipt_root = app_root.clone();
        let receipt_policy = arguments.configuration_policy.clone();
        let candidate_revisions = Arc::new(Mutex::new(HashMap::<String, String>::new()));
        let initial_revisions = candidate_revisions.clone();
        let reconcile_revisions = candidate_revisions.clone();
        host::control::serve_with_reconcile_and_receipt(
            options,
            tokio::io::stdin(),
            tokio::io::stdout(),
            move || async move {
                let result = start_host(
                    distribution,
                    app_root,
                    arguments.configuration_policy.clone(),
                    arguments.startup_timeout,
                    arguments.stop_timeout,
                    initial_revisions,
                    codecs,
                )
                .await;
                if let Err(error) = &result {
                    eprintln!("Host startup failed: {error}");
                }
                result
            },
            move || {
                let distribution = reconcile_distribution.clone();
                let root = reconcile_root.clone();
                let policy = reconcile_policy.clone();
                let revisions = reconcile_revisions.clone();
                async move {
                    prepare_generation(&distribution, &root, policy.as_deref())
                        .await
                        .and_then(|prepared| {
                            remember_revision(&revisions, &prepared)?;
                            Ok(prepared.generation)
                        })
                }
            },
            move |generation| {
                let distribution = receipt_distribution.clone();
                let root = receipt_root.clone();
                let policy = receipt_policy.clone();
                let revisions = candidate_revisions.clone();
                let digest = generation.spec.digest().to_owned();
                async move {
                    if policy.is_none() {
                        return Ok(());
                    }
                    let revision = revisions
                        .lock()
                        .map_err(host_failure)?
                        .get(&digest)
                        .cloned()
                        .ok_or_else(|| host_failure("active Generation lacks a Root revision"))?;
                    record_activation(&distribution, &root, &revision).await
                }
            },
        )
        .await
    }))?;
    Ok(())
}

async fn start_host(
    distribution: VerifiedDistribution,
    app_root: PathBuf,
    configuration_policy: Option<PathBuf>,
    startup_timeout: Duration,
    stop_timeout: Duration,
    candidate_revisions: Arc<Mutex<HashMap<String, String>>>,
    codecs: Vec<Rc<dyn JsonCapabilityCodec>>,
) -> Result<(host::Host<lenso_kernel::NativeApp>, ResolvedGeneration), ControlPlaneError> {
    let app_id = distribution.app_id().to_owned();
    let bun = distribution.root().join("runtime/bun");
    let prepared =
        prepare_generation(&distribution, &app_root, configuration_policy.as_deref()).await?;
    remember_revision(&candidate_revisions, &prepared)?;
    let factory =
        HostCatalogFactory::new(bun, app_root.clone(), &prepared.generation.plan, codecs)?;
    let runtime = KernelGenerationRuntime::new(factory);
    let store = FileControlStateStore::open(app_root.join(".lenso/runtime-control"))?;
    let state = store.load(&app_id)?;
    let candidate = prepared.generation;
    let live = state
        .generations
        .iter()
        .filter(|record| {
            matches!(
                record.lifecycle,
                ControlLifecycle::Staged
                    | ControlLifecycle::Ready
                    | ControlLifecycle::Active
                    | ControlLifecycle::Draining
                    | ControlLifecycle::Standby
            )
        })
        .map(|record| record.generation_spec_digest.as_str())
        .collect::<BTreeSet<_>>();
    let builder = HostBuilder::new(&app_id, runtime, store);
    let host = if live.is_empty() {
        activate_initial(builder.build()?, &candidate, startup_timeout, stop_timeout).await?
    } else if live.iter().all(|digest| *digest == candidate.spec.digest()) {
        let generations = BTreeMap::from([(candidate.spec.digest().to_owned(), candidate.clone())]);
        builder.recover(&generations, unix_nanos()?).await?
    } else if state.host_suspended {
        activate_initial(
            builder.replace_suspended()?,
            &candidate,
            startup_timeout,
            stop_timeout,
        )
        .await?
    } else {
        return Err(host_failure(
            "durable state needs an unavailable previous Generation; cleanly suspend the old Host before replacing its build",
        ));
    };
    Ok((host, candidate))
}

fn remember_revision(
    revisions: &Arc<Mutex<HashMap<String, String>>>,
    prepared: &PreparedHostGeneration,
) -> Result<(), ControlPlaneError> {
    revisions.lock().map_err(host_failure)?.insert(
        prepared.generation.spec.digest().to_owned(),
        prepared.plugin_root_revision.clone(),
    );
    Ok(())
}

async fn record_activation(
    distribution: &VerifiedDistribution,
    app_root: &Path,
    revision: &str,
) -> Result<(), ControlPlaneError> {
    let mut command =
        tokio::process::Command::new(distribution.root().join("runtime/lenso-resolver"));
    command
        .args(["app", "config-activated", "--root"])
        .arg(app_root)
        .arg("--host-build")
        .arg(distribution.root().join(".lenso/host-build.json"))
        .arg("--plugin-root-revision")
        .arg(revision)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(host_failure)?;
    let status = tokio::time::timeout(Duration::from_secs(30), child.wait())
        .await
        .map_err(host_failure)?
        .map_err(host_failure)?;
    if !status.success() {
        return Err(host_failure(
            "active Generation configuration receipt was not recorded",
        ));
    }
    Ok(())
}

async fn prepare_generation(
    distribution: &VerifiedDistribution,
    app_root: &Path,
    policy: Option<&Path>,
) -> Result<PreparedHostGeneration, ControlPlaneError> {
    if let Some(policy) = policy {
        let mut command =
            tokio::process::Command::new(distribution.root().join("runtime/lenso-resolver"));
        command
            .args(["app", "config-sync", "--root"])
            .arg(app_root)
            .arg("--host-build")
            .arg(distribution.root().join(".lenso/host-build.json"))
            .arg("--policy")
            .arg(policy)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(host_failure)?;
        let status = tokio::time::timeout(Duration::from_secs(30), child.wait())
            .await
            .map_err(host_failure)?
            .map_err(host_failure)?;
        if !status.success() {
            return Err(host_failure(
                "external configuration was not accepted; active Generation is unchanged",
            ));
        }
    } else {
        match std::fs::symlink_metadata(app_root.join(".lenso/configuration-source-state.json")) {
            Ok(_) => {
                return Err(host_failure(
                    "external configuration is active; --configuration-policy is required",
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(host_failure(error)),
        }
    }
    let distribution = distribution.clone();
    let app_root = app_root.to_path_buf();
    tokio::task::spawn_blocking(move || distribution.resolve(app_root))
        .await
        .map_err(host_failure)?
        .map_err(host_failure)
}

async fn activate_initial(
    mut host: host::Host<lenso_kernel::NativeApp>,
    candidate: &ResolvedGeneration,
    startup_timeout: Duration,
    stop_timeout: Duration,
) -> Result<host::Host<lenso_kernel::NativeApp>, ControlPlaneError> {
    let transition = CanonicalDocument::from_value(
        "lenso-generation-transition.json",
        AppGenerationTransitionSpec {
            schema_version: 1,
            app_id: candidate.spec.value().app_id.clone(),
            from_generation_spec_digest: None,
            to_generation_spec_digest: candidate.spec.digest().to_owned(),
            replacement_mode: ReplacementMode::Initial,
            state_compatibility_receipt_digests: Vec::new(),
            rollout_policy: RolloutPolicy {
                ready_timeout_nanos: startup_timeout.as_nanos().to_string(),
                drain_timeout_nanos: stop_timeout.as_nanos().to_string(),
                rollback_window_nanos: "0".to_owned(),
                automatic_rollback_on_generation_failure: false,
            },
        },
    )?;
    if let Err(error) = host
        .transition(transition, candidate.clone(), BTreeMap::new())
        .await
    {
        return match host.drain_and_suspend(stop_timeout).await {
            Ok(_) => Err(error),
            Err(cleanup) => Err(host_failure(format!(
                "initial Generation activation failed: {error}; cleanup failed: {cleanup}"
            ))),
        };
    }
    Ok(host)
}

#[derive(Debug)]
struct HostCatalogFactory {
    bun: PathBuf,
    working_directory: PathBuf,
    codecs: Vec<PortableJsonCodec>,
    typed: Vec<Rc<dyn JsonCapabilityCodec>>,
}

impl HostCatalogFactory {
    fn new(
        bun: PathBuf,
        working_directory: PathBuf,
        plan: &lenso_app_plan::ResolvedAppPlan,
        typed: Vec<Rc<dyn JsonCapabilityCodec>>,
    ) -> Result<Self, ControlPlaneError> {
        let typed_by_id = typed
            .iter()
            .map(|codec| (codec.capability_id(), codec))
            .collect::<BTreeMap<_, _>>();
        if typed_by_id.len() != typed.len() {
            return Err(host_failure("duplicate typed Capability codecs"));
        }
        let mut endpoints = BTreeMap::<String, (String, Vec<String>)>::new();
        for endpoint in plan
            .plugin_instances()
            .iter()
            .flat_map(|instance| instance.provided_capabilities().iter())
        {
            if !endpoint.event_operations().is_empty() {
                return Err(host_failure(
                    "TypeScript Host runtime does not admit Event Capabilities",
                ));
            }
            if let Some(codec) = typed_by_id.get(endpoint.capability_id()) {
                codecs::validate_typed_codec(
                    codec.as_ref(),
                    endpoint.descriptor_version(),
                    &endpoint.request_operations(),
                    &endpoint.stream_operations(),
                )
                .map_err(host_failure)?;
                continue;
            }
            if !endpoint.stream_operations().is_empty() {
                return Err(host_failure(
                    "Stream Capability requires an explicitly linked generated typed codec",
                ));
            }
            if plan.plugin_instances().iter().any(|instance| {
                instance.authoring_version() == 2
                    && (instance
                        .provided_capabilities()
                        .iter()
                        .any(|provided| provided.capability_id() == endpoint.capability_id())
                        || instance
                            .required_capabilities()
                            .iter()
                            .any(|required| required.capability_id() == endpoint.capability_id()))
            }) {
                return Err(host_failure(
                    "Bun Authoring V2 Capability requires an explicitly linked generated typed codec",
                ));
            }
            let identity = (
                endpoint.descriptor_version().to_owned(),
                endpoint
                    .operations()
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
            );
            if endpoints
                .insert(endpoint.capability_id().to_owned(), identity.clone())
                .is_some_and(|previous| previous != identity)
            {
                return Err(host_failure(format!(
                    "conflicting descriptors for Capability `{}`",
                    endpoint.capability_id()
                )));
            }
        }
        let codecs = endpoints
            .into_iter()
            .map(|(id, (version, operations))| PortableJsonCodec {
                capability_id: intern_string(&id),
                descriptor_version: intern_string(&version),
                operations: intern_operations(&operations),
            })
            .collect();
        Ok(Self {
            bun,
            working_directory,
            codecs,
            typed,
        })
    }
}

impl CatalogFactory for HostCatalogFactory {
    fn catalog(
        &self,
        generation: &ResolvedGeneration,
    ) -> Result<ExecutionAdapterCatalog, ControlPlaneError> {
        // Every candidate Generation is checked against the Host's immutable
        // codec set; a transition cannot borrow the initial endpoint table.
        let current = Self::new(
            self.bun.clone(),
            self.working_directory.clone(),
            &generation.plan,
            self.typed.clone(),
        )?;
        for instance in generation.plan.plugin_instances() {
            if instance.execution_class().as_str() == "lenso.process@1"
                && instance
                    .provided_capabilities()
                    .iter()
                    .any(|endpoint| !endpoint.stream_operations().is_empty())
            {
                return Err(host_failure(
                    "Process target does not admit Stream Capabilities",
                ));
            }
        }
        let selected = generation
            .plan
            .plugin_instances()
            .iter()
            .map(|instance| instance.execution_class().as_str())
            .collect::<BTreeSet<_>>();
        let mut catalog = ExecutionAdapterCatalog::new();
        if selected.contains("lenso.bun-process@1") {
            let config = BunAdapterConfig::new(&self.bun, BunWire::JsonRpcHttp)
                .with_working_directory(&self.working_directory);
            let adapter = current.codecs.iter().cloned().fold(
                BunAdapter::production(&self.bun)
                    .with_config(config)
                    .with_artifacts(generation.artifacts.clone()),
                BunAdapter::with_codec,
            );
            let adapter = self.typed.iter().fold(adapter, |adapter, codec| {
                adapter.with_shared_authoring_codec(codec.clone())
            });
            catalog = catalog.with_adapter(adapter).map_err(host_failure)?;
        }
        if selected.contains("lenso.process@1") {
            let adapter = current.codecs.iter().cloned().fold(
                ProcessAdapter::new(generation.artifacts.clone()),
                ProcessAdapter::with_codec,
            );
            let adapter = self.typed.iter().fold(adapter, |adapter, codec| {
                adapter.with_shared_codec(codec.clone())
            });
            catalog = catalog.with_adapter(adapter).map_err(host_failure)?;
        }
        if selected
            .iter()
            .any(|class| !matches!(*class, "lenso.bun-process@1" | "lenso.process@1"))
        {
            return Err(host_failure(
                "distribution selects an unsupported execution class",
            ));
        }
        Ok(catalog)
    }
}

fn unix_nanos() -> Result<u128, ControlPlaneError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .map_err(host_failure)
}

fn host_failure(error: impl std::fmt::Display) -> ControlPlaneError {
    ControlPlaneError::HostFailure {
        detail: error.to_string(),
    }
}

#[derive(Debug)]
struct Arguments {
    distribution_lock: PathBuf,
    app_root: PathBuf,
    configuration_policy: Option<PathBuf>,
    startup_timeout: Duration,
    stop_timeout: Duration,
}

fn parse_arguments() -> Result<Arguments, Box<dyn std::error::Error>> {
    let mut arguments = env::args_os().skip(1);
    let mut distribution_lock = None;
    let mut app_root = None;
    let mut configuration_policy = None;
    let mut startup_timeout = None;
    let mut stop_timeout = None;
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--distribution") if distribution_lock.is_none() => {
                distribution_lock = arguments.next().map(PathBuf::from);
            }
            Some("--root") if app_root.is_none() => {
                app_root = arguments.next().map(PathBuf::from);
            }
            Some("--configuration-policy") if configuration_policy.is_none() => {
                configuration_policy = Some(PathBuf::from(
                    arguments
                        .next()
                        .ok_or("--configuration-policy needs an absolute path")?,
                ));
            }
            Some("--startup-ms") if startup_timeout.is_none() => {
                startup_timeout = Some(parse_budget(arguments.next(), "--startup-ms")?);
            }
            Some("--stop-ms") if stop_timeout.is_none() => {
                stop_timeout = Some(parse_budget(arguments.next(), "--stop-ms")?);
            }
            _ => return Err("usage: lenso-host-runtime --distribution LOCK --root ROOT".into()),
        }
    }
    if configuration_policy
        .as_ref()
        .is_some_and(|path: &PathBuf| !path.is_absolute())
    {
        return Err("--configuration-policy needs an absolute path".into());
    }
    Ok(Arguments {
        distribution_lock: distribution_lock.ok_or("missing --distribution")?,
        app_root: app_root.ok_or("missing --root")?,
        configuration_policy,
        startup_timeout: startup_timeout.unwrap_or(STARTUP_TIMEOUT),
        stop_timeout: stop_timeout.unwrap_or(STOP_TIMEOUT),
    })
}

fn parse_budget(
    value: Option<std::ffi::OsString>,
    name: &str,
) -> Result<Duration, Box<dyn std::error::Error>> {
    let milliseconds = value
        .and_then(|value| value.to_str().and_then(|value| value.parse::<u64>().ok()))
        .ok_or_else(|| format!("{name} requires integer milliseconds"))?;
    if !(1..=60_000).contains(&milliseconds) {
        return Err(format!("{name} must be between 1 and 60000 milliseconds").into());
    }
    Ok(Duration::from_millis(milliseconds))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lenso_app_plan::{
        CapabilityEndpointPlan, CapabilityOperationKind, PluginInstancePlan, ResolvedAppPlan,
    };
    use lenso_kernel::RuntimeFailure;
    use serde_json::Value;
    use std::any::Any;

    #[derive(Debug)]
    struct TypedCodec {
        version: &'static str,
        digest: &'static str,
        requests: &'static [&'static str],
        streams: &'static [&'static str],
        events: &'static [&'static str],
    }

    impl JsonCapabilityCodec for TypedCodec {
        fn capability_id(&self) -> &'static str {
            "example.service@1"
        }
        fn descriptor_version(&self) -> &'static str {
            self.version
        }
        fn descriptor_digest(&self) -> &'static str {
            self.digest
        }
        fn request_operations(&self) -> &'static [&'static str] {
            self.requests
        }
        fn stream_operations(&self) -> &'static [&'static str] {
            self.streams
        }
        fn event_operations(&self) -> &'static [&'static str] {
            self.events
        }
        fn encode_request(&self, _: &str, _: &dyn Any) -> Result<Value, RuntimeFailure> {
            panic!("preflight must not invoke codecs")
        }
        fn decode_response(&self, _: &str, _: Value) -> Result<Box<dyn Any>, RuntimeFailure> {
            panic!("preflight must not invoke codecs")
        }
        fn decode_domain_error(&self, _: &str, _: Value) -> Result<Box<dyn Any>, RuntimeFailure> {
            panic!("preflight must not invoke codecs")
        }
    }

    fn typed_codec() -> TypedCodec {
        TypedCodec {
            version: "1.0.0",
            digest: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            requests: &["get"],
            streams: &["watch"],
            events: &[],
        }
    }

    fn stream_plan() -> ResolvedAppPlan {
        ResolvedAppPlan::new(
            vec![
                PluginInstancePlan::new("example.provider/default", "example.provider")
                    .with_authoring(2, "lenso.bun-authoring@2")
                    .with_capability(
                        CapabilityEndpointPlan::new("example.service@1", "1.0.0", ["get", "watch"])
                            .with_operation_kind("watch", CapabilityOperationKind::Stream),
                    ),
            ],
            vec![],
        )
    }

    #[test]
    fn stream_preflight_requires_exact_explicit_codecs() {
        let plan = stream_plan();
        let factory = |codecs| {
            HostCatalogFactory::new(PathBuf::from("bun"), PathBuf::from("app"), &plan, codecs)
        };
        assert!(
            factory(vec![])
                .unwrap_err()
                .to_string()
                .contains("Stream Capability requires")
        );
        let codec: Rc<dyn JsonCapabilityCodec> = Rc::new(typed_codec());
        assert!(factory(vec![codec.clone()]).unwrap().codecs.is_empty());
        assert!(
            factory(vec![codec.clone(), codec])
                .unwrap_err()
                .to_string()
                .contains("duplicate typed")
        );
        for wrong in [
            TypedCodec {
                version: "2.0.0",
                ..typed_codec()
            },
            TypedCodec {
                digest: "",
                ..typed_codec()
            },
            TypedCodec {
                requests: &["get", "extra"],
                ..typed_codec()
            },
            TypedCodec {
                streams: &[],
                ..typed_codec()
            },
            TypedCodec {
                streams: &["watch", "watch"],
                ..typed_codec()
            },
            TypedCodec {
                events: &["changed"],
                ..typed_codec()
            },
        ] {
            assert!(factory(vec![Rc::new(wrong)]).is_err());
        }
    }

    #[test]
    fn v2_request_cannot_use_the_legacy_json_fallback() {
        let plan = ResolvedAppPlan::new(
            vec![
                PluginInstancePlan::new("example.provider/default", "example.provider")
                    .with_authoring(2, "lenso.bun-authoring@2")
                    .with_capability(CapabilityEndpointPlan::new(
                        "example.service@1",
                        "1.0.0",
                        ["get"],
                    )),
            ],
            vec![],
        );
        let error =
            HostCatalogFactory::new(PathBuf::from("bun"), PathBuf::from("app"), &plan, vec![])
                .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Bun Authoring V2 Capability requires")
        );
    }

    #[test]
    fn interners_reuse_runtime_contract_storage() {
        assert!(std::ptr::eq(
            intern_string("company.capability"),
            intern_string("company.capability")
        ));
        let operations = vec!["get".to_owned(), "put".to_owned()];
        assert!(std::ptr::eq(
            intern_operations(&operations),
            intern_operations(&operations)
        ));
    }
}
