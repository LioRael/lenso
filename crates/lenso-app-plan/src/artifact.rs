//! Exact package identity selected by authoring-time lock resolution.

/// Exact package artifact selected by authoring-time lock resolution.
///
/// The Kernel treats these fields as opaque identity. Package acquisition and
/// integrity policy remain owned by the authoring tool and package manager.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleArtifact {
    source: String,
    locator: String,
    version: String,
    digest: String,
}

impl ModuleArtifact {
    /// Creates an immutable artifact identity for one Module Instance.
    pub fn new(
        source: impl Into<String>,
        locator: impl Into<String>,
        version: impl Into<String>,
        digest: impl Into<String>,
    ) -> Self {
        Self {
            source: source.into(),
            locator: locator.into(),
            version: version.into(),
            digest: digest.into(),
        }
    }

    /// Returns the package-manager or Adapter source family.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns the exact locked package locator or artifact path.
    pub fn locator(&self) -> &str {
        &self.locator
    }

    /// Returns the exact locked package version.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Returns the package-manager-provided integrity digest.
    pub fn digest(&self) -> &str {
        &self.digest
    }
}
