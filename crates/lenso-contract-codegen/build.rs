use sha2::{Digest, Sha256};
fn main() {
    let mut hasher = Sha256::new();
    let mut sources = std::fs::read_dir("src")
        .expect("generator sources")
        .map(|entry| entry.expect("source entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
        .collect::<Vec<_>>();
    sources.sort();
    for path in sources {
        println!("cargo:rerun-if-changed={}", path.display());
        hasher.update(path.to_string_lossy().as_bytes());
        hasher.update(std::fs::read(path).expect("generator source"));
    }
    let revision = format!("{:x}", hasher.finalize());
    println!("cargo:rustc-env=LENSO_GENERATOR_REVISION={revision}");
}
