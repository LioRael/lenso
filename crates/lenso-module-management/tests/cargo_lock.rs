use lenso_module_management::*;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

const CHECKSUM_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const CHECKSUM_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn lock(module_version: &str, unrelated_version: &str) -> Vec<u8> {
    format!(
        "version = 4\n\n[[package]]\nname = \"host\"\nversion = \"0.1.0\"\ndependencies = [\n \"module\",\n]\n\n[[package]]\nname = \"module\"\nversion = \"{module_version}\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\nchecksum = \"{CHECKSUM_A}\"\ndependencies = [\n \"module-dep\",\n]\n\n[[package]]\nname = \"module-dep\"\nversion = \"1.0.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\nchecksum = \"{CHECKSUM_B}\"\n\n[[package]]\nname = \"unrelated\"\nversion = \"{unrelated_version}\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\nchecksum = \"{CHECKSUM_B}\"\n"
    )
    .into_bytes()
}

#[test]
fn candidate_exposes_allowed_package_diff_and_checks_release_checksum() {
    let evidence = validate_cargo_lock_candidate(
        &lock("1.0.0", "1.0.0"),
        &lock("1.1.0", "1.0.0"),
        &["host".to_owned(), "module".to_owned()],
        &[ExpectedLinkedPackage {
            package: "module".to_owned(),
            version: "1.0.0".to_owned(),
            archive_checksum: Some(format!("sha256:{CHECKSUM_A}")),
            default_features: false,
            features: vec!["runtime".to_owned()],
        }],
        &[ExpectedLinkedPackage {
            package: "module".to_owned(),
            version: "1.1.0".to_owned(),
            archive_checksum: Some(format!("sha256:{CHECKSUM_A}")),
            default_features: true,
            features: vec!["runtime".to_owned(), "search".to_owned()],
        }],
        vec!["cargo".to_owned(), "generate-lockfile".to_owned()],
    )
    .unwrap();

    assert_eq!(evidence.protocol, CARGO_LOCK_CANDIDATE_PROTOCOL);
    assert_eq!(
        evidence
            .changed_packages
            .iter()
            .map(|change| change.package.as_str())
            .collect::<Vec<_>>(),
        vec!["module"]
    );
    assert_eq!(
        evidence.changed_packages[0].candidate_features,
        vec!["default", "runtime", "search"]
    );
}

#[test]
fn candidate_rejects_unrelated_cargo_lock_churn() {
    let error = validate_cargo_lock_candidate(
        &lock("1.0.0", "1.0.0"),
        &lock("1.1.0", "2.0.0"),
        &["module".to_owned()],
        &[],
        &[],
        Vec::new(),
    )
    .unwrap_err();

    assert!(matches!(
        error,
        CargoLockResolutionError::UnrelatedPackageChurn { package }
            if package == "unrelated"
    ));
}

#[test]
fn candidate_rejects_checksum_that_does_not_match_verified_release() {
    let error = validate_cargo_lock_candidate(
        &lock("1.0.0", "1.0.0"),
        &lock("1.1.0", "1.0.0"),
        &["module".to_owned()],
        &[],
        &[ExpectedLinkedPackage {
            package: "module".to_owned(),
            version: "1.1.0".to_owned(),
            archive_checksum: Some(format!("sha256:{CHECKSUM_B}")),
            default_features: false,
            features: Vec::new(),
        }],
        Vec::new(),
    )
    .unwrap_err();

    assert!(matches!(
        error,
        CargoLockResolutionError::PackageProvenanceMismatch { package, .. }
            if package == "module"
    ));
}

#[derive(Debug, Clone)]
struct FixtureGenerator {
    candidate_lock: Vec<u8>,
}

impl CargoLockGenerator for FixtureGenerator {
    fn generate(
        &self,
        sandbox: &Path,
        manifest_path: &Path,
        _offline: bool,
    ) -> Result<Vec<String>, CargoLockResolutionError> {
        assert!(manifest_path.is_file());
        assert!(!sandbox.join("not-in-read-set.txt").exists());
        fs::write(sandbox.join("Cargo.lock"), &self.candidate_lock)?;
        Ok(vec!["fixture-cargo".to_owned()])
    }
}

#[test]
fn isolated_resolver_materializes_only_the_exact_read_set_and_candidate_files() {
    let current = lock("1.0.0", "1.0.0");
    let candidate = lock("1.1.0", "1.0.0");
    let resolver = IsolatedCargoLockResolver::new(FixtureGenerator {
        candidate_lock: candidate.clone(),
    });
    let result = resolver
        .resolve(&CargoLockResolutionRequest {
            read_set: BTreeMap::from([
                ("Cargo.toml".to_owned(), b"[workspace]\n".to_vec()),
                ("Cargo.lock".to_owned(), current),
            ]),
            candidate_files: BTreeMap::from([(
                "generated/module/Cargo.toml".to_owned(),
                b"[package]\nname = \"generated\"\nversion = \"0.1.0\"\n".to_vec(),
            )]),
            root_manifest_path: "Cargo.toml".to_owned(),
            lock_path: "Cargo.lock".to_owned(),
            allowed_root_packages: vec!["module".to_owned()],
            current_linked_packages: Vec::new(),
            expected_linked_packages: vec![ExpectedLinkedPackage {
                package: "module".to_owned(),
                version: "1.1.0".to_owned(),
                archive_checksum: Some(CHECKSUM_A.to_owned()),
                default_features: false,
                features: vec!["runtime".to_owned()],
            }],
            offline: true,
        })
        .unwrap();

    assert_eq!(result.candidate_lock, candidate);
    assert_eq!(result.evidence.command, vec!["fixture-cargo"]);
}

#[test]
fn isolated_resolver_rejects_paths_that_escape_the_sandbox() {
    let resolver = IsolatedCargoLockResolver::new(FixtureGenerator {
        candidate_lock: lock("1.0.0", "1.0.0"),
    });
    let error = resolver
        .resolve(&CargoLockResolutionRequest {
            read_set: BTreeMap::new(),
            candidate_files: BTreeMap::new(),
            root_manifest_path: "../Cargo.toml".to_owned(),
            lock_path: "Cargo.lock".to_owned(),
            allowed_root_packages: Vec::new(),
            current_linked_packages: Vec::new(),
            expected_linked_packages: Vec::new(),
            offline: true,
        })
        .unwrap_err();
    assert!(matches!(
        error,
        CargoLockResolutionError::InvalidPath { .. }
    ));
}
