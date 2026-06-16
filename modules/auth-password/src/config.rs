use crate::jwt::JwtConfig;
use platform_core::{
    AppContext, AppError, AppResult, RuntimeConfigDescriptor, RuntimeConfigGroupDescriptor,
    RuntimeConfigScope, RuntimeConfigSnapshot, RuntimeConfigType, RuntimeConfigVisibilityCondition,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::LazyLock;

pub const CONFIG_PREFIX: &str = "auth-password";

const DEFAULT_ARGON2_MEMORY_KIB: u32 = argon2::Params::DEFAULT_M_COST;
const DEFAULT_ARGON2_TIME_COST: u32 = argon2::Params::DEFAULT_T_COST;
const DEFAULT_ARGON2_PARALLELISM: u32 = argon2::Params::DEFAULT_P_COST;
const MIN_ARGON2_MEMORY_KIB: i64 = 8 * 1024;
const MAX_ARGON2_MEMORY_KIB: i64 = 1024 * 1024;
const MAX_ARGON2_TIME_COST: i64 = 10;
const MAX_ARGON2_PARALLELISM: i64 = 8;
const DEFAULT_JWT_TTL_HOURS: u32 = 1;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub enum HashAlgorithm {
    #[serde(rename = "argon2id")]
    Argon2id,
    #[serde(rename = "argon2i")]
    Argon2i,
}

impl Default for HashAlgorithm {
    fn default() -> Self {
        Self::Argon2id
    }
}

/// Token issuance strategy for auth-password.
///
/// - `Session`: create a database-backed session token (default).
/// - `Jwt`: issue a stateless JWT instead of a session — no row in `auth.sessions`.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub enum TokenStrategy {
    #[serde(rename = "session")]
    Session,
    #[serde(rename = "jwt")]
    Jwt,
}

