fn main() {
    if let Err(error) = lenso_host_runtime::run_with_codecs(Vec::new()) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
