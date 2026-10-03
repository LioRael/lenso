//! Two independently selected Plugins in one Cargo package.
pub mod greeting;
#[path = "../plugins/example.health/plugin.rs"]
pub mod health;
