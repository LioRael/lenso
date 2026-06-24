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
    AdminSurface, ConsoleArea, ConsoleNavigation, ConsoleNavigationGroup, ConsolePackage,
    ConsoleSurface, ConsoleWorkspaceRef, CronParseError, CronSchedule, EntitySchema,
    EventHandlerDeclaration, EventSurface, FieldSchema, FieldType,
    LifecycleActivationJobDeclaration, LifecycleActivationRunPolicy,
    LifecycleStartupCheckDeclaration, LifecycleStartupCheckKind, LifecycleSurface,
    ModuleCapabilityReference, ModuleHttpMethod, ModuleHttpRoute, ModuleManifest,
    ModuleManifestBuilder, ModuleManifestLint, ModuleManifestLintSeverity, ModuleRouteLint,
    ModuleRouteLintSeverity, ModuleSource, RuntimeFunctionDeclaration,
    RuntimeRetryPolicyDeclaration, RuntimeSurface, StoryDisplayDescriptor, StoryDisplaySource,
    lint_module_http_routes, lint_module_manifest, lint_module_manifest_parts,
    module_capability_references, validate_cron_expression,
};
pub use linked::{
    LinkedBinding, LinkedBindingBuilder, LinkedHttpContribution, LinkedHttpRouteMerger,
};
pub use module::{Module, ModuleLoadStatus};

#[cfg(test)]
mod tests {
    use super::*;
    use platform_core::{AppContext, Migration};

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct TestContribution(&'static str);

    const TEST_MIGRATIONS: &[Migration] = &[Migration {
        name: "test/0001_init",
        sql: "select 1;",
    }];

    fn manifest() -> ModuleManifest {
        ModuleManifest::builder("test").build()
    }

    fn module(_ctx: &AppContext) -> Module {
        Module::linked(manifest(), LinkedBinding::builder().build())
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
}
