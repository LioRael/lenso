//! Composition root: the single place that knows which modules exist.
//!
//! Both the API and the worker assemble their module wiring from this crate, so
//! a module is registered here once rather than in scattered per-app edits.
//!
//! A module's contributions are split by how they are consumed:
//! - [`modules`]: context-bound bindings (runtime functions + event handlers)
//!   and runtime config (API + worker). The authoritative module list.
//! - [`module_manifests`]: context-free manifest data (no [`AppContext`]) for
//!   read-only / `OpenAPI` paths.
//! - [`merge_linked_http`]: context-free HTTP routes and their OpenAPI docs
//!   (API only), assembled without a live [`AppContext`].
//! - [`story_display_descriptors`]: console display metadata, sourced from the
//!   context-free [`module_manifests`].
//!
//! When adding a module, register it in [`modules`] and [`module_manifests`]
//! and — if it has them — in [`merge_linked_http`].

use platform_admin_data::{AdminModule, AdminModuleMetadata};
use platform_core::error::ErrorDetail;
use platform_core::{
    ActorContext, AppContext, AppError, CorrelationId, EventHandlerRegistry,
    RuntimeConfigDescriptor, StoryDisplayDescriptor, TraceContext,
};
use platform_http::ApiOpenApiRouter;
use platform_module::{
    AdminSchema, AdminSurface, LifecycleActivationRunPolicy, LinkedBinding, Module,
    ModuleLoadStatus, ModuleManifest, ModuleSource,
};
use platform_module_remote::{RemoteHttpProxyRegistry, RemoteModuleConfig, RemoteModuleSource};
use platform_runtime::{EnqueueFunctionRequest, FunctionRegistry, RuntimeClient};

struct LinkedModuleEntry {
    module_name: &'static str,
    manifest: fn() -> ModuleManifest,
    load: fn(&AppContext) -> Module,
    http_binding: Option<fn() -> LinkedBinding>,
}

const LINKED_MODULE_ENTRIES: &[LinkedModuleEntry] = &[
    LinkedModuleEntry {
        module_name: "identity",
        manifest: identity::module::manifest,
        load: identity::module::module,
        http_binding: Some(identity::module::binding),
    },
    LinkedModuleEntry {
        module_name: "notifications",
        manifest: notifications::module::manifest,
        load: notifications::module::module,
        http_binding: None,
    },
];

/// The authoritative list of loaded modules (context-bound: builds bindings).
///
/// The only function that enumerates concrete modules for the running apps.
#[must_use]
pub fn modules(ctx: &AppContext) -> Vec<Module> {
    LINKED_MODULE_ENTRIES
        .iter()
        .map(|entry| (entry.load)(ctx))
        .collect()
}

/// Load every configured module, including out-of-process remote modules.
///
/// The synchronous [`modules`] function remains Linked-only for call sites that
/// must stay context-local and infallible. Startup paths that can perform IO
/// should use this async loader.
pub async fn load_modules(ctx: &AppContext) -> platform_core::AppResult<Vec<Module>> {
    let mut loaded = modules(ctx);

    for remote in &ctx.config.module_sources.remote {
        let source = RemoteModuleSource::new(remote_module_config(remote))?;
        loaded.push(source.load().await?);
    }

    Ok(loaded)
}

/// Context-free module manifests for read-only / OpenAPI paths that have no
/// [`AppContext`]. Kept in sync with [`modules`] by listing the same modules.
#[must_use]
pub fn module_manifests() -> Vec<ModuleManifest> {
    LINKED_MODULE_ENTRIES
        .iter()
        .map(|entry| (entry.manifest)())
        .collect()
}

/// Runtime function declaration sources for context-free linked modules.
#[must_use]
pub fn linked_runtime_function_declaration_sources() -> Vec<(
    String,
    ModuleSource,
    Option<platform_module::RuntimeSurface>,
)> {
    module_manifests()
        .into_iter()
        .map(|manifest| (manifest.name, ModuleSource::Linked, manifest.runtime))
        .collect()
}

