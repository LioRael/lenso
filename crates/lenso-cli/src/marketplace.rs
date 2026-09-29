use std::path::PathBuf;

use clap::{Args, Subcommand};
use lenso_app_authoring::keyless_catalog::{KeylessCatalogVerification, verify_official_catalog};

#[derive(Debug, Subcommand)]
pub enum MarketplaceCommand {
    /// Verify official catalog provenance offline without changing an App.
    Verify(VerifyArgs),
}

#[derive(Debug, Args)]
pub struct VerifyArgs {
    #[arg(long)]
    catalog: PathBuf,
    #[arg(long)]
    bundle: PathBuf,
    /// Independently installed public Sigstore trusted root set.
    #[arg(long)]
    trusted_root: PathBuf,
    /// Independent SHA-256 pin for that root set, not a value from the catalog.
    #[arg(long)]
    trusted_root_sha256: String,
    /// Independently reviewed publishing source commit.
    #[arg(long)]
    source_sha: String,
    /// Absolute path to a trusted GitHub CLI installation.
    #[arg(long)]
    gh: PathBuf,
}

pub fn run(command: MarketplaceCommand) -> anyhow::Result<()> {
    let MarketplaceCommand::Verify(args) = command;
    let verified = verify_official_catalog(&KeylessCatalogVerification {
        gh: &args.gh,
        catalog: &args.catalog,
        bundle: &args.bundle,
        trusted_root: &args.trusted_root,
        trusted_root_sha256: &args.trusted_root_sha256,
        source_sha: &args.source_sha,
    })?;
    println!(
        "{}",
        serde_json::json!({
            "schema": "lenso.marketplace.verified-provenance.v1",
            "catalog_sha256": verified.sha256(),
            "catalog": verified.catalog(),
            "adoption_authorized": false,
            "current_status_verified": false,
        })
    );
    Ok(())
}
