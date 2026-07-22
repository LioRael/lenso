use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::extraction_input_digest;

use super::{
    DeliveryDecision, DeliveryEffects, DeliveryIssue, DeliveryIssueCode, ReleaseSignature,
    ServiceRelease, issue, service_release_integrity_is_valid,
};

pub const RELEASE_TRUST_EVIDENCE_PROTOCOL: &str = "lenso.release-trust-evidence.v1";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseSignerStatus {
    Trusted,
    Untrusted,
    Revoked,
    Invalid,
}

pub trait ReleaseTrustProvider: std::fmt::Debug + Send + Sync {
    fn sign(&self, signer: &str, subject_digest: &str) -> Option<String>;

    fn verify(&self, signer: &str, subject_digest: &str, signature: &str) -> ReleaseSignerStatus;
}

#[derive(Debug, Clone, Default)]
pub struct DeterministicTrustProvider {
    trusted_signers: BTreeMap<String, String>,
    revoked_signers: BTreeSet<String>,
}

impl DeterministicTrustProvider {
    #[must_use]
    pub fn new<I, K, V>(trusted_signers: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        Self {
            trusted_signers: trusted_signers
                .into_iter()
                .map(|(signer, key)| (signer.into(), key.into()))
                .collect(),
            revoked_signers: BTreeSet::new(),
        }
    }

    #[must_use]
    pub fn with_revoked(mut self, signer: impl Into<String>) -> Self {
        self.revoked_signers.insert(signer.into());
        self
    }

    fn expected_signature(&self, signer: &str, subject_digest: &str) -> Option<String> {
        let key = self.trusted_signers.get(signer)?;
        Some(extraction_input_digest(
            format!("{key}\0{subject_digest}").as_bytes(),
        ))
    }
}

impl ReleaseTrustProvider for DeterministicTrustProvider {
    fn sign(&self, signer: &str, subject_digest: &str) -> Option<String> {
        if self.revoked_signers.contains(signer) {
            return None;
        }
        self.expected_signature(signer, subject_digest)
    }

