//! A bounded, run-scoped observation of the Web Ingress table at the Host Ready Gate.
//! The receipt is not part of a distribution and is never interpreted as source intent.

use std::{collections::BTreeSet, fs, path::Path};

use anyhow::{Context as _, Result, ensure};
use serde::Deserialize;
use sha2::{Digest as _, Sha256};

use super::{ObservedWebRouteFacts, ObservedWebRoutesFacts, SourceLocation};

const MAX_RECEIPT_BYTES: u64 = 128 * 1024;
const MAX_LOCK_BYTES: u64 = 8 * 1024 * 1024;
const MAX_ROUTES: usize = 256;
const MAX_ROUTE_BYTES: usize = 4096;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadyWebRoutesReceipt {
    schema: String,
    capture: String,
    plugin_root_revision: String,
    distribution_lock_sha256: String,
    routes: Vec<ReadyWebRoute>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadyWebRoute {
    method: String,
    path: String,
    route_id: String,
}

/// Return no route facts when this Host version supplied no receipt. A present but
/// invalid receipt is an error so the caller can fail closed without fabricating
/// facts; neither case makes the underlying App distribution invalid.
pub fn inspect_ready_web_routes(
    distribution: &Path,
    receipt_path: &Path,
    plugin_root_revision: &str,
    run_request_id: &str,
) -> Result<Option<ObservedWebRoutesFacts>> {
    let metadata = match fs::symlink_metadata(receipt_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("inspect Web route receipt"),
    };
    ensure!(
        metadata.file_type().is_file(),
        "Web route receipt is not a regular file"
    );
    ensure!(
        metadata.len() <= MAX_RECEIPT_BYTES,
        "Web route receipt exceeds size limit"
    );
    let bytes = super::read_bounded(receipt_path, MAX_RECEIPT_BYTES)?;
    let receipt: ReadyWebRoutesReceipt =
        serde_json::from_slice(&bytes).context("decode Web route receipt")?;
    ensure!(
        receipt.schema == "lenso.live-web-routes.v1",
        "unsupported Web route receipt schema"
    );
    ensure!(
        receipt.capture == "ready_gate",
        "unsupported Web route capture point"
    );
    ensure!(
        receipt.plugin_root_revision == plugin_root_revision,
        "Web route receipt belongs to a different Plugin Root revision"
    );
    ensure!(
        receipt.routes.len() <= MAX_ROUTES,
        "Web route receipt has too many routes"
    );

    let lock_path = distribution.join(".lenso/distribution.lock.json");
    let lock_metadata = fs::symlink_metadata(&lock_path).context("inspect distribution lock")?;
    ensure!(
        lock_metadata.file_type().is_file(),
        "distribution lock is not a regular file"
    );
    ensure!(
        lock_metadata.len() <= MAX_LOCK_BYTES,
        "distribution lock exceeds size limit"
    );
    let lock_bytes = super::read_bounded(&lock_path, MAX_LOCK_BYTES)?;
    let lock_digest = format!("sha256:{}", hex::encode(Sha256::digest(&lock_bytes)));
    ensure!(
        receipt.distribution_lock_sha256 == lock_digest,
        "Web route receipt belongs to a different distribution lock"
    );

    let mut seen = BTreeSet::new();
    let mut routes = Vec::with_capacity(receipt.routes.len());
    for route in receipt.routes {
        ensure!(
            !route.method.is_empty()
                && route.method.len() <= 64
                && route.method.bytes().all(|byte| byte.is_ascii_graphic()),
            "Web route receipt has an invalid method"
        );
        ensure!(
            route.path.starts_with('/')
                && !route.path.contains(['?', '#'])
                && !route.route_id.trim().is_empty()
                && route.method.len() + route.path.len() + route.route_id.len() <= MAX_ROUTE_BYTES,
            "Web route receipt has an invalid route"
        );
        ensure!(
            seen.insert((route.method.clone(), route.path.clone())),
            "Web route receipt has duplicate method and path"
        );
        routes.push(ObservedWebRouteFacts {
            method: route.method,
            path: route.path,
            route_id: route.route_id,
        });
    }

    Ok(Some(ObservedWebRoutesFacts {
        capture: "ready_gate",
        run_request_id: run_request_id.to_owned(),
        source_location: SourceLocation {
            path: receipt_path.to_path_buf(),
        },
        plugin_root_revision: receipt.plugin_root_revision,
        distribution_lock_sha256: receipt.distribution_lock_sha256,
        routes,
    }))
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use serde_json::json;
    use sha2::{Digest as _, Sha256};

    use super::inspect_ready_web_routes;

    fn write_receipt(root: &Path, revision: &str) -> std::path::PathBuf {
        let lock_dir = root.join("dist/.lenso");
        fs::create_dir_all(&lock_dir).unwrap();
        let lock = b"fixed-distribution-lock";
        fs::write(lock_dir.join("distribution.lock.json"), lock).unwrap();
        let receipt = root.join("host.web-routes.json");
        fs::write(
            &receipt,
            serde_json::to_vec(&json!({
                "schema": "lenso.live-web-routes.v1",
                "capture": "ready_gate",
                "plugin_root_revision": revision,
                "distribution_lock_sha256": format!("sha256:{}", hex::encode(Sha256::digest(lock))),
                "routes": [{"method":"GET","path":"/notes/{id}","route_id":"notes.read"}],
            }))
            .unwrap(),
        )
        .unwrap();
        receipt
    }

    #[test]
    fn accepts_only_exact_revision_and_distribution_lock() {
        let temp = tempfile::tempdir().unwrap();
        let receipt = write_receipt(temp.path(), "revision-a");
        let dist = temp.path().join("dist");
        let observed = inspect_ready_web_routes(&dist, &receipt, "revision-a", "run-1")
            .unwrap()
            .unwrap();
        assert_eq!(observed.run_request_id, "run-1");
        assert_eq!(observed.capture, "ready_gate");
        assert_eq!(observed.routes[0].path, "/notes/{id}");
        assert!(inspect_ready_web_routes(&dist, &receipt, "revision-b", "run-1").is_err());
        fs::write(dist.join(".lenso/distribution.lock.json"), b"changed").unwrap();
        assert!(inspect_ready_web_routes(&dist, &receipt, "revision-a", "run-1").is_err());
    }

    #[test]
    fn missing_receipt_is_optional_but_malformed_receipt_is_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let dist = temp.path().join("dist");
        let receipt = temp.path().join("host.web-routes.json");
        assert!(
            inspect_ready_web_routes(&dist, &receipt, "revision", "run-1")
                .unwrap()
                .is_none()
        );
        fs::write(&receipt, b"{\"schema\":\"wrong\"}").unwrap();
        assert!(inspect_ready_web_routes(&dist, &receipt, "revision", "run-1").is_err());
        let receipt = write_receipt(temp.path(), "revision");
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&receipt).unwrap()).unwrap();
        value["routes"][0]["path"] = "/notes?fake".into();
        fs::write(&receipt, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(inspect_ready_web_routes(&dist, &receipt, "revision", "run-1").is_err());
    }
}
