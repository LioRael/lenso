//! Module framework contracts: the data/behavior split a module exposes to the
//! composition root.
//!
//! - [`ModuleManifest`]: serializable data (name, story display, reserved
//!   seams). Produced by every loading source.
//! - [`ModuleBinding`]: behavior (register functions/event handlers). One impl
//!   per loading source; [`LinkedBinding`] is the compile-time one.
//! - [`Module`]: a loaded module bundling manifest + binding + internal config.
//! - [`AdminDataSource`]: the schema-admin read seam — a module's read access
//!   to its admin entities. [`AdminSchema`] is the declared admin surface data.
//! - [`AdminActionSource`]: executable behavior for manifest-declared admin
//!   actions.
//! - [`AdminQuerySource`]: read-only behavior for manifest-declared admin
//!   queries.

mod admin_data;
mod binding;
mod host;
mod linked;
mod module;
mod source;

pub use admin_data::{
    AdminActionSource, AdminDataSource, AdminListQuery, AdminPage, AdminQuerySource,
};
pub use binding::{EventHandlerRegistrationContext, EventHandlerRuntimeContext, ModuleBinding};
pub use host::{HostContribution, HostLinkedModule};
pub use lenso_contracts::{
    AdminAction, AdminActionConfirmation, AdminActionDangerLevel, AdminActionInputField,
    AdminActionInputSchema, AdminDeclarativeComponent, AdminDeclarativePage,
    AdminDeclarativeSection, AdminDeclarativeSurface, AdminEmbeddedEntry, AdminEmbeddedRuntime,
    AdminEmbeddedSurface, AdminMetricBinding, AdminPermission, AdminSandboxPolicy, AdminSchema,
    AdminSurface, CONSOLE_BRIDGE_PROTOCOL, CONSOLE_MODULE_PROTOCOL, CONSOLE_MODULE_PROTOCOL_MAJOR,
    CONSOLE_UI_ESM_FORMAT, ConsoleActionInputBinding, ConsoleActionInputValue, ConsoleContribution,
    ConsoleContributionAction, ConsoleContributionKind, ConsoleModuleManifest,
    ConsoleModuleSurface, ConsoleNavigation, ConsoleNavigationGroup, ConsolePermissionGrant,
    ConsolePermissionRequest, ConsoleSlot, ConsoleSlotContext, ConsoleSlotContextField,
    ConsoleSlotContextFieldType, ConsoleSurface, ConsoleSurfaceArea, ConsoleSurfacePresentation,
    ConsoleUiArtifact, ConsoleUiArtifactEntry, ConsoleUiArtifactFormat,
    ConsoleUiArtifactStyleAsset, ConsoleWorkspaceRef, CronParseError, CronSchedule, EntitySchema,
    EventHandlerDeclaration, EventSurface, FieldSchema, FieldType,
    LifecycleActivationJobDeclaration, LifecycleActivationRunPolicy,
    LifecycleStartupCheckDeclaration, LifecycleStartupCheckKind, LifecycleSurface,
    ModuleCapabilityReference, ModuleHttpMethod, ModuleHttpRoute, ModuleManifest,
    ModuleManifestBuilder, ModuleManifestLint, ModuleManifestLintSeverity, ModuleRequirement,
    ModuleRouteLint, ModuleRouteLintSeverity, RuntimeFunctionDeclaration,
    RuntimeRetryPolicyDeclaration, RuntimeSurface, StoryDisplayDescriptor, StoryDisplaySource,
    lint_module_http_routes, lint_module_manifest, lint_module_manifest_parts,
    module_capability_references, validate_cron_expression,
};
pub use linked::{
    LinkedBinding, LinkedBindingBuilder, LinkedHttpContribution, LinkedHttpRouteMerger,
};
pub use module::{Module, ModuleLoadStatus};
pub use source::ModuleSource;

#[cfg(test)]
mod tests {
    use super::*;
    use platform_core::{
        AppConfig, AppContext, AppError, AuthConfig, DatabaseConfig, ErrorCode, HttpConfig,
        LoggingEventPublisher, Migration, ModuleSourcesConfig, RedisConfig, ServiceConfig,
        TelemetryConfig,
    };
    use std::collections::BTreeMap;
    use std::sync::Arc;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct TestContribution(&'static str);

    const TEST_MIGRATIONS: &[Migration] = &[Migration {
        name: "test/0001_init",
        sql: "select 1;",
    }];

    fn manifest() -> ModuleManifest {
        ModuleManifest::builder("fixture/test").build()
    }

    fn module(_ctx: &AppContext) -> Module {
        Module::linked(manifest(), LinkedBinding::builder().build())
    }

    fn fallible_module(_ctx: &AppContext) -> platform_core::AppResult<Module> {
        Err(AppError::new(
            ErrorCode::ExternalDependency,
            "object storage configuration is invalid",
        ))
    }

    #[test]
    fn host_linked_module_keeps_typed_contributions() {
        let linked_module = HostLinkedModule::linked("test", manifest, module, TEST_MIGRATIONS)
            .with_contribution(TestContribution("wired"));

        let contributions = linked_module
            .contributions::<TestContribution>()
            .collect::<Vec<_>>();

        assert_eq!(contributions, vec![&TestContribution("wired")]);
    }

    #[tokio::test]
    async fn fallible_host_linked_module_preserves_structured_loader_error() {
        let linked_module =
            HostLinkedModule::try_linked("test", manifest, fallible_module, TEST_MIGRATIONS);
        let db = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
            .expect("lazy pool should build");
        let ctx = AppContext::new(
            AppConfig {
                service: ServiceConfig::default(),
                database: DatabaseConfig {
                    url: "postgres://localhost/lenso_test".to_owned(),
                    max_connections: 1,
                },
                redis: RedisConfig::default(),
                http: HttpConfig::default(),
                telemetry: TelemetryConfig::default(),
                auth: AuthConfig::default(),
                module_sources: ModuleSourcesConfig {
                    linked_profile: "core".to_owned(),
                },
                modules: BTreeMap::new(),
            },
            db,
            Arc::new(LoggingEventPublisher),
        );

        let error = linked_module
            .try_load_module(&ctx)
            .expect_err("fallible loader error must remain structured");

        assert_eq!(error.code, ErrorCode::ExternalDependency);
        assert_eq!(
            error.public_message,
            "object storage configuration is invalid"
        );
    }
}
