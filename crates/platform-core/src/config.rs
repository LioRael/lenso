use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub service: ServiceConfig,
    pub database: DatabaseConfig,
    pub http: HttpConfig,
    pub telemetry: TelemetryConfig,
    pub auth: AuthConfig,
    #[serde(default)]
    pub module_sources: ModuleSourcesConfig,
    #[serde(default)]
    pub modules: BTreeMap<String, ModuleConfig>,
}

impl AppConfig {
    pub fn from_env() -> Self {
        Self {
            service: ServiceConfig::default(),
            database: DatabaseConfig::from_env(),
            http: HttpConfig::default(),
            telemetry: TelemetryConfig::default(),
            auth: AuthConfig::default(),
            module_sources: ModuleSourcesConfig {
                remote: remote_module_sources_from_env(),
            },
            modules: BTreeMap::new(),
        }
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

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ModuleConfig {
    #[serde(flatten)]
    pub values: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ModuleSourcesConfig {
    #[serde(default)]
    pub remote: Vec<RemoteModuleSourceConfig>,
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
}
