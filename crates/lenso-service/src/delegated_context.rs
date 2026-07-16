use crate::{ContextClaimProof, DelegatedActorContext, ServiceTenancyMode, TenantContext};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::OpenOptions,
    io::Write as _,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    sync::{Arc, Mutex},
};

const DEVELOPMENT_ISSUER: &str = "lenso-system-sandbox-development-only";
const CREDENTIAL_KIND: &str = "lenso.delegated-context.v1";
const MAX_SANDBOX_CREDENTIAL_TTL_MS: u64 = 5 * 60 * 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DelegatedContextErrorCode {
    DevelopmentProviderForbidden,
    InvalidRequest,
    InvalidProof,
    CredentialExpired,
    AudienceMismatch,
    DelegationRequired,
    IntentMismatch,
    PermissionRequired,
    OverbroadPermissions,
    TenantRequired,
    TenantIncompatible,
    PolicyRequired,
    EvidencePersistenceFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityDecisionEvidence {
    pub outcome: String,
    pub actor_subject: Option<String>,
    pub delegation_id: Option<String>,
    pub tenant_id: Option<String>,
    pub tenant_claim_id: Option<String>,
    pub audience: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegatedContextError {
    pub code: DelegatedContextErrorCode,
    pub message: String,
    pub evidence: IdentityDecisionEvidence,
}

impl std::fmt::Display for DelegatedContextError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for DelegatedContextError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegatedActorCredentialRequest {
    pub subject: String,
    pub audience: String,
    pub intent: String,
    pub permissions: Vec<String>,
    pub issued_at_unix_ms: u64,
    pub ttl_ms: u64,
}

impl DelegatedActorCredentialRequest {
    #[must_use]
    pub fn new<I, S>(
        subject: impl Into<String>,
        audience: impl Into<String>,
        intent: impl Into<String>,
        permissions: I,
        issued_at_unix_ms: u64,
        ttl_ms: u64,
    ) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            subject: subject.into(),
            audience: audience.into(),
            intent: intent.into(),
            permissions: permissions.into_iter().map(Into::into).collect(),
            issued_at_unix_ms,
            ttl_ms,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantCredentialRequest {
    pub tenant_id: String,
    pub actor_subject: String,
    pub delegation_id: String,
    pub audience: String,
    pub issued_at_unix_ms: u64,
    pub ttl_ms: u64,
}

impl TenantCredentialRequest {
    #[must_use]
    pub fn new(
        tenant_id: impl Into<String>,
        actor_subject: impl Into<String>,
        delegation_id: impl Into<String>,
        audience: impl Into<String>,
        issued_at_unix_ms: u64,
        ttl_ms: u64,
    ) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            actor_subject: actor_subject.into(),
            delegation_id: delegation_id.into(),
            audience: audience.into(),
            issued_at_unix_ms,
            ttl_ms,
        }
    }
}

pub trait DelegatedContextProvider: std::fmt::Debug + Send + Sync {
    fn issue_actor(
        &self,
        request: DelegatedActorCredentialRequest,
    ) -> Result<DelegatedActorContext, DelegatedContextError>;

    fn issue_tenant(
        &self,
        request: TenantCredentialRequest,
    ) -> Result<TenantContext, DelegatedContextError>;

    fn verify_actor(
        &self,
        context: &DelegatedActorContext,
        audience: &str,
        now_unix_ms: u64,
    ) -> Result<(), DelegatedContextError>;

    fn verify_tenant(
        &self,
        context: &TenantContext,
        audience: &str,
        now_unix_ms: u64,
    ) -> Result<(), DelegatedContextError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceContext {
    pub actor: DelegatedActorContext,
    pub tenant: Option<TenantContext>,
}

impl ServiceContext {
    #[must_use]
    pub const fn new(actor: DelegatedActorContext, tenant: Option<TenantContext>) -> Self {
        Self { actor, tenant }
    }
}

pub trait IdentityDecisionRecorder: std::fmt::Debug + Send + Sync {
    fn record(&self, evidence: &IdentityDecisionEvidence) -> Result<(), String>;
}

#[derive(Debug, Default)]
pub struct MemoryIdentityDecisionRecorder {
    evidence: Mutex<Vec<IdentityDecisionEvidence>>,
}

impl MemoryIdentityDecisionRecorder {
    #[must_use]
    pub fn evidence(&self) -> Vec<IdentityDecisionEvidence> {
        self.evidence
            .lock()
            .expect("evidence lock poisoned")
            .clone()
    }
}

impl IdentityDecisionRecorder for MemoryIdentityDecisionRecorder {
    fn record(&self, evidence: &IdentityDecisionEvidence) -> Result<(), String> {
        self.evidence
            .lock()
            .map_err(|_| "identity evidence lock poisoned".to_owned())?
            .push(evidence.clone());
        Ok(())
    }
}

#[derive(Debug)]
pub struct JsonlIdentityDecisionRecorder {
    file: Mutex<std::fs::File>,
}

impl JsonlIdentityDecisionRecorder {
    pub fn open(path: impl AsRef<Path>) -> std::io::Result<Self> {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map(|file| Self {
                file: Mutex::new(file),
            })
    }
}

impl IdentityDecisionRecorder for JsonlIdentityDecisionRecorder {
    fn record(&self, evidence: &IdentityDecisionEvidence) -> Result<(), String> {
        let mut line = serde_json::to_vec(evidence).map_err(|error| error.to_string())?;
        line.push(b'\n');
        self.file
            .lock()
            .map_err(|_| "identity evidence journal lock poisoned".to_owned())?
            .write_all(&line)
            .map_err(|error| error.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct ServiceContextAdmission {
    provider: Arc<dyn DelegatedContextProvider>,
    policies: std::collections::BTreeMap<String, ServiceContextPolicy>,
}

impl ServiceContextAdmission {
    #[must_use]
    pub fn new<I, S>(provider: Arc<dyn DelegatedContextProvider>, policies: I) -> Self
    where
        I: IntoIterator<Item = (S, ServiceContextPolicy)>,
        S: Into<String>,
    {
        Self {
            provider,
            policies: policies
                .into_iter()
                .map(|(boundary, policy)| (boundary.into(), policy))
                .collect(),
        }
    }

    pub fn admit(
        &self,
        boundary: &str,
        context: Option<&ServiceContext>,
        now_unix_ms: u64,
    ) -> Result<AuthenticatedServiceContext, DelegatedContextError> {
        let policy = self.policies.get(boundary).ok_or_else(|| {
            bare_error(
                DelegatedContextErrorCode::PolicyRequired,
                "Service Context policy is required for this boundary",
                boundary,
            )
        })?;
        policy.verify(
            self.provider.as_ref(),
            context.map(|item| &item.actor),
            context.and_then(|item| item.tenant.as_ref()),
            now_unix_ms,
        )
    }

    pub fn invalid_proof(&self, boundary: &str) -> DelegatedContextError {
        let audience = self
            .policies
            .get(boundary)
            .map_or(boundary, |policy| policy.audience.as_str());
        invalid_proof(audience)
    }

    #[must_use]
    pub fn tenancy_mode(&self, boundary: &str) -> Option<ServiceTenancyMode> {
        self.policies
            .get(boundary)
            .map(|policy| policy.tenancy_mode.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceContextPolicy {
    pub audience: String,
    pub intent: String,
    pub required_permissions: BTreeSet<String>,
    pub allowed_permissions: BTreeSet<String>,
    pub tenancy_mode: ServiceTenancyMode,
}

impl ServiceContextPolicy {
    #[must_use]
    pub fn new<R, RS, A, AS>(
        audience: impl Into<String>,
        intent: impl Into<String>,
        required_permissions: R,
        allowed_permissions: A,
        tenancy_mode: ServiceTenancyMode,
    ) -> Self
    where
        R: IntoIterator<Item = RS>,
        RS: Into<String>,
        A: IntoIterator<Item = AS>,
        AS: Into<String>,
    {
        Self {
            audience: audience.into(),
            intent: intent.into(),
            required_permissions: required_permissions.into_iter().map(Into::into).collect(),
            allowed_permissions: allowed_permissions.into_iter().map(Into::into).collect(),
            tenancy_mode,
        }
    }

    pub fn verify(
        &self,
        provider: &dyn DelegatedContextProvider,
        actor: Option<&DelegatedActorContext>,
        tenant: Option<&TenantContext>,
        now_unix_ms: u64,
    ) -> Result<AuthenticatedServiceContext, DelegatedContextError> {
        let actor = actor.ok_or_else(|| {
            self.error(
                DelegatedContextErrorCode::DelegationRequired,
                "Delegated Actor Context is required",
                None,
                tenant,
            )
        })?;
        provider.verify_actor(actor, &self.audience, now_unix_ms)?;
        if actor.intent != self.intent {
            return Err(self.error(
                DelegatedContextErrorCode::IntentMismatch,
                "delegated intent is not authorized for this operation",
                Some(actor),
                tenant,
            ));
        }
        let permissions = actor.permissions.iter().cloned().collect::<BTreeSet<_>>();
        if !self.required_permissions.is_subset(&permissions) {
            return Err(self.error(
                DelegatedContextErrorCode::PermissionRequired,
                "delegation does not grant every required permission",
                Some(actor),
                tenant,
            ));
        }
        if !permissions.is_subset(&self.allowed_permissions) {
            return Err(self.error(
                DelegatedContextErrorCode::OverbroadPermissions,
                "delegation contains permissions broader than this operation allows",
                Some(actor),
                tenant,
            ));
        }

        let tenant = match (&self.tenancy_mode, tenant) {
            (ServiceTenancyMode::Required, None) => {
                return Err(self.error(
                    DelegatedContextErrorCode::TenantRequired,
                    "Tenant Context is required",
                    Some(actor),
                    None,
                ));
            }
            (ServiceTenancyMode::None, Some(_)) => {
                return Err(self.error(
                    DelegatedContextErrorCode::TenantIncompatible,
                    "Tenant Context is incompatible with none tenancy mode",
                    Some(actor),
                    None,
                ));
            }
            (_, Some(tenant)) => {
                provider.verify_tenant(tenant, &self.audience, now_unix_ms)?;
                if tenant.issuer != actor.issuer
                    || tenant.expires_at_unix_ms > actor.expires_at_unix_ms
                    || tenant.actor_subject != actor.subject
                    || tenant.delegation_id != actor.delegation_id
                {
                    return Err(self.error(
                        DelegatedContextErrorCode::TenantIncompatible,
                        "Tenant Context must be bound to this actor delegation and cannot outlive it",
                        Some(actor),
                        None,
                    ));
                }
                Some(tenant.clone())
            }
            (_, None) => None,
        };

        Ok(AuthenticatedServiceContext {
            actor: actor.clone(),
            tenant: tenant.clone(),
            evidence: IdentityDecisionEvidence {
                outcome: "identity_context_accepted".to_owned(),
                actor_subject: Some(actor.subject.clone()),
                delegation_id: Some(actor.delegation_id.clone()),
                tenant_id: tenant.as_ref().map(|item| item.tenant_id.clone()),
                tenant_claim_id: tenant.as_ref().map(|item| item.claim_id.clone()),
                audience: self.audience.clone(),
            },
        })
    }

    fn error(
        &self,
        code: DelegatedContextErrorCode,
        message: impl Into<String>,
        actor: Option<&DelegatedActorContext>,
        tenant: Option<&TenantContext>,
    ) -> DelegatedContextError {
        DelegatedContextError {
            code,
            message: message.into(),
            evidence: IdentityDecisionEvidence {
                outcome: error_outcome(code).to_owned(),
                actor_subject: actor.map(|item| item.subject.clone()),
                delegation_id: actor.map(|item| item.delegation_id.clone()),
                tenant_id: tenant.map(|item| item.tenant_id.clone()),
                tenant_claim_id: tenant.map(|item| item.claim_id.clone()),
                audience: self.audience.clone(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticatedServiceContext {
    pub actor: DelegatedActorContext,
    pub tenant: Option<TenantContext>,
    pub evidence: IdentityDecisionEvidence,
}

#[derive(Debug)]
pub struct SystemSandboxDelegatedContextProvider {
    secret: String,
    next_actor: AtomicU64,
    next_tenant: AtomicU64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DelegatedClaims {
    iss: String,
    sub: String,
    aud: Vec<String>,
    exp: u64,
    iat: u64,
    jti: String,
    credential_kind: String,
    claim_kind: String,
    intent: Option<String>,
    permissions: Vec<String>,
    tenant_id: Option<String>,
    actor_subject: Option<String>,
    delegation_id: Option<String>,
    expires_at_unix_ms: u64,
}

impl SystemSandboxDelegatedContextProvider {
    pub fn new(
        environment: &str,
        secret: impl Into<String>,
    ) -> Result<Self, DelegatedContextError> {
        if !matches!(environment, "local" | "development" | "test") {
            return Err(bare_error(
                DelegatedContextErrorCode::DevelopmentProviderForbidden,
                "sandbox delegated context provider is development-only",
                "",
            ));
        }
        let secret = secret.into();
        if secret.trim().is_empty() {
            return Err(bare_error(
                DelegatedContextErrorCode::InvalidRequest,
                "sandbox delegated context secret is required",
                "",
            ));
        }
        Ok(Self {
            secret,
            next_actor: AtomicU64::new(1),
            next_tenant: AtomicU64::new(1),
        })
    }

    fn issue(
        &self,
        mut claims: DelegatedClaims,
    ) -> Result<(DelegatedClaims, ContextClaimProof), DelegatedContextError> {
        validate_issue_claims(&claims)?;
        claims.exp = claims.expires_at_unix_ms.div_ceil(1_000);
        claims.iat /= 1_000;
        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some("sandbox-delegated-context-1".to_owned());
        let token = encode(
            &header,
            &claims,
            &EncodingKey::from_secret(self.secret.as_bytes()),
        )
        .map_err(|_| invalid_proof(&claims.aud[0]))?;
        Ok((
            claims,
            ContextClaimProof {
                verification_method: "sandbox-delegated-context-1".to_owned(),
                algorithm: "HS256".to_owned(),
                signature: token,
            },
        ))
    }

    fn verify(
        &self,
        token: &str,
        audience: &str,
        now_unix_ms: u64,
        expected_kind: &str,
    ) -> Result<DelegatedClaims, DelegatedContextError> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = false;
        validation.validate_aud = false;
        validation.required_spec_claims.clear();
        let claims = decode::<DelegatedClaims>(
            token,
            &DecodingKey::from_secret(self.secret.as_bytes()),
            &validation,
        )
        .map_err(|_| invalid_proof(audience))?
        .claims;
        if claims.credential_kind != CREDENTIAL_KIND || claims.claim_kind != expected_kind {
            return Err(invalid_proof(audience));
        }
        if !claims.aud.iter().any(|item| item == audience) {
            return Err(bare_error(
                DelegatedContextErrorCode::AudienceMismatch,
                "delegated context is not intended for this audience",
                audience,
            ));
        }
        if claims.expires_at_unix_ms <= now_unix_ms {
            return Err(bare_error(
                DelegatedContextErrorCode::CredentialExpired,
                "delegated context has expired",
                audience,
            ));
        }
        Ok(claims)
    }
}

impl DelegatedContextProvider for SystemSandboxDelegatedContextProvider {
    fn issue_actor(
        &self,
        request: DelegatedActorCredentialRequest,
    ) -> Result<DelegatedActorContext, DelegatedContextError> {
        let delegation_id = format!(
            "delegation_{}",
            self.next_actor.fetch_add(1, Ordering::SeqCst)
        );
        let expires_at_unix_ms = request.issued_at_unix_ms.saturating_add(request.ttl_ms);
        let (claims, proof) = self.issue(DelegatedClaims {
            iss: DEVELOPMENT_ISSUER.to_owned(),
            sub: request.subject,
            aud: vec![request.audience],
            exp: 0,
            iat: request.issued_at_unix_ms,
            jti: delegation_id.clone(),
            credential_kind: CREDENTIAL_KIND.to_owned(),
            claim_kind: "actor".to_owned(),
            intent: Some(request.intent),
            permissions: request.permissions,
            tenant_id: None,
            actor_subject: None,
            delegation_id: None,
            expires_at_unix_ms,
        })?;
        Ok(DelegatedActorContext {
            issuer: claims.iss,
            subject: claims.sub,
            audiences: claims.aud,
            intent: claims.intent.expect("actor claims include intent"),
            permissions: claims.permissions,
            expires_at_unix_ms: claims.expires_at_unix_ms,
            delegation_id,
            proof,
        })
    }

    fn issue_tenant(
        &self,
        request: TenantCredentialRequest,
    ) -> Result<TenantContext, DelegatedContextError> {
        let claim_id = format!(
            "tenant_claim_{}",
            self.next_tenant.fetch_add(1, Ordering::SeqCst)
        );
        let expires_at_unix_ms = request.issued_at_unix_ms.saturating_add(request.ttl_ms);
        let (claims, proof) = self.issue(DelegatedClaims {
            iss: DEVELOPMENT_ISSUER.to_owned(),
            sub: request.actor_subject.clone(),
            aud: vec![request.audience],
            exp: 0,
            iat: request.issued_at_unix_ms,
            jti: claim_id.clone(),
            credential_kind: CREDENTIAL_KIND.to_owned(),
            claim_kind: "tenant".to_owned(),
            intent: None,
            permissions: Vec::new(),
            tenant_id: Some(request.tenant_id),
            actor_subject: Some(request.actor_subject),
            delegation_id: Some(request.delegation_id),
            expires_at_unix_ms,
        })?;
        Ok(TenantContext {
            issuer: claims.iss,
            tenant_id: claims.tenant_id.expect("tenant claims include tenant id"),
            actor_subject: claims
                .actor_subject
                .expect("tenant claims include actor subject"),
            delegation_id: claims
                .delegation_id
                .expect("tenant claims include delegation id"),
            audiences: claims.aud,
            expires_at_unix_ms: claims.expires_at_unix_ms,
            claim_id,
            proof,
        })
    }

    fn verify_actor(
        &self,
        context: &DelegatedActorContext,
        audience: &str,
        now_unix_ms: u64,
    ) -> Result<(), DelegatedContextError> {
        let claims = self.verify(&context.proof.signature, audience, now_unix_ms, "actor")?;
        if context.proof.algorithm != "HS256"
            || context.proof.verification_method != "sandbox-delegated-context-1"
            || context.issuer != claims.iss
            || context.subject != claims.sub
            || context.audiences != claims.aud
            || context.intent != claims.intent.as_deref().unwrap_or_default()
            || context.permissions != claims.permissions
            || context.expires_at_unix_ms != claims.expires_at_unix_ms
            || context.delegation_id != claims.jti
        {
            return Err(invalid_proof(audience));
        }
        Ok(())
    }

    fn verify_tenant(
        &self,
        context: &TenantContext,
        audience: &str,
        now_unix_ms: u64,
    ) -> Result<(), DelegatedContextError> {
        let claims = self.verify(&context.proof.signature, audience, now_unix_ms, "tenant")?;
        if context.proof.algorithm != "HS256"
            || context.proof.verification_method != "sandbox-delegated-context-1"
            || context.issuer != claims.iss
            || Some(context.tenant_id.as_str()) != claims.tenant_id.as_deref()
            || Some(context.actor_subject.as_str()) != claims.actor_subject.as_deref()
            || Some(context.delegation_id.as_str()) != claims.delegation_id.as_deref()
            || context.audiences != claims.aud
            || context.expires_at_unix_ms != claims.expires_at_unix_ms
            || context.claim_id != claims.jti
        {
            return Err(invalid_proof(audience));
        }
        Ok(())
    }
}

fn validate_issue_claims(claims: &DelegatedClaims) -> Result<(), DelegatedContextError> {
    let ttl_ms = claims.expires_at_unix_ms.saturating_sub(claims.iat);
    let actor_invalid = claims.claim_kind == "actor"
        && (claims.intent.as_deref().is_none_or(str::is_empty)
            || claims.permissions.is_empty()
            || claims.permissions.iter().any(String::is_empty));
    if claims.sub.trim().is_empty()
        || claims.aud.len() != 1
        || claims.aud[0].trim().is_empty()
        || ttl_ms == 0
        || ttl_ms > MAX_SANDBOX_CREDENTIAL_TTL_MS
        || actor_invalid
        || (claims.claim_kind == "tenant"
            && (claims.tenant_id.as_deref().is_none_or(str::is_empty)
                || claims.actor_subject.as_deref().is_none_or(str::is_empty)
                || claims.delegation_id.as_deref().is_none_or(str::is_empty)))
    {
        return Err(bare_error(
            DelegatedContextErrorCode::InvalidRequest,
            "delegated context request must be bounded and non-empty",
            claims.aud.first().map_or("", String::as_str),
        ));
    }
    Ok(())
}

fn invalid_proof(audience: &str) -> DelegatedContextError {
    bare_error(
        DelegatedContextErrorCode::InvalidProof,
        "delegated context proof is invalid",
        audience,
    )
}

fn bare_error(
    code: DelegatedContextErrorCode,
    message: impl Into<String>,
    audience: &str,
) -> DelegatedContextError {
    DelegatedContextError {
        code,
        message: message.into(),
        evidence: IdentityDecisionEvidence {
            outcome: error_outcome(code).to_owned(),
            actor_subject: None,
            delegation_id: None,
            tenant_id: None,
            tenant_claim_id: None,
            audience: audience.to_owned(),
        },
    }
}

const fn error_outcome(code: DelegatedContextErrorCode) -> &'static str {
    match code {
        DelegatedContextErrorCode::DevelopmentProviderForbidden => "development_provider_forbidden",
        DelegatedContextErrorCode::InvalidRequest => "delegated_context_invalid_request",
        DelegatedContextErrorCode::InvalidProof => "delegated_context_invalid_proof",
        DelegatedContextErrorCode::CredentialExpired => "delegated_context_expired",
        DelegatedContextErrorCode::AudienceMismatch => "delegated_context_audience_mismatch",
        DelegatedContextErrorCode::DelegationRequired => "delegation_required",
        DelegatedContextErrorCode::IntentMismatch => "delegated_intent_mismatch",
        DelegatedContextErrorCode::PermissionRequired => "delegated_permission_required",
        DelegatedContextErrorCode::OverbroadPermissions => "delegated_permissions_overbroad",
        DelegatedContextErrorCode::TenantRequired => "tenant_context_required",
        DelegatedContextErrorCode::TenantIncompatible => "tenant_context_incompatible",
        DelegatedContextErrorCode::PolicyRequired => "service_context_policy_required",
        DelegatedContextErrorCode::EvidencePersistenceFailed => {
            "identity_evidence_persistence_failed"
        }
    }
}
