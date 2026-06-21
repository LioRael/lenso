use crate::error::{AppError, AppResult, ErrorDetail};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::net::IpAddr;

pub const DEFAULT_LINKED_MODULE_PROFILE: &str = "demo";
pub const LENSO_COMPOSITION_PROFILE_ENV: &str = "LENSO_COMPOSITION_PROFILE";
pub const LENSO_ALLOW_DEV_AUTH_ON_PUBLIC_BIND_ENV: &str = "LENSO_ALLOW_DEV_AUTH_ON_PUBLIC_BIND";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub service: ServiceConfig,
    pub database: DatabaseConfig,
    #[serde(default)]
    pub redis: RedisConfig,
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
        let http = HttpConfig::default();
        validate_dev_auth_http_bind(&service, &http, dev_auth_public_bind_override_from_env())?;
        Ok(Self {
            module_sources: ModuleSourcesConfig::try_from_env_for_environment(
                &service.environment,
            )?,
            service,
            database: DatabaseConfig::from_env(),
            redis: RedisConfig::from_env(),
            http,
            telemetry: TelemetryConfig::default(),
            auth: AuthConfig::default(),
            console: ConsoleConfig::default(),
            modules: module_configs_from_env(),
        })
    }

    pub fn module_local_config<T: DeserializeOwned>(&self, module_name: &str) -> AppResult<T> {
        let values = self
            .modules
            .get(module_name)
            .map(|config| config.values.clone())
            .unwrap_or_default();
        decode_module_local_config(module_name, &values)
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

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RedisConfig {
    pub url: Option<String>,
}

impl RedisConfig {
    fn from_env() -> Self {
        Self::from_url_value(std::env::var("REDIS_URL").ok().as_deref())
    }

    #[must_use]
    pub fn from_url_value(value: Option<&str>) -> Self {
        Self {
            url: value
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned),
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
            host: std::env::var("HTTP_HOST").unwrap_or_else(|_| "127.0.0.1".to_owned()),
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

    pub fn local_config<T: DeserializeOwned>(&self, module_name: &str) -> AppResult<T> {
        decode_module_local_config(module_name, &self.values)
    }
}

fn module_configs_from_env() -> BTreeMap<String, ModuleConfig> {
    let mut configs = BTreeMap::new();
    for (key, value) in std::env::vars() {
        if let Some((module_name, update)) = module_config_from_env_entry(&key, &value) {
            merge_module_config(&mut configs, module_name, update);
        }
    }
    configs
}

fn module_config_from_env_entry(key: &str, value: &str) -> Option<(String, ModuleConfig)> {
    let rest = key.strip_prefix("LENSO_MODULE_")?;
    if !rest.contains("__")
        && let Some(module_name) = rest.strip_suffix("_ENABLED").and_then(module_env_name)
    {
        return Some((
            module_name,
            ModuleConfig {
                enabled: Some(parse_bool_env(value)?),
                values: BTreeMap::new(),
            },
        ));
    }

    let (module_name, config_key) = rest.split_once("__")?;
    let module_name = module_env_name(module_name)?;
    let config_key = module_value_env_key(config_key)?;
    let mut values = BTreeMap::new();
    values.insert(config_key, parse_module_env_value(value));
    Some((
        module_name,
        ModuleConfig {
            enabled: None,
            values,
        },
    ))
}

fn merge_module_config(
    configs: &mut BTreeMap<String, ModuleConfig>,
    module_name: String,
    update: ModuleConfig,
) {
    let config = configs.entry(module_name).or_default();
    if update.enabled.is_some() {
        config.enabled = update.enabled;
    }
    config.values.extend(update.values);
}

fn module_env_name(value: &str) -> Option<String> {
    let name = value
        .trim_matches('_')
        .to_ascii_lowercase()
        .replace('_', "-");
    (!name.is_empty()).then_some(name)
}

fn module_value_env_key(value: &str) -> Option<String> {
    let key = value.trim_matches('_').to_ascii_lowercase();
    (!key.is_empty()).then_some(key)
}

fn parse_module_env_value(value: &str) -> serde_json::Value {
    let trimmed = value.trim();
    serde_json::from_str(trimmed).unwrap_or_else(|_| serde_json::json!(trimmed))
}

fn decode_module_local_config<T: DeserializeOwned>(
    module_name: &str,
    values: &BTreeMap<String, serde_json::Value>,
) -> AppResult<T> {
    let object = values
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    serde_json::from_value(serde_json::Value::Object(object)).map_err(|source| {
        AppError::validation(
            "Invalid module local configuration",
            vec![ErrorDetail {
                field: Some(format!("modules.{module_name}")),
                reason: source.to_string(),
            }],
        )
    })
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

fn dev_auth_public_bind_override_from_env() -> bool {
    std::env::var(LENSO_ALLOW_DEV_AUTH_ON_PUBLIC_BIND_ENV)
        .ok()
        .and_then(|value| parse_bool_env(&value))
        .unwrap_or(false)
}

fn validate_dev_auth_http_bind(
    service: &ServiceConfig,
    http: &HttpConfig,
    allow_public_bind: bool,
) -> AppResult<()> {
    if !is_local_development_environment(&service.environment)
        || allow_public_bind
        || is_loopback_http_host(&http.host)
    {
        return Ok(());
    }

    Err(AppError::validation(
        "Development auth cannot listen on a public HTTP bind by default",
        vec![ErrorDetail {
            field: Some("HTTP_HOST".to_owned()),
            reason: format!(
                "set HTTP_HOST=127.0.0.1, set APP_ENV outside local development, or set {LENSO_ALLOW_DEV_AUTH_ON_PUBLIC_BIND_ENV}=true"
            ),
        }],
    ))
}

fn is_loopback_http_host(host: &str) -> bool {
    let host = host.trim().trim_start_matches('[').trim_end_matches(']');
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
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
    use serde::Deserialize;

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
    fn local_environment_rejects_public_http_bind_without_override() {
        let service = ServiceConfig {
            environment: "local".to_owned(),
            ..ServiceConfig::default()
        };
        let http = HttpConfig {
            host: "0.0.0.0".to_owned(),
            ..HttpConfig::default()
        };

        let error = validate_dev_auth_http_bind(&service, &http, false)
            .expect_err("public bind must be explicit in local env");

        assert_eq!(error.code, crate::ErrorCode::Validation);
        assert_eq!(error.details[0].field.as_deref(), Some("HTTP_HOST"));
    }

    #[test]
    fn local_environment_allows_loopback_http_bind() {
        let service = ServiceConfig {
            environment: "local".to_owned(),
            ..ServiceConfig::default()
        };
        let http = HttpConfig {
            host: "127.0.0.1".to_owned(),
            ..HttpConfig::default()
        };

        validate_dev_auth_http_bind(&service, &http, false).expect("loopback bind is local only");
    }

    #[test]
    fn http_config_defaults_to_loopback_host() {
        assert_eq!(HttpConfig::default().host, "127.0.0.1");
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

    #[test]
    fn module_config_from_env_entry_parses_local_values() {
        let (name, config) =
            module_config_from_env_entry("LENSO_MODULE_AUTH_PASSWORD__JWT_TTL_HOURS", "12")
                .expect("module local value env should parse");

        assert_eq!(name, "auth-password");
        assert_eq!(
            config.values.get("jwt_ttl_hours"),
            Some(&serde_json::json!(12))
        );

        let (_, config) =
            module_config_from_env_entry("LENSO_MODULE_AUTH__PUBLIC_URL", "https://example.test")
                .expect("module string value env should parse");
        assert_eq!(
            config.values.get("public_url"),
            Some(&serde_json::json!("https://example.test"))
        );

        let (_, config) = module_config_from_env_entry("LENSO_MODULE_AUTH__ENABLED", "\"local\"")
            .expect("module local enabled key should parse as a value");
        assert_eq!(
            config.values.get("enabled"),
            Some(&serde_json::json!("local"))
        );
        assert_eq!(config.enabled, None);
    }

    #[test]
    fn merge_module_config_keeps_enabled_and_values() {
        let mut configs = BTreeMap::new();
        let (_, enabled) = module_config_from_env_entry("LENSO_MODULE_AUTH_ENABLED", "false")
            .expect("enabled parses");
        let (_, local) = module_config_from_env_entry("LENSO_MODULE_AUTH__PUBLIC_URL", "\"/auth\"")
            .expect("local value parses");

        merge_module_config(&mut configs, "auth".to_owned(), enabled);
        merge_module_config(&mut configs, "auth".to_owned(), local);

        let config = configs.get("auth").expect("merged module config");
        assert_eq!(config.enabled, Some(false));
        assert_eq!(
            config.values.get("public_url"),
            Some(&serde_json::json!("/auth"))
        );
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct DemoModuleLocalConfig {
        public_url: String,
        #[serde(default)]
        ttl_hours: u64,
    }

    #[test]
    fn module_config_decodes_local_values() {
        let (_, config) = module_config_from_env_entry("LENSO_MODULE_DEMO__PUBLIC_URL", "/demo")
            .expect("local value parses");

        let decoded: DemoModuleLocalConfig = config.local_config("demo").expect("decode config");

        assert_eq!(
            decoded,
            DemoModuleLocalConfig {
                public_url: "/demo".to_owned(),
                ttl_hours: 0,
            }
        );
    }

    #[test]
    fn redis_config_treats_empty_url_as_disabled() {
        assert!(RedisConfig::from_url_value(None).url.is_none());
        assert!(RedisConfig::from_url_value(Some("  ")).url.is_none());
        assert_eq!(
            RedisConfig::from_url_value(Some(" redis://localhost:6379/0 "))
                .url
                .as_deref(),
            Some("redis://localhost:6379/0")
        );
    }
}