/// Runtime function declaration sources from loaded module metadata, including
/// configured remote modules.
#[must_use]
pub fn runtime_function_declaration_sources_from_metadata(
    modules: &[AdminModuleMetadata],
) -> Vec<(
    String,
    ModuleSource,
    Option<platform_module::RuntimeSurface>,
)> {
    modules
        .iter()
        .map(|module| {
            (
                module.module_name.clone(),
                module.source,
                module.runtime.clone(),
            )
        })
        .collect()
}

/// Public HTTP path ownership for linked modules.
///
/// Projected from context-free linked modules so OpenAPI guards and router
/// assembly consume the same source-specific binding data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedHttpRouteOwner {
    pub module_name: String,
    pub public_prefixes: &'static [&'static str],
}

#[must_use]
pub fn linked_http_route_owners() -> Vec<LinkedHttpRouteOwner> {
    LINKED_MODULE_ENTRIES
        .iter()
        .filter_map(|entry| {
            let http = entry.http_binding?().http?;
            Some(LinkedHttpRouteOwner {
                module_name: entry.module_name.to_owned(),
                public_prefixes: http.public_prefixes,
            })
        })
        .collect()
}

/// Context-free linked modules that contribute Axum/OpenAPI HTTP routers.
#[must_use]
pub fn linked_http_modules() -> Vec<Module> {
    LINKED_MODULE_ENTRIES
        .iter()
        .filter_map(|entry| {
            let http_binding = entry.http_binding?;
            Some(Module::linked((entry.manifest)(), http_binding()))
        })
        .collect()
}

/// Aggregate schema-admin capable modules: those declaring an
/// `AdminSurface::Schema` AND providing an `AdminDataSource`. Modules without a
/// schema data source are filtered out — "optional capability" semantics.
#[must_use]
pub fn admin_modules(ctx: &AppContext) -> Vec<AdminModule> {
    admin_modules_from_modules(modules(ctx))
}

/// Load schema-admin capable modules, including configured remotes.
pub async fn load_admin_modules(ctx: &AppContext) -> platform_core::AppResult<Vec<AdminModule>> {
    let mut admin_modules = admin_modules_from_modules(modules(ctx));

    for remote in &ctx.config.module_sources.remote {
        let source = RemoteModuleSource::new(remote_module_config(remote))?;
        match source.load().await {
            Ok(module) => admin_modules.extend(admin_modules_from_modules(vec![module])),
            Err(error) => admin_modules.push(failed_remote_admin_module(
                remote.name.clone(),
                error.public_message,
            )),
        }
    }

    Ok(admin_modules)
}

/// Load registry metadata for every configured module, including modules with
/// no admin surface and custom surfaces not consumable by schema-admin
/// list/detail.
pub async fn load_admin_module_metadata(
    ctx: &AppContext,
) -> platform_core::AppResult<Vec<AdminModuleMetadata>> {
    let mut metadata = admin_metadata_from_modules(modules(ctx));

    for remote in &ctx.config.module_sources.remote {
        let source = RemoteModuleSource::new(remote_module_config(remote))?;
        match source.load().await {
            Ok(module) => metadata.extend(admin_metadata_from_modules(vec![module])),
            Err(error) => metadata.push(failed_remote_admin_metadata(
                remote.name.clone(),
                error.public_message,
            )),
        }
    }

    Ok(metadata)
}

pub async fn load_remote_http_proxy_registry(
    ctx: &AppContext,
) -> platform_core::AppResult<RemoteHttpProxyRegistry> {
    let mut remote_modules = Vec::new();
    let mut remote_configs = Vec::new();

    for remote in &ctx.config.module_sources.remote {
        let config = remote_module_config(remote);
        let source = RemoteModuleSource::new(config.clone())?;
        if let Ok(module) = source.load().await {
            remote_modules.push(module);
            remote_configs.push(config);
        }
    }

    Ok(RemoteHttpProxyRegistry::from_modules(
        &remote_modules,
        &remote_configs,
    ))
}

