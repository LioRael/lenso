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

mod admin;
mod admin_data;
mod admin_schema;
mod binding;
mod console;
mod http;
mod lifecycle;
mod linked;
mod manifest;
mod module;
mod runtime;

pub use admin::{
    AdminAction, AdminActionConfirmation, AdminActionDangerLevel, AdminActionInputField,
    AdminActionInputSchema, AdminDeclarativeComponent, AdminDeclarativePage,
    AdminDeclarativeSection, AdminDeclarativeSurface, AdminEmbeddedEntry, AdminEmbeddedRuntime,
    AdminEmbeddedSurface, AdminMetricBinding, AdminPermission, AdminSandboxPolicy, AdminSurface,
};
pub use admin_data::{AdminActionSource, AdminDataSource, AdminListQuery, AdminPage};
pub use admin_schema::{AdminSchema, EntitySchema, FieldSchema, FieldType};
pub use binding::ModuleBinding;
pub use console::{ConsoleArea, ConsolePackage, ConsoleSurface};
pub use http::{
    ModuleHttpMethod, ModuleHttpRoute, ModuleRouteLint, ModuleRouteLintSeverity,
    lint_module_http_routes,
};
pub use lifecycle::{
    LifecycleActivationJobDeclaration, LifecycleActivationRunPolicy,
    LifecycleStartupCheckDeclaration, LifecycleStartupCheckKind, LifecycleSurface,
};
pub use linked::{
    LinkedBinding, LinkedBindingBuilder, LinkedHttpContribution, LinkedHttpRouteMerger,
};
pub use manifest::{
    ModuleCapabilityReference, ModuleManifest, ModuleManifestBuilder, ModuleManifestLint,
    ModuleManifestLintSeverity, lint_module_manifest, lint_module_manifest_parts,
    module_capability_references,
};
pub use module::{Module, ModuleLoadStatus, ModuleSource};
pub use runtime::{RuntimeFunctionDeclaration, RuntimeRetryPolicyDeclaration, RuntimeSurface};
