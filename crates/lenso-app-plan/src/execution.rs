//! Execution Adapter class identities preserved in the Resolved App Plan.

use std::fmt;

/// Stable, open identity of the execution mechanism selected for a Module
/// Instance.
///
/// Execution Adapter packages own these IDs. The Plan preserves them as opaque
/// authoring data so third-party Adapters do not require changes to this crate.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ExecutionClassId(String);

impl ExecutionClassId {
    /// Creates an execution-class identity selected by App Composition.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the official statically linked Rust execution class.
    pub fn native_rust() -> Self {
        Self::new("lenso.native-rust@1")
    }

    /// Returns the official trusted Bun child-process execution class.
    pub fn bun_child_process() -> Self {
        Self::new("lenso.bun-process@1")
    }

    /// Returns the stable execution-class identity.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ExecutionClassId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}
