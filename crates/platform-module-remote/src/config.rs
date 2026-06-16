#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteModuleConfig {
    pub name: String,
    pub base_url: String,
    pub transport: RemoteModuleTransport,
    pub auth_token: Option<String>,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteModuleTransport {
    HttpJson,
    Grpc,
}

impl RemoteModuleConfig {
    #[must_use]
    pub fn new(name: impl Into<String>, base_url: impl Into<String>) -> Self {
        let (transport, base_url) = normalize_base_url(base_url.into());
        Self {
            name: name.into(),
            base_url,
            transport,
            auth_token: None,
            timeout_ms: 5_000,
        }
    }

    #[must_use]
    pub fn with_auth_token(mut self, token: impl Into<String>) -> Self {
        self.auth_token = Some(token.into());
        self
    }

    #[must_use]
    pub fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }
}

fn normalize_base_url(base_url: String) -> (RemoteModuleTransport, String) {
    let trimmed = base_url.trim().trim_end_matches('/');
    match trimmed.strip_prefix("grpc://") {
        Some(rest) => (
            RemoteModuleTransport::Grpc,
            format!("http://{}", rest.trim_end_matches('/')),
        ),
        None => (RemoteModuleTransport::HttpJson, trimmed.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_http_json_transport() {
        let config =
            RemoteModuleConfig::new("remote-crm", "http://127.0.0.1:4100/lenso/module/v1/");

        assert_eq!(config.transport, RemoteModuleTransport::HttpJson);
        assert_eq!(config.base_url, "http://127.0.0.1:4100/lenso/module/v1");
    }

    #[test]
    fn grpc_scheme_selects_grpc_transport() {
        let config = RemoteModuleConfig::new("remote-crm", "grpc://127.0.0.1:50051/");

        assert_eq!(config.transport, RemoteModuleTransport::Grpc);
        assert_eq!(config.base_url, "http://127.0.0.1:50051");
    }
}
