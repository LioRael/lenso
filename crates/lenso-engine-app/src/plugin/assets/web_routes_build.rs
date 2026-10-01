//! Default Web authoring configuration; selection and lowering are reusable.
fn main() {
    let options = lenso_engine_web::WebOptions {
        provider: "GreetingsHttp".into(),
        ..Default::default()
    };
    for root in &options.roots {
        println!("cargo:rerun-if-changed={root}");
    }
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    lenso_engine_web::build(std::path::Path::new("."), &output, options)
        .expect("compile selected Web routes");
}