impl Default for TokenStrategy {
    fn default() -> Self {
        Self::Session
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct AuthPasswordConfig {
    #[serde(default)]
    pub hash_algorithm: HashAlgorithm,
    #[serde(default = "default_argon2_memory_kib")]
    pub argon2_memory_kib: u32,
    #[serde(default = "default_argon2_time_cost")]
    pub argon2_time_cost: u32,
    #[serde(default = "default_argon2_parallelism")]
    pub argon2_parallelism: u32,
    #[serde(default)]
    pub token_strategy: TokenStrategy,
    #[serde(default)]
    pub jwt_secret: Option<String>,
    #[serde(default)]
    pub jwt_issuer: Option<String>,
    #[serde(default)]
    pub jwt_audience: Option<String>,
    #[serde(default)]
    pub jwt_ttl_hours: Option<u32>,
}

impl Default for AuthPasswordConfig {
    fn default() -> Self {
        Self {
            hash_algorithm: HashAlgorithm::Argon2id,
            argon2_memory_kib: DEFAULT_ARGON2_MEMORY_KIB,
            argon2_time_cost: DEFAULT_ARGON2_TIME_COST,
            argon2_parallelism: DEFAULT_ARGON2_PARALLELISM,
            token_strategy: TokenStrategy::Session,
            jwt_secret: None,
            jwt_issuer: None,
            jwt_audience: None,
            jwt_ttl_hours: None,
        }
    }
}

impl AuthPasswordConfig {
    pub fn from_context(ctx: &AppContext) -> AppResult<Self> {
        Self::from_snapshot(&ctx.runtime_config.snapshot())
    }

    pub fn from_snapshot(snapshot: &RuntimeConfigSnapshot) -> AppResult<Self> {
        snapshot.get(CONFIG_PREFIX)
    }

    pub fn argon2_algorithm(&self) -> argon2::Algorithm {
        match self.hash_algorithm {
            HashAlgorithm::Argon2id => argon2::Algorithm::Argon2id,
            HashAlgorithm::Argon2i => argon2::Algorithm::Argon2i,
        }
    }

    /// Returns a fully resolved [`JwtConfig`] when `token_strategy` is [`TokenStrategy::Jwt`].
    ///
    /// Returns an error if JWT is selected but `jwt_secret` is missing.
    /// Falls back to defaults for optional fields: issuer `"lenso"`, audience `"lenso"`,
    /// TTL 1 hour.
    pub fn jwt_config(&self) -> AppResult<Option<JwtConfig>> {
        if self.token_strategy != TokenStrategy::Jwt {
            return Ok(None);
        }

        let secret = self.jwt_secret.clone().ok_or_else(|| {
            AppError::validation(
                "Request validation failed",
                vec![platform_core::error::ErrorDetail {
                    field: Some("jwt_secret".to_owned()),
                    reason: "jwt_secret is required when token_strategy is jwt".to_owned(),
                }],
            )
        })?;

        Ok(Some(JwtConfig {
            secret,
            issuer: self
                .jwt_issuer
                .clone()
                .unwrap_or_else(|| "lenso".to_owned()),
            audience: self
                .jwt_audience
                .clone()
                .unwrap_or_else(|| "lenso".to_owned()),
            ttl_hours: self.jwt_ttl_hours.unwrap_or(DEFAULT_JWT_TTL_HOURS),
        }))
    }
}

pub static RUNTIME_CONFIG_GROUPS: LazyLock<Vec<RuntimeConfigGroupDescriptor>> =
    LazyLock::new(|| {
        vec![
            RuntimeConfigGroupDescriptor {
                id: "auth-password.hashing",
                label: "Password Hashing",
                description: "Password hash algorithm and Argon2 parameters.",
                order: 30,
            },
            RuntimeConfigGroupDescriptor {
                id: "auth-password.tokens",
                label: "Tokens",
                description: "Token issuance strategy and JWT settings.",
                order: 40,
            },
        ]
    });

pub static RUNTIME_CONFIG: LazyLock<Vec<RuntimeConfigDescriptor>> = LazyLock::new(|| {
    vec![
        RuntimeConfigDescriptor {
            key: "auth-password.hash_algorithm".to_owned(),
            scope: RuntimeConfigScope::Shared,
            group: Some("auth-password.hashing"),
            section: None,
            order: 10,
            visible_when: None,
            value_type: RuntimeConfigType::Enum(&["argon2id", "argon2i"]),
            default: json!("argon2id"),
            editable: true,
            restart_only: false,
            description: "Password hash algorithm used for new password hashes.",
        },
        RuntimeConfigDescriptor {
            key: "auth-password.argon2_memory_kib".to_owned(),
            scope: RuntimeConfigScope::Shared,
            group: Some("auth-password.hashing"),
            section: None,
            order: 20,
            visible_when: None,
            value_type: RuntimeConfigType::Int {
                min: Some(MIN_ARGON2_MEMORY_KIB),
                max: Some(MAX_ARGON2_MEMORY_KIB),
            },
            default: json!(DEFAULT_ARGON2_MEMORY_KIB),
            editable: true,
            restart_only: false,
            description: "Argon2 memory cost in KiB for new password hashes.",
        },
        RuntimeConfigDescriptor {
            key: "auth-password.argon2_time_cost".to_owned(),
            scope: RuntimeConfigScope::Shared,
            group: Some("auth-password.hashing"),
            section: None,
            order: 30,
            visible_when: None,
            value_type: RuntimeConfigType::Int {
                min: Some(i64::from(argon2::Params::MIN_T_COST)),
                max: Some(MAX_ARGON2_TIME_COST),
            },
            default: json!(DEFAULT_ARGON2_TIME_COST),
            editable: true,
            restart_only: false,
            description: "Argon2 iteration count for new password hashes.",
        },
        RuntimeConfigDescriptor {
            key: "auth-password.argon2_parallelism".to_owned(),
            scope: RuntimeConfigScope::Shared,
            group: Some("auth-password.hashing"),
            section: None,
            order: 40,
            visible_when: None,
            value_type: RuntimeConfigType::Int {
                min: Some(i64::from(argon2::Params::MIN_P_COST)),
                max: Some(MAX_ARGON2_PARALLELISM),
            },
            default: json!(DEFAULT_ARGON2_PARALLELISM),
            editable: true,
            restart_only: false,
            description: "Argon2 parallelism for new password hashes.",
        },
        RuntimeConfigDescriptor {
            key: "auth-password.token_strategy".to_owned(),
            scope: RuntimeConfigScope::Shared,
            group: Some("auth-password.tokens"),
            section: Some("Issuance"),
            order: 10,
            visible_when: None,
            value_type: RuntimeConfigType::Enum(&["session", "jwt"]),
            default: json!("session"),
            editable: true,
            restart_only: true,
            description: "Token issuance strategy: session (stateful, DB-backed) or jwt (stateless, self-contained).",
        },
        RuntimeConfigDescriptor {
            key: "auth-password.jwt_secret".to_owned(),
            scope: RuntimeConfigScope::Shared,
            group: Some("auth-password.tokens"),
            section: Some("JWT"),
            order: 20,
            visible_when: Some(jwt_visibility_condition()),
            value_type: RuntimeConfigType::String,
            default: json!(null),
            editable: true,
            restart_only: true,
            description: "HMAC-SHA256 secret for JWT signing. Required when token_strategy is jwt.",
        },
        RuntimeConfigDescriptor {
            key: "auth-password.jwt_issuer".to_owned(),
            scope: RuntimeConfigScope::Shared,
            group: Some("auth-password.tokens"),
            section: Some("JWT"),
            order: 30,
            visible_when: Some(jwt_visibility_condition()),
            value_type: RuntimeConfigType::String,
            default: json!("lenso"),
            editable: true,
            restart_only: false,
            description: "JWT issuer claim (iss).",
        },
        RuntimeConfigDescriptor {
            key: "auth-password.jwt_audience".to_owned(),
            scope: RuntimeConfigScope::Shared,
            group: Some("auth-password.tokens"),
            section: Some("JWT"),
            order: 40,
            visible_when: Some(jwt_visibility_condition()),
            value_type: RuntimeConfigType::String,
            default: json!("lenso"),
            editable: true,
            restart_only: false,
            description: "JWT audience claim (aud).",
        },
        RuntimeConfigDescriptor {
            key: "auth-password.jwt_ttl_hours".to_owned(),
            scope: RuntimeConfigScope::Shared,
            group: Some("auth-password.tokens"),
            section: Some("JWT"),
            order: 50,
            visible_when: Some(jwt_visibility_condition()),
            value_type: RuntimeConfigType::Int {
                min: Some(1),
                max: Some(168),
            },
            default: json!(DEFAULT_JWT_TTL_HOURS),
            editable: true,
            restart_only: false,
            description: "JWT time-to-live in hours.",
        },
    ]
});

fn jwt_visibility_condition() -> RuntimeConfigVisibilityCondition {
    RuntimeConfigVisibilityCondition::Equals {
        service: "*",
        key: "auth-password.token_strategy",
        value: json!("jwt"),
    }
}

fn default_argon2_memory_kib() -> u32 {
    DEFAULT_ARGON2_MEMORY_KIB
}

fn default_argon2_time_cost() -> u32 {
    DEFAULT_ARGON2_TIME_COST
}

fn default_argon2_parallelism() -> u32 {
    DEFAULT_ARGON2_PARALLELISM
}

#[cfg(test)]
mod tests {
    use super::*;
    use platform_core::{RuntimeConfigRegistry, RuntimeConfigSnapshot};
    use std::collections::BTreeMap;

    #[test]
    fn reads_defaults_from_snapshot() {
        let registry = RuntimeConfigRegistry::try_new(RUNTIME_CONFIG.clone()).unwrap();
        let snapshot = RuntimeConfigSnapshot::resolve(&registry, "api", &BTreeMap::new());
        let config = AuthPasswordConfig::from_snapshot(&snapshot).unwrap();

        assert_eq!(
            config,
            AuthPasswordConfig {
                hash_algorithm: HashAlgorithm::Argon2id,
                argon2_memory_kib: DEFAULT_ARGON2_MEMORY_KIB,
                argon2_time_cost: DEFAULT_ARGON2_TIME_COST,
                argon2_parallelism: DEFAULT_ARGON2_PARALLELISM,
                token_strategy: TokenStrategy::Session,
                jwt_secret: None,
                jwt_issuer: Some("lenso".to_owned()),
                jwt_audience: Some("lenso".to_owned()),
                jwt_ttl_hours: Some(DEFAULT_JWT_TTL_HOURS),
            }
        );
    }

    #[test]
    fn reads_configured_hash_policy_from_snapshot() {
        let registry = RuntimeConfigRegistry::try_new(RUNTIME_CONFIG.clone()).unwrap();
        let mut stored = BTreeMap::new();
        stored.insert(
            ("*".to_owned(), "auth-password.hash_algorithm".to_owned()),
            json!("argon2i"),
        );
        stored.insert(
            ("*".to_owned(), "auth-password.argon2_memory_kib".to_owned()),
            json!(16384),
        );
        stored.insert(
            ("*".to_owned(), "auth-password.argon2_time_cost".to_owned()),
            json!(3),
        );
        stored.insert(
            (
                "*".to_owned(),
                "auth-password.argon2_parallelism".to_owned(),
            ),
            json!(2),
        );
        let snapshot = RuntimeConfigSnapshot::resolve(&registry, "api", &stored);
        let config = AuthPasswordConfig::from_snapshot(&snapshot).unwrap();

        assert_eq!(
            config,
            AuthPasswordConfig {
                hash_algorithm: HashAlgorithm::Argon2i,
                argon2_memory_kib: 16384,
                argon2_time_cost: 3,
                argon2_parallelism: 2,
                token_strategy: TokenStrategy::Session,
                jwt_secret: None,
                jwt_issuer: Some("lenso".to_owned()),
                jwt_audience: Some("lenso".to_owned()),
                jwt_ttl_hours: Some(DEFAULT_JWT_TTL_HOURS),
            }
        );
    }

    #[test]
    fn jwt_config_returns_none_when_session_strategy() {
        let config = AuthPasswordConfig::default();
        assert!(config.jwt_config().unwrap().is_none());
    }

    #[test]
    fn jwt_config_returns_error_when_jwt_without_secret() {
        let config = AuthPasswordConfig {
            token_strategy: TokenStrategy::Jwt,
            ..AuthPasswordConfig::default()
        };
        assert!(config.jwt_config().is_err());
    }

    #[test]
    fn jwt_config_returns_config_when_jwt_with_secret() {
        let config = AuthPasswordConfig {
            token_strategy: TokenStrategy::Jwt,
            jwt_secret: Some("my-secret".to_owned()),
            jwt_issuer: Some("custom-issuer".to_owned()),
            jwt_audience: Some("custom-audience".to_owned()),
            jwt_ttl_hours: Some(24),
            ..AuthPasswordConfig::default()
        };
        let jwt_config = config.jwt_config().unwrap().unwrap();
        assert_eq!(jwt_config.secret, "my-secret");
        assert_eq!(jwt_config.issuer, "custom-issuer");
        assert_eq!(jwt_config.audience, "custom-audience");
        assert_eq!(jwt_config.ttl_hours, 24);
    }

    #[test]
    fn jwt_config_uses_defaults_for_optional_fields() {
        let config = AuthPasswordConfig {
            token_strategy: TokenStrategy::Jwt,
            jwt_secret: Some("secret".to_owned()),
            ..AuthPasswordConfig::default()
        };
        let jwt_config = config.jwt_config().unwrap().unwrap();
        assert_eq!(jwt_config.issuer, "lenso");
        assert_eq!(jwt_config.audience, "lenso");
        assert_eq!(jwt_config.ttl_hours, DEFAULT_JWT_TTL_HOURS);
    }
}
