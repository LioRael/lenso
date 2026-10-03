//! Source-owned Plugins; Instance intent stays beside their implementation.
pub use metadata_contract as metadata;

#[path = "../plugins/example.label/plugin.rs"]
pub mod label;
#[path = "../plugins/example.pair/plugin.rs"]
pub mod pair;
