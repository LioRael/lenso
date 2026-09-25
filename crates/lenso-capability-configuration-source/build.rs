use std::path::Path;

use lenso_contract_codegen::{ProjectionLanguage, check_projection};

fn main() {
    println!("cargo:rerun-if-changed=capability.json");
    println!("cargo:rerun-if-changed=schemas");
    println!("cargo:rerun-if-changed=src/generated.rs");
    println!("cargo:rerun-if-changed=src/generated_plugin.rs");
    check_projection(
        Path::new("capability.json"),
        ProjectionLanguage::RustRuntime,
        Path::new("src/generated.rs"),
    )
    .expect("stale Configuration Source Host projection");
    check_projection(
        Path::new("capability.json"),
        ProjectionLanguage::RustPlugin,
        Path::new("src/generated_plugin.rs"),
    )
    .expect("stale Configuration Source Plugin projection");
}