    fn verify(&self, signer: &str, subject_digest: &str, signature: &str) -> ReleaseSignerStatus {
        if self.revoked_signers.contains(signer) {
            return ReleaseSignerStatus::Revoked;
        }
        let Some(expected) = self.expected_signature(signer, subject_digest) else {
            return ReleaseSignerStatus::Untrusted;
        };
        if signature == expected {
            ReleaseSignerStatus::Trusted
        } else {
            ReleaseSignerStatus::Invalid
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkloadTrustEvidence {
    pub workload_id: String,
    pub artifact_digest: String,
    pub sbom_reference: String,
    pub provenance_reference: String,
    pub provenance_subject_matches: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignatureTrustEvidence {
    pub signer: String,
    pub subject_digest: String,
    pub status: ReleaseSignerStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseTrustEvidence {
    pub protocol: String,
    pub release_id: String,
    pub release_digest: String,
    pub decision: DeliveryDecision,
    pub evidence_digest: String,
    pub workloads: Vec<WorkloadTrustEvidence>,
    pub signatures: Vec<SignatureTrustEvidence>,
    pub issues: Vec<DeliveryIssue>,
    pub effects: DeliveryEffects,
}

pub fn attach_service_release_signature(
    release: &mut ServiceRelease,
    provider: &dyn ReleaseTrustProvider,
    signer: &str,
) -> Result<(), DeliveryIssue> {
    if !service_release_integrity_is_valid(release) {
        return Err(issue(
            DeliveryIssueCode::ReleaseTampered,
            "The Service Release content no longer matches its canonical identity.",
            "Discard the changed release and assemble a new immutable release.",
            "Assemble the release again before signing.",
        ));
    }
    let Some(signature) = provider.sign(signer, &release.release_digest) else {
        return Err(issue(
            DeliveryIssueCode::SignerUntrusted,
            format!("Signer `{signer}` is not available through the selected trust provider."),
            "Select an authorized signing identity without exposing its signing material.",
            "Configure the signer and retry signing.",
        ));
    };
    release.signatures.retain(|item| item.signer != signer);
    release.signatures.push(ReleaseSignature {
        signer: signer.to_owned(),
        subject_digest: release.release_digest.clone(),
        signature,
    });
    release.signatures.sort_by(|left, right| {
        (&left.signer, &left.subject_digest).cmp(&(&right.signer, &right.subject_digest))
    });
    Ok(())
}

#[must_use]
pub fn verify_service_release_trust(
    release: &ServiceRelease,
    provider: &dyn ReleaseTrustProvider,
) -> ReleaseTrustEvidence {
    let mut issues = Vec::new();
    if !service_release_integrity_is_valid(release) {
        issues.push(issue(
            DeliveryIssueCode::ReleaseTampered,
            "The Service Release content no longer matches its signed canonical identity.",
            "Discard the changed content and assemble and sign a new release.",
            "Reassemble the release before any protected action.",
        ));
        return trust_evidence(release, Vec::new(), Vec::new(), issues);
    }

    let workloads = release
        .workloads
        .iter()
        .map(|workload| {
            let provenance_subject_matches = workload
                .provenance
                .subject_digests
                .iter()
                .any(|digest| digest == &workload.artifact_digest);
            if !provenance_subject_matches {
                issues.push(DeliveryIssue {
                    code: DeliveryIssueCode::ProvenanceSubjectMismatch,
                    message: format!(
                        "Provenance for Workload `{}` does not name its exact artifact digest.",
                        workload.workload_id
                    ),
                    evidence_references: vec![workload.provenance.reference.clone()],
                    remediation: "Regenerate provenance whose subjects include the exact Workload artifact digest.".to_owned(),
                    next_actions: vec!["Attach matching provenance and verify the release again.".to_owned()],
                });
            }
            WorkloadTrustEvidence {
                workload_id: workload.workload_id.clone(),
                artifact_digest: workload.artifact_digest.clone(),
                sbom_reference: workload.sbom.reference.clone(),
                provenance_reference: workload.provenance.reference.clone(),
                provenance_subject_matches,
            }
        })
        .collect::<Vec<_>>();

    if release.signatures.is_empty() {
        issues.push(issue(
            DeliveryIssueCode::SignatureMissing,
            "The Service Release has no signature over its canonical identity.",
            "Sign the release digest with a trusted release identity.",
            "Sign the release and verify it again.",
        ));
    }
    let signatures = release
        .signatures
        .iter()
        .map(|signature| {
            let status = if signature.subject_digest != release.release_digest {
                ReleaseSignerStatus::Invalid
            } else {
                provider.verify(
                    &signature.signer,
                    &signature.subject_digest,
                    &signature.signature,
                )
            };
            if status != ReleaseSignerStatus::Trusted {
                let (code, message) = match status {
                    ReleaseSignerStatus::Untrusted => (
                        DeliveryIssueCode::SignerUntrusted,
                        format!("Release signer `{}` is not trusted.", signature.signer),
                    ),
                    ReleaseSignerStatus::Revoked => (
                        DeliveryIssueCode::SignerRevoked,
                        format!("Release signer `{}` is revoked.", signature.signer),
                    ),
                    ReleaseSignerStatus::Invalid => (
                        DeliveryIssueCode::SignatureInvalid,
                        format!("Release signature from `{}` is invalid.", signature.signer),
                    ),
                    ReleaseSignerStatus::Trusted => unreachable!(),
                };
                issues.push(issue(
                    code,
                    message,
                    "Use an authorized, non-revoked signer and an exact release digest subject.",
                    "Correct the signature and verify the release again.",
                ));
            }
            SignatureTrustEvidence {
                signer: signature.signer.clone(),
                subject_digest: signature.subject_digest.clone(),
                status,
            }
        })
        .collect::<Vec<_>>();

    trust_evidence(release, workloads, signatures, issues)
}

fn trust_evidence(
    release: &ServiceRelease,
    workloads: Vec<WorkloadTrustEvidence>,
    signatures: Vec<SignatureTrustEvidence>,
    issues: Vec<DeliveryIssue>,
) -> ReleaseTrustEvidence {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct EvidenceContent<'a> {
        protocol: &'a str,
        release_id: &'a str,
        release_digest: &'a str,
        workloads: &'a [WorkloadTrustEvidence],
        signatures: &'a [SignatureTrustEvidence],
        issues: &'a [DeliveryIssue],
    }

    let content = EvidenceContent {
        protocol: RELEASE_TRUST_EVIDENCE_PROTOCOL,
        release_id: &release.release_id,
        release_digest: &release.release_digest,
        workloads: &workloads,
        signatures: &signatures,
        issues: &issues,
    };
    let evidence_digest = extraction_input_digest(
        serde_json::to_vec(&content).expect("trust evidence must serialize"),
    );
    ReleaseTrustEvidence {
        protocol: RELEASE_TRUST_EVIDENCE_PROTOCOL.to_owned(),
        release_id: release.release_id.clone(),
        release_digest: release.release_digest.clone(),
        decision: if issues.is_empty() {
            DeliveryDecision::Passed
        } else {
            DeliveryDecision::Blocked
        },
        evidence_digest,
        workloads,
        signatures,
        issues,
        effects: DeliveryEffects::default(),
    }
}

#[must_use]
pub fn release_trust_evidence_integrity_is_valid(
    evidence: &ReleaseTrustEvidence,
    release: &ServiceRelease,
    provider: &dyn ReleaseTrustProvider,
) -> bool {
    service_release_integrity_is_valid(release)
        && evidence == &verify_service_release_trust(release, provider)
}
