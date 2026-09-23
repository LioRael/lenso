use serde::{Deserialize, Serialize};

use lenso_app_plan::authoring::{PluginContract, PluginImplementation};

/// Generated one-entry Plugin Manifest produced from source evidence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifestV2 {
    pub schema_version: u32,
    pub plugin_id: String,
    pub release_version: String,
    pub artifact: PluginArtifactV2,
    pub entry: PluginEntryV2,
}

/// Exact final Component facts computed by the Bundle builder.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginArtifactV2 {
    pub path: String,
    pub digest: String,
    pub size: u64,
    pub media_type: String,
    pub target: String,
}

/// The single executable Plugin entry derived from source.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginEntryV2 {
    pub descriptor: serde_json::Value,
}

/// One Plugin Release contract with every publisher-provided implementation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifestV3 {
    pub schema_version: u32,
    pub contract: PluginContract,
    pub implementations: Vec<PluginImplementationV3>,
}

/// One exact executable implementation of a V3 Plugin contract.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginImplementationV3 {
    pub id: String,
    pub host_targets: Vec<String>,
    pub artifact: PluginArtifactV2,
    pub runtime: PluginImplementation,
}

/// One Plugin Release contract with exact authoring and runtime profile versions.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifestV4 {
    pub schema_version: u32,
    pub contract: PluginContract,
    pub implementations: Vec<PluginImplementationV4>,
}

/// One exact executable implementation of a V4 Plugin contract.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginImplementationV4 {
    pub id: String,
    pub host_targets: Vec<String>,
    pub artifact: PluginArtifactV2,
    pub runtime: PluginImplementation,
}

/// One release with explicit behavioral implementation groups and artifact variants.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifestV5 {
    pub schema_version: u32,
    pub contract: PluginContract,
    pub implementations: Vec<PluginImplementationV5>,
}

/// A publisher-declared behavior group; variants are never inferred from language.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginImplementationV5 {
    pub id: String,
    pub variants: Vec<PluginVariantV5>,
}

/// One exact executable output and its own runtime requirements.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginVariantV5 {
    pub id: String,
    pub host_targets: Vec<String>,
    pub artifact: PluginArtifactV2,
    pub runtime: PluginImplementation,
}

/// A release that distinguishes executable artifacts from Host build inputs.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifestV6 {
    pub schema_version: u32,
    pub contract: PluginContract,
    pub implementations: Vec<PluginImplementationV6>,
}

/// Publisher-declared behavioral group, independent of language or target.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginImplementationV6 {
    pub id: String,
    pub variants: Vec<PluginVariantV6>,
}

/// One immutable input, with requirements for the eventual execution binding.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginVariantV6 {
    pub id: String,
    pub host_targets: Vec<String>,
    pub input: PluginVariantInputV6,
    pub runtime: PluginImplementation,
}

/// Build inputs are not runtime-loadable artifacts, even with a Native class.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PluginVariantInputV6 {
    Artifact {
        artifact: PluginArtifactV2,
    },
    CargoBuildInput {
        build_input: PluginCargoBuildInputV6,
    },
}

/// Exact `.crate` bytes and Cargo coordinate to link into a newly built Host.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginCargoBuildInputV6 {
    pub path: String,
    pub digest: String,
    pub size: u64,
    pub package: String,
    pub version: String,
}

/// A strictly parsed Plugin Manifest, including the legacy single-artifact form.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PluginManifest {
    V2(PluginManifestV2),
    V3(PluginManifestV3),
    V4(PluginManifestV4),
    V5(PluginManifestV5),
    V6(PluginManifestV6),
}

impl PluginManifest {
    pub fn plugin_id(&self) -> &str {
        match self {
            Self::V2(value) => &value.plugin_id,
            Self::V3(value) => value.contract.plugin_id(),
            Self::V4(value) => value.contract.plugin_id(),
            Self::V5(value) => value.contract.plugin_id(),
            Self::V6(value) => value.contract.plugin_id(),
        }
    }

    pub fn release_version(&self) -> &str {
        match self {
            Self::V2(value) => &value.release_version,
            Self::V3(value) => value.contract.release_version(),
            Self::V4(value) => value.contract.release_version(),
            Self::V5(value) => value.contract.release_version(),
            Self::V6(value) => value.contract.release_version(),
        }
    }
}
