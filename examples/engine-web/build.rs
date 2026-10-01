fn main() {
    println!("cargo:rerun-if-changed=endpoints");
    let options = lenso_engine_web::WebOptions {
        provider: "Plugin".into(),
        roots: vec!["endpoints".into()],
        exclude: vec!["endpoints/private".into()],
        ..Default::default()
    };
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    lenso_engine_web::build(std::path::Path::new("."), &output, options).expect("routes");
}
