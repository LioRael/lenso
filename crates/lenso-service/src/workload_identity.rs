use crate::{ContextClaimProof, ServicePrincipal};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{
    Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, decode_header, encode,
    errors::ErrorKind,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use spiffe::{
    JwtSource, JwtSvid, JwtSvidError, SpiffeId, TrustDomain, X509Source, X509Svid,
    x509_source::SvidPicker,
};
use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::{
        RwLock,
        atomic::{AtomicU64, Ordering},
    },
};

const DEVELOPMENT_ISSUER: &str = "lenso-system-sandbox-development-only";
const CREDENTIAL_KIND: &str = "lenso.workload-identity.v1";
const MAX_SANDBOX_CREDENTIAL_TTL_MS: u64 = 5 * 60 * 1_000;

/// Proof supplied by a transport adapter after it authenticates the connection.
/// It is deliberately separate from endpoint, hostname, IP, replica, and region metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthenticatedTransportBinding(pub String);

impl AuthenticatedTransportBinding {
    #[must_use]
    pub fn new(proof: impl Into<String>) -> Self {
        Self(proof.into())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkloadIdentityErrorCode {
    DevelopmentProviderForbidden,
    InvalidRequest,
    InvalidProof,
    IssuerMismatch,
    AudienceMismatch,
    CredentialExpired,
    TransportBindingMismatch,
    StaleCredential,
    RotationFailed,
    ProviderUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkloadIdentityEvidence {
    pub outcome: String,
    pub service_principal: Option<String>,
    pub credential_id: Option<String>,
    pub key_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkloadIdentityError {
    pub code: WorkloadIdentityErrorCode,
    pub message: String,
    pub evidence: WorkloadIdentityEvidence,
}

impl std::fmt::Display for WorkloadIdentityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for WorkloadIdentityError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkloadCredentialRequest {
    pub service_principal: String,
    pub audience: String,
    pub authenticated_transport_binding: String,
    pub issued_at_unix_ms: u64,
    pub ttl_ms: u64,
}

impl WorkloadCredentialRequest {
    #[must_use]
    pub fn new(
        service_principal: impl Into<String>,
        audience: impl Into<String>,
        authenticated_transport_binding: impl Into<String>,
        issued_at_unix_ms: u64,
        ttl_ms: u64,
    ) -> Self {
        Self {
            service_principal: service_principal.into(),
            audience: audience.into(),
            authenticated_transport_binding: authenticated_transport_binding.into(),
            issued_at_unix_ms,
            ttl_ms,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkloadCredential {
    pub token: String,
    pub issuer: String,
    pub service_principal: String,
    pub audience: String,
    pub expires_at_unix_ms: u64,
    pub credential_id: String,
    pub key_id: String,
    pub algorithm: String,
}

impl WorkloadCredential {
    #[must_use]
    pub fn service_principal_context(&self) -> ServicePrincipal {
        ServicePrincipal {
            issuer: self.issuer.clone(),
            subject: self.service_principal.clone(),
            audiences: vec![self.audience.clone()],
            expires_at_unix_ms: self.expires_at_unix_ms,
            credential_id: self.credential_id.clone(),
            proof: ContextClaimProof {
                verification_method: self.key_id.clone(),
                algorithm: self.algorithm.clone(),
                signature: self.token.clone(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkloadIdentityVerification {
    pub audience: String,
    pub authenticated_transport_binding: String,
    pub now_unix_ms: u64,
}

impl WorkloadIdentityVerification {
    #[must_use]
    pub fn new(
        audience: impl Into<String>,
        authenticated_transport_binding: impl Into<String>,
        now_unix_ms: u64,
    ) -> Self {
        Self {
            audience: audience.into(),
            authenticated_transport_binding: authenticated_transport_binding.into(),
            now_unix_ms,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticatedServicePrincipal {
    pub service_principal: String,
    pub credential_id: String,
    pub issuer: String,
    pub audience: String,
    pub expires_at_unix_ms: u64,
    pub key_id: String,
    pub algorithm: String,
    pub evidence: WorkloadIdentityEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkloadIdentityRotationEvidence {
    pub outcome: String,
    pub previous_key_id: String,
    pub active_key_id: String,
}

pub trait WorkloadIdentityProvider: std::fmt::Debug + Send + Sync {
    fn issue(
        &self,
        request: WorkloadCredentialRequest,
    ) -> Result<WorkloadCredential, WorkloadIdentityError>;

    /// Asynchronous credential issuance for providers backed by networked or
    /// local-agent protocols. Existing synchronous providers inherit a
    /// source-compatible adapter around `issue`.
    fn issue_async(
        &self,
        request: WorkloadCredentialRequest,
    ) -> Pin<Box<dyn Future<Output = Result<WorkloadCredential, WorkloadIdentityError>> + Send + '_>>
    {
        Box::pin(async move { self.issue(request) })
    }

    fn verify(
        &self,
        token: &str,
        verification: &WorkloadIdentityVerification,
    ) -> Result<AuthenticatedServicePrincipal, WorkloadIdentityError>;
}

/// Operator-owned SPIFFE composition for one stable Lenso Service Principal.
/// The Workload API endpoint supplies both JWT-SVIDs and rotating X.509-SVIDs;
/// Lenso never receives certificate-authority signing material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpiffeWorkloadIdentityConfig {
    endpoint: String,
    trust_domain: TrustDomain,
    service_principal: String,
    spiffe_id: SpiffeId,
}

impl SpiffeWorkloadIdentityConfig {
    pub fn new(
        endpoint: impl Into<String>,
        trust_domain: impl AsRef<str>,
        service_principal: impl Into<String>,
    ) -> Result<Self, WorkloadIdentityError> {
        let endpoint = endpoint.into();
        let service_principal = service_principal.into();
        if endpoint.trim().is_empty() {
            return Err(identity_error(
                WorkloadIdentityErrorCode::InvalidRequest,
                "invalid_spiffe_endpoint",
                "SPIFFE Workload Identity requires a Workload API endpoint",
                Some(service_principal),
                None,
                None,
            ));
        }
        let Some(service_id) = service_principal.strip_prefix("service:") else {
            return Err(invalid_service_principal());
        };
        if !valid_service_principal(&service_principal) {
            return Err(invalid_service_principal());
        }
        let trust_domain = TrustDomain::new(trust_domain.as_ref()).map_err(|_| {
            identity_error(
                WorkloadIdentityErrorCode::InvalidRequest,
                "invalid_spiffe_trust_domain",
                "SPIFFE Workload Identity requires a valid trust domain",
                Some(service_principal.clone()),
                None,
                None,
            )
        })?;
        let spiffe_id = SpiffeId::from_segments(trust_domain.clone(), &["service", service_id])
            .map_err(|_| invalid_service_principal())?;
        Ok(Self {
            endpoint,
            trust_domain,
            service_principal,
            spiffe_id,
        })
    }

    #[must_use]
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    #[must_use]
    pub const fn trust_domain(&self) -> &TrustDomain {
        &self.trust_domain
    }

    #[must_use]
    pub fn service_principal(&self) -> &str {
        &self.service_principal
    }

    #[must_use]
    pub const fn spiffe_id(&self) -> &SpiffeId {
        &self.spiffe_id
    }
}

#[derive(Debug)]
struct ExactSpiffeIdPicker(SpiffeId);

impl SvidPicker for ExactSpiffeIdPicker {
    fn pick_svid(&self, svids: &[std::sync::Arc<X509Svid>]) -> Option<usize> {
        svids.iter().position(|svid| svid.spiffe_id() == &self.0)
    }
}

/// Production Workload Identity provider backed by a SPIFFE Workload API.
/// JWT-SVIDs authenticate the application request and are bound to the peer
/// SPIFFE ID authenticated by the live X.509-SVID mTLS connection.
#[derive(Debug)]
pub struct SpiffeWorkloadIdentityProvider {
    config: SpiffeWorkloadIdentityConfig,
    jwt_source: JwtSource,
    x509_source: X509Source,
}

impl SpiffeWorkloadIdentityProvider {
    pub async fn connect(
        config: SpiffeWorkloadIdentityConfig,
    ) -> Result<Self, WorkloadIdentityError> {
        let x509_source = X509Source::builder()
            .endpoint(config.endpoint())
            .picker(ExactSpiffeIdPicker(config.spiffe_id().clone()))
            .build()
            .await
            .map_err(|_| provider_unavailable("spiffe_x509_source_unavailable"))?;
        let selected = x509_source
            .svid()
            .map_err(|_| provider_unavailable("spiffe_x509_source_unavailable"))?;
        if selected.spiffe_id() != config.spiffe_id() {
            x509_source.shutdown().await;
            return Err(identity_error(
                WorkloadIdentityErrorCode::IssuerMismatch,
                "spiffe_identity_mismatch",
                "The SPIFFE Workload API did not provide the configured Service identity",
                Some(config.service_principal().to_owned()),
                None,
                None,
            ));
        }
        let jwt_source = match JwtSource::builder()
            .endpoint(config.endpoint())
            .build()
            .await
        {
            Ok(source) => source,
            Err(_) => {
                x509_source.shutdown().await;
                return Err(provider_unavailable("spiffe_jwt_source_unavailable"));
            }
        };
        Ok(Self {
            config,
            jwt_source,
            x509_source,
        })
    }

    #[must_use]
    pub fn config(&self) -> &SpiffeWorkloadIdentityConfig {
        &self.config
    }

    /// Live X.509 material for `spiffe-rustls` mTLS composition. New handshakes
    /// consume rotated SVIDs without rebuilding this provider.
    #[must_use]
    pub fn x509_source(&self) -> X509Source {
        self.x509_source.clone()
    }

    #[must_use]
    pub fn authenticated_transport_binding(
        peer_spiffe_id: &SpiffeId,
    ) -> AuthenticatedTransportBinding {
        AuthenticatedTransportBinding::new(peer_spiffe_id.to_string())
    }

    pub async fn shutdown(&self) {
        self.jwt_source.shutdown().await;
        self.x509_source.shutdown().await;
    }
}

impl WorkloadIdentityProvider for SpiffeWorkloadIdentityProvider {
    fn issue(
        &self,
        request: WorkloadCredentialRequest,
    ) -> Result<WorkloadCredential, WorkloadIdentityError> {
        Err(identity_error(
            WorkloadIdentityErrorCode::ProviderUnavailable,
            "spiffe_async_issuance_required",
            "SPIFFE credential issuance requires the asynchronous provider method",
            Some(request.service_principal),
            None,
            None,
        ))
    }

    fn issue_async(
        &self,
        request: WorkloadCredentialRequest,
    ) -> Pin<Box<dyn Future<Output = Result<WorkloadCredential, WorkloadIdentityError>> + Send + '_>>
    {
        Box::pin(async move {
            if request.service_principal != self.config.service_principal
                || request.audience.trim().is_empty()
                || request.authenticated_transport_binding != self.config.spiffe_id.to_string()
                || request.ttl_ms == 0
            {
                return Err(identity_error(
                    WorkloadIdentityErrorCode::InvalidRequest,
                    "invalid_spiffe_credential_request",
                    "SPIFFE credentials require the configured Service Principal, audience, mTLS peer identity, and positive lifetime",
                    Some(request.service_principal),
                    None,
                    None,
                ));
            }
            let svid = self
                .jwt_source
                .fetch_jwt_svid_with_id([&request.audience], Some(&self.config.spiffe_id))
                .await
                .map_err(|_| provider_unavailable("spiffe_credential_issuance_failed"))?;
            if svid.spiffe_id() != &self.config.spiffe_id {
                return Err(identity_error(
                    WorkloadIdentityErrorCode::IssuerMismatch,
                    "spiffe_identity_mismatch",
                    "The SPIFFE Workload API issued an unexpected Service identity",
                    Some(request.service_principal),
                    None,
                    Some(svid.key_id().to_owned()),
                ));
            }
            let expires_at_unix_ms = expiry_unix_ms(&svid)?;
            let maximum_expiry = request
                .issued_at_unix_ms
                .checked_add(request.ttl_ms)
                .and_then(|value| value.checked_add(999))
                .ok_or_else(|| {
                    identity_error(
                        WorkloadIdentityErrorCode::InvalidRequest,
                        "invalid_credential_lifetime",
                        "Workload credential lifetime exceeds the supported timestamp range",
                        Some(request.service_principal.clone()),
                        None,
                        Some(svid.key_id().to_owned()),
                    )
                })?;
            if expires_at_unix_ms <= request.issued_at_unix_ms
                || expires_at_unix_ms > maximum_expiry
            {
                return Err(identity_error(
                    WorkloadIdentityErrorCode::InvalidRequest,
                    "credential_lifetime_out_of_policy",
                    "The SPIFFE issuer credential lifetime exceeds the requested maximum",
                    Some(request.service_principal),
                    None,
                    Some(svid.key_id().to_owned()),
                ));
            }
            let token = svid.token().to_owned();
            let algorithm = jwt_algorithm(&token)?;
            Ok(WorkloadCredential {
                credential_id: credential_id(&token),
                token,
                issuer: format!("spiffe://{}", self.config.trust_domain),
                service_principal: self.config.service_principal.clone(),
                audience: request.audience,
                expires_at_unix_ms,
                key_id: svid.key_id().to_owned(),
                algorithm,
            })
        })
    }

    fn verify(
        &self,
        token: &str,
        verification: &WorkloadIdentityVerification,
    ) -> Result<AuthenticatedServicePrincipal, WorkloadIdentityError> {
        let svid = JwtSvid::parse_and_validate(token, &self.jwt_source, &[&verification.audience])
            .map_err(spiffe_verification_error)?;
        if svid.spiffe_id().trust_domain() != &self.config.trust_domain {
            return Err(identity_error(
                WorkloadIdentityErrorCode::IssuerMismatch,
                "issuer_mismatch",
                "SPIFFE Workload Identity belongs to an untrusted domain",
                None,
                Some(credential_id(token)),
                Some(svid.key_id().to_owned()),
            ));
        }
        let service_principal =
            service_principal_from_spiffe_id(svid.spiffe_id()).ok_or_else(invalid_proof)?;
        let expires_at_unix_ms = expiry_unix_ms(&svid)?;
        if verification.now_unix_ms >= expires_at_unix_ms {
            return Err(identity_error(
                WorkloadIdentityErrorCode::CredentialExpired,
                "credential_expired",
                "Workload Identity credential has expired",
                Some(service_principal),
                Some(credential_id(token)),
                Some(svid.key_id().to_owned()),
            ));
        }
        if verification.authenticated_transport_binding != svid.spiffe_id().to_string() {
            return Err(identity_error(
                WorkloadIdentityErrorCode::TransportBindingMismatch,
                "transport_binding_mismatch",
                "JWT-SVID identity is not bound to the authenticated X.509-SVID mTLS peer",
                Some(service_principal),
                Some(credential_id(token)),
                Some(svid.key_id().to_owned()),
            ));
        }
        let credential_id = credential_id(token);
        let key_id = svid.key_id().to_owned();
        Ok(AuthenticatedServicePrincipal {
            service_principal: service_principal.clone(),
            credential_id: credential_id.clone(),
            issuer: format!("spiffe://{}", self.config.trust_domain),
            audience: verification.audience.clone(),
            expires_at_unix_ms,
            key_id: key_id.clone(),
            algorithm: jwt_algorithm(token)?,
            evidence: WorkloadIdentityEvidence {
                outcome: "authenticated".to_owned(),
                service_principal: Some(service_principal),
                credential_id: Some(credential_id),
                key_id: Some(key_id),
            },
        })
    }
}

#[derive(Debug)]
pub struct SystemSandboxWorkloadIdentityProvider {
    state: RwLock<SystemSandboxProviderState>,
    next_credential: AtomicU64,
}

#[derive(Debug)]
struct SystemSandboxProviderState {
    secret: String,
    generation: u64,
    retired_secrets: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkloadClaims {
    iss: String,
    sub: String,
    aud: Vec<String>,
    exp: u64,
    iat: u64,
    jti: String,
    credential_kind: String,
    authenticated_transport_binding: String,
    expires_at_unix_ms: u64,
}

impl SystemSandboxWorkloadIdentityProvider {
    pub fn new(
        environment: &str,
        secret: impl Into<String>,
    ) -> Result<Self, WorkloadIdentityError> {
        if !matches!(
            environment.trim().to_ascii_lowercase().as_str(),
            "local" | "dev" | "development" | "test"
        ) {
            return Err(identity_error(
                WorkloadIdentityErrorCode::DevelopmentProviderForbidden,
                "development_provider_forbidden",
                "The System Sandbox Workload Identity provider is development-only",
                None,
                None,
                None,
            ));
        }
        let secret = secret.into();
        if secret.is_empty() {
            return Err(identity_error(
                WorkloadIdentityErrorCode::InvalidRequest,
                "invalid_provider_secret",
                "The System Sandbox signing secret must not be empty",
                None,
                None,
                None,
            ));
        }
        Ok(Self {
            state: RwLock::new(SystemSandboxProviderState {
                secret,
                generation: 1,
                retired_secrets: BTreeMap::new(),
            }),
            next_credential: AtomicU64::new(1),
        })
    }

    pub fn rotate(
        &self,
        new_secret: impl Into<String>,
    ) -> Result<WorkloadIdentityRotationEvidence, WorkloadIdentityError> {
        let new_secret = new_secret.into();
        let mut state = self.state.write().expect("identity provider lock poisoned");
        let active_key_id = key_id(state.generation);
        if new_secret.is_empty() || new_secret == state.secret {
            return Err(identity_error(
                WorkloadIdentityErrorCode::RotationFailed,
                "rotation_failed",
                "The System Sandbox credential rotation did not provide a new signing secret",
                None,
                None,
                Some(active_key_id),
            ));
        }
        let previous_key_id = active_key_id;
        let previous_secret = std::mem::replace(&mut state.secret, new_secret);
        state
            .retired_secrets
            .insert(previous_key_id.clone(), previous_secret);
        state.generation += 1;
        Ok(WorkloadIdentityRotationEvidence {
            outcome: "rotated".to_owned(),
            previous_key_id,
            active_key_id: key_id(state.generation),
        })
    }
}

impl WorkloadIdentityProvider for SystemSandboxWorkloadIdentityProvider {
    fn issue(
        &self,
        request: WorkloadCredentialRequest,
    ) -> Result<WorkloadCredential, WorkloadIdentityError> {
        if !valid_service_principal(&request.service_principal)
            || request.audience.trim().is_empty()
            || request.authenticated_transport_binding.trim().is_empty()
            || request.ttl_ms == 0
            || request.ttl_ms > MAX_SANDBOX_CREDENTIAL_TTL_MS
        {
            return Err(identity_error(
                WorkloadIdentityErrorCode::InvalidRequest,
                "invalid_credential_request",
                "Workload credentials require a Service Principal, audience, authenticated transport binding, and positive lifetime",
                None,
                None,
                None,
            ));
        }
        let expires_at_unix_ms = request
            .issued_at_unix_ms
            .checked_add(request.ttl_ms)
            .ok_or_else(|| {
                identity_error(
                    WorkloadIdentityErrorCode::InvalidRequest,
                    "invalid_credential_lifetime",
                    "Workload credential lifetime exceeds the supported timestamp range",
                    Some(request.service_principal.clone()),
                    None,
                    None,
                )
            })?;
        let sequence = self.next_credential.fetch_add(1, Ordering::SeqCst);
        let state = self.state.read().expect("identity provider lock poisoned");
        let active_key_id = key_id(state.generation);
        let credential_id = format!(
            "sandbox:{}:{}:{sequence}",
            state.generation, request.service_principal
        );
        let claims = WorkloadClaims {
            iss: DEVELOPMENT_ISSUER.to_owned(),
            sub: request.service_principal.clone(),
            aud: vec![request.audience.clone()],
            exp: expires_at_unix_ms.div_ceil(1_000),
            iat: request.issued_at_unix_ms / 1_000,
            jti: credential_id.clone(),
            credential_kind: CREDENTIAL_KIND.to_owned(),
            authenticated_transport_binding: request.authenticated_transport_binding,
            expires_at_unix_ms,
        };
        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some(active_key_id.clone());
        let token = encode(
            &header,
            &claims,
            &EncodingKey::from_secret(state.secret.as_bytes()),
        )
        .map_err(|_| {
            identity_error(
                WorkloadIdentityErrorCode::InvalidProof,
                "credential_signing_failed",
                "The System Sandbox provider could not sign the Workload Identity credential",
                Some(request.service_principal.clone()),
                Some(credential_id.clone()),
                Some(active_key_id.clone()),
            )
        })?;
        Ok(WorkloadCredential {
            token,
            issuer: DEVELOPMENT_ISSUER.to_owned(),
            service_principal: request.service_principal,
            audience: request.audience,
            expires_at_unix_ms,
            credential_id,
            key_id: active_key_id,
            algorithm: "HS256-development-only".to_owned(),
        })
    }

    fn verify(
        &self,
        token: &str,
        verification: &WorkloadIdentityVerification,
    ) -> Result<AuthenticatedServicePrincipal, WorkloadIdentityError> {
        let header = decode_header(token).map_err(|_| invalid_proof())?;
        let state = self.state.read().expect("identity provider lock poisoned");
        let active_key_id = key_id(state.generation);
        let credential_key_id = header.kid.ok_or_else(invalid_proof)?;
        let (secret, stale) = if credential_key_id == active_key_id {
            (state.secret.as_str(), false)
        } else if let Some(secret) = state.retired_secrets.get(&credential_key_id) {
            (secret.as_str(), true)
        } else {
            return Err(invalid_proof());
        };
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = false;
        validation.validate_aud = false;
        validation.required_spec_claims.clear();
        let claims = decode::<WorkloadClaims>(
            token,
            &DecodingKey::from_secret(secret.as_bytes()),
            &validation,
        )
        .map_err(|_| invalid_proof())?
        .claims;
        let evidence = |outcome: &str| WorkloadIdentityEvidence {
            outcome: outcome.to_owned(),
            service_principal: Some(claims.sub.clone()),
            credential_id: Some(claims.jti.clone()),
            key_id: Some(credential_key_id.clone()),
        };
        if claims.credential_kind != CREDENTIAL_KIND {
            return Err(WorkloadIdentityError {
                code: WorkloadIdentityErrorCode::InvalidProof,
                message: "Provider host tokens cannot be used as Workload Identity credentials"
                    .to_owned(),
                evidence: evidence("credential_kind_mismatch"),
            });
        }
        if claims.iss != DEVELOPMENT_ISSUER {
            return Err(WorkloadIdentityError {
                code: WorkloadIdentityErrorCode::IssuerMismatch,
                message: "Workload Identity issuer does not match the configured provider"
                    .to_owned(),
                evidence: evidence("issuer_mismatch"),
            });
        }
        if stale {
            return Err(WorkloadIdentityError {
                code: WorkloadIdentityErrorCode::StaleCredential,
                message:
                    "The Workload Identity credential was issued by a superseded System Sandbox key"
                        .to_owned(),
                evidence: evidence("stale_credential"),
            });
        }
        if !claims
            .aud
            .iter()
            .any(|audience| audience == &verification.audience)
        {
            return Err(WorkloadIdentityError {
                code: WorkloadIdentityErrorCode::AudienceMismatch,
                message: "Workload Identity credential is not intended for this receiver"
                    .to_owned(),
                evidence: evidence("audience_mismatch"),
            });
        }
        if verification.now_unix_ms >= claims.expires_at_unix_ms {
            return Err(WorkloadIdentityError {
                code: WorkloadIdentityErrorCode::CredentialExpired,
                message: "Workload Identity credential has expired".to_owned(),
                evidence: evidence("credential_expired"),
            });
        }
        if claims.authenticated_transport_binding != verification.authenticated_transport_binding {
            return Err(WorkloadIdentityError {
                code: WorkloadIdentityErrorCode::TransportBindingMismatch,
                message: "Workload Identity proof is not bound to the authenticated transport"
                    .to_owned(),
                evidence: evidence("transport_binding_mismatch"),
            });
        }
        let authenticated_evidence = evidence("authenticated");
        Ok(AuthenticatedServicePrincipal {
            service_principal: claims.sub,
            credential_id: claims.jti,
            issuer: claims.iss,
            audience: verification.audience.clone(),
            expires_at_unix_ms: claims.expires_at_unix_ms,
            key_id: credential_key_id,
            algorithm: "HS256-development-only".to_owned(),
            evidence: authenticated_evidence,
        })
    }
}

fn key_id(generation: u64) -> String {
    format!("system-sandbox-key-{generation}")
}

fn valid_service_principal(value: &str) -> bool {
    value.strip_prefix("service:").is_some_and(|service_id| {
        !service_id.is_empty()
            && service_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    })
}

fn invalid_service_principal() -> WorkloadIdentityError {
    identity_error(
        WorkloadIdentityErrorCode::InvalidRequest,
        "invalid_service_principal",
        "Workload Identity requires a stable Service Principal",
        None,
        None,
        None,
    )
}

fn service_principal_from_spiffe_id(spiffe_id: &SpiffeId) -> Option<String> {
    let service_id = spiffe_id.path().strip_prefix("/service/")?;
    let service_principal = format!("service:{service_id}");
    (spiffe_id.path().split('/').count() == 3 && valid_service_principal(&service_principal))
        .then_some(service_principal)
}

fn expiry_unix_ms(svid: &JwtSvid) -> Result<u64, WorkloadIdentityError> {
    u64::try_from(svid.expiry().unix_timestamp())
        .ok()
        .and_then(|seconds| seconds.checked_mul(1_000))
        .ok_or_else(|| {
            identity_error(
                WorkloadIdentityErrorCode::InvalidProof,
                "invalid_credential_expiry",
                "SPIFFE credential expiration is outside the supported timestamp range",
                None,
                None,
                Some(svid.key_id().to_owned()),
            )
        })
}

fn credential_id(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    format!("spiffe-jwt:{}", URL_SAFE_NO_PAD.encode(digest))
}

fn jwt_algorithm(token: &str) -> Result<String, WorkloadIdentityError> {
    decode_header(token)
        .map(|header| format!("{:?}", header.alg))
        .map_err(|_| invalid_proof())
}

fn spiffe_verification_error(error: JwtSvidError) -> WorkloadIdentityError {
    let (code, outcome, message) = match &error {
        JwtSvidError::InvalidAudience(_, _) => (
            WorkloadIdentityErrorCode::AudienceMismatch,
            "audience_mismatch",
            "Workload Identity credential is not intended for this receiver",
        ),
        JwtSvidError::InvalidToken(error) if matches!(error.kind(), ErrorKind::InvalidAudience) => {
            (
                WorkloadIdentityErrorCode::AudienceMismatch,
                "audience_mismatch",
                "Workload Identity credential is not intended for this receiver",
            )
        }
        JwtSvidError::InvalidToken(error)
            if matches!(error.kind(), ErrorKind::ExpiredSignature) =>
        {
            (
                WorkloadIdentityErrorCode::CredentialExpired,
                "credential_expired",
                "Workload Identity credential has expired",
            )
        }
        _ => (
            WorkloadIdentityErrorCode::InvalidProof,
            "invalid_proof",
            "SPIFFE Workload Identity proof could not be verified",
        ),
    };
    identity_error(code, outcome, message, None, None, None)
}

fn provider_unavailable(outcome: &str) -> WorkloadIdentityError {
    identity_error(
        WorkloadIdentityErrorCode::ProviderUnavailable,
        outcome,
        "SPIFFE Workload Identity material is unavailable",
        None,
        None,
        None,
    )
}

fn invalid_proof() -> WorkloadIdentityError {
    identity_error(
        WorkloadIdentityErrorCode::InvalidProof,
        "invalid_proof",
        "Workload Identity proof could not be verified",
        None,
        None,
        None,
    )
}

fn identity_error(
    code: WorkloadIdentityErrorCode,
    outcome: &str,
    message: &str,
    service_principal: Option<String>,
    credential_id: Option<String>,
    key_id: Option<String>,
) -> WorkloadIdentityError {
    WorkloadIdentityError {
        code,
        message: message.to_owned(),
        evidence: WorkloadIdentityEvidence {
            outcome: outcome.to_owned(),
            service_principal,
            credential_id,
            key_id,
        },
    }
}
