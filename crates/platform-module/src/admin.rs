//! Reserved seam for a module's admin surface.

use serde::{Deserialize, Serialize};

/// RESERVED SEAM. Variants (`Schema { .. }` / `Custom { .. }`) are defined by
/// future specs that build the schema-driven and self-rendered admin surfaces.
///
/// `#[non_exhaustive]` so adding variants later is not a breaking change. Empty
/// for now: a manifest's `admin` field is always `None` in this step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum AdminSurface {}
