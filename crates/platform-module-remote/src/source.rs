use crate::admin_data::RemoteAdminDataSource;
use crate::binding::RemoteBinding;
use crate::config::RemoteModuleConfig;
use crate::protocol::RemoteManifestResponse;
use platform_core::{AppError, AppResult, ErrorCode};
use platform_module::{AdminSurface, Module};
use reqwest::StatusCode;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct RemoteModuleSource {
    client: reqwest::Client,
    config: RemoteModuleConfig,
}

impl RemoteModuleSource {
    pub fn new(config: RemoteModuleConfig) -> AppResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("failed to build remote module client: {error}"),
                )
            })?;
        Ok(Self { client, config })
    }

    pub async fn load(&self) -> AppResult<Module> {
        let manifest = self.fetch_manifest().await?;
        if manifest.name != self.config.name {
            return Err(AppError::new(
                ErrorCode::Internal,
                format!(
                    "remote module manifest name '{}' does not match configured name '{}'",
                    manifest.name, self.config.name
                ),
            ));
        }

        let has_schema_admin = matches!(&manifest.admin, Some(AdminSurface::Schema(_)));
        let mut module = Module::remote(manifest, Arc::new(RemoteBinding));
        if has_schema_admin {
            module =
                module.with_admin_data(Arc::new(RemoteAdminDataSource::new(self.config.clone())?));
        }
        Ok(module)
    }

    async fn fetch_manifest(&self) -> AppResult<RemoteManifestResponse> {
        let request = self.client.get(format!("{}/manifest", self.config.base_url));
        let request = match &self.config.auth_token {
            Some(token) => request.bearer_auth(token),
            None => request,
        };
        let response = request.send().await.map_err(|error| {
            AppError::new(
                ErrorCode::ExternalDependency,
                format!("remote manifest request failed: {error}"),
            )
            .retryable()
        })?;

        if response.status() == StatusCode::NOT_FOUND {
            return Err(AppError::new(
                ErrorCode::NotFound,
                "remote module manifest not found",
            ));
        }
        if !response.status().is_success() {
            return Err(AppError::new(
                ErrorCode::ExternalDependency,
                format!("remote manifest returned status {}", response.status()),
            )
            .retryable());
        }

        response
            .json::<RemoteManifestResponse>()
            .await
            .map_err(|error| {
                AppError::new(
                    ErrorCode::ExternalDependency,
                    format!("remote manifest response was invalid JSON: {error}"),
                )
            })
    }
}
