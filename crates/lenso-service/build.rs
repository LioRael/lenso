fn main() {
    let output = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    write_schema(
        output.join("lenso.module-manifest.v1.schema.json"),
        &lenso_contracts::module_manifest_schema(),
    );
    write_schema(
        output.join("lenso.module-release.v1.schema.json"),
        &lenso_contracts::module_release_schema(),
    );
    println!("cargo:rerun-if-changed=fixtures/contracts/v2/support-grpc.v1.proto");
    let mut prost = prost_build::Config::new();
    prost.protoc_executable(
        protoc_bin_vendored::protoc_bin_path().expect("vendored protoc must be available"),
    );
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .file_descriptor_set_path(output.join("support_descriptor.bin"))
        .compile_with_config(
            prost,
            &["fixtures/contracts/v2/support-grpc.v1.proto"],
            &["fixtures/contracts/v2"],
        )
        .expect("support gRPC Service Contract must compile");
}

fn write_schema(path: std::path::PathBuf, schema: &serde_json::Value) {
    let mut rendered = serde_json::to_string_pretty(schema).expect("Module schema must serialize");
    rendered.push('\n');
    std::fs::write(path, rendered).expect("Module schema must be written to OUT_DIR");
}
