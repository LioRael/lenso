use crate::admin_data::RemoteAdminDataSource;
use crate::binding::RemoteBinding;
use crate::config::RemoteModuleConfig;
use crate::protocol::RemoteManifestResponse;
use crate::response::decode_json_response;
use platform_core::error::ErrorDetail;
use platform_core::{AppError, AppResult, ErrorCode};
use platform_module::{AdminSurface, Module, ModuleHttpRoute};
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
        validate_remote_http_routes(&manifest.http_routes)?;
        let binding =
            RemoteBinding::from_runtime_surface(self.config.clone(), manifest.runtime.as_ref())?;

        let has_admin_data = match &manifest.admin {
            Some(AdminSurface::Schema(_)) => true,
            Some(AdminSurface::DeclarativeCustom(surface)) => surface.fallback_schema.is_some(),
            _ => false,
        };
        let mut module = Module::remote(manifest, Arc::new(binding));
        if has_admin_data {
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

fn validate_remote_http_routes(routes: &[ModuleHttpRoute]) -> AppResult<()> {
    let mut details = Vec::new();
    for (index, route) in routes.iter().enumerate() {
        if !is_valid_remote_http_route_path(&route.path) {
            details.push(ErrorDetail {
                field: Some(format!("http_routes.{index}.path")),
                reason: "remote HTTP route path must be module-local, start with '/', and not contain empty or '..' segments".to_owned(),
            });
        }
    }

    if details.is_empty() {
        Ok(())
    } else {
        Err(AppError::validation(
            "remote module manifest contains invalid HTTP route declarations",
            details,
        ))
    }
}

fn is_valid_remote_http_route_path(path: &str) -> bool {
    path.starts_with('/')
        && !path.starts_with("//")
        && !path.contains("://")
        && !path.contains('?')
        && !path.contains('#')
        && path
            .split('/')
            .skip(1)
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}
