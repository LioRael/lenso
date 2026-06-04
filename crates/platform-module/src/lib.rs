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

mod admin;
mod admin_data;
mod admin_schema;
mod binding;
mod http;
mod linked;
mod manifest;
mod module;

pub use admin::{
    AdminAction, AdminDeclarativeComponent, AdminDeclarativePage, AdminDeclarativeSection,
    AdminDeclarativeSurface, AdminEmbeddedEntry, AdminEmbeddedRuntime, AdminEmbeddedSurface,
    AdminMetricBinding, AdminPermission, AdminSandboxPolicy, AdminSurface,
};
pub use admin_data::{AdminDataSource, AdminListQuery, AdminPage};
pub use admin_schema::{AdminSchema, EntitySchema, FieldSchema, FieldType};
pub use binding::ModuleBinding;
pub use http::{
    ModuleHttpMethod, ModuleHttpRoute, ModuleRouteLint, ModuleRouteLintSeverity,
    lint_module_http_routes,
};
pub use linked::{
    LinkedBinding, LinkedBindingBuilder, LinkedHttpContribution, LinkedHttpRouteMerger,
};
pub use manifest::{ModuleManifest, ModuleManifestBuilder};
pub use module::{Module, ModuleLoadStatus, ModuleSource};