fn admin_modules_from_modules(modules: Vec<Module>) -> Vec<AdminModule> {
    modules
        .into_iter()
        .filter_map(|module| {
            // `modules(ctx)` yields owned Modules — move the fields out.
            let data_source = module.admin_data?;
            let ModuleManifest { name, admin, .. } = module.manifest;
            let (schema, listed_in_schema) = match admin? {
                AdminSurface::Schema(schema) => (schema, true),
                AdminSurface::DeclarativeCustom(surface) => (surface.fallback_schema?, false),
                AdminSurface::EmbeddedCustom(_) => return None,
                _ => return None,
            };
            Some(AdminModule {
                module_name: name,
                source: module.source,
                load_status: module.load_status,
                schema,
                listed_in_schema,
                data_source: Some(data_source),
            })
        })
        .collect()
}

fn admin_metadata_from_modules(modules: Vec<Module>) -> Vec<AdminModuleMetadata> {
    modules
        .into_iter()
        .map(|module| {
            let ModuleManifest {
                name,
                admin,
                http_routes,
                runtime,
                lifecycle,
                story_display,
                capabilities,
                ..
            } = module.manifest;
            AdminModuleMetadata {
                module_name: name,
                source: module.source,
                load_status: module.load_status,
                http_routes,
                runtime,
                lifecycle,
                story_display,
                capabilities,
                admin,
            }
        })
        .collect()
}

fn failed_remote_admin_module(name: String, message: String) -> AdminModule {
    AdminModule {
        module_name: name,
        source: ModuleSource::Remote,
        load_status: ModuleLoadStatus::Error { message },
        schema: AdminSchema {
            entities: Vec::new(),
        },
        listed_in_schema: true,
        data_source: None,
    }
}

fn failed_remote_admin_metadata(name: String, message: String) -> AdminModuleMetadata {
    AdminModuleMetadata {
        module_name: name,
        source: ModuleSource::Remote,
        load_status: ModuleLoadStatus::Error { message },
        http_routes: Vec::new(),
        runtime: None,
        lifecycle: None,
        story_display: Vec::new(),
        capabilities: Vec::new(),
        admin: None,
    }
}

fn remote_module_config(source: &platform_core::RemoteModuleSourceConfig) -> RemoteModuleConfig {
    let mut config = RemoteModuleConfig::new(source.name.clone(), source.base_url.clone())
        .with_timeout_ms(source.timeout_ms);

    if let Some(env_name) = &source.auth_token_env {
        if let Ok(token) = std::env::var(env_name) {
            config = config.with_auth_token(token);
        }
    }

    config
}

/// Build a [`FunctionRegistry`] from every module's binding.
#[must_use]
pub fn function_registry(modules: &[Module]) -> FunctionRegistry {
    let mut registry = FunctionRegistry::default();
    for module in modules {
        module.binding.register_functions(&mut registry);
    }
    registry
}

/// Validate and enqueue every startup activation job declared by loaded modules.
///
/// Lifecycle activation is host-owned: module manifests declare the work, and
/// the app bootstrap validates those declarations against the runtime registry
/// before scheduling function runs.
pub async fn enqueue_lifecycle_activation_jobs(
    ctx: &AppContext,
    modules: &[Module],
    registry: &FunctionRegistry,
) -> platform_core::AppResult<Vec<String>> {
    validate_lifecycle_activation_jobs(modules, registry)?;

    let client = RuntimeClient::new(ctx.db.clone());
    let mut run_ids = Vec::new();

    for module in modules {
        let Some(lifecycle) = &module.manifest.lifecycle else {
            continue;
        };

        for job in &lifecycle.activation_jobs {
            if job.run_policy != LifecycleActivationRunPolicy::EveryStartup {
                continue;
            }

            let Some(definition) = registry.get(&job.function_name) else {
                continue;
            };

            run_ids.push(
                client
                    .enqueue_function(EnqueueFunctionRequest {
                        function_name: job.function_name.clone(),
                        input_json: job.input.clone(),
                        correlation_id: CorrelationId::new(ctx.ids.new_id("corr_lifecycle")),
                        actor: ActorContext::Service {
                            service_id: "worker".to_owned(),
                            scopes: vec!["runtime.functions.enqueue".to_owned()],
                        },
                        trace: TraceContext::default(),
                        causation_id: Some(format!(
                            "module_lifecycle:{}:{}",
                            module.manifest.name, job.name
                        )),
                        max_attempts: Some(definition.retry_policy.max_attempts as i32),
                    })
                    .await?,
            );
        }
    }

    Ok(run_ids)
}

