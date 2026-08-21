use std::{env, path::PathBuf, process::ExitCode, time::Duration};

use lenso_authoring::{
    AddModule, CheckOptions, Module, PackageInput, PackageSource, ProjectPath, ResolutionOptions,
    run_project,
};
use lenso_kernel::ExecutionAdapterCatalog;
use lenso_runner::TokioDriver;

fn usage() -> &'static str {
    "usage:
  lenso add --project <lenso.json> --key <key> --package <package> --source <cargo|bun|npm|oci> --version <version> [--entrypoint <path>] [--manifest <path>]
  lenso check --project <lenso.json> [--execution-class <id>]...
  lenso resolve --project <lenso.json> [--profile <name>] [--execution-class <id>]... [--output <path>]
  lenso run --project <lenso.json> [--profile <name>]"
}

fn value(arguments: &[String], name: &str) -> Result<String, String> {
    let index = arguments
        .iter()
        .position(|argument| argument == name)
        .ok_or_else(|| format!("missing {name}\n{}", usage()))?;
    arguments
        .get(index + 1)
        .cloned()
        .ok_or_else(|| format!("missing value for {name}"))
}

fn optional_value(arguments: &[String], name: &str) -> Option<String> {
    arguments
        .iter()
        .position(|argument| argument == name)
        .and_then(|index| arguments.get(index + 1))
        .cloned()
}

fn values(arguments: &[String], name: &str) -> Vec<String> {
    arguments
        .iter()
        .enumerate()
        .filter(|(_, argument)| argument.as_str() == name)
        .filter_map(|(index, _)| arguments.get(index + 1))
        .cloned()
        .collect()
}

fn check_options(arguments: &[String]) -> CheckOptions {
    let classes = values(arguments, "--execution-class");
    if classes.is_empty() {
        CheckOptions::default()
    } else {
        CheckOptions::new(classes)
    }
}

fn source(value: &str) -> Result<PackageSource, String> {
    match value {
        "cargo" => Ok(PackageSource::Cargo),
        "bun" => Ok(PackageSource::Bun),
        "npm" => Ok(PackageSource::Npm),
        "oci" => Ok(PackageSource::Oci),
        _ => Err(format!("unknown package source {value}")),
    }
}

fn project_path(arguments: &[String]) -> PathBuf {
    optional_value(arguments, "--project")
        .map_or_else(|| PathBuf::from("lenso.json"), PathBuf::from)
}

fn add(arguments: &[String]) -> Result<(), String> {
    let path = project_path(arguments);
    let package_name = value(arguments, "--package")?;
    let package_source = source(&value(arguments, "--source")?)?;
    let mut package = PackageInput::new(
        &package_name,
        package_source,
        value(arguments, "--version")?,
    );
    if let Some(manifest) = optional_value(arguments, "--manifest") {
        package = package.with_manifest(manifest);
    }
    let mut module = Module::new(value(arguments, "--key")?, &package_name);
    if let Some(entrypoint) = optional_value(arguments, "--entrypoint") {
        module = module.with_entrypoint(entrypoint);
    }
    let request = AddModule::new(module, package);
    let result = ProjectPath::new(&path)
        .add(&request)
        .map_err(|error| error.to_string())?;
    for changed in result.changed_files() {
        println!("updated {}", changed.display());
    }
    Ok(())
}

fn check(arguments: &[String]) -> Result<(), String> {
    let path = project_path(arguments);
    let project = ProjectPath::load(&path).map_err(|error| error.to_string())?;
    let root = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let options = check_options(arguments);
    let report = project
        .check(root, &options)
        .map_err(|error| error.to_string())?;
    println!(
        "checked {} Module Instances, {} bindings, {} contracts",
        report.modules, report.bindings, report.contracts
    );
    Ok(())
}

fn resolve(arguments: &[String]) -> Result<(), String> {
    let path = project_path(arguments);
    let project = ProjectPath::load(&path).map_err(|error| error.to_string())?;
    let root = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let mut options = ResolutionOptions::default().with_check_options(check_options(arguments));
    if let Some(profile) = optional_value(arguments, "--profile") {
        options = options.with_profile(profile);
    }
    let resolved = project
        .resolve(root, &options)
        .map_err(|error| error.to_string())?;
    let output = optional_value(arguments, "--output")
        .map_or_else(|| root.join(".lenso/resolved-plan.json"), PathBuf::from);
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(&output, resolved.canonical_bytes()).map_err(|error| error.to_string())?;
    println!("resolved {} ({})", output.display(), resolved.fingerprint());
    Ok(())
}

async fn run(arguments: &[String]) -> Result<(), String> {
    let path = project_path(arguments);
    let project = ProjectPath::load(&path).map_err(|error| error.to_string())?;
    let root = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let mut options = ResolutionOptions::default();
    if let Some(profile) = optional_value(arguments, "--profile") {
        options = options.with_profile(profile);
    }
    let driver = TokioDriver::new();
    driver.request_shutdown();
    let local = tokio::task::LocalSet::new();
    let outcome = local
        .run_until(run_project(
            &project,
            root,
            driver,
            ExecutionAdapterCatalog::new(),
            Duration::from_secs(1),
            options,
        ))
        .await
        .map_err(|error| error.to_string())?;
    println!("{outcome:?}");
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    let result = match arguments.first().map(String::as_str) {
        Some("add") => add(&arguments[1..]),
        Some("check") => check(&arguments[1..]),
        Some("resolve") => resolve(&arguments[1..]),
        Some("run") => run(&arguments[1..]).await,
        _ => Err(usage().to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
