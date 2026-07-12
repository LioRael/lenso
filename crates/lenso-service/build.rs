fn main() {
    println!("cargo:rerun-if-changed=fixtures/contracts/v2/support-grpc.v1.proto");
    let mut prost = prost_build::Config::new();
    prost.protoc_executable(
        protoc_bin_vendored::protoc_bin_path().expect("vendored protoc must be available"),
    );
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .file_descriptor_set_path(std::env::var("OUT_DIR").unwrap() + "/support_descriptor.bin")
        .compile_with_config(
            prost,
            &["fixtures/contracts/v2/support-grpc.v1.proto"],
            &["fixtures/contracts/v2"],
        )
        .expect("support gRPC Service Contract must compile");
}
