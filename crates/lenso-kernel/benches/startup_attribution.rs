//! Reproducible characterization of the portable Kernel startup path.

use std::{
    any::Any,
    collections::BTreeMap,
    hint::black_box,
    process::Command,
    rc::Rc,
    time::{Duration, Instant},
};

use futures::future::{LocalBoxFuture, ready};
use lenso_app_plan::{
    AppComposition, CapabilityBinding, CapabilityEndpointPlan, CapabilityRequirementPlan,
    PluginInstancePlan, ResolvedAppPlan,
};
use lenso_kernel::{
    DeterministicDriver, InvocationContext, Kernel, NativeExecutionAdapter, NativeRequestEndpoint,
    NoopPluginLifecycle, PreparedBinding, PreparedNativeApp, PreparedNativePlugin, RuntimeFailure,
};
use serde_json::{Value, json};

const CAPABILITY_ID: &str = "lenso.benchmark.chain@1";
const DESCRIPTOR_VERSION: &str = "1.0.0";
const OPERATION: &str = "chain.call";
const DEFAULT_SIZES: &[usize] = &[10, 100, 500];
const DEFAULT_ITERATIONS: usize = 20;
const DEFAULT_WARMUP_ITERATIONS: usize = 3;

#[derive(Clone, Debug)]
struct Configuration {
    sizes: Vec<usize>,
    iterations: usize,
    warmup_iterations: usize,
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            sizes: DEFAULT_SIZES.to_vec(),
            iterations: DEFAULT_ITERATIONS,
            warmup_iterations: DEFAULT_WARMUP_ITERATIONS,
        }
    }
}

#[derive(Debug)]
struct BenchmarkEndpoint;

impl NativeRequestEndpoint for BenchmarkEndpoint {
    fn capability_id(&self) -> &'static str {
        CAPABILITY_ID
    }

    fn descriptor_version(&self) -> &'static str {
        DESCRIPTOR_VERSION
    }

    fn operations(&self) -> &'static [&'static str] {
        &[OPERATION]
    }

    fn invoke(
        &self,
        _operation: &str,
        _request: Box<dyn Any>,
        _context: InvocationContext,
    ) -> LocalBoxFuture<'static, Result<Result<Box<dyn Any>, Box<dyn Any>>, RuntimeFailure>> {
        Box::pin(ready(Ok(Ok(Box::new(()) as Box<dyn Any>))))
    }
}

#[derive(Clone, Copy, Debug)]
struct BenchmarkAdapter;

