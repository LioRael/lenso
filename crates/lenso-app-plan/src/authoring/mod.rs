//! Language-independent App authoring data consumed before Plan resolution.

mod binding_policy;
mod composition;
mod configuration;
mod definition;
mod package;
mod project;

pub use binding_policy::*;
pub use composition::*;
pub use definition::*;
pub use package::*;
pub use project::*;
