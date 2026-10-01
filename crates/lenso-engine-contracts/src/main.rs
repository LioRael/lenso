//! A small official-generator entry point; no package scripts are evaluated.
use lenso_engine_contracts::{DiscoveryOptions, Mode, discover, run};
use std::{fs, path::PathBuf};
fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "--help".into());
    if command == "--help" || command == "-h" {
        println!(
            "lenso-engine-contracts <generate|check> [root] [--config <json>]\nDefaults: contracts/**/capability.json -> TypeScript; configuration selects projections, modules, roots and exclusions."
        );
        return Ok(());
    }
    let mode = match command.as_str() {
        "generate" => Mode::Generate,
        "check" => Mode::Check,
        _ => anyhow::bail!("expected generate or check"),
    };
    let mut root = PathBuf::from(".");
    let mut config = None;
    while let Some(argument) = args.next() {
        if argument == "--config" {
            config = Some(PathBuf::from(
                args.next()
                    .ok_or_else(|| anyhow::anyhow!("--config needs a path"))?,
            ));
        } else if argument.starts_with('-') {
            anyhow::bail!("unknown option {argument}");
        } else {
            root = argument.into();
        }
    }
    let config = config.unwrap_or_else(|| root.join("lenso.contracts.json"));
    let options: DiscoveryOptions = if config.exists() {
        serde_json::from_slice(&fs::read(&config)?)?
    } else {
        Default::default()
    };
    let (mut snapshot, inputs) = discover(&root, &options)?;
    let lock = root.join("Cargo.lock");
    if lock.exists() {
        snapshot.insert("tool-inputs/Cargo.lock".into(), fs::read(lock)?)?;
    }
    let count = inputs.len();
    run(
        snapshot.clone(),
        inputs.clone(),
        &root,
        Some(&root.join(".lenso/contracts/cache")),
        mode,
    )?;
    if matches!(mode, Mode::Generate)
        && let Some(baselines) = &options.baseline_root
    {
        lenso_engine_contracts::accept(&root, baselines, &snapshot, &inputs)?;
    }
    println!("{command}: {count} selected contracts");
    Ok(())
}
