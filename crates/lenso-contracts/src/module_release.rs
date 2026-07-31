use crate::{
    CONSOLE_BRIDGE_PROTOCOL, ConsolePermissionRequest, ConsoleSurfacePresentation,
    MODULE_MANIFEST_PROTOCOL, ModuleManifest, lint_module_manifest,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::collections::BTreeSet;
use std::fmt::Write as _;

pub const MODULE_RELEASE_PROTOCOL: &str = "lenso.module-release.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactReference {
    pub locator: String,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AttestationReference {
    pub locator: String,
    pub digest: String,
    pub issuer: String,
    pub signer: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LinkedModuleDelivery {
    pub package: String,
    pub crate_version: String,
    pub archive_checksum: String,
    #[serde(default)]
    pub default_features: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
    pub binding: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attestations: Vec<AttestationReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub migrations: Vec<ArtifactReference>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ServiceResponsibilityProfile {
    Provider,
    Autonomous,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServiceModuleDelivery {
    pub service_id: String,
    pub service_release_version: String,
    pub service_release_digest: String,
    pub export: String,
    pub responsibility_profile: ServiceResponsibilityProfile,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contract_digests: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModuleDelivery {
    Linked(LinkedModuleDelivery),
    Service(ServiceModuleDelivery),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConsoleUiArtifactEntry {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConsoleUiArtifactFormat {
    IsolatedWeb,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConsoleUiArtifact {
    pub artifact: ArtifactReference,
    pub format: ConsoleUiArtifactFormat,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<ConsoleUiArtifactEntry>,
    pub bridge_protocol: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requested_permissions: Vec<ConsolePermissionRequest>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance: Vec<ArtifactReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleCompatibilityDeclaration {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lenso_requirement: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_api_requirement: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rust_requirement: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transports: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protocol_digests: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleRelease {
    pub protocol: String,
    pub module_id: String,
    pub version: String,
    pub manifest: ModuleManifest,
    pub manifest_digest: String,
    pub delivery: ModuleDelivery,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub console_ui_artifact: Option<ConsoleUiArtifact>,
    #[serde(default)]
    pub compatibility: ModuleCompatibilityDeclaration,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance: Vec<ArtifactReference>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleContractIssue {
    pub path: String,
    pub message: String,
}

impl ModuleRelease {
    pub fn new(
        module_id: impl Into<String>,
        version: impl AsRef<str>,
        manifest: ModuleManifest,
        delivery: ModuleDelivery,
    ) -> Result<Self, Vec<ModuleContractIssue>> {
        let version = semver::Version::parse(version.as_ref())
            .map_err(|error| {
                vec![issue(
                    "$.version",
                    format!("Module version must be normalized SemVer: {error}"),
                )]
            })?
            .to_string();
        let manifest_digest = digest_json(&manifest).map_err(|error| {
            vec![issue(
                "$.manifest",
                format!("Manifest cannot be canonicalized: {error}"),
            )]
        })?;
        let release = Self {
            protocol: MODULE_RELEASE_PROTOCOL.to_owned(),
            module_id: module_id.into(),
            version,
            manifest,
            manifest_digest,
            delivery,
            console_ui_artifact: None,
            compatibility: ModuleCompatibilityDeclaration::default(),
            provenance: Vec::new(),
        };
        let issues = release.validate();
        if issues.is_empty() {
            Ok(release)
        } else {
            Err(issues)
        }
    }

    #[must_use]
    pub fn validate(&self) -> Vec<ModuleContractIssue> {
        let mut issues = Vec::new();
        if self.protocol != MODULE_RELEASE_PROTOCOL {
            issues.push(issue(
                "$.protocol",
                format!("protocol must be {MODULE_RELEASE_PROTOCOL}"),
            ));
        }
        if !valid_module_id(&self.module_id) {
            issues.push(issue("$.module_id", "ModuleId must use namespace/name"));
        }
        if self.manifest.protocol != MODULE_MANIFEST_PROTOCOL {
            issues.push(issue(
                "$.manifest.protocol",
                format!("protocol must be {MODULE_MANIFEST_PROTOCOL}"),
            ));
        }
        if self.manifest.module_id != self.module_id {
            issues.push(issue(
                "$.manifest.module_id",
                "Manifest and Release ModuleIds must match",
            ));
        }
        if !matches!(
            semver::Version::parse(&self.version),
            Ok(version) if version.to_string() == self.version
        ) {
            issues.push(issue(
                "$.version",
                "Module version must be normalized SemVer",
            ));
        }
        match digest_json(&self.manifest) {
            Ok(digest) if digest == self.manifest_digest => {}
            Ok(_) => issues.push(issue(
                "$.manifest_digest",
                "Manifest digest does not match canonical Manifest bytes",
            )),
            Err(error) => issues.push(issue(
                "$.manifest",
                format!("Manifest cannot be canonicalized: {error}"),
            )),
        }
        for lint in lint_module_manifest(&self.manifest) {
            if matches!(lint.severity, crate::ModuleManifestLintSeverity::Error) {
                issues.push(issue(format!("$.manifest.{}", lint.subject), lint.message));
            }
        }
        validate_delivery(&self.delivery, &mut issues);
        validate_artifacts("$.provenance", &self.provenance, &mut issues);
        if let Some(console) = &self.console_ui_artifact {
            validate_artifact_reference(
                "$.console_ui_artifact.artifact",
                &console.artifact,
                &mut issues,
            );
            if console.bridge_protocol != CONSOLE_BRIDGE_PROTOCOL {
                issues.push(issue(
                    "$.console_ui_artifact.bridge_protocol",
                    format!("bridge_protocol must be {CONSOLE_BRIDGE_PROTOCOL}"),
                ));
            }
            validate_console_ui_entries(&console.entries, &mut issues);
            validate_console_permission_requests(&console.requested_permissions, &mut issues);
            validate_artifacts(
                "$.console_ui_artifact.provenance",
                &console.provenance,
                &mut issues,
            );
            validate_console_surface_entries(&self.manifest, console, &mut issues);
        } else if self.manifest.console.iter().any(|surface| {
            matches!(
                surface.presentation,
                ConsoleSurfacePresentation::Isolated { .. }
            )
        }) {
            issues.push(issue(
                "$.console_ui_artifact",
                "Isolated Console surfaces require a Console UI Artifact in the same Module Release",
            ));
        }
        for (path, requirement) in [
            (
                "$.compatibility.lenso_requirement",
                self.compatibility.lenso_requirement.as_deref(),
            ),
            (
                "$.compatibility.host_api_requirement",
                self.compatibility.host_api_requirement.as_deref(),
            ),
            (
                "$.compatibility.rust_requirement",
                self.compatibility.rust_requirement.as_deref(),
            ),
        ] {
            if let Some(requirement) = requirement {
                validate_version_requirement(path, requirement, &mut issues);
            }
        }
        for (path, values) in [
            ("$.compatibility.targets", &self.compatibility.targets),
            ("$.compatibility.transports", &self.compatibility.transports),
            (
                "$.compatibility.protocol_digests",
                &self.compatibility.protocol_digests,
            ),
        ] {
            validate_sorted_unique(path, values, &mut issues);
        }
        for digest in &self.compatibility.protocol_digests {
            validate_digest("$.compatibility.protocol_digests", digest, &mut issues);
        }
        issues
    }
}

pub fn canonical_json<T: Serialize>(value: &T) -> serde_json::Result<Vec<u8>> {
    serde_json_canonicalizer::to_vec(value)
}

pub fn digest_json<T: Serialize>(value: &T) -> serde_json::Result<String> {
    let bytes = canonical_json(value)?;
    let mut rendered = String::with_capacity("sha256:".len() + 64);
    rendered.push_str("sha256:");
    for byte in Sha256::digest(bytes) {
        write!(&mut rendered, "{byte:02x}").expect("writing to a String cannot fail");
    }
    Ok(rendered)
}

fn validate_delivery(delivery: &ModuleDelivery, issues: &mut Vec<ModuleContractIssue>) {
    match delivery {
        ModuleDelivery::Linked(linked) => {
            if linked.package.trim().is_empty() || linked.binding.trim().is_empty() {
                issues.push(issue(
                    "$.delivery",
                    "Linked delivery requires package and binding",
                ));
            }
            validate_semver("$.delivery.crate_version", &linked.crate_version, issues);
            validate_digest(
                "$.delivery.archive_checksum",
                &linked.archive_checksum,
                issues,
            );
            validate_sorted_unique("$.delivery.features", &linked.features, issues);
            let mut attestation_keys = BTreeSet::new();
            for attestation in &linked.attestations {
                if attestation.locator.trim().is_empty()
                    || attestation.issuer.trim().is_empty()
                    || attestation.signer.trim().is_empty()
                    || !attestation_keys.insert((&attestation.locator, &attestation.digest))
                {
                    issues.push(issue(
                        "$.delivery.attestations",
                        "Attestations require unique locator/digest pairs and complete identity",
                    ));
                }
                validate_digest(
                    "$.delivery.attestations.digest",
                    &attestation.digest,
                    issues,
                );
            }
            validate_artifacts("$.delivery.migrations", &linked.migrations, issues);
        }
        ModuleDelivery::Service(service) => {
            if !valid_module_id(&service.service_id) || service.export.trim().is_empty() {
                issues.push(issue(
                    "$.delivery",
                    "Service delivery requires a fully qualified ServiceId and export",
                ));
            }
            validate_semver(
                "$.delivery.service_release_version",
                &service.service_release_version,
                issues,
            );
            validate_digest(
                "$.delivery.service_release_digest",
                &service.service_release_digest,
                issues,
            );
            if service.contract_digests.is_empty() {
                issues.push(issue(
                    "$.delivery.contract_digests",
                    "Service delivery requires governing contract digests",
                ));
            }
            for digest in &service.contract_digests {
                validate_digest("$.delivery.contract_digests", digest, issues);
            }
            validate_sorted_unique(
                "$.delivery.contract_digests",
                &service.contract_digests,
                issues,
            );
        }
    }
}

fn validate_semver(path: &str, value: &str, issues: &mut Vec<ModuleContractIssue>) {
    if !matches!(
        semver::Version::parse(value),
        Ok(version) if version.to_string() == value
    ) {
        issues.push(issue(path, "value must be normalized SemVer"));
    }
}

fn validate_version_requirement(path: &str, value: &str, issues: &mut Vec<ModuleContractIssue>) {
    if !matches!(
        semver::VersionReq::parse(value),
        Ok(requirement) if requirement.to_string() == value
    ) {
        issues.push(issue(path, "value must be a normalized SemVer requirement"));
    }
}

fn validate_digest(path: &str, value: &str, issues: &mut Vec<ModuleContractIssue>) {
    if !value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }) {
        issues.push(issue(path, "value must be sha256:<64 lowercase hex>"));
    }
}

fn validate_sorted_unique(path: &str, values: &[String], issues: &mut Vec<ModuleContractIssue>) {
    if values.iter().any(|value| value.trim().is_empty())
        || values.windows(2).any(|pair| pair[0] >= pair[1])
    {
        issues.push(issue(path, "values must be non-empty, sorted, and unique"));
    }
}

fn validate_unique<T, F>(path: &str, values: &[T], key: F, issues: &mut Vec<ModuleContractIssue>)
where
    F: Fn(&T) -> &String,
{
    let mut seen = BTreeSet::new();
    if values.iter().any(|value| !seen.insert(key(value))) {
        issues.push(issue(path, "values must be unique"));
    }
}

fn validate_artifacts(
    path: &str,
    values: &[ArtifactReference],
    issues: &mut Vec<ModuleContractIssue>,
) {
    validate_unique(path, values, |item| &item.digest, issues);
    for artifact in values {
        if artifact.locator.trim().is_empty() {
            issues.push(issue(path, "artifact locator must be non-empty"));
        }
        validate_digest(path, &artifact.digest, issues);
    }
}

fn validate_artifact_reference(
    path: &str,
    artifact: &ArtifactReference,
    issues: &mut Vec<ModuleContractIssue>,
) {
    if artifact.locator.trim().is_empty() {
        issues.push(issue(path, "artifact locator must be non-empty"));
    }
    validate_digest(path, &artifact.digest, issues);
}

fn validate_console_ui_entries(
    entries: &[ConsoleUiArtifactEntry],
    issues: &mut Vec<ModuleContractIssue>,
) {
    validate_unique(
        "$.console_ui_artifact.entries",
        entries,
        |entry| &entry.name,
        issues,
    );
    for entry in entries {
        if entry.name.trim().is_empty()
            || entry.path.trim().is_empty()
            || entry.path.starts_with('/')
            || entry
                .path
                .split('/')
                .any(|segment| segment.is_empty() || segment == "..")
        {
            issues.push(issue(
                "$.console_ui_artifact.entries",
                "Console UI entries require a stable name and a relative artifact path without traversal",
            ));
        }
    }
}

fn validate_console_permission_requests(
    permissions: &[ConsolePermissionRequest],
    issues: &mut Vec<ModuleContractIssue>,
) {
    validate_unique(
        "$.console_ui_artifact.requested_permissions",
        permissions,
        |permission| &permission.permission_id,
        issues,
    );
    for permission in permissions {
        if permission.permission_id.trim().is_empty() {
            issues.push(issue(
                "$.console_ui_artifact.requested_permissions",
                "Console permission identifiers must be non-empty",
            ));
        }
        for (path, values) in [
            ("operations", &permission.operations),
            ("resources", &permission.resources),
            ("outbound_destinations", &permission.outbound_destinations),
            ("secret_references", &permission.secret_references),
        ] {
            validate_sorted_unique(
                &format!("$.console_ui_artifact.requested_permissions.{path}"),
                values,
                issues,
            );
        }
    }
}

fn validate_console_surface_entries(
    manifest: &ModuleManifest,
    artifact: &ConsoleUiArtifact,
    issues: &mut Vec<ModuleContractIssue>,
) {
    let entries = artifact
        .entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect::<BTreeSet<_>>();
    for surface in &manifest.console {
        if let ConsoleSurfacePresentation::Isolated {
            entry,
            bridge_protocol,
        } = &surface.presentation
        {
            if bridge_protocol != CONSOLE_BRIDGE_PROTOCOL {
                issues.push(issue(
                    "$.manifest.console.presentation.bridge_protocol",
                    format!("bridge_protocol must be {CONSOLE_BRIDGE_PROTOCOL}"),
                ));
            }
            if !entries.contains(entry.as_str()) {
                issues.push(issue(
                    "$.manifest.console.presentation.entry",
                    format!(
                        "Console surface entry `{entry}` is missing from the release UI artifact"
                    ),
                ));
            }
        }
    }
}

fn valid_module_id(value: &str) -> bool {
    let Some((namespace, name)) = value.split_once('/') else {
        return false;
    };
    !namespace.is_empty()
        && !name.is_empty()
        && !name.contains('/')
        && [namespace, name].into_iter().all(|segment| {
            segment
                .bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_lowercase())
                && segment.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || byte == b'-'
                        || byte == b'_'
                })
        })
}

fn issue(path: impl Into<String>, message: impl Into<String>) -> ModuleContractIssue {
    ModuleContractIssue {
        path: path.into(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(value: &str) -> String {
        format!("sha256:{}", value.repeat(64))
    }

    fn manifest() -> ModuleManifest {
        ModuleManifest::builder("acme/support-ticket").build()
    }

    fn linked_delivery() -> ModuleDelivery {
        ModuleDelivery::Linked(LinkedModuleDelivery {
            package: "acme-support-ticket".to_owned(),
            crate_version: "1.2.3".to_owned(),
            archive_checksum: digest("a"),
            default_features: false,
            features: Vec::new(),
            binding: "support_ticket".to_owned(),
            attestations: Vec::new(),
            migrations: Vec::new(),
        })
    }

    #[test]
    fn canonical_digest_is_order_independent() {
        let left = serde_json::json!({"b": 2, "a": 1});
        let right = serde_json::json!({"a": 1, "b": 2});
        assert_eq!(digest_json(&left).unwrap(), digest_json(&right).unwrap());
        assert_eq!(canonical_json(&left).unwrap(), br#"{"a":1,"b":2}"#);
    }

    #[test]
    fn linked_release_binds_exact_manifest() {
        let manifest = manifest();
        let release = ModuleRelease::new(
            "acme/support-ticket",
            "1.2.3",
            manifest.clone(),
            ModuleDelivery::Linked(LinkedModuleDelivery {
                package: "acme-support-ticket".to_owned(),
                crate_version: "4.2.0".to_owned(),
                archive_checksum: digest("a"),
                default_features: false,
                features: vec!["postgres".to_owned()],
                binding: "support_ticket".to_owned(),
                attestations: Vec::new(),
                migrations: Vec::new(),
            }),
        )
        .unwrap();

        assert_eq!(release.manifest_digest, digest_json(&manifest).unwrap());
        assert!(release.validate().is_empty());
    }

    #[test]
    fn isolated_console_surface_requires_ui_artifact_in_same_release() {
        let manifest = ModuleManifest::builder("acme/support-ticket")
            .console(vec![crate::ConsoleSurface {
                name: "tickets".to_owned(),
                label: "Tickets".to_owned(),
                route: "/support/tickets".to_owned(),
                presentation: ConsoleSurfacePresentation::Isolated {
                    entry: "tickets".to_owned(),
                    bridge_protocol: CONSOLE_BRIDGE_PROTOCOL.to_owned(),
                },
                icon: None,
                required_capabilities: Vec::new(),
                navigation: None,
            }])
            .build();
        let issues = ModuleRelease::new(
            "acme/support-ticket",
            "1.2.3",
            manifest.clone(),
            linked_delivery(),
        )
        .unwrap_err();
        assert!(
            issues
                .iter()
                .any(|issue| issue.path == "$.console_ui_artifact")
        );

        let mut release = ModuleRelease {
            protocol: MODULE_RELEASE_PROTOCOL.to_owned(),
            module_id: "acme/support-ticket".to_owned(),
            version: "1.2.3".to_owned(),
            manifest_digest: digest_json(&manifest).unwrap(),
            manifest,
            delivery: linked_delivery(),
            console_ui_artifact: Some(ConsoleUiArtifact {
                artifact: ArtifactReference {
                    locator: "oci://registry.example/acme/support-ticket-ui@sha256:ui".to_owned(),
                    digest: digest("b"),
                },
                format: ConsoleUiArtifactFormat::IsolatedWeb,
                entries: vec![ConsoleUiArtifactEntry {
                    name: "other".to_owned(),
                    path: "entries/other/index.html".to_owned(),
                }],
                bridge_protocol: CONSOLE_BRIDGE_PROTOCOL.to_owned(),
                requested_permissions: Vec::new(),
                provenance: Vec::new(),
            }),
            compatibility: ModuleCompatibilityDeclaration::default(),
            provenance: Vec::new(),
        };
        assert!(
            release
                .validate()
                .iter()
                .any(|issue| { issue.path == "$.manifest.console.presentation.entry" })
        );

        release.console_ui_artifact.as_mut().unwrap().entries[0].name = "tickets".to_owned();
        assert!(release.validate().is_empty());
    }

    #[test]
    fn console_ui_permissions_and_bridge_are_exact_and_digest_bound() {
        let mut release = ModuleRelease::new(
            "acme/support-ticket",
            "1.2.3",
            manifest(),
            linked_delivery(),
        )
        .unwrap();
        release.console_ui_artifact = Some(ConsoleUiArtifact {
            artifact: ArtifactReference {
                locator: "oci://registry.example/acme/support-ticket-ui@sha256:ui".to_owned(),
                digest: digest("b"),
            },
            format: ConsoleUiArtifactFormat::IsolatedWeb,
            entries: Vec::new(),
            bridge_protocol: "lenso.console-bridge.latest".to_owned(),
            requested_permissions: vec![ConsolePermissionRequest {
                permission_id: "tickets.read".to_owned(),
                operations: vec!["read".to_owned(), "read".to_owned()],
                resources: Vec::new(),
                outbound_destinations: Vec::new(),
                secret_references: Vec::new(),
            }],
            provenance: Vec::new(),
        });

        let paths = release
            .validate()
            .into_iter()
            .map(|issue| issue.path)
            .collect::<Vec<_>>();
        assert!(paths.contains(&"$.console_ui_artifact.bridge_protocol".to_owned()));
        assert!(
            paths.contains(&"$.console_ui_artifact.requested_permissions.operations".to_owned())
        );
    }

    #[test]
    fn release_rejects_identity_and_digest_drift() {
        let mut release = ModuleRelease::new(
            "acme/support-ticket",
            "1.2.3",
            manifest(),
            ModuleDelivery::Service(ServiceModuleDelivery {
                service_id: "acme/support-suite".to_owned(),
                service_release_version: "4.2.0".to_owned(),
                service_release_digest: digest("b"),
                export: "support".to_owned(),
                responsibility_profile: ServiceResponsibilityProfile::Provider,
                contract_digests: vec![digest("c")],
            }),
        )
        .unwrap();
        release.manifest.module_id = "acme/other".to_owned();

        let paths = release
            .validate()
            .into_iter()
            .map(|issue| issue.path)
            .collect::<Vec<_>>();
        assert!(paths.contains(&"$.manifest.module_id".to_owned()));
        assert!(paths.contains(&"$.manifest_digest".to_owned()));
    }

    #[test]
    fn delivery_union_rejects_removed_and_future_kinds() {
        for kind in ["remote", "bundled", "wasm"] {
            let value = serde_json::json!({"kind": kind});
            assert!(serde_json::from_value::<ModuleDelivery>(value).is_err());
        }
    }

    #[test]
    fn provider_and_autonomous_service_profiles_are_valid() {
        for responsibility_profile in [
            ServiceResponsibilityProfile::Provider,
            ServiceResponsibilityProfile::Autonomous,
        ] {
            let release = ModuleRelease::new(
                "acme/support-ticket",
                "1.2.3",
                manifest(),
                ModuleDelivery::Service(ServiceModuleDelivery {
                    service_id: "acme/support-service".to_owned(),
                    service_release_version: "4.2.0".to_owned(),
                    service_release_digest: digest("b"),
                    export: "support".to_owned(),
                    responsibility_profile,
                    contract_digests: vec![digest("c")],
                }),
            )
            .unwrap();

            assert!(release.validate().is_empty());
        }
    }

    #[test]
    fn unknown_delivery_and_secret_fields_are_rejected() {
        let release = ModuleRelease::new(
            "acme/support-ticket",
            "1.2.3",
            manifest(),
            ModuleDelivery::Linked(LinkedModuleDelivery {
                package: "acme-support-ticket".to_owned(),
                crate_version: "1.2.3".to_owned(),
                archive_checksum: digest("a"),
                default_features: false,
                features: Vec::new(),
                binding: "support_ticket".to_owned(),
                attestations: Vec::new(),
                migrations: Vec::new(),
            }),
        )
        .unwrap();
        let mut value = serde_json::to_value(release).unwrap();
        value["delivery"]["endpoint"] = serde_json::json!("https://example.test");
        value["delivery"]["credential"] = serde_json::json!("secret");

        assert!(serde_json::from_value::<ModuleRelease>(value).is_err());
    }

    #[test]
    fn manifest_rejects_embedded_secret_defaults_and_unknown_fields() {
        use crate::{
            ModuleConfigActivation, ModuleConfigContract, ModuleConfigField, ModuleConfigFieldType,
            ModuleConfigMutability, ModuleConfigScope,
        };

        let manifest = ModuleManifest::builder("acme/support-ticket")
            .config(ModuleConfigContract {
                fields: vec![ModuleConfigField {
                    key: "api_token".to_owned(),
                    field_type: ModuleConfigFieldType::String,
                    required: true,
                    scope: ModuleConfigScope::Service,
                    sensitive: true,
                    secret_reference: true,
                    mutability: ModuleConfigMutability::Static,
                    activation: ModuleConfigActivation::ServiceRestart,
                    default: Some(serde_json::json!("plaintext-secret")),
                    validation: None,
                }],
            })
            .build();
        assert!(lint_module_manifest(&manifest).iter().any(|lint| {
            lint.severity == crate::ModuleManifestLintSeverity::Error
                && lint.subject == "config api_token default"
        }));

        let mut value = serde_json::to_value(manifest).unwrap();
        value["source"] = serde_json::json!("linked");
        assert!(serde_json::from_value::<ModuleManifest>(value).is_err());
    }
}
