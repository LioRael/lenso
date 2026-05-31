#[derive(Debug, Clone)]
pub struct IdentityConfig {
    pub password_reset_ttl_minutes: u64,
}

impl Default for IdentityConfig {
    fn default() -> Self {
        Self {
            password_reset_ttl_minutes: 30,
        }
    }
}
