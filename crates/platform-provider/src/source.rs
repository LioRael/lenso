use crate::admin_action::ProviderAdminActionSource;
use crate::admin_data::ProviderAdminDataSource;
use crate::binding::ProviderBinding;
use crate::config::{ProviderConfig, ProviderTransport};
use crate::protocol::{PROVIDER_PROTOCOL, ProviderDescriptor, ProviderManifestResponse};
use crate::response::{
    MAX_PROVIDER_JSON_RESPONSE_BYTES, ResponseBodyPolicy, decode_json_response_with_policy,
};
use platform_core::error::ErrorDetail;
use platform_core::{AppError, AppResult, ErrorCode};
use platform_module::{
    AdminDeclarativeComponent, AdminDeclarativeSurface, AdminSurface, Module, ModuleHttpRoute,
    ModuleManifest,
};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ProviderSource {
    client: reqwest::Client,
    config: ProviderConfig,
}

#[derive(Debug)]
pub struct LoadedProvider {
    pub module: Module,
    pub config: ProviderConfig,
}

impl ProviderSource {
    pub fn new(config: ProviderConfig) -> AppResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("failed to build Provider Service client: {error}"),
                )
            })?;
        Ok(Self { client, config })
    }

    /// Verifies the live Provider descriptor against a locked Manifest, then
    /// builds behavior exclusively from the locked copy. The endpoint can
    /// confirm identity but can never discover or replace a Module.
    pub async fn load_locked(
        &self,
        expected_service_id: &str,
        expected_service_version: &str,
        expected_service_release_digest: &str,
        expected_export_key: &str,
        expected_module_release_digest: &str,
        expected_manifest_digest: &str,
        expected_contract_digests: &[String],
        locked: &ModuleManifest,
    ) -> AppResult<LoadedProvider> {
        let descriptor = self.fetch_descriptor().await?;
        if descriptor.protocol != PROVIDER_PROTOCOL
            || descriptor.service_id != expected_service_id
            || descriptor.service_release_version != expected_service_version
            || descriptor.service_release_digest != expected_service_release_digest
        {
            return Err(AppError::new(
                ErrorCode::ExternalDependency,
                format!(
                    "Provider descriptor Service identity for '{}' differs from the locked Service Release",
                    locked.module_id
                ),
            ));
        }
        let export = descriptor
            .exports
            .into_iter()
            .find(|export| export.export_key == expected_export_key)
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::ExternalDependency,
                    format!("Provider descriptor omitted locked export '{expected_export_key}'"),
                )
            })?;
        let mut observed_contracts = export
            .contract_digests
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let mut expected_contracts = expected_contract_digests.to_vec();
        observed_contracts.sort();
        expected_contracts.sort();
        if export.module_id != locked.module_id
            || export.module_release_digest != expected_module_release_digest
            || export.manifest_digest != expected_manifest_digest
            || export.manifest != *locked
            || observed_contracts != expected_contracts
            || !export.ready
        {
            return Err(AppError::new(
                ErrorCode::ExternalDependency,
                format!(
                    "Provider export '{}' differs from the locked Module Release",
                    expected_export_key
                ),
            ));
        }

        self.load_module(locked.clone(), self.config.clone())
    }

    fn load_module(
        &self,
        manifest: ProviderManifestResponse,
        config: ProviderConfig,
    ) -> AppResult<LoadedProvider> {
        validate_provider_http_routes(&manifest.http_routes)?;
        let binding = ProviderBinding::from_surfaces(
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
        let mut module = Module::service(manifest, Arc::new(binding));
        if has_admin_data {
            module =
                module.with_admin_data(Arc::new(ProviderAdminDataSource::new(config.clone())?));
        }
        if has_admin_actions {
            module = module
                .with_admin_actions(Arc::new(ProviderAdminActionSource::new(config.clone())?));
        }
        if has_admin_queries {
            module =
                module.with_admin_queries(Arc::new(ProviderAdminDataSource::new(config.clone())?));
        }
        Ok(LoadedProvider { module, config })
    }

    async fn fetch_descriptor(&self) -> AppResult<ProviderDescriptor> {
        if self.config.transport == ProviderTransport::Grpc {
            return crate::grpc::fetch_descriptor(&self.config).await;
        }

        let request = self.client.get(&self.config.base_url);
        let request = match &self.config.auth_token {
            Some(token) => request.bearer_auth(token),
            None => request,
        };
        let response = request.send().await.map_err(|error| {
            AppError::new(
                ErrorCode::ExternalDependency,
                format!("Provider descriptor request failed: {error}"),
            )
            .retryable()
        })?;

        decode_json_response_with_policy(
            response,
            "Provider descriptor",
            false,
            ResponseBodyPolicy {
                max_bytes: Some(MAX_PROVIDER_JSON_RESPONSE_BYTES),
                require_json_content_type: true,
                allow_empty_success: false,
            },
        )
        .await?
        .ok_or_else(|| AppError::new(ErrorCode::NotFound, "Provider descriptor not found"))
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

fn validate_provider_http_routes(routes: &[ModuleHttpRoute]) -> AppResult<()> {
    let mut details = Vec::new();
    for (index, route) in routes.iter().enumerate() {
        if !is_valid_provider_http_route_path(&route.path) {
            details.push(ErrorDetail {
                field: Some(format!("http_routes.{index}.path")),
                reason: "provider HTTP route path must be module-local, start with '/', and not contain empty or '..' segments".to_owned(),
            });
        }
    }

    if details.is_empty() {
        Ok(())
    } else {
        Err(AppError::validation(
            "Service-delivered Module manifest contains invalid HTTP route declarations",
            details,
        ))
    }
}

fn is_valid_provider_http_route_path(path: &str) -> bool {
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
            capability: Some("provider_crm.contacts.read".to_owned()),
            display_name: None,
            story_title: None,
            operation: None,
        };

        assert!(validate_provider_http_routes(&[route]).is_err());
    }
}