impl NativeExecutionAdapter for BenchmarkAdapter {
    fn prepare(&self, plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure> {
        plan.validate()
            .map_err(|error| RuntimeFailure::InvalidResolvedPlan {
                detail: error.to_string(),
            })?;
        let endpoints = plan
            .plugin_instances()
            .iter()
            .map(|instance| {
                (
                    instance.instance_key().to_owned(),
                    Rc::new(BenchmarkEndpoint) as Rc<dyn NativeRequestEndpoint>,
                )
            })
            .collect::<BTreeMap<_, _>>();
        let generations = endpoints
            .iter()
            .map(|(instance_key, endpoint)| {
                (
                    instance_key.clone(),
                    PreparedNativePlugin::new(vec![endpoint.clone()], NoopPluginLifecycle),
                )
            })
            .collect();
        let bindings = plan
            .capability_bindings()
            .iter()
            .map(|binding| {
                PreparedBinding::new(
                    binding.consumer_instance(),
                    binding.provider_instance(),
                    endpoints
                        .get(binding.provider_instance())
                        .expect("the deterministic fixture binds a selected provider")
                        .clone(),
                )
            })
            .collect();
        Ok(PreparedNativeApp::new(bindings, generations))
    }
}

fn main() {
    let configuration = parse_arguments().unwrap_or_else(|error| {
        eprintln!("startup_attribution: {error}");
        std::process::exit(2);
    });
    let mut graphs = Vec::with_capacity(configuration.sizes.len());

    for &graph_size in &configuration.sizes {
        let plan = chain_plan(graph_size);
        // Clone an unchecked snapshot outside each timed interval. Otherwise a
        // Plan-local memo would turn a cold-start comparison into a warm lookup.
        let cold_plan = || {
            ResolvedAppPlan::new(
                plan.plugin_instances().to_vec(),
                plan.capability_bindings().to_vec(),
            )
        };
        let checked_plan = cold_plan();
        checked_plan.validate().expect("valid fixture");
        let stages = [
            measure_pre_timed_stage("resolved_plan_validation", &configuration, || {
                let measured_plan = cold_plan();
                let started_at = Instant::now();
                let result = black_box(&measured_plan).validate();
                let elapsed = started_at.elapsed();
                black_box(result).expect("the benchmark Plan must remain valid");
                elapsed
            }),
            measure_pre_timed_stage("activation_order_derivation", &configuration, || {
                let measured_plan = cold_plan();
                let started_at = Instant::now();
                let order = black_box(&measured_plan)
                    .activation_order()
                    .expect("the benchmark Plan must remain acyclic");
                let elapsed = started_at.elapsed();
                assert_eq!(black_box(order).len(), graph_size);
                elapsed
            }),
            measure_pre_timed_stage("execution_adapter_preparation", &configuration, || {
                let measured_plan = cold_plan();
                let started_at = Instant::now();
                let prepared = BenchmarkAdapter
                    .prepare(black_box(&measured_plan))
                    .expect("the benchmark Adapter must prepare the valid Plan");
                let elapsed = started_at.elapsed();
                black_box(prepared);
                elapsed
            }),
            measure_stage("checked_plan_validation", &configuration, || {
                black_box(&checked_plan).validate().expect("valid fixture");
            }),
            measure_stage("checked_activation_order", &configuration, || {
                black_box(checked_plan.activation_order().expect("valid fixture"));
            }),
            measure_stage("checked_adapter_preparation", &configuration, || {
                black_box(
                    BenchmarkAdapter
                        .prepare(&checked_plan)
                        .expect("valid fixture"),
                );
            }),
            measure_pre_timed_stage("complete_kernel_start", &configuration, || {
                let driver = DeterministicDriver::new();
                let measured_plan = black_box(cold_plan());
                let started_at = Instant::now();
                let started = driver.run(Kernel::start_native(
                    measured_plan,
                    driver.clone(),
                    BenchmarkAdapter,
                ));
                let elapsed = started_at.elapsed();
                let app = black_box(started).expect("the benchmark App must start");
                assert!(black_box(app.is_ready()));
                black_box(driver.run(app.shutdown(Duration::from_secs(1))));
                elapsed
            }),
        ];

        graphs.push(json!({
            "graph_size": graph_size,
            "binding_count": graph_size.saturating_sub(1),
            "topology": "linear_required_request_chain",
            "stages": stages,
        }));
    }

    let output = json!({
        "schema": "lenso.startup-attribution.v2",
        "measurement_kind": "local_characterization_not_production_latency",
        "metadata": metadata(&configuration),
        "graphs": graphs,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&output).expect("benchmark output must serialize")
    );
}

fn measure_stage(name: &str, configuration: &Configuration, mut operation: impl FnMut()) -> Value {
    for _ in 0..configuration.warmup_iterations {
        operation();
    }

    let mut samples_ns = Vec::with_capacity(configuration.iterations);
    for _ in 0..configuration.iterations {
        let started_at = Instant::now();
        operation();
        samples_ns.push(started_at.elapsed().as_nanos());
    }
    stage_output(name, &samples_ns)
}

fn measure_pre_timed_stage(
    name: &str,
    configuration: &Configuration,
    mut operation: impl FnMut() -> Duration,
) -> Value {
    for _ in 0..configuration.warmup_iterations {
        black_box(operation());
    }
    let samples_ns: Vec<u128> = (0..configuration.iterations)
        .map(|_| black_box(operation()).as_nanos())
        .collect();
    stage_output(name, &samples_ns)
}

fn stage_output(name: &str, samples_ns: &[u128]) -> Value {
    let mut sorted_ns = samples_ns.to_owned();
    sorted_ns.sort_unstable();
    let sum = sorted_ns.iter().sum::<u128>();

    json!({
        "stage": name,
        "unit": "ns",
        "samples": samples_ns,
        "minimum": sorted_ns[0],
        "median_p50": percentile(&sorted_ns, 50),
        "p95": percentile(&sorted_ns, 95),
        "maximum": sorted_ns[sorted_ns.len() - 1],
        "mean": sum / sorted_ns.len() as u128,
    })
}

fn percentile(sorted: &[u128], percentile: usize) -> u128 {
    let index = (sorted.len() - 1) * percentile / 100;
    sorted[index]
}

fn chain_plan(graph_size: usize) -> ResolvedAppPlan {
    let mut instances = Vec::with_capacity(graph_size);
    let mut bindings = Vec::with_capacity(graph_size.saturating_sub(1));

    for index in 0..graph_size {
        let current_key = instance_key(index);
        let mut instance = PluginInstancePlan::new(&current_key, "lenso.benchmark.plugin")
            .with_entrypoint("benchmark")
            .with_configuration(format!(r#"{{"index":{index}}}"#))
            .with_package_revision("benchmark-fixture-v1")
            .with_capability(CapabilityEndpointPlan::new(
                CAPABILITY_ID,
                DESCRIPTOR_VERSION,
                [OPERATION],
            ));

        if index > 0 {
            instance = instance.with_requirement(CapabilityRequirementPlan::one(
                CAPABILITY_ID,
                DESCRIPTOR_VERSION,
            ));
            bindings.push(CapabilityBinding::new(
                &current_key,
                CAPABILITY_ID,
                DESCRIPTOR_VERSION,
                instance_key(index - 1),
            ));
        }
        instances.push(instance);
    }

    let plan = AppComposition::new(instances, bindings)
        .resolve()
        .expect("the deterministic chain fixture must resolve");
    assert_eq!(plan.plugin_instances().len(), graph_size);
    assert_eq!(plan.capability_bindings().len(), graph_size - 1);
    plan
}

fn instance_key(index: usize) -> String {
    format!("instance-{index:08}")
}

fn metadata(configuration: &Configuration) -> Value {
    json!({
        "package": env!("CARGO_PKG_NAME"),
        "package_version": env!("CARGO_PKG_VERSION"),
        "build_identity": option_env!("LENSO_STARTUP_BUILD_ID").unwrap_or("unspecified"),
        "profile": "bench",
        "debug_assertions": cfg!(debug_assertions),
        "target_arch": std::env::consts::ARCH,
        "target_os": std::env::consts::OS,
        "target_family": std::env::consts::FAMILY,
        "rustc": command_version("rustc", &["-Vv"]),
        "cargo": command_version("cargo", &["-V"]),
        "iterations": configuration.iterations,
        "warmup_iterations": configuration.warmup_iterations,
        "graph_sizes": configuration.sizes,
    })
}

fn command_version(command: &str, arguments: &[&str]) -> String {
    Command::new(command)
        .args(arguments)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map_or_else(
            || "unavailable".to_owned(),
            |output| String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        )
}

fn parse_arguments() -> Result<Configuration, String> {
    let mut configuration = Configuration::default();
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--bench" => {}
            "--iterations" => {
                configuration.iterations = parse_positive("iterations", arguments.next())?;
            }
            "--warmup" => {
                configuration.warmup_iterations = parse_usize("warmup", arguments.next())?;
            }
            "--sizes" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--sizes requires a comma-separated value".to_owned())?;
                configuration.sizes = value
                    .split(',')
                    .map(|size| parse_positive("graph size", Some(size.to_owned())))
                    .collect::<Result<Vec<_>, _>>()?;
                if configuration.sizes.is_empty() {
                    return Err("--sizes requires at least one graph size".to_owned());
                }
            }
            "--help" | "-h" => {
                println!(
                    "Usage: startup_attribution [--iterations N] [--warmup N] [--sizes N,N,N]"
                );
                std::process::exit(0);
            }
            _ => return Err(format!("unknown argument `{argument}`; use --help")),
        }
    }
    Ok(configuration)
}

fn parse_positive(name: &str, value: Option<String>) -> Result<usize, String> {
    let parsed = parse_usize(name, value)?;
    if parsed == 0 {
        Err(format!("{name} must be greater than zero"))
    } else {
        Ok(parsed)
    }
}

fn parse_usize(name: &str, value: Option<String>) -> Result<usize, String> {
    value
        .ok_or_else(|| format!("--{name} requires a value"))?
        .parse()
        .map_err(|_| format!("{name} must be an unsigned integer"))
}
