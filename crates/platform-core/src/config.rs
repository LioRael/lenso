use crate::error::{AppError, AppResult, ErrorDetail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const DEFAULT_LINKED_MODULE_PROFILE: &str = "demo";
pub const LENSO_COMPOSITION_PROFILE_ENV: &str = "LENSO_COMPOSITION_PROFILE";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub service: ServiceConfig,
    pub database: DatabaseConfig,
    pub http: HttpConfig,
    pub telemetry: TelemetryConfig,
    pub auth: AuthConfig,
    #[serde(default)]
    pub console: ConsoleConfig,
    #[serde(default)]
    pub module_sources: ModuleSourcesConfig,
    #[serde(default)]
    pub modules: BTreeMap<String, ModuleConfig>,
}

impl AppConfig {
    pub fn from_env() -> Self {
        Self::try_from_env().expect("valid Lenso application configuration")
    }

    pub fn try_from_env() -> AppResult<Self> {
        let _ = dotenvy::dotenv();
        let service = ServiceConfig::default();
        Ok(Self {
            module_sources: ModuleSourcesConfig::try_from_env_for_environment(
                &service.environment,
            )?,
            service,
            database: DatabaseConfig::from_env(),
            http: HttpConfig::default(),
            telemetry: TelemetryConfig::default(),
            auth: AuthConfig::default(),
            console: ConsoleConfig::default(),
            modules: module_configs_from_env(),
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServiceConfig {
    pub name: String,
    pub environment: String,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            name: std::env::var("SERVICE_NAME").unwrap_or_else(|_| "lenso".to_owned()),
            environment: std::env::var("APP_ENV").unwrap_or_else(|_| "local".to_owned()),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
}

impl DatabaseConfig {
    fn from_env() -> Self {
        Self {
            url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://lenso:lenso@localhost:5432/lenso".to_owned()),
            max_connections: std::env::var("DATABASE_MAX_CONNECTIONS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(10),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HttpConfig {
    pub host: String,
    pub port: u16,
    /// Origins permitted by CORS. Defaults to the local Runtime Console dev
    /// ports; override with `CORS_ALLOWED_ORIGINS` (comma-separated).
    #[serde(default)]
    pub cors_allowed_origins: Vec<String>,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            host: std::env::var("HTTP_HOST").unwrap_or_else(|_| "0.0.0.0".to_owned()),
            port: std::env::var("HTTP_PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(3000),
            cors_allowed_origins: std::env::var("CORS_ALLOWED_ORIGINS").map_or_else(
                |_| default_cors_allowed_origins(),
                |value| parse_cors_allowed_origins(&value),
            ),
        }
    }
}

fn default_cors_allowed_origins() -> Vec<String> {
    (5173..=5177)
        .map(|port| format!("http://localhost:{port}"))
        .collect()
}

/// Parse a comma-separated `CORS_ALLOWED_ORIGINS` value into trimmed, non-empty
/// origins.
#[must_use]
pub fn parse_cors_allowed_origins(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TelemetryConfig {
    pub log_level: String,
    #[serde(default)]
    pub log_format: LogFormat,
    pub otlp_endpoint: Option<String>,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            log_level: std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_owned()),
            log_format: std::env::var("LOG_FORMAT")
                .ok()
                .and_then(|value| LogFormat::from_env_value(&value))
                .unwrap_or_default(),
            otlp_endpoint: std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LogFormat {
    #[default]
    Compact,
    Json,
}

impl LogFormat {
    pub fn from_env_value(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "compact" | "terminal" | "text" => Some(Self::Compact),
            "json" => Some(Self::Json),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AuthConfig {
    pub issuer: Option<String>,
    pub audience: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConsoleConfig {
    pub dist_dir: String,
    pub extensions_dir: String,
}

impl Default for ConsoleConfig {
    fn default() -> Self {
        Self {
            dist_dir: std::env::var("LENSO_CONSOLE_DIST_DIR")
                .unwrap_or_else(|_| ".lenso/console/dist".to_owned()),
            extensions_dir: std::env::var("LENSO_CONSOLE_EXTENSIONS_DIR")
                .unwrap_or_else(|_| ".lenso/console/extensions".to_owned()),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ModuleConfig {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(flatten)]
    pub values: BTreeMap<String, serde_json::Value>,
}

impl ModuleConfig {
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }
}

fn module_configs_from_env() -> BTreeMap<String, ModuleConfig> {
    std::env::vars()
        .filter_map(|(key, value)| module_config_from_env_entry(&key, &value))
        .collect()
}

fn module_config_from_env_entry(key: &str, value: &str) -> Option<(String, ModuleConfig)> {
    let module_name = key
        .strip_prefix("LENSO_MODULE_")?
        .strip_suffix("_ENABLED")?
        .to_ascii_lowercase()
        .replace('_', "-");
    if module_name.is_empty() {
        return None;
    }
    let enabled = parse_bool_env(value)?;
    Some((
        module_name,
        ModuleConfig {
            enabled: Some(enabled),
            values: BTreeMap::new(),
        },
    ))
}

fn parse_bool_env(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModuleSourcesConfig {
    #[serde(default = "default_linked_module_profile")]
    pub linked_profile: String,
    #[serde(default)]
    pub remote: Vec<RemoteModuleSourceConfig>,
}

impl ModuleSourcesConfig {
    fn try_from_env_for_environment(environment: &str) -> AppResult<Self> {
        Ok(Self {
            linked_profile: linked_module_profile_from_env_value(
                std::env::var(LENSO_COMPOSITION_PROFILE_ENV).ok().as_deref(),
                environment,
            )?,
            remote: remote_module_sources_from_env(),
        })
    }
}

impl Default for ModuleSourcesConfig {
    fn default() -> Self {
        Self {
            linked_profile: default_linked_module_profile(),
            remote: Vec::new(),
        }
    }
}

fn default_linked_module_profile() -> String {
    DEFAULT_LINKED_MODULE_PROFILE.to_owned()
}

fn linked_module_profile_from_env_value(
    value: Option<&str>,
    environment: &str,
) -> AppResult<String> {
    let Some(profile) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        if is_local_development_environment(environment) {
            return Ok(DEFAULT_LINKED_MODULE_PROFILE.to_owned());
        }
        return Err(AppError::validation(
            "Lenso composition profile is required outside local development",
            vec![ErrorDetail {
                field: Some(LENSO_COMPOSITION_PROFILE_ENV.to_owned()),
                reason: format!(
                    "set {LENSO_COMPOSITION_PROFILE_ENV}=core or {LENSO_COMPOSITION_PROFILE_ENV}=demo when APP_ENV is `{}`",
                    environment.trim()
                ),
            }],
        ));
    };

    Ok(profile.to_owned())
}

#[must_use]
pub fn is_local_development_environment(environment: &str) -> bool {
    matches!(
        environment.trim().to_ascii_lowercase().as_str(),
        "local" | "dev" | "development" | "test"
    )
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RemoteModuleSourceConfig {
    pub name: String,
    pub base_url: String,
    pub auth_token_env: Option<String>,
    pub timeout_ms: u64,
}

fn remote_module_sources_from_env() -> Vec<RemoteModuleSourceConfig> {
    let Some(raw) = std::env::var("REMOTE_MODULES").ok() else {
        return Vec::new();
    };

    raw.split(',')
        .filter_map(|entry| parse_remote_module_source(entry.trim()))
        .collect()
}

fn parse_remote_module_source(entry: &str) -> Option<RemoteModuleSourceConfig> {
    if entry.is_empty() {
        return None;
    }
    let (name, base_url) = entry.split_once('=')?;
    let name = name.trim();
    let base_url = base_url.trim();
    if name.is_empty() || base_url.is_empty() {
        return None;
    }

    let env_prefix = name.replace('-', "_").to_ascii_uppercase();
    let token_env = format!("REMOTE_MODULE_{env_prefix}_TOKEN");
    let timeout_env = format!("REMOTE_MODULE_{env_prefix}_TIMEOUT_MS");

    Some(RemoteModuleSourceConfig {
        name: name.to_owned(),
        base_url: base_url.trim_end_matches('/').to_owned(),
        auth_token_env: Some(token_env),
        timeout_ms: std::env::var(timeout_env)
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(5_000),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_sources_default_to_demo_linked_profile() {
        let config = ModuleSourcesConfig::default();

        assert_eq!(config.linked_profile, DEFAULT_LINKED_MODULE_PROFILE);
        assert!(config.remote.is_empty());
    }

    #[test]
    fn linked_module_profile_from_env_value_trims_empty_to_default() {
        assert_eq!(
            linked_module_profile_from_env_value(None, "local").expect("local default"),
            DEFAULT_LINKED_MODULE_PROFILE
        );
        assert_eq!(
            linked_module_profile_from_env_value(Some("  "), "development")
                .expect("development default"),
            DEFAULT_LINKED_MODULE_PROFILE
        );
        assert_eq!(
            linked_module_profile_from_env_value(Some("core"), "production")
                .expect("explicit profile"),
            "core"
        );
        assert_eq!(
            linked_module_profile_from_env_value(Some(" demo "), "production")
                .expect("explicit profile"),
            "demo"
        );
    }

    #[test]
    fn linked_module_profile_from_env_value_requires_explicit_profile_outside_local() {
        let error = linked_module_profile_from_env_value(None, "production")
            .expect_err("production requires explicit linked profile");

        assert_eq!(error.code, crate::ErrorCode::Validation);
        assert_eq!(
            error.details[0].field.as_deref(),
            Some(LENSO_COMPOSITION_PROFILE_ENV)
        );
        assert!(
            error.details[0]
                .reason
                .contains("LENSO_COMPOSITION_PROFILE=core")
        );
    }

    #[test]
    fn module_sources_deserialize_missing_linked_profile_to_default() {
        let config: ModuleSourcesConfig =
            serde_json::from_value(serde_json::json!({ "remote": [] }))
                .expect("module sources deserialize");

        assert_eq!(config.linked_profile, DEFAULT_LINKED_MODULE_PROFILE);
        assert!(config.remote.is_empty());
    }

    #[test]
    fn parses_remote_module_source_entry() {
        let config = parse_remote_module_source("remote-crm=http://localhost:4100/lenso/module/v1")
            .expect("parse remote source");
        assert_eq!(config.name, "remote-crm");
        assert_eq!(config.base_url, "http://localhost:4100/lenso/module/v1");
        assert_eq!(
            config.auth_token_env.as_deref(),
            Some("REMOTE_MODULE_REMOTE_CRM_TOKEN")
        );
        assert_eq!(config.timeout_ms, 5_000);
    }

    #[test]
    fn ignores_malformed_remote_module_source_entry() {
        assert!(parse_remote_module_source("").is_none());
        assert!(parse_remote_module_source("missing-url").is_none());
        assert!(parse_remote_module_source("=http://localhost:4100").is_none());
    }

    #[test]
    fn module_config_from_env_entry_parses_enabled_override() {
        let (name, config) =
            module_config_from_env_entry("LENSO_MODULE_AUTH_PASSWORD_ENABLED", "false")
                .expect("module enabled env should parse");

        assert_eq!(name, "auth-password");
        assert_eq!(config.enabled, Some(false));
    }
}
