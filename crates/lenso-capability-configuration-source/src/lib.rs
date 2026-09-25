//! Versioned source Plugin role used only by a Host-owned pre-App bootstrap Plan.
//!
//! The response contains values, not source identity, writable scopes, trust roots,
//! publication approval, or App bindings. The Host supplies and validates those.

#[allow(unknown_lints)]
#[allow(clippy::all)]
pub mod host {
    include!("generated.rs");
}

#[allow(unknown_lints)]
#[allow(clippy::all)]
pub mod plugin {
    include!("generated_plugin.rs");
}

// The generated export macro addresses its contract through `$crate`.
pub use plugin::*;
