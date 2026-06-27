use axum::{Json, Router, routing::get};
use platform_module::ModuleManifest;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceHealth {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ready_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liveness_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceProvider {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceContract {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<ServiceProvider>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<ServiceHealth>,
    pub modules: Vec<ModuleManifest>,
}

impl ServiceContract {
    #[must_use]
    pub fn new(name: impl Into<String>, modules: Vec<ModuleManifest>) -> Self {
        Self {
            name: name.into(),
            version: None,
            provider: None,
            health: None,
            modules,
        }
    }

    #[must_use]
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    #[must_use]
    pub fn provider(mut self, provider: ServiceProvider) -> Self {
        self.provider = Some(provider);
        self
    }

    #[must_use]
    pub fn health(mut self, health: ServiceHealth) -> Self {
        self.health = Some(health);
        self
    }
}

#[must_use]
pub fn health_router() -> Router {
    Router::new()
        .route(
            "/lenso/service/v1/ready",
            get(|| async { Json(serde_json::json!({"ready": true})) }),
        )
        .route(
            "/lenso/service/v1/status",
            get(|| async { Json(serde_json::json!({"state": "ready"})) }),
        )
}
