use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=capability.json");
    println!("cargo:rerun-if-changed=schemas");
    println!("cargo:rerun-if-changed=runtime.rs");
    lenso_contract_codegen::check_projection(
        Path::new("capability.json"),
        lenso_contract_codegen::ProjectionLanguage::RustRuntime,
        Path::new("runtime.rs"),
    )
    .expect("regenerate the metadata Capability projection using the example README command");
}
