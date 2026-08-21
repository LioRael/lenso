use std::fmt;

use serde::{Deserialize, Serialize};

/// Package-manager or artifact source selected by App authoring.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageSource {
    /// A statically linked Cargo/Rust Module package.
    #[default]
    Cargo,
    /// A Bun child-process Module package.
    Bun,
    /// An npm package executed by a Bun Adapter.
    Npm,
    /// An OCI-hosted artifact whose execution class is selected explicitly.
    Oci,
}

impl PackageSource {
    /// Returns the default official execution class for this package source.
    pub const fn default_execution_class(self) -> Option<&'static str> {
        match self {
            Self::Cargo => Some("lenso.native-rust@1"),
            Self::Bun | Self::Npm => Some("lenso.bun-process@1"),
            Self::Oci => None,
        }
    }

    /// Returns the stable source label embedded in a Resolved App Plan.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cargo => "cargo",
            Self::Bun => "bun",
            Self::Npm => "npm",
            Self::Oci => "oci",
        }
    }
}

impl fmt::Display for PackageSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One package-manager input owned by an App project.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PackageInput {
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    package_name: Option<String>,
    source: PackageSource,
    version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    locked_revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    manifest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lockfile: Option<String>,
}

impl PackageInput {
    /// Declares a package dependency without mutating a running App.
    pub fn new(name: impl Into<String>, source: PackageSource, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            package_name: None,
            source,
            version: version.into(),
            locked_revision: None,
            manifest: None,
            lockfile: None,
        }
    }

    /// Selects a package-manager name different from the runtime package ID.
    #[must_use]
    pub fn with_package_name(mut self, package_name: impl Into<String>) -> Self {
        self.package_name = Some(package_name.into());
        self
    }

    /// Associates the package input with a reviewable package-manager file.
    #[must_use]
    pub fn with_manifest(mut self, manifest: impl Into<String>) -> Self {
        self.manifest = Some(manifest.into());
        self
    }

    /// Selects the ordinary package-manager lockfile consumed by resolution.
    #[must_use]
    pub fn with_lockfile(mut self, lockfile: impl Into<String>) -> Self {
        self.lockfile = Some(lockfile.into());
        self
    }
    /// Selects the exact opaque lock revision when the manifest requirement is
    /// a path, range, tag, or other non-version specifier.
    #[must_use]
    pub fn with_locked_revision(mut self, revision: impl Into<String>) -> Self {
        self.locked_revision = Some(revision.into());
        self
    }

    /// Returns the package identity.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Returns the identity looked up in the ordinary package-manager lockfile.
    pub fn package_name(&self) -> &str {
        self.package_name.as_deref().unwrap_or(&self.name)
    }
    /// Returns the selected package source.
    pub const fn source(&self) -> PackageSource {
        self.source
    }
    /// Returns the requested package version.
    pub fn version(&self) -> &str {
        &self.version
    }
    /// Returns the exact opaque revision expected from package-manager lock state.
    pub fn locked_revision(&self) -> &str {
        self.locked_revision.as_deref().unwrap_or(&self.version)
    }
    /// Returns the package-manager manifest path, when configured.
    pub fn manifest(&self) -> Option<&str> {
        self.manifest.as_deref()
    }
    /// Returns the selected package-manager lockfile path, when explicit.
    pub fn lockfile(&self) -> Option<&str> {
        self.lockfile.as_deref()
    }
}
