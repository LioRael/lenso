//! The cross-repository gate supplies the exact immutable JavaScript checkout.
use super::*;

#[test]
#[ignore = "requires the pinned LENSO_JS_ROOT checkout; run by the Bun CI gate"]
fn qualified_runtime_rejects_old_same_version_and_modified_bytes() {
    let root = PathBuf::from(std::env::var_os("LENSO_JS_ROOT").expect("pinned JS checkout"));
    let source = root.join("packages/lenso-workers-runtime");
    let fixture = tempfile::tempdir().unwrap();
    let package = fixture.path();
    for name in [
        "package.json",
        "host.mjs",
        "http.mjs",
        "runner.mjs",
        "scope.mjs",
        "clock.mjs",
        "facilities.mjs",
    ] {
        fs::copy(source.join(name), package.join(name)).unwrap();
    }
    for facilities in [false, true] {
        let loaded = load_runtime(package, facilities).unwrap();
        assert_eq!(loaded.package_version, "0.1.6");
        assert_eq!(loaded.modules.len(), if facilities { 6 } else { 5 });
        for module in loaded.modules {
            assert_eq!(module.bytes, fs::read(source.join(module.name)).unwrap());
        }
    }
    let original = fs::read(package.join("scope.mjs")).unwrap();
    let old = include_bytes!("old-scope.mjs");
    assert_eq!(
        super::super::digest_bytes(old),
        "sha256:450241c47c4468a37ef932dd24d610c49318d9afc9dc369bf051acac86f38a74"
    );
    for invalid in [old.as_slice(), b"// modified runtime".as_slice()] {
        fs::write(package.join("scope.mjs"), invalid).unwrap();
        for facilities in [false, true] {
            let error = load_runtime(package, facilities).err().unwrap();
            assert!(error.to_string().contains("module scope.mjs differs"));
        }
    }
    fs::write(package.join("scope.mjs"), original).unwrap();
    for version in ["0.1.5", "9.9.9"] {
        fs::write(
            package.join("package.json"),
            serde_json::to_vec(&json!({"name":"@lenso/workers-runtime","version":version}))
                .unwrap(),
        )
        .unwrap();
        let error = load_runtime(package, false).err().unwrap();
        assert!(
            error
                .to_string()
                .contains("qualified @lenso/workers-runtime 0.1.6")
        );
    }
}
