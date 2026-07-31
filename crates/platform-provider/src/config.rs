#[derive(Clone, PartialEq, Eq)]
pub struct ProviderConfig {
    pub name: String,
    pub export_key: String,
    pub base_url: String,
    pub transport: ProviderTransport,
    pub(crate) auth_token: Option<String>,
    pub timeout_ms: u64,
    pub(crate) service_release_digest: Option<String>,
    pub(crate) module_release_digest: Option<String>,
    pub(crate) manifest_digest: Option<String>,
    pub(crate) contract_digests: Vec<String>,
    pub(crate) allowed_host_function_names: Vec<String>,
}

impl std::fmt::Debug for ProviderConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProviderConfig")
            .field("name", &self.name)
            .field("export_key", &self.export_key)
            .field("base_url", &self.base_url)
            .field("transport", &self.transport)
            .field("auth_configured", &self.auth_token.is_some())
            .field("timeout_ms", &self.timeout_ms)
            .field("locked", &self.service_release_digest.is_some())
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderTransport {
    HttpJson,
    Grpc,
}

impl ProviderConfig {
    #[must_use]
    pub fn new(name: impl Into<String>, base_url: impl Into<String>) -> Self {
        let (transport, base_url) = normalize_base_url(base_url.into());
        Self {
            name: name.into(),
            export_key: String::new(),
            base_url,
            transport,
            auth_token: None,
            timeout_ms: 5_000,
            service_release_digest: None,
            module_release_digest: None,
            manifest_digest: None,
            contract_digests: Vec::new(),
            allowed_host_function_names: Vec::new(),
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
        transport: ProviderTransport,
        base_url: impl Into<String>,
    ) -> Self {
        self.transport = transport;
        self.base_url = base_url.into().trim_end_matches('/').to_owned();
        self
    }

    #[must_use]
    pub(crate) fn with_export_key(mut self, export_key: impl Into<String>) -> Self {
        self.export_key = export_key.into();
        self
    }

    #[must_use]
    pub(crate) fn with_locked_contract(
        mut self,
        service_release_digest: impl Into<String>,
        module_release_digest: impl Into<String>,
        manifest_digest: impl Into<String>,
        contract_digests: Vec<String>,
    ) -> Self {
        self.service_release_digest = Some(service_release_digest.into());
        self.module_release_digest = Some(module_release_digest.into());
        self.manifest_digest = Some(manifest_digest.into());
        self.contract_digests = contract_digests;
        self
    }

    #[must_use]
    pub(crate) fn with_allowed_host_functions(
        mut self,
        function_names: impl IntoIterator<Item = String>,
    ) -> Self {
        self.allowed_host_function_names = function_names.into_iter().collect();
        self.allowed_host_function_names.sort();
        self.allowed_host_function_names.dedup();
        self
    }

    /// Matches the canonical manifest identity to this legacy runtime source.
    ///
    /// Standalone provider sources predate fully-qualified Module IDs, so their
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

fn normalize_base_url(base_url: String) -> (ProviderTransport, String) {
    let trimmed = base_url.trim().trim_end_matches('/');
    match trimmed.strip_prefix("grpc://") {
        Some(rest) => (
            ProviderTransport::Grpc,
            format!("http://{}", rest.trim_end_matches('/')),
        ),
        None => match trimmed.strip_prefix("grpcs://") {
            Some(rest) => (
                ProviderTransport::Grpc,
                format!("https://{}", rest.trim_end_matches('/')),
            ),
            None => (ProviderTransport::HttpJson, trimmed.to_owned()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_http_json_transport() {
        let config =
            ProviderConfig::new("provider-crm", "http://127.0.0.1:4100/lenso/provider/v1/");

        assert_eq!(config.transport, ProviderTransport::HttpJson);
        assert_eq!(config.base_url, "http://127.0.0.1:4100/lenso/provider/v1");
    }

    #[test]
    fn grpc_scheme_selects_grpc_transport() {
        let config = ProviderConfig::new("provider-crm", "grpc://127.0.0.1:50051/");

        assert_eq!(config.transport, ProviderTransport::Grpc);
        assert_eq!(config.base_url, "http://127.0.0.1:50051");
    }

    #[test]
    fn grpcs_scheme_selects_grpc_transport_with_tls_endpoint() {
        let config = ProviderConfig::new("provider-crm", "grpcs://provider.example.test:50051/");

        assert_eq!(config.transport, ProviderTransport::Grpc);
        assert_eq!(config.base_url, "https://provider.example.test:50051");
    }
}
