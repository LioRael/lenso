//! Typed execution input for the Lenso vNext Kernel.

/// The Resolved App Plan schema understood by this Kernel version.
pub const PLAN_SCHEMA_VERSION: u32 = 1;

/// Exact, immutable execution input supplied to the Kernel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedAppPlan {
    schema_version: u32,
}

impl ResolvedAppPlan {
    /// Creates a valid Plan containing no Module Instances.
    pub const fn empty() -> Self {
        Self {
            schema_version: PLAN_SCHEMA_VERSION,
        }
    }

    /// Creates a Plan with an explicit schema version.
    ///
    /// This is primarily useful to decode authoring-tool output before validation.
    pub const fn with_schema_version(schema_version: u32) -> Self {
        Self { schema_version }
    }

    /// Returns the Plan schema version.
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }
}
