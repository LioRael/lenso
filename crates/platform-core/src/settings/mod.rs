//! Layered, console-editable configuration overlaid on the static `AppConfig`.
//!
//! Domains and platform crates declare typed [`SettingDescriptor`]s; a
//! [`SettingsRegistry`] aggregates them; later tasks add the snapshot and
//! provider that resolve effective values from defaults plus stored overrides.

mod descriptor;
mod provider;
mod snapshot;

pub use descriptor::{SettingDescriptor, SettingScope, SettingType, SettingsRegistry};
pub use provider::{SettingsProvider, SnapshotCell, StaticSettingsProvider};
pub use snapshot::{SettingSource, SettingsSnapshot};
