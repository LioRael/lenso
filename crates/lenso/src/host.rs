//! Narrow host boot helpers for Lenso applications.
//!
//! Enable the `host` feature to use these helpers.

pub use crate::ModuleManifest;
pub use lenso_bootstrap::{HostComposition, HostLinkedModule};
pub use platform_core::Migration;

/// First-party linked modules that a host can opt into explicitly.
pub mod builtins {
    pub use lenso_bootstrap::{
        auth_linked_module as auth, auth_password_linked_module as auth_password,
    };
}

/// HTTP authoring helpers for host-owned linked modules.
pub mod http {
    pub use axum::Json;
    pub use axum::extract::{Path, State};
    pub use axum::routing::{delete, get, patch, post, put};
    pub use platform_core::{AppContext, AppError, ErrorCode, RequestContext};
    pub use platform_http::responses::json;
    pub use platform_http::{
        ApiErrorResponse, ApiOpenApiRouter, ErrorResponse, HttpRequestContext, JsonBody,
        OpenApiRouter, UserActor, routes,
    };
    pub use platform_module::{
        LinkedBinding, LinkedHttpContribution, ModuleHttpMethod, ModuleHttpRoute,
    };
}

/// Common host-authoring imports for starter applications.
pub mod prelude {
    pub use crate::host::http::{
        LinkedBinding, LinkedHttpContribution, ModuleHttpMethod, ModuleHttpRoute,
    };
    pub use crate::host::{
        HostBuilder, HostComposition, HostLinkedModule, Migration, ModuleManifest, builtins,
    };
}

#[derive(Debug, Clone, Default)]
pub struct HostBuilder {
    composition: HostComposition,
}

impl HostBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn linked_module(mut self, module: HostLinkedModule) -> Self {
        self.composition.add_linked_module(module);
        self
    }

    #[must_use]
    pub fn build(self) -> HostComposition {
        self.composition
    }

    pub async fn run_api_from_env(self) -> anyhow::Result<()> {
        run_api_from_env_with_composition(self.composition).await
    }

    pub async fn run_worker_from_env(self) -> anyhow::Result<()> {
        run_worker_from_env_with_composition(self.composition).await
    }

    pub async fn run_migrations_from_env(self) -> anyhow::Result<()> {
        run_migrations_from_env_with_composition(self.composition).await
    }
}

/// Start the Lenso API host from environment configuration.
pub async fn run_api_from_env() -> anyhow::Result<()> {
    run_api_from_env_with_composition(HostComposition::default()).await
}

/// Start the Lenso API host with additional host-owned linked modules.
pub async fn run_api_from_env_with_composition(composition: HostComposition) -> anyhow::Result<()> {
    lenso_api::run_from_env_with_composition(composition).await
}

/// Start the Lenso worker from environment configuration.
pub async fn run_worker_from_env() -> anyhow::Result<()> {
    run_worker_from_env_with_composition(HostComposition::default()).await
}

/// Start the Lenso worker with additional host-owned linked modules.
pub async fn run_worker_from_env_with_composition(
    composition: HostComposition,
) -> anyhow::Result<()> {
    lenso_worker::run_from_env_with_composition(composition).await
}

/// Apply Lenso migrations from environment configuration.
pub async fn run_migrations_from_env() -> anyhow::Result<()> {
    run_migrations_from_env_with_composition(HostComposition::default()).await
}

/// Apply Lenso migrations with additional host-owned linked module migrations.
pub async fn run_migrations_from_env_with_composition(
    composition: HostComposition,
) -> anyhow::Result<()> {
    lenso_migrate::run_from_env_with_composition(composition).await
}

#[cfg(test)]
mod tests {
    use super::http::{ApiOpenApiRouter, LinkedBinding, LinkedHttpContribution};
    use super::prelude::*;

    fn merge_http(base: ApiOpenApiRouter) -> ApiOpenApiRouter {
        base
    }

    fn manifest() -> ModuleManifest {
        ModuleManifest::builder("app")
            .http_routes(vec![ModuleHttpRoute {
                method: ModuleHttpMethod::Get,
                path: "/v1/app/status".to_owned(),
                capability: None,
                display_name: Some("App Status".to_owned()),
                story_title: Some("App Status".to_owned()),
            }])
            .build()
    }

    #[test]
    fn prelude_exports_host_authoring_types() {
        let _binding = LinkedBinding::builder()
            .http(LinkedHttpContribution {
                public_prefixes: &["/v1/app/"],
                merge: merge_http,
            })
            .build();

        let _composition: HostComposition = HostBuilder::new()
            .linked_module(HostLinkedModule::manifest_only(
                "app",
                manifest,
                &[Migration {
                    name: "app/0001_init",
                    sql: "select 1;",
                }],
            ))
            .build();
    }
}
