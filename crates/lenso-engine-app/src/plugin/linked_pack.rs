use std::{fs, io::Read as _, path::Path};

use anyhow::{Context, bail};
use lenso_app_plan::ExecutionClassId;
use lenso_plugin_bundle::{
    SourcePluginImplementationGroupV6, SourcePluginReleaseBuildV6, SourcePluginVariantInputV6,
    SourcePluginVariantV6, build_source_plugin_release_bundle_v6, verify_bundle_directory,
};

use super::{CargoPackage, print_verified, read_package, run_cargo, web_dev};
use crate::{
    app::convention_authoring::linked_catalog::unpack_archive,
    archive::{archive_bundle, with_bundle_directory},
};

const MAX_CRATE_BYTES: u64 = 32 * 1024 * 1024;

pub(super) fn pack(
    root: &Path,
    package: &CargoPackage,
    archive: &Path,
    output: &Path,
    json: bool,
) -> anyhow::Result<()> {
    if !web_dev::is_web_plugin(root)? {
        bail!("--linked-crate requires a linked native Web Plugin project");
    }
    let bytes = read_archive(archive)?;
    let staging = tempfile::tempdir().context("stage exact linked Cargo authoring input")?;
    let source = staging.path().join("source");
    fs::create_dir(&source)?;
    unpack_archive(
        &bytes,
        &source,
        &package.name,
        &package.version,
        &package.metadata.lenso.plugin_id,
    )
    .context("verify exact linked Cargo archive before compiling it")?;
    let packaged = read_package(&source.join("Cargo.toml"))?;
    if packaged.name != package.name
        || packaged.version != package.version
        || packaged.metadata.lenso.plugin_id != package.metadata.lenso.plugin_id
        || packaged.metadata.lenso.root_slot != package.metadata.lenso.root_slot
        || packaged.metadata.lenso_cli.is_some()
    {
        bail!("linked Cargo archive differs from this source Plugin identity or root Slot");
    }
    if !source.join("Cargo.lock").exists() {
        run_cargo(
            &source,
            &["generate-lockfile"],
            "lock exact linked Cargo archive",
        )?;
    }
    let host = web_dev::DevHost::prepare(&source, &packaged)?;
    host.build()?;
    let catalog = host.describe()?;
    let mut matching = catalog
        .plugins()
        .iter()
        .filter(|release| release.descriptor().plugin_id() == package.metadata.lenso.plugin_id);
    let descriptor = matching
        .next()
        .context("exact linked Cargo archive generated no Plugin Descriptor")?
        .descriptor();
    if matching.next().is_some() {
        bail!("exact linked Cargo archive generated duplicate Plugin Descriptors");
    }
    if descriptor.release_version() != package.version
        || descriptor.root_slot() != package.metadata.lenso.root_slot
        || descriptor.runtime_package_id() != package.metadata.lenso.plugin_id
        || descriptor.runtime_package_revision() != package.version
        || descriptor.execution_class().as_str() != "lenso.native-rust@1"
        || descriptor.entrypoint() != "default"
    {
        bail!("exact linked Cargo Descriptor differs from package identity or native factory");
    }
    if descriptor.authoring_version() != 2
        || descriptor.runtime_profile() != "lenso.native-authoring@2"
    {
        bail!("V6 linked Cargo pack requires source-generated native authoring version 2");
    }
    if !descriptor.required_target_capabilities().is_empty() {
        bail!("V6 linked Cargo pack cannot claim unproven target capabilities");
    }
    let staged_archive = staging
        .path()
        .join(format!("{}-{}.crate", package.name, package.version));
    fs::write(&staged_archive, bytes)?;
    let bundle = staging.path().join("bundle");
    let verified = build_source_plugin_release_bundle_v6(&SourcePluginReleaseBuildV6 {
        contract: descriptor.contract(),
        implementations: vec![SourcePluginImplementationGroupV6 {
            id: "native".to_owned(),
            variants: vec![SourcePluginVariantV6 {
                id: "cargo".to_owned(),
                host_targets: vec![lenso_app_authoring::native_host_target().to_owned()],
                input: SourcePluginVariantInputV6::CargoBuildInput {
                    path: staged_archive,
                    bundle_path: format!(
                        "implementations/native/{}-{}.crate",
                        package.name, package.version
                    ),
                    package: package.name.clone(),
                    version: package.version.clone(),
                },
                entrypoint: descriptor.entrypoint().to_owned(),
                execution_class: ExecutionClassId::native_rust(),
                runtime_profile: "lenso.native-rust@1".to_owned(),
                required_target_capabilities: Vec::new(),
                execution_requirements: Vec::new(),
            }],
        }],
        output: bundle.clone(),
    })?;
    archive_bundle(&bundle, output)?;
    let reopened = with_bundle_directory(output, |directory| {
        verify_bundle_directory(directory)
            .with_context(|| format!("reopen packed Plugin `{}`", output.display()))
    })?;
    if verified != reopened {
        bail!("packed Plugin verification result changed after publication");
    }
    print_verified(&reopened, Some(output), json)
}

fn read_archive(path: &Path) -> anyhow::Result<Vec<u8>> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(nix::libc::O_NOFOLLOW);
    }
    let mut file = options
        .open(path)
        .with_context(|| format!("open linked Cargo archive {}", path.display()))?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_CRATE_BYTES {
        bail!("linked Cargo input must be a regular `.crate` file of at most 32 MiB");
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_CRATE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.is_empty() || u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_CRATE_BYTES {
        bail!("linked Cargo archive exceeds size limit");
    }
    Ok(bytes)
}
