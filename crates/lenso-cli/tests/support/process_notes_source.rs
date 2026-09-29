use std::path::{Path, PathBuf};

/// Verify the candidate SDK from source, not its availability in the registry.
pub fn endpoint_sdk_source() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("lenso-capability-http-endpoint")
        .canonicalize()
        .unwrap()
}
