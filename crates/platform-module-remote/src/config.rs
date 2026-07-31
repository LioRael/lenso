#[derive(Clone, PartialEq, Eq)]
pub struct RemoteModuleConfig {
    pub name: String,
    pub base_url: String,
    pub transport: RemoteModuleTransport,
    pub(crate) auth_token: Option<String>,
    pub timeout_ms: u64,
}

impl std::fmt::Debug for RemoteModuleConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RemoteModuleConfig")
            .field("name", &self.name)
            .field("base_url", &self.base_url)
            .field("transport", &self.transport)
            .field("auth_configured", &self.auth_token.is_some())
            .field("timeout_ms", &self.timeout_ms)
            .finish()
    }
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
    pub fn auth_configured(&self) -> bool {
        self.auth_token.is_some()
    }

    #[must_use]
    pub fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    #[must_use]
    pub(crate) fn with_transport(
        mut self,
        transport: RemoteModuleTransport,
        base_url: impl Into<String>,
    ) -> Self {
        self.transport = transport;
        self.base_url = base_url.into().trim_end_matches('/').to_owned();
        self
    }

    #[must_use]
    pub fn for_service_module(&self, module_name: &str) -> Self {
        let mut config = self.clone();
        config.name = module_name.to_owned();
        if config.transport == RemoteModuleTransport::HttpJson {
            config.base_url = format!("{}/modules/{module_name}", self.base_url);
        }
        config
    }

    /// Matches the canonical manifest identity to this legacy runtime source.
    ///
    /// Standalone remote sources predate fully-qualified Module IDs, so their
    /// host-local source key remains a path-safe slug. Service-provided modules
    /// use their full Module ID as the source key and therefore match exactly.
    #[must_use]
    pub fn matches_module_id(&self, module_id: &str) -> bool {
        self.name == module_id
            || module_id
                .rsplit_once('/')
                .is_some_and(|(_, slug)| slug == self.name)
    }
}

fn normalize_base_url(base_url: String) -> (RemoteModuleTransport, String) {
    let trimmed = base_url.trim().trim_end_matches('/');
    match trimmed.strip_prefix("grpc://") {
        Some(rest) => (
            RemoteModuleTransport::Grpc,
            format!("http://{}", rest.trim_end_matches('/')),
        ),
        None => match trimmed.strip_prefix("grpcs://") {
            Some(rest) => (
                RemoteModuleTransport::Grpc,
                format!("https://{}", rest.trim_end_matches('/')),
            ),
            None => (RemoteModuleTransport::HttpJson, trimmed.to_owned()),
        },
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

    #[test]
    fn grpcs_scheme_selects_grpc_transport_with_tls_endpoint() {
        let config = RemoteModuleConfig::new("remote-crm", "grpcs://remote.example.test:50051/");

        assert_eq!(config.transport, RemoteModuleTransport::Grpc);
        assert_eq!(config.base_url, "https://remote.example.test:50051");
    }
}
