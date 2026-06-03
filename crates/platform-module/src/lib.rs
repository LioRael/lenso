//! Module framework contracts: the data/behavior split a module exposes to the
//! composition root.
//!
//! - [`ModuleManifest`]: serializable data (name, story display, reserved
//!   seams). Produced by every loading source.
//! - [`ModuleBinding`]: behavior (register functions/event handlers). One impl
//!   per loading source; [`LinkedBinding`] is the compile-time one.
//! - [`Module`]: a loaded module bundling manifest + binding + internal config.

mod admin;
mod binding;
mod linked;
mod manifest;
mod module;

pub use admin::AdminSurface;
pub use binding::ModuleBinding;
pub use linked::{LinkedBinding, LinkedBindingBuilder};
pub use manifest::{ModuleManifest, ModuleManifestBuilder};
pub use module::Module;