fn validate_lifecycle_activation_jobs(
    modules: &[Module],
    registry: &FunctionRegistry,
) -> platform_core::AppResult<()> {
    for module in modules {
        let Some(lifecycle) = &module.manifest.lifecycle else {
            continue;
        };

        for job in &lifecycle.activation_jobs {
            if job.run_policy != LifecycleActivationRunPolicy::EveryStartup {
                continue;
            }
            if !job.required || registry.get(&job.function_name).is_some() {
                continue;
            }

            return Err(AppError::validation(
                "Lifecycle activation job references an unregistered runtime function",
                vec![ErrorDetail {
                    field: Some(format!(
                        "module.{}.lifecycle.activation_jobs.{}",
                        module.manifest.name, job.name
                    )),
                    reason: format!(
                        "required activation job `{}` references missing function `{}`",
                        job.name, job.function_name
                    ),
                }],
            ));
        }
    }

    Ok(())
}

/// Build an [`EventHandlerRegistry`] from every module's binding.
#[must_use]
pub fn event_handlers(modules: &[Module]) -> EventHandlerRegistry {
    let mut registry = EventHandlerRegistry::new();
    for module in modules {
        module.binding.register_event_handlers(&mut registry);
    }
    registry
}

/// Merge every linked module's HTTP routes (and their `OpenAPI` docs) onto `base`.
///
/// Linked route builders are context-free, so this assembles the HTTP surface
/// without constructing the full module set (which requires an [`AppContext`])
/// — usable both for serving and for standalone `OpenAPI` document assembly.
/// This is the single source for linked API routes until HTTP joins the
/// [`platform_module::ModuleBinding`] seam.
pub fn merge_linked_http(base: ApiOpenApiRouter) -> ApiOpenApiRouter {
    linked_http_modules()
        .into_iter()
        .filter_map(|module| module.linked_http)
        .fold(base, |router, contribution| (contribution.merge)(router))
}

/// Story-display descriptors for every module. Sourced from context-free
/// manifests so the `OpenAPI` path stays pure (no [`AppContext`]).
#[must_use]
pub fn story_display_descriptors() -> Vec<StoryDisplayDescriptor> {
    module_manifests()
        .into_iter()
        .flat_map(|manifest| manifest.story_display)
        .collect()
}

