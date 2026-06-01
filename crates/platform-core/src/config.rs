use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub service: ServiceConfig,
    pub database: DatabaseConfig,
    pub http: HttpConfig,
    pub telemetry: TelemetryConfig,
    pub runtime: RuntimeConfig,
    pub auth: AuthConfig,
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
            runtime: RuntimeConfig::default(),
            auth: AuthConfig::default(),
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
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            host: std::env::var("HTTP_HOST").unwrap_or_else(|_| "0.0.0.0".to_owned()),
            port: std::env::var("HTTP_PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(3000),
        }
    }
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RuntimeConfig {
    pub worker_poll_interval_ms: u64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            worker_poll_interval_ms: 1_000,
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
