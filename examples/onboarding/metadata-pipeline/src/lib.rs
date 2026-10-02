//! A metadata-only pipeline with a required, Plan-bound Plugin dependency.
pub mod health;
pub use metadata_contract as metadata;
pub mod normalizer;
pub mod summary;

/// Selecting a consumer without its normalizer must fail during resolution.
pub fn host(normalizer: bool, summary: bool) -> lenso_web_host::NativeWebHost {
    let mut host = lenso_web_host::NativeWebHost::new()
        .plugin::<health::Plugin>()
        .bind("127.0.0.1:0".parse().expect("static loopback address"));
    if normalizer {
        host = host.plugin::<normalizer::Plugin>();
    }
    if summary {
        host = host.plugin::<summary::Plugin>();
    }
    host
}