/// Every module's setting descriptors.
///
/// The single source for the editable configuration registry. Apps build a
/// `RuntimeConfigRegistry` from this list at startup.
#[must_use]
pub fn runtime_config_descriptors(ctx: &AppContext) -> Vec<RuntimeConfigDescriptor> {
    let module_descriptors = modules(ctx)
        .iter()
        .flat_map(|module| module.runtime_config.iter().cloned())
        .collect::<Vec<_>>();
    // Platform-owned descriptors (e.g. worker knobs) plus every module's; keys
    // are globally unique, so chain order is presentation-only.
    platform_core::worker_runtime_config::RUNTIME_CONFIG
        .iter()
        .cloned()
        .chain(module_descriptors)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use platform_core::{
        AppConfig, AuthConfig, DatabaseConfig, ErrorCode, ExecutionContext, HttpConfig,
        LoggingEventPublisher, ModuleSourcesConfig, PLATFORM_MIGRATIONS, ServiceConfig,
        TelemetryConfig, apply_migrations,
    };
    use platform_module::{LifecycleActivationJobDeclaration, LifecycleSurface};
    use platform_runtime::{FunctionDefinition, FunctionHandler, RUNTIME_MIGRATIONS, RetryPolicy};
    use platform_testing::{SequentialIdGenerator, TestDatabase};
    use serde_json::{Value, json};
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn linked_module_entry_names_match_manifests() {
        for entry in LINKED_MODULE_ENTRIES {
            assert_eq!(
                entry.module_name,
                (entry.manifest)().name,
                "linked module entry name must match ModuleManifest::name"
            );
        }
    }

    #[test]
    fn linked_http_route_owners_are_projected_from_modules() {
        assert_eq!(
            linked_http_route_owners(),
            vec![LinkedHttpRouteOwner {
                module_name: "identity".to_owned(),
                public_prefixes: &["/v1/identity/"],
            }]
        );
    }

    #[test]
    fn linked_http_bindings_are_declared_in_manifests() {
        for module in linked_http_modules() {
            let http = module
                .linked_http
                .expect("linked HTTP module should carry HTTP contribution");
            assert!(
                !module.manifest.http_routes.is_empty(),
                "linked HTTP module `{}` must declare ModuleManifest::http_routes",
                module.manifest.name
            );
            for route in &module.manifest.http_routes {
                assert!(
                    http.public_prefixes
                        .iter()
                        .any(|prefix| route.path.starts_with(prefix)),
                    "linked HTTP module `{}` declares manifest route `{}` outside its public prefixes",
                    module.manifest.name,
                    route.path
                );
            }
        }
    }

    #[test]
    fn linked_http_modules_are_registered_modules() {
        let manifests = module_manifests();

        for module in linked_http_modules() {
            let registered_manifest = manifests
                .iter()
                .find(|manifest| manifest.name == module.manifest.name)
                .unwrap_or_else(|| {
                    panic!(
                        "linked HTTP module `{}` is missing from module_manifests",
                        module.manifest.name
                    )
                });
            assert_eq!(
                registered_manifest, &module.manifest,
                "linked HTTP module `{}` must use the registered ModuleManifest",
                module.manifest.name
            );
        }
    }

    #[tokio::test]
    async fn lifecycle_activation_enqueue_creates_function_run() {
        let Some(db) = TestDatabase::create().await else {
            return;
        };
        apply_runtime_stack_migrations(&db).await;

        let mut ctx = AppContext::new(
            test_config(&db),
            db.pool.clone(),
            Arc::new(LoggingEventPublisher),
        );
        ctx.ids = Arc::new(SequentialIdGenerator::default());
        let modules = vec![test_lifecycle_module(lifecycle_activation_job(
            true,
            json!({ "warm": "cache" }),
        ))];
        let registry = registry_with_lifecycle_function(7);

        let run_ids = enqueue_lifecycle_activation_jobs(&ctx, &modules, &registry)
            .await
            .expect("lifecycle activation job should enqueue");

        assert_eq!(run_ids.len(), 1);
        let row = sqlx::query_as::<_, (String, Value, i32, String, Value)>(
            r#"
            select function_name, input_json, max_attempts, correlation_id, actor
            from runtime.function_runs
            where id = $1
            "#,
        )
        .bind(&run_ids[0])
        .fetch_one(&db.pool)
        .await
        .expect("function run should exist");

        assert_eq!(row.0, LIFECYCLE_FUNCTION_NAME);
        assert_eq!(row.1["warm"], "cache");
        assert_eq!(
            row.1["_lenso_runtime"]["correlation_id"],
            "corr_lifecycle_1"
        );
        assert_eq!(
            row.1["_lenso_runtime"]["causation_id"],
            "module_lifecycle:test-module:warm cache"
        );
        assert_eq!(row.2, 7);
        assert_eq!(row.3, "corr_lifecycle_1");
        assert_eq!(row.4["kind"], "service");
        assert_eq!(row.4["service_id"], "worker");
        assert_eq!(row.4["scopes"][0], "runtime.functions.enqueue");

        db.cleanup().await;
    }

    #[test]
    fn lifecycle_activation_validation_rejects_required_missing_function() {
        let modules = vec![test_lifecycle_module(lifecycle_activation_job(
            true,
            Value::Null,
        ))];
        let registry = FunctionRegistry::default();

        let error = validate_lifecycle_activation_jobs(&modules, &registry)
            .expect_err("required missing activation function should fail validation");

        assert_eq!(error.code, ErrorCode::Validation);
        assert_eq!(
            error.details[0].field.as_deref(),
            Some("module.test-module.lifecycle.activation_jobs.warm cache")
        );
        assert!(
            error.details[0].reason.contains("test.warm_cache.v1"),
            "validation detail should name the missing function"
        );
    }

    #[tokio::test]
    async fn optional_missing_lifecycle_activation_is_skipped() {
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let ctx = AppContext::new(
            test_config_with_database_url("postgres://localhost/lenso_test"),
            db,
            Arc::new(LoggingEventPublisher),
        );
        let modules = vec![test_lifecycle_module(lifecycle_activation_job(
            false,
            Value::Null,
        ))];
        let registry = FunctionRegistry::default();

        let run_ids = enqueue_lifecycle_activation_jobs(&ctx, &modules, &registry)
            .await
            .expect("optional missing activation function should be skipped");

        assert!(run_ids.is_empty());
    }

    const LIFECYCLE_FUNCTION_NAME: &str = "test.warm_cache.v1";

    #[derive(Debug)]
    struct NoopFunctionHandler;

    #[async_trait]
    impl FunctionHandler for NoopFunctionHandler {
        async fn call(
            &self,
            _ctx: ExecutionContext,
            _input: Value,
        ) -> platform_core::AppResult<Value> {
            Ok(Value::Null)
        }
    }

    fn lifecycle_activation_job(required: bool, input: Value) -> LifecycleActivationJobDeclaration {
        LifecycleActivationJobDeclaration {
            name: "warm cache".to_owned(),
            function_name: LIFECYCLE_FUNCTION_NAME.to_owned(),
            run_policy: LifecycleActivationRunPolicy::EveryStartup,
            input,
            required,
        }
    }

    fn test_lifecycle_module(job: LifecycleActivationJobDeclaration) -> Module {
        let manifest = ModuleManifest::builder("test-module")
            .lifecycle(LifecycleSurface {
                startup_checks: Vec::new(),
                activation_jobs: vec![job],
            })
            .build();
        Module::linked(manifest, LinkedBinding::builder().build())
    }

    fn registry_with_lifecycle_function(max_attempts: u32) -> FunctionRegistry {
        let mut registry = FunctionRegistry::default();
        registry.register(FunctionDefinition {
            name: LIFECYCLE_FUNCTION_NAME.to_owned(),
            version: 1,
            queue: "test".to_owned(),
            retry_policy: RetryPolicy::fixed(max_attempts, Duration::ZERO),
            handler: Arc::new(NoopFunctionHandler),
        });
        registry
    }

    fn test_config(db: &TestDatabase) -> AppConfig {
        test_config_with_database_url(db.url.clone())
    }

    fn test_config_with_database_url(database_url: impl Into<String>) -> AppConfig {
        AppConfig {
            service: ServiceConfig::default(),
            database: DatabaseConfig {
                url: database_url.into(),
                max_connections: 5,
            },
            http: HttpConfig::default(),
            telemetry: TelemetryConfig::default(),
            auth: AuthConfig::default(),
            module_sources: ModuleSourcesConfig::default(),
            modules: BTreeMap::new(),
        }
    }

    async fn apply_runtime_stack_migrations(db: &TestDatabase) {
        let migrations = PLATFORM_MIGRATIONS
            .iter()
            .chain(RUNTIME_MIGRATIONS)
            .copied()
            .collect::<Vec<_>>();
        apply_migrations(&db.pool, &migrations)
            .await
            .expect("platform and runtime migrations should apply");
    }
}
