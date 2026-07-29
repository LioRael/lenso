use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signature, Signer as _, SigningKey, Verifier as _, VerifyingKey};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use utoipa::ToSchema;

pub const ENROLLMENT_OFFER_PROTOCOL: &str = "lenso.system-plane.enrollment-offer.v1";
pub const ENROLLMENT_RECEIPT_PROTOCOL: &str = "lenso.system-plane.enrollment-receipt.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnrollmentCapabilityGrant {
    #[schema(pattern = r"^lenso\.system-plane\.[a-z0-9]+(?:[.-][a-z0-9]+)*\.v[1-9][0-9]*$")]
    pub contract_id: String,
    #[schema(pattern = r"^sha256:[0-9a-f]{64}$")]
    pub schema_digest: String,
    pub feature_ids: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnrollmentPolicyGrant {
    #[schema(min_length = 1)]
    pub policy_id: String,
    #[schema(min_length = 1)]
    pub policy_revision: String,
    #[schema(pattern = r"^sha256:[0-9a-f]{64}$")]
    pub policy_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnrollmentSignature {
    pub algorithm: EnrollmentSignatureAlgorithm,
    #[schema(min_length = 1)]
    pub key_id: String,
    #[schema(pattern = r"^sha256:[0-9a-f]{64}$")]
    pub subject_digest: String,
    #[schema(min_length = 1)]
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EnrollmentSignatureAlgorithm {
    Ed25519,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnrollmentOffer {
    pub protocol: String,
    #[schema(min_length = 1)]
    pub system_id: String,
    #[schema(min_length = 1)]
    pub console_service_principal: String,
    #[schema(min_length = 16)]
    pub nonce: String,
    pub issued_at_unix_ms: u64,
    pub expires_at_unix_ms: u64,
    pub requested_capabilities: Vec<EnrollmentCapabilityGrant>,
    pub requested_policy: EnrollmentPolicyGrant,
    pub signature: EnrollmentSignature,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnrollmentReceipt {
    pub protocol: String,
    #[schema(pattern = r"^sha256:[0-9a-f]{64}$")]
    pub offer_digest: String,
    #[schema(min_length = 1)]
    pub system_id: String,
    #[schema(min_length = 1)]
    pub managed_service_id: String,
    #[schema(min_length = 1)]
    pub managed_service_principal: String,
    #[schema(min_length = 1)]
    pub managed_service_revision: String,
    #[schema(min_length = 1)]
    pub console_service_principal: String,
    #[schema(min_length = 16)]
    pub nonce: String,
    pub issued_at_unix_ms: u64,
    pub expires_at_unix_ms: u64,
    #[schema(minimum = 1)]
    pub grant_revision: u64,
    pub authorization_epoch: u64,
    pub granted_capabilities: Vec<EnrollmentCapabilityGrant>,
    pub granted_policy: EnrollmentPolicyGrant,
    pub signature: EnrollmentSignature,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnrollmentContractIssueCode {
    InvalidProtocol,
    MissingIdentity,
    InvalidNonce,
    InvalidLifetime,
    InvalidCapability,
    DuplicateCapability,
    InvalidPolicy,
    InvalidRevision,
    DigestMismatch,
    SignatureInvalid,
    SignerUntrusted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrollmentContractIssue {
    pub code: EnrollmentContractIssueCode,
    pub path: String,
    pub message: String,
}

pub trait EnrollmentSigner: std::fmt::Debug + Send + Sync {
    fn key_id(&self) -> &str;
    fn sign_digest(&self, subject_digest: &str) -> Result<EnrollmentSignature, String>;
}

pub trait EnrollmentSignatureVerifier: std::fmt::Debug + Send + Sync {
    fn verify_signature(
        &self,
        signature: &EnrollmentSignature,
    ) -> Result<(), EnrollmentSignatureVerificationError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnrollmentSignatureVerificationError {
    UntrustedKey,
    RevokedKey,
    InvalidEncoding,
    InvalidSignature,
}

pub struct Ed25519EnrollmentSigner {
    key_id: String,
    signing_key: SigningKey,
}

impl std::fmt::Debug for Ed25519EnrollmentSigner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Ed25519EnrollmentSigner")
            .field("key_id", &self.key_id)
            .field("signing_key", &"[REDACTED]")
            .finish()
    }
}

impl Ed25519EnrollmentSigner {
    pub fn new(key_id: impl Into<String>, secret_key: [u8; 32]) -> Result<Self, String> {
        let key_id = key_id.into();
        if key_id.trim().is_empty() {
            return Err("Enrollment signing key identity must not be empty".to_owned());
        }
        Ok(Self {
            key_id,
            signing_key: SigningKey::from_bytes(&secret_key),
        })
    }

    #[must_use]
    pub fn verifying_key_bytes(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }
}

impl EnrollmentSigner for Ed25519EnrollmentSigner {
    fn key_id(&self) -> &str {
        &self.key_id
    }

    fn sign_digest(&self, subject_digest: &str) -> Result<EnrollmentSignature, String> {
        let signature = self.signing_key.sign(subject_digest.as_bytes());
        Ok(EnrollmentSignature {
            algorithm: EnrollmentSignatureAlgorithm::Ed25519,
            key_id: self.key_id.clone(),
            subject_digest: subject_digest.to_owned(),
            value: URL_SAFE_NO_PAD.encode(signature.to_bytes()),
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct Ed25519EnrollmentTrustStore {
    verifying_keys: BTreeMap<String, VerifyingKey>,
    revoked_key_ids: BTreeSet<String>,
}

impl Ed25519EnrollmentTrustStore {
    pub fn new<I, K>(keys: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = (K, [u8; 32])>,
        K: Into<String>,
    {
        let mut verifying_keys = BTreeMap::new();
        for (key_id, bytes) in keys {
            let key_id = key_id.into();
            if key_id.trim().is_empty() {
                return Err("Enrollment verifying key identity must not be empty".to_owned());
            }
            let key = VerifyingKey::from_bytes(&bytes)
                .map_err(|_| "Enrollment verifying key is invalid".to_owned())?;
            verifying_keys.insert(key_id, key);
        }
        Ok(Self {
            verifying_keys,
            revoked_key_ids: BTreeSet::new(),
        })
    }

    #[must_use]
    pub fn with_revoked(mut self, key_id: impl Into<String>) -> Self {
        self.revoked_key_ids.insert(key_id.into());
        self
    }
}

impl EnrollmentSignatureVerifier for Ed25519EnrollmentTrustStore {
    fn verify_signature(
        &self,
        signature: &EnrollmentSignature,
    ) -> Result<(), EnrollmentSignatureVerificationError> {
        if self.revoked_key_ids.contains(&signature.key_id) {
            return Err(EnrollmentSignatureVerificationError::RevokedKey);
        }
        let key = self
            .verifying_keys
            .get(&signature.key_id)
            .ok_or(EnrollmentSignatureVerificationError::UntrustedKey)?;
        let bytes = URL_SAFE_NO_PAD
            .decode(&signature.value)
            .map_err(|_| EnrollmentSignatureVerificationError::InvalidEncoding)?;
        let decoded_signature = Signature::try_from(bytes.as_slice())
            .map_err(|_| EnrollmentSignatureVerificationError::InvalidEncoding)?;
        key.verify(signature.subject_digest.as_bytes(), &decoded_signature)
            .map_err(|_| EnrollmentSignatureVerificationError::InvalidSignature)
    }
}

pub fn sign_enrollment_offer(
    mut offer: EnrollmentOffer,
    signer: &dyn EnrollmentSigner,
) -> Result<EnrollmentOffer, Vec<EnrollmentContractIssue>> {
    offer.protocol = ENROLLMENT_OFFER_PROTOCOL.to_owned();
    offer
        .requested_capabilities
        .sort_by(|left, right| left.contract_id.cmp(&right.contract_id));
    offer.signature = unsigned_signature(signer.key_id());
    let issues = validate_offer_content(&offer);
    if !issues.is_empty() {
        return Err(issues);
    }
    let digest = enrollment_offer_digest(&offer);
    offer.signature = signer.sign_digest(&digest).map_err(|message| {
        vec![issue(
            EnrollmentContractIssueCode::SignatureInvalid,
            "$.signature",
            message,
        )]
    })?;
    Ok(offer)
}

pub fn verify_enrollment_offer(
    offer: &EnrollmentOffer,
    verifier: &dyn EnrollmentSignatureVerifier,
    now_unix_ms: u64,
) -> Result<String, Vec<EnrollmentContractIssue>> {
    let mut issues = validate_offer_content(offer);
    let digest = enrollment_offer_digest(offer);
    verify_bound_signature(&offer.signature, &digest, verifier, &mut issues);
    if offer.expires_at_unix_ms <= now_unix_ms {
        issues.push(issue(
            EnrollmentContractIssueCode::InvalidLifetime,
            "$.expiresAtUnixMs",
            "Enrollment Offer has expired",
        ));
    }
    if issues.is_empty() {
        Ok(digest)
    } else {
        Err(issues)
    }
}

pub fn sign_enrollment_receipt(
    mut receipt: EnrollmentReceipt,
    signer: &dyn EnrollmentSigner,
) -> Result<EnrollmentReceipt, Vec<EnrollmentContractIssue>> {
    receipt.protocol = ENROLLMENT_RECEIPT_PROTOCOL.to_owned();
    receipt
        .granted_capabilities
        .sort_by(|left, right| left.contract_id.cmp(&right.contract_id));
    receipt.signature = unsigned_signature(signer.key_id());
    let issues = validate_receipt_content(&receipt);
    if !issues.is_empty() {
        return Err(issues);
    }
    let digest = enrollment_receipt_digest(&receipt);
    receipt.signature = signer.sign_digest(&digest).map_err(|message| {
        vec![issue(
            EnrollmentContractIssueCode::SignatureInvalid,
            "$.signature",
            message,
        )]
    })?;
    Ok(receipt)
}

pub fn verify_enrollment_receipt(
    receipt: &EnrollmentReceipt,
    offer: &EnrollmentOffer,
    verifier: &dyn EnrollmentSignatureVerifier,
    now_unix_ms: u64,
) -> Result<String, Vec<EnrollmentContractIssue>> {
    let mut issues = validate_receipt_content(receipt);
    let expected_offer_digest = enrollment_offer_digest(offer);
    if receipt.offer_digest != expected_offer_digest
        || receipt.system_id != offer.system_id
        || receipt.console_service_principal != offer.console_service_principal
        || receipt.nonce != offer.nonce
    {
        issues.push(issue(
            EnrollmentContractIssueCode::DigestMismatch,
            "$.offerDigest",
            "Enrollment Receipt is not bound to the exact Enrollment Offer",
        ));
    }
    if receipt.issued_at_unix_ms < offer.issued_at_unix_ms
        || receipt.expires_at_unix_ms > offer.expires_at_unix_ms
        || receipt.expires_at_unix_ms <= now_unix_ms
    {
        issues.push(issue(
            EnrollmentContractIssueCode::InvalidLifetime,
            "$.expiresAtUnixMs",
            "Enrollment Receipt lifetime is expired or exceeds its bound Offer",
        ));
    }
    if receipt.granted_policy != offer.requested_policy
        || receipt.granted_capabilities.iter().any(|granted| {
            !offer.requested_capabilities.iter().any(|requested| {
                requested.contract_id == granted.contract_id
                    && requested.schema_digest == granted.schema_digest
                    && granted.feature_ids.is_subset(&requested.feature_ids)
            })
        })
    {
        issues.push(issue(
            EnrollmentContractIssueCode::InvalidCapability,
            "$.grantedCapabilities",
            "Enrollment Receipt cannot widen or substitute the Offer capability and policy request",
        ));
    }
    let digest = enrollment_receipt_digest(receipt);
    verify_bound_signature(&receipt.signature, &digest, verifier, &mut issues);
    if issues.is_empty() {
        Ok(digest)
    } else {
        Err(issues)
    }
}

#[must_use]
pub fn enrollment_offer_digest(offer: &EnrollmentOffer) -> String {
    digest_without_signature(offer)
}

#[must_use]
pub fn enrollment_receipt_digest(receipt: &EnrollmentReceipt) -> String {
    digest_without_signature(receipt)
}

#[must_use]
pub fn enrollment_offer_schema() -> Value {
    let mut schema = contract_schema::<EnrollmentOffer>(
        "https://contracts.lenso.local/system-plane/lenso.system-plane.enrollment-offer.v1.schema.json",
        "Lenso System Plane Enrollment Offer",
        ENROLLMENT_OFFER_PROTOCOL,
    );
    patch_common_schema(&mut schema);
    schema
}

#[must_use]
pub fn enrollment_receipt_schema() -> Value {
    let mut schema = contract_schema::<EnrollmentReceipt>(
        "https://contracts.lenso.local/system-plane/lenso.system-plane.enrollment-receipt.v1.schema.json",
        "Lenso System Plane Enrollment Receipt",
        ENROLLMENT_RECEIPT_PROTOCOL,
    );
    patch_common_schema(&mut schema);
    schema["properties"]["offerDigest"]["pattern"] = json!("^sha256:[0-9a-f]{64}$");
    schema["properties"]["grantRevision"]["minimum"] = json!(1);
    for field in [
        "managedServiceId",
        "managedServicePrincipal",
        "managedServiceRevision",
    ] {
        schema["properties"][field]["minLength"] = json!(1);
    }
    schema
}

fn validate_offer_content(offer: &EnrollmentOffer) -> Vec<EnrollmentContractIssue> {
    let mut issues = Vec::new();
    if offer.protocol != ENROLLMENT_OFFER_PROTOCOL {
        issues.push(issue(
            EnrollmentContractIssueCode::InvalidProtocol,
            "$.protocol",
            "Enrollment Offer protocol is unsupported",
        ));
    }
    validate_common(
        &offer.system_id,
        &offer.console_service_principal,
        &offer.nonce,
        offer.issued_at_unix_ms,
        offer.expires_at_unix_ms,
        &offer.requested_capabilities,
        &offer.requested_policy,
        &mut issues,
    );
    issues
}

fn validate_receipt_content(receipt: &EnrollmentReceipt) -> Vec<EnrollmentContractIssue> {
    let mut issues = Vec::new();
    if receipt.protocol != ENROLLMENT_RECEIPT_PROTOCOL {
        issues.push(issue(
            EnrollmentContractIssueCode::InvalidProtocol,
            "$.protocol",
            "Enrollment Receipt protocol is unsupported",
        ));
    }
    validate_common(
        &receipt.system_id,
        &receipt.console_service_principal,
        &receipt.nonce,
        receipt.issued_at_unix_ms,
        receipt.expires_at_unix_ms,
        &receipt.granted_capabilities,
        &receipt.granted_policy,
        &mut issues,
    );
    if receipt.managed_service_id.trim().is_empty()
        || receipt.managed_service_principal.trim().is_empty()
        || receipt.managed_service_revision.trim().is_empty()
    {
        issues.push(issue(
            EnrollmentContractIssueCode::MissingIdentity,
            "$.managedServiceId",
            "Enrollment Receipt requires the managed Service identity, principal, and revision",
        ));
    }
    if !valid_digest(&receipt.offer_digest) {
        issues.push(issue(
            EnrollmentContractIssueCode::DigestMismatch,
            "$.offerDigest",
            "Enrollment Receipt requires a canonical Offer digest",
        ));
    }
    if receipt.grant_revision == 0 {
        issues.push(issue(
            EnrollmentContractIssueCode::InvalidRevision,
            "$.grantRevision",
            "Enrollment Grant revision must be positive",
        ));
    }
    issues
}

fn validate_common(
    system_id: &str,
    console_principal: &str,
    nonce: &str,
    issued_at: u64,
    expires_at: u64,
    capabilities: &[EnrollmentCapabilityGrant],
    policy: &EnrollmentPolicyGrant,
    issues: &mut Vec<EnrollmentContractIssue>,
) {
    if system_id.trim().is_empty() || console_principal.trim().is_empty() {
        issues.push(issue(
            EnrollmentContractIssueCode::MissingIdentity,
            "$.systemId",
            "Enrollment requires System and Console Service identities",
        ));
    }
    if nonce.len() < 16 {
        issues.push(issue(
            EnrollmentContractIssueCode::InvalidNonce,
            "$.nonce",
            "Enrollment nonce must contain at least 16 characters",
        ));
    }
    if issued_at == 0 || expires_at <= issued_at {
        issues.push(issue(
            EnrollmentContractIssueCode::InvalidLifetime,
            "$.expiresAtUnixMs",
            "Enrollment expiry must follow a positive issue time",
        ));
    }
    let mut seen = BTreeSet::new();
    for (index, capability) in capabilities.iter().enumerate() {
        if !seen.insert(&capability.contract_id) {
            issues.push(issue(
                EnrollmentContractIssueCode::DuplicateCapability,
                format!("$.capabilities[{index}]"),
                "Enrollment capability contracts must be unique",
            ));
        }
        if index > 0 && capabilities[index - 1].contract_id >= capability.contract_id {
            issues.push(issue(
                EnrollmentContractIssueCode::InvalidCapability,
                format!("$.capabilities[{index}]"),
                "Enrollment capability contracts must use canonical contract order",
            ));
        }
        if !valid_contract_id(&capability.contract_id)
            || !valid_digest(&capability.schema_digest)
            || capability
                .feature_ids
                .iter()
                .any(|feature| !valid_feature_id(feature))
        {
            issues.push(issue(
                EnrollmentContractIssueCode::InvalidCapability,
                format!("$.capabilities[{index}]"),
                "Enrollment capability grant is not canonical",
            ));
        }
    }
    if policy.policy_id.trim().is_empty()
        || policy.policy_revision.trim().is_empty()
        || !valid_digest(&policy.policy_digest)
    {
        issues.push(issue(
            EnrollmentContractIssueCode::InvalidPolicy,
            "$.policy",
            "Enrollment policy identity, revision, and digest are required",
        ));
    }
}

fn verify_bound_signature(
    signature: &EnrollmentSignature,
    digest: &str,
    verifier: &dyn EnrollmentSignatureVerifier,
    issues: &mut Vec<EnrollmentContractIssue>,
) {
    if signature.subject_digest != digest {
        issues.push(issue(
            EnrollmentContractIssueCode::DigestMismatch,
            "$.signature.subjectDigest",
            "Enrollment signature is not bound to the canonical artifact digest",
        ));
        return;
    }
    if let Err(verification_error) = verifier.verify_signature(signature) {
        let (code, message) = match verification_error {
            EnrollmentSignatureVerificationError::UntrustedKey => (
                EnrollmentContractIssueCode::SignerUntrusted,
                "Enrollment signing key is not trusted",
            ),
            EnrollmentSignatureVerificationError::RevokedKey => (
                EnrollmentContractIssueCode::SignerUntrusted,
                "Enrollment signing key is revoked",
            ),
            EnrollmentSignatureVerificationError::InvalidEncoding => (
                EnrollmentContractIssueCode::SignatureInvalid,
                "Enrollment signature encoding is invalid",
            ),
            EnrollmentSignatureVerificationError::InvalidSignature => (
                EnrollmentContractIssueCode::SignatureInvalid,
                "Enrollment signature is invalid",
            ),
        };
        issues.push(issue(code, "$.signature", message));
    }
}

fn digest_without_signature<T: Serialize>(artifact: &T) -> String {
    let mut value = serde_json::to_value(artifact).expect("Enrollment artifact serializes");
    value
        .as_object_mut()
        .expect("Enrollment artifact is an object")
        .remove("signature");
    let bytes = serde_json::to_vec(&value).expect("Enrollment artifact serializes to bytes");
    format!("sha256:{}", hex(&Sha256::digest(bytes)))
}

fn unsigned_signature(key_id: &str) -> EnrollmentSignature {
    EnrollmentSignature {
        algorithm: EnrollmentSignatureAlgorithm::Ed25519,
        key_id: key_id.to_owned(),
        subject_digest: String::new(),
        value: String::new(),
    }
}

fn contract_schema<T: JsonSchema>(id: &str, title: &str, protocol: &str) -> Value {
    let mut schema =
        serde_json::to_value(schemars::schema_for!(T)).expect("Enrollment schema serializes");
    schema["$id"] = Value::String(id.to_owned());
    schema["title"] = Value::String(title.to_owned());
    schema["properties"]["protocol"] = json!({ "const": protocol });
    schema
}

fn patch_common_schema(schema: &mut Value) {
    for field in ["systemId", "consoleServicePrincipal"] {
        schema["properties"][field]["minLength"] = json!(1);
    }
    schema["properties"]["nonce"]["minLength"] = json!(16);
    schema["properties"]["issuedAtUnixMs"]["minimum"] = json!(1);
    schema["properties"]["expiresAtUnixMs"]["minimum"] = json!(1);
    let capability = &mut schema["$defs"]["EnrollmentCapabilityGrant"]["properties"];
    capability["contractId"]["pattern"] =
        json!(r"^lenso\.system-plane\.[a-z0-9]+(?:[.-][a-z0-9]+)*\.v[1-9][0-9]*$");
    capability["schemaDigest"]["pattern"] = json!("^sha256:[0-9a-f]{64}$");
    capability["featureIds"]["items"]["pattern"] = json!("^[a-z0-9]+(?:-[a-z0-9]+)*$");
    let policy = &mut schema["$defs"]["EnrollmentPolicyGrant"]["properties"];
    policy["policyId"]["minLength"] = json!(1);
    policy["policyRevision"]["minLength"] = json!(1);
    policy["policyDigest"]["pattern"] = json!("^sha256:[0-9a-f]{64}$");
    let signature = &mut schema["$defs"]["EnrollmentSignature"]["properties"];
    signature["keyId"]["minLength"] = json!(1);
    signature["subjectDigest"]["pattern"] = json!("^sha256:[0-9a-f]{64}$");
    signature["value"]["pattern"] = json!("^[A-Za-z0-9_-]{86}$");
}

fn valid_contract_id(value: &str) -> bool {
    let Some((prefix, major)) = value.rsplit_once(".v") else {
        return false;
    };
    prefix.starts_with("lenso.system-plane.")
        && !prefix.ends_with('.')
        && prefix.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        })
        && !major.starts_with('0')
        && major.parse::<u32>().is_ok_and(|major| major > 0)
}

fn valid_feature_id(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn issue(
    code: EnrollmentContractIssueCode,
    path: impl Into<String>,
    message: impl Into<String>,
) -> EnrollmentContractIssue {
    EnrollmentContractIssue {
        code,
        path: path.into(),
        message: message.into(),
    }
}
