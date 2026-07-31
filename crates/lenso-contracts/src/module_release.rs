use crate::{MODULE_MANIFEST_PROTOCOL, ModuleManifest, lint_module_manifest};
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
pub struct ModuleConsoleArtifact {
    pub package: String,
    pub version: String,
    pub integrity: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exports: Vec<String>,
    pub host_api_requirement: String,
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
    pub console_artifact: Option<ModuleConsoleArtifact>,
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
            console_artifact: None,
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
        if let Some(console) = &self.console_artifact {
            if console.package.trim().is_empty() {
                issues.push(issue(
                    "$.console_artifact.package",
                    "Console package must be non-empty",
                ));
            }
            validate_semver("$.console_artifact.version", &console.version, &mut issues);
            validate_digest(
                "$.console_artifact.integrity",
                &console.integrity,
                &mut issues,
            );
            validate_sorted_unique("$.console_artifact.exports", &console.exports, &mut issues);
            validate_version_requirement(
                "$.console_artifact.host_api_requirement",
                &console.host_api_requirement,
                &mut issues,
            );
            validate_artifacts(
                "$.console_artifact.provenance",
                &console.provenance,
                &mut issues,
            );
            if !console
                .provenance
                .iter()
                .any(|reference| reference.digest == console.integrity)
            {
                issues.push(issue(
                    "$.console_artifact.provenance",
                    "Console artifact provenance must include a downloadable locator whose digest matches integrity",
                ));
            }
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
    fn console_artifact_requires_downloadable_provenance_matching_integrity() {
        let mut release = ModuleRelease::new(
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
        release.console_artifact = Some(ModuleConsoleArtifact {
            package: "@acme/support-console".to_owned(),
            version: "1.0.0".to_owned(),
            integrity: digest("b"),
            exports: vec!["supportConsoleModule".to_owned()],
            host_api_requirement: "^1".to_owned(),
            provenance: vec![ArtifactReference {
                locator: "https://modules.example/support.js".to_owned(),
                digest: digest("c"),
            }],
        });

        assert!(release.validate().iter().any(|issue| {
            issue.path == "$.console_artifact.provenance"
                && issue.message.contains("matches integrity")
        }));
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
