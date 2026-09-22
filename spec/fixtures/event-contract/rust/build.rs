use std::path::Path;

use lenso_contract_codegen::check_generated;

fn main() {
    println!("cargo:rerun-if-changed=../../../crates/lenso-contract-codegen/tests/fixtures/event");
    println!("cargo:rerun-if-changed=../generated/notifications.rs");
    println!("cargo:rerun-if-changed=../generated/notifications.ts");

    check_generated(
        Path::new("../../../crates/lenso-contract-codegen/tests/fixtures/event/capability.json"),
        Path::new("../generated/notifications.rs"),
        Path::new("../generated/notifications.ts"),
    )
    .unwrap_or_else(|error| panic!("Event generated artifacts are stale: {error}"));
}
