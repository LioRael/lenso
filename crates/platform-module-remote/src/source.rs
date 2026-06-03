use crate::admin_data::RemoteAdminDataSource;
use crate::binding::RemoteBinding;
use crate::config::RemoteModuleConfig;
use crate::protocol::RemoteManifestResponse;
use crate::response::decode_json_response;
use platform_core::{AppError, AppResult, ErrorCode};
use platform_module::{AdminSurface, Module};
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
        let request = self
            .client
            .get(format!("{}/manifest", self.config.base_url));
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

        decode_json_response(response, "manifest", false)
            .await?
            .ok_or_else(|| AppError::new(ErrorCode::NotFound, "remote module manifest not found"))
    }
}
