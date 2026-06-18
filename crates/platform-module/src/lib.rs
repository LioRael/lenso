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

mod admin_data;
mod binding;
mod linked;
mod module;

pub use admin_data::{AdminActionSource, AdminDataSource, AdminListQuery, AdminPage};
pub use binding::{EventHandlerRegistrationContext, EventHandlerRuntimeContext, ModuleBinding};
pub use lenso_contracts::{
    AdminAction, AdminActionConfirmation, AdminActionDangerLevel, AdminActionInputField,
    AdminActionInputSchema, AdminDeclarativeComponent, AdminDeclarativePage,
    AdminDeclarativeSection, AdminDeclarativeSurface, AdminEmbeddedEntry, AdminEmbeddedRuntime,
    AdminEmbeddedSurface, AdminMetricBinding, AdminPermission, AdminSandboxPolicy, AdminSchema,
    AdminSurface, ConsoleArea, ConsoleNavigation, ConsoleNavigationGroup, ConsolePackage,
    ConsoleSurface, ConsoleWorkspaceRef, EntitySchema, EventHandlerDeclaration, EventSurface,
    FieldSchema, FieldType, LifecycleActivationJobDeclaration, LifecycleActivationRunPolicy,
    LifecycleStartupCheckDeclaration, LifecycleStartupCheckKind, LifecycleSurface,
    ModuleCapabilityReference, ModuleHttpMethod, ModuleHttpRoute, ModuleManifest,
    ModuleManifestBuilder, ModuleManifestLint, ModuleManifestLintSeverity, ModuleRouteLint,
    ModuleRouteLintSeverity, ModuleSource, RuntimeFunctionDeclaration,
    RuntimeRetryPolicyDeclaration, RuntimeSurface, StoryDisplayDescriptor, StoryDisplaySource,
    lint_module_http_routes, lint_module_manifest, lint_module_manifest_parts,
    module_capability_references,
};
pub use linked::{
    LinkedBinding, LinkedBindingBuilder, LinkedHttpContribution, LinkedHttpRouteMerger,
};
pub use module::{Module, ModuleLoadStatus};
