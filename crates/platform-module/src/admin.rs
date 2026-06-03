//! Reserved seam for a module's admin surface.

use crate::admin_schema::AdminSchema;
use serde::{Deserialize, Serialize};

/// A module's admin surface. `Schema` is the generic schema-driven CRUD lane.
///
/// `#[non_exhaustive]` so adding variants later (e.g. `Custom` for plugin
/// self-rendering, Step 4) is not a breaking change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum AdminSurface {
    /// Schema-driven CRUD: console renders a generic UI from this declaration.
    Schema(AdminSchema),
}
