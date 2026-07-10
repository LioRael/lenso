use crate::admin_action::RemoteAdminActionSource;
use crate::admin_data::RemoteAdminDataSource;
use crate::binding::RemoteBinding;
use crate::config::{RemoteModuleConfig, RemoteModuleTransport};
use crate::protocol::{RemoteManifestEnvelope, RemoteManifestResponse};
use crate::response::{
    MAX_REMOTE_JSON_RESPONSE_BYTES, ResponseBodyPolicy, decode_json_response_with_policy,
};
use platform_core::error::ErrorDetail;
use platform_core::{AppError, AppResult, ErrorCode};
use platform_module::{
    AdminDeclarativeComponent, AdminDeclarativeSurface, AdminSurface, Module, ModuleHttpRoute,
};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct RemoteModuleSource {
    client: reqwest::Client,
    config: RemoteModuleConfig,
}

#[derive(Debug)]
pub struct LoadedRemoteModule {
    pub module: Module,
    pub config: RemoteModuleConfig,
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
        let mut loaded = self.load_all().await?;
        if loaded.len() == 1 {
            return Ok(loaded.remove(0).module);
        }
        loaded
            .into_iter()
            .find(|loaded| loaded.module.manifest.name == self.config.name)
            .map(|loaded| loaded.module)
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::Internal,
                    format!(
                        "remote service '{}' did not provide a module named '{}'",
                        self.config.name, self.config.name
                    ),
                )
            })
    }

    pub async fn load_all(&self) -> AppResult<Vec<LoadedRemoteModule>> {
        match self.fetch_manifest().await? {
            RemoteManifestEnvelope::Module(manifest) => {
                if manifest.name != self.config.name {
                    return Err(AppError::new(
                        ErrorCode::Internal,
                        format!(
                            "remote module manifest name '{}' does not match configured name '{}'",
                            manifest.name, self.config.name
                        ),
                    ));
                }
                Ok(vec![self.load_module(manifest, self.config.clone())?])
            }
            RemoteManifestEnvelope::Service(service) => {
                if service.name != self.config.name {
                    return Err(AppError::new(
                        ErrorCode::Internal,
                        format!(
                            "remote service manifest name '{}' does not match configured name '{}'",
                            service.name, self.config.name
                        ),
                    ));
                }
                service
                    .modules
                    .into_iter()
                    .map(|manifest| {
                        let config = self.config.for_service_module(&manifest.name);
                        self.load_module(manifest, config)
                    })
                    .collect()
            }
        }
    }

    fn load_module(
        &self,
        manifest: RemoteManifestResponse,
        config: RemoteModuleConfig,
    ) -> AppResult<LoadedRemoteModule> {
        validate_remote_http_routes(&manifest.http_routes)?;
        let binding = RemoteBinding::from_surfaces(
            config.clone(),
            manifest.runtime.as_ref(),
            manifest.events.as_ref(),
        )?;

        let has_admin_data = match &manifest.admin {
            Some(AdminSurface::Schema(_)) => true,
            Some(AdminSurface::DeclarativeCustom(surface)) => surface.fallback_schema.is_some(),
            _ => false,
        };
        let has_admin_actions = matches!(
            &manifest.admin,
            Some(AdminSurface::DeclarativeCustom(surface)) if !surface.actions.is_empty()
        );
        let has_admin_queries = matches!(
            &manifest.admin,
            Some(AdminSurface::DeclarativeCustom(surface)) if has_query_value_component(surface)
        );
        let mut module = Module::remote(manifest, Arc::new(binding));
        if has_admin_data {
            module = module.with_admin_data(Arc::new(RemoteAdminDataSource::new(config.clone())?));
        }
        if has_admin_actions {
            module =
                module.with_admin_actions(Arc::new(RemoteAdminActionSource::new(config.clone())?));
        }
        if has_admin_queries {
            module =
                module.with_admin_queries(Arc::new(RemoteAdminDataSource::new(config.clone())?));
        }
        Ok(LoadedRemoteModule { module, config })
    }

    async fn fetch_manifest(&self) -> AppResult<RemoteManifestEnvelope> {
        if self.config.transport == RemoteModuleTransport::Grpc {
            return crate::grpc::fetch_manifest(&self.config)
                .await
                .map(RemoteManifestEnvelope::Module);
        }

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

        decode_json_response_with_policy(
            response,
            "manifest",
            false,
            ResponseBodyPolicy {
                max_bytes: Some(MAX_REMOTE_JSON_RESPONSE_BYTES),
                require_json_content_type: true,
                allow_empty_success: false,
            },
        )
        .await?
        .ok_or_else(|| AppError::new(ErrorCode::NotFound, "remote module manifest not found"))
    }
}

fn has_query_value_component(surface: &AdminDeclarativeSurface) -> bool {
    surface.pages.iter().any(|page| {
        page.sections.iter().any(|section| {
            matches!(
                section.component,
                AdminDeclarativeComponent::QueryValue { .. }
            )
        })
    })
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
        && !path.contains('\\')
        && !path.contains("://")
        && !path.contains('?')
        && !path.contains('#')
        && path
            .split('/')
            .skip(1)
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

#[cfg(test)]
mod tests {
    use super::*;
    use platform_module::{ModuleHttpMethod, ModuleHttpRoute};

    #[test]
    fn manifest_routes_reject_backslashes() {
        let route = ModuleHttpRoute {
            method: ModuleHttpMethod::Get,
            path: "/contacts\\..\\admin".to_owned(),
            capability: Some("remote_crm.contacts.read".to_owned()),
            display_name: None,
            story_title: None,
            operation: None,
        };

        assert!(validate_remote_http_routes(&[route]).is_err());
    }
}
