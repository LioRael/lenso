//! Layered, console-editable configuration overlaid on the static `AppConfig`.
//!
//! Domains and platform crates declare typed [`RuntimeConfigDescriptor`]s; a
//! [`RuntimeConfigRegistry`] aggregates them; later tasks add the snapshot and
//! provider that resolve effective values from defaults plus stored overrides.

mod descriptor;
mod postgres;
mod provider;
mod snapshot;
pub mod store;

pub use descriptor::{
    RuntimeConfigDescriptor, RuntimeConfigGroupDescriptor, RuntimeConfigRegistry,
    RuntimeConfigScope, RuntimeConfigType, RuntimeConfigVisibilityCondition,
};
pub use postgres::{CONFIG_NOTIFY_CHANNEL, PostgresRuntimeConfigProvider};
pub use provider::{RuntimeConfigCell, RuntimeConfigProvider, StaticRuntimeConfigProvider};
pub use snapshot::{RuntimeConfigSnapshot, RuntimeConfigSource};
pub use store::{RuntimeConfigAuditEntry, StoredRuntimeConfig};
