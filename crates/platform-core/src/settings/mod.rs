//! Layered, console-editable configuration overlaid on the static `AppConfig`.
//!
//! Domains and platform crates declare typed [`SettingDescriptor`]s; a
//! [`SettingsRegistry`] aggregates them; later tasks add the snapshot and
//! provider that resolve effective values from defaults plus stored overrides.

mod descriptor;
mod postgres;
mod provider;
mod snapshot;
pub mod store;

pub use descriptor::{SettingDescriptor, SettingScope, SettingType, SettingsRegistry};
pub use postgres::{CONFIG_NOTIFY_CHANNEL, PostgresSettingsProvider};
pub use provider::{SettingsProvider, SnapshotCell, StaticSettingsProvider};
pub use snapshot::{SettingSource, SettingsSnapshot};
pub use store::{SettingAuditEntry, StoredSetting};
