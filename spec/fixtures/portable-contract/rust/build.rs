use std::path::Path;

use lenso_contract_codegen::check_generated;

fn main() {
    println!(
        "cargo:rerun-if-changed=../../../crates/lenso-contract-codegen/tests/fixtures/profile"
    );
    println!("cargo:rerun-if-changed=../generated/profile.rs");
    println!("cargo:rerun-if-changed=../generated/profile.ts");

    check_generated(
        Path::new("../../../crates/lenso-contract-codegen/tests/fixtures/profile/capability.json"),
        Path::new("../generated/profile.rs"),
        Path::new("../generated/profile.ts"),
    )
    .unwrap_or_else(|error| panic!("portable profile generated artifacts are stale: {error}"));
}
