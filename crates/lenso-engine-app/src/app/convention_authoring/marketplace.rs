//! Normal Marketplace adoption consumes typed current admission, never v1 envelopes.

use super::{AddArgs, linked_catalog, npm_catalog};
use anyhow::{Context as _, ensure};
use lenso_app_authoring::{
    keyless_current::{
        ReleaseRecord, admit_current, cargo_artifact_url, download_official_artifact,
        npm_artifact_url,
    },
    keyless_managed::{managed_state_directory, verify_managed_catalog},
};
use lenso_plugin_catalog::DistributionKind;
use std::{fs, path::Path};

pub(super) fn add(root: &Path, args: &AddArgs) -> anyhow::Result<()> {
    let (plugin_id, version) = args
        .source
        .split_once('@')
        .context("Marketplace adoption requires exact PLUGIN_ID@VERSION")?;
    lenso_app_authoring::identity::validate_plugin_id_v1(plugin_id)?;
    lenso_app_authoring::identity::validate_release_version(version)?;
    ensure!(
        args.linked_snapshot.is_none()
            && args.portable_snapshot.is_none()
            && args.package_snapshot.is_none()
            && args.release_details.is_none()
            && args.content_snapshot.is_none()
            && args.trust.is_none()
            && args.origin.is_none(),
        "Marketplace adoption cannot use legacy trust or snapshots"
    );
    let current = admit_current(
        &managed_state_directory()?.join("history"),
        verify_managed_catalog,
    )?;
    let admitted = current.select(plugin_id, version)?;
    let scratch = tempfile::tempdir()?;
    let downloaded = scratch.path().join("artifact");
    let mut prepared = args.clone();
    match admitted.record() {
        ReleaseRecord::LinkedCargo(release) => {
            ensure!(
                args.archive.is_none()
                    && args.tgz.is_none()
                    && args.content_id.is_none()
                    && args.content_archive.is_none()
                    && args.content_destination.is_none()
                    && !args.content_preview
                    && args.distribution.is_none(),
                "linked Cargo release does not accept another channel's inputs"
            );
            if args.crate_archive.is_none() && args.bundle.is_none() {
                let url = cargo_artifact_url(release)?;
                fs::write(
                    &downloaded,
                    download_official_artifact(&url, 32 * 1024 * 1024)?,
                )?;
                prepared.crate_archive = Some(downloaded);
            }
            linked_catalog::add_admitted(root, &prepared, &admitted, &current)
        }
        ReleaseRecord::Package(release) => {
            ensure!(
                args.crate_archive.is_none()
                    && args.bundle.is_none()
                    && args.archive.is_none()
                    && args.content_id.is_none()
                    && args.content_archive.is_none()
                    && args.content_destination.is_none()
                    && !args.content_preview,
                "npm-only release does not accept another channel's inputs"
            );
            let target = lenso_app_authoring::native_host_target();
            let candidates: Vec<_> = release
                .distributions
                .iter()
                .filter(|distribution| {
                    distribution.kind == DistributionKind::NpmPackage
                        && args
                            .distribution
                            .as_deref()
                            .is_none_or(|id| distribution.id == id)
                        && (distribution.targets.is_empty()
                            || distribution
                                .targets
                                .iter()
                                .any(|item| item == "*" || item == target))
                })
                .collect();
            let [distribution] = candidates.as_slice() else {
                anyhow::bail!(
                    "select exactly one npm distribution; use --distribution when needed"
                );
            };
            if args.tgz.is_none() {
                let url = npm_artifact_url(distribution)?;
                fs::write(
                    &downloaded,
                    download_official_artifact(&url, 32 * 1024 * 1024)?,
                )?;
                prepared.tgz = Some(downloaded);
            }
            npm_catalog::add_admitted(root, &prepared, &admitted, &current)
        }
        ReleaseRecord::Portable(release) => {
            ensure!(
                args.crate_archive.is_none()
                    && args.bundle.is_none()
                    && args.tgz.is_none()
                    && args.content_id.is_none()
                    && args.content_archive.is_none()
                    && args.content_destination.is_none()
                    && !args.content_preview
                    && args.distribution.is_none(),
                "Portable release does not accept another channel's inputs"
            );
            if args.archive.is_none() {
                let bytes =
                    download_official_artifact(&release.artifact.url, release.artifact.size)?;
                ensure!(
                    bytes.len() as u64 == release.artifact.size,
                    "Portable archive size differs from verified record"
                );
                fs::write(&downloaded, bytes)?;
                prepared.archive = Some(downloaded);
            }
            crate::plugins::signed_install::adopt_admitted_source(
                root,
                &admitted,
                prepared.archive.as_deref().context("Portable archive")?,
                args.replace,
                &current,
            )
        }
        ReleaseRecord::Content(release) => {
            ensure!(
                args.crate_archive.is_none()
                    && args.bundle.is_none()
                    && args.tgz.is_none()
                    && args.archive.is_none()
                    && args.distribution.is_none(),
                "source content does not accept runtime artifact inputs"
            );
            let content = release.select(
                args.content_id
                    .as_deref()
                    .context("--content-id required for source content")?,
            )?;
            if args.content_archive.is_none() {
                let bytes = download_official_artifact(&content.url, content.size)?;
                ensure!(
                    bytes.len() as u64 == content.size,
                    "content archive size differs from verified record"
                );
                fs::write(&downloaded, bytes)?;
                prepared.content_archive = Some(downloaded);
            }
            linked_catalog::content::add_admitted(root, &prepared, &admitted, &current)
        }
    }
}
