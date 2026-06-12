#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteModuleConfig {
    pub name: String,
    pub base_url: String,
    pub auth_token: Option<String>,
    pub timeout_ms: u64,
}

impl RemoteModuleConfig {
    #[must_use]
    pub fn new(name: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            base_url: base_url.into().trim_end_matches('/').to_owned(),
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
