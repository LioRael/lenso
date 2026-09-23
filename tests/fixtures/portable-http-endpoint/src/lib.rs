//! Business behavior compiled unchanged into the native and Component fixtures.

pub const ROUTES: [(&str, &str, &str); 4] = [
    ("method", "GET", "/method/{item}"),
    ("bytes", "POST", "/bytes"),
    ("reject", "GET", "/reject"),
    ("failure", "GET", "/failure"),
];

/// One outcome of the target-independent business handler.
#[derive(Debug)]
pub enum Reply {
    Bytes(Vec<u8>),
    DomainError,
    RuntimeFailure,
}

/// Handles one already-routed HTTP request without using a host API.
pub fn handle(route_id: &str, method: &str, path: &str, body: &[u8]) -> Reply {
    match (route_id, method, path) {
        ("method", "GET", path) if path.starts_with("/method/") => {
            Reply::Bytes(format!("{method} {path}").into_bytes())
        }
        ("bytes", "POST", "/bytes") => Reply::Bytes(body.to_vec()),
        ("reject", "GET", "/reject") => Reply::DomainError,
        ("failure", "GET", "/failure") => Reply::RuntimeFailure,
        _ => Reply::DomainError,
    }
}
