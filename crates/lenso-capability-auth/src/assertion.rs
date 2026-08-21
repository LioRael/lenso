use std::{collections::BTreeMap, fmt};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use lenso_kernel::{InvocationContext, InvocationContextError, SealedInvocationExtension};
use ring::hmac;
use serde::Serialize;

use crate::{ACTOR_ASSERTION_EXTENSION, AuthActorAssertion, AuthResponse, AuthResponseKind};

/// The non-error outcomes of the Auth Capability.
#[derive(Clone, Debug, PartialEq)]
pub enum AuthOutcome {
    /// No credential was selected for this ingress path.
    Absent,
    /// Authenticated evidence with a short-lived sealed assertion.
    Authenticated(ActorAssertion),
}

/// Converts a generated response into the semantic Auth outcome.
pub fn decode_auth_response(response: AuthResponse) -> Result<AuthOutcome, AuthResponseError> {
    match (response.kind, response.assertion) {
        (AuthResponseKind::Absent, None) => Ok(AuthOutcome::Absent),
        (AuthResponseKind::Authenticated, Some(assertion)) => Ok(AuthOutcome::Authenticated(
            ActorAssertion::from_wire(assertion)?,
        )),
        (kind, assertion) => Err(AuthResponseError::InconsistentOutcome {
            kind: format!("{kind:?}"),
            has_assertion: assertion.is_some(),
        }),
    }
}

/// Creates the generated Auth response for an authenticated result.
pub fn authenticated_response(assertion: &ActorAssertion) -> AuthResponse {
    AuthResponse {
        kind: AuthResponseKind::Authenticated,
        assertion: Some(assertion.to_wire()),
    }
}

/// Creates the generated Auth response for an absent credential.
pub const fn absent_response() -> AuthResponse {
    AuthResponse {
        kind: AuthResponseKind::Absent,
        assertion: None,
    }
}

/// Error returned when a provider emits an invalid Auth response shape.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthResponseError {
    /// `kind` and assertion presence did not agree.
    InconsistentOutcome { kind: String, has_assertion: bool },
    /// A wire assertion was missing a required invariant.
    InvalidAssertionWire,
}

/// Stable Capability/Operation audience identity.
pub fn audience(capability_id: &str, operation: &str) -> String {
    format!("{capability_id}:{operation}")
}

/// Short-lived assertion validity represented in portable monotonic nanoseconds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Validity {
    /// The first Driver-monotonic instant at which the assertion is valid.
    pub issued_at_nanos: u64,
    /// The exclusive Driver-monotonic expiry instant.
    pub expires_at_nanos: u64,
}

impl Validity {
    /// Creates one validity interval.
    pub const fn new(issued_at_nanos: u64, expires_at_nanos: u64) -> Self {
        Self {
            issued_at_nanos,
            expires_at_nanos,
        }
    }
}

/// An Auth-issued assertion that can only be verified through its issuer.
#[derive(Clone, Debug, PartialEq)]
pub struct ActorAssertion {
    wire: AuthActorAssertion,
}

impl ActorAssertion {
    fn from_wire(wire: AuthActorAssertion) -> Result<Self, AuthResponseError> {
        let Ok(issued_at_nanos) = wire.issued_at_nanos.parse::<u64>() else {
            return Err(AuthResponseError::InvalidAssertionWire);
        };
        let Ok(expires_at_nanos) = wire.expires_at_nanos.parse::<u64>() else {
            return Err(AuthResponseError::InvalidAssertionWire);
        };
        if wire.issuer.is_empty()
            || wire.subject.is_empty()
            || wire.actor_kind.is_empty()
            || wire.assurance.is_empty()
            || wire.audience.is_empty()
            || wire.audience.iter().any(|audience| audience.is_empty())
            || wire.proof.is_empty()
            || issued_at_nanos > expires_at_nanos
        {
            return Err(AuthResponseError::InvalidAssertionWire);
        }
        Ok(Self { wire })
    }

    /// Returns the issuer provenance.
    pub fn issuer(&self) -> &str {
        &self.wire.issuer
    }

    /// Returns the authenticated subject identifier.
    pub fn subject(&self) -> &str {
        &self.wire.subject
    }

    /// Returns the issuer-namespaced actor kind.
    pub fn actor_kind(&self) -> &str {
        &self.wire.actor_kind
    }

    /// Returns the assurance level established by Auth.
    pub fn assurance(&self) -> &str {
        &self.wire.assurance
    }

    /// Returns the stable Capability/Operation audience entries.
    pub fn audience(&self) -> &[String] {
        &self.wire.audience
    }

    /// Returns issuer-namespaced claims carried by this assertion.
    pub fn claims(&self) -> Option<&BTreeMap<String, serde_json::Value>> {
        self.wire.claims.as_ref()
    }

    /// Returns the optional bounded delegation provenance reference.
    pub fn parent_provenance(&self) -> Option<&str> {
        self.wire.parent_provenance.as_deref()
    }

    /// Returns the signed proof reference.
    pub fn proof(&self) -> &str {
        &self.wire.proof
    }

    /// Returns the issued-at monotonic instant.
    pub fn issued_at_nanos(&self) -> u64 {
        self.wire
            .issued_at_nanos
            .parse()
            .expect("ActorAssertion was structurally validated")
    }

    /// Returns the exclusive expiry monotonic instant.
    pub fn expires_at_nanos(&self) -> u64 {
        self.wire
            .expires_at_nanos
            .parse()
            .expect("ActorAssertion was structurally validated")
    }

    /// Returns a copy suitable for the generated portable Auth response.
    pub fn to_wire(&self) -> AuthActorAssertion {
        self.wire.clone()
    }

    /// Attaches the assertion as a sealed Kernel extension.
    pub fn attach(
        &self,
        context: InvocationContext,
    ) -> Result<InvocationContext, InvocationContextError> {
        let value = serde_json::to_vec(&self.wire)
            .expect("generated ActorAssertion wire value must serialize");
        context.with_sealed_extension(SealedInvocationExtension::new(
            ACTOR_ASSERTION_EXTENSION,
            self.issuer(),
            self.audience().iter().cloned(),
            value,
        ))
    }

    /// Reads an assertion from the sealed extension without granting it validity.
    pub fn from_context(context: &InvocationContext) -> Result<Self, AuthResponseError> {
        let Some(extension) = context.sealed_extension(ACTOR_ASSERTION_EXTENSION) else {
            return Err(AuthResponseError::InvalidAssertionWire);
        };
        let wire = serde_json::from_slice(extension.value())
            .map_err(|_| AuthResponseError::InvalidAssertionWire)?;
        let assertion = Self::from_wire(wire)?;
        if assertion.issuer() != extension.issuer() || assertion.audience() != extension.audience()
        {
            return Err(AuthResponseError::InvalidAssertionWire);
        }
        Ok(assertion)
    }
}

/// A bounded identity projection owned by the target Module or SDK.
pub trait TypedActor: Sized {
    /// Projects a domain-specific Actor after generic assertion checks succeed.
    fn from_assertion(assertion: &ActorAssertion) -> Result<Self, ActorProjectionError>;
}

/// Failure returned while projecting a sealed assertion into a typed Actor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActorProjectionError {
    /// The assertion was not issued by the verifier's configured Auth Module.
    Assertion(AssertionValidationError),
    /// The target Module requested a different actor kind.
    UnexpectedActorKind { expected: String, actual: String },
}

impl From<AssertionValidationError> for ActorProjectionError {
    fn from(error: AssertionValidationError) -> Self {
        Self::Assertion(error)
    }
}

/// Failure returned when generic ActorAssertion validation fails.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssertionValidationError {
    /// Issuer provenance does not match the configured verifier.
    IssuerMismatch { expected: String, actual: String },
    /// The proof does not cover the assertion fields.
    InvalidProof,
    /// The assertion is not valid yet.
    NotYetValid {
        issued_at_nanos: u64,
        now_nanos: u64,
    },
    /// The assertion has expired.
    Expired {
        expires_at_nanos: u64,
        now_nanos: u64,
    },
    /// The assertion does not cover the requested Capability/Operation.
    AudienceMismatch { audience: String },
    /// The requested delegation would widen the parent assertion.
    DelegationWidensAuthority,
    /// The requested delegation interval is outside the parent validity.
    DelegationWidensValidity,
}

/// Auth issuer/verifier for one configured issuer and signing key.
#[derive(Clone)]
pub struct ActorAssertionIssuer {
    issuer: String,
    signing_key: Vec<u8>,
}

impl fmt::Debug for ActorAssertionIssuer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ActorAssertionIssuer")
            .field("issuer", &self.issuer)
            .field("signing_key", &"<redacted>")
            .finish()
    }
}

impl ActorAssertionIssuer {
    /// Creates one issuer/verifier from App-selected secret material.
    pub fn new(issuer: impl Into<String>, signing_key: impl AsRef<[u8]>) -> Self {
        Self {
            issuer: issuer.into(),
            signing_key: signing_key.as_ref().to_vec(),
        }
    }

    /// Returns the stable issuer identity.
    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    /// Issues a short-lived authenticated assertion.
    pub fn issue(
        &self,
        subject: impl Into<String>,
        actor_kind: impl Into<String>,
        assurance: impl Into<String>,
        audience: impl IntoIterator<Item = String>,
        validity: Validity,
        claims: BTreeMap<String, serde_json::Value>,
    ) -> ActorAssertion {
        let mut wire = AuthActorAssertion {
            actor_kind: actor_kind.into(),
            assurance: assurance.into(),
            audience: audience.into_iter().collect(),
            claims: Some(claims),
            expires_at_nanos: validity.expires_at_nanos.to_string(),
            issued_at_nanos: validity.issued_at_nanos.to_string(),
            issuer: self.issuer.clone(),
            parent_provenance: None,
            proof: String::new(),
            subject: subject.into(),
        };
        wire.proof = self.sign(&wire);
        ActorAssertion { wire }
    }

    /// Narrows one assertion to an already-covered audience and validity.
    pub fn attenuate(
        &self,
        parent: &ActorAssertion,
        audience: impl IntoIterator<Item = String>,
        expires_at_nanos: u64,
    ) -> Result<ActorAssertion, AssertionValidationError> {
        self.verify_proof(parent)?;
        let audience = audience.into_iter().collect::<Vec<_>>();
        if audience
            .iter()
            .any(|entry| !parent.audience().contains(entry))
        {
            return Err(AssertionValidationError::DelegationWidensAuthority);
        }
        if expires_at_nanos > parent.expires_at_nanos() {
            return Err(AssertionValidationError::DelegationWidensValidity);
        }
        let mut wire = parent.to_wire();
        wire.audience = audience;
        wire.expires_at_nanos = expires_at_nanos.to_string();
        wire.parent_provenance = Some(parent.proof().to_owned());
        wire.proof = self.sign(&wire);
        Ok(ActorAssertion { wire })
    }

    /// Verifies proof, provenance, validity, and audience before projection.
    pub fn project<T: TypedActor>(
        &self,
        assertion: &ActorAssertion,
        expected_audience: &str,
        now_nanos: u64,
    ) -> Result<T, ActorProjectionError> {
        self.verify_for(assertion, expected_audience, now_nanos)?;
        T::from_assertion(assertion)
    }

    /// Validates and projects the ActorAssertion carried by one Invocation Context.
    pub fn project_context<T: TypedActor>(
        &self,
        context: &InvocationContext,
        expected_audience: &str,
        now_nanos: u64,
    ) -> Result<T, ActorProjectionError> {
        let assertion = ActorAssertion::from_context(context)
            .map_err(|_| ActorProjectionError::Assertion(AssertionValidationError::InvalidProof))?;
        self.project(&assertion, expected_audience, now_nanos)
    }

    /// Decodes and verifies a generated wire assertion at a provider boundary.
    pub fn decode_and_verify(
        &self,
        wire: AuthActorAssertion,
        expected_audience: &str,
        now_nanos: u64,
    ) -> Result<ActorAssertion, AssertionValidationError> {
        let assertion =
            ActorAssertion::from_wire(wire).map_err(|_| AssertionValidationError::InvalidProof)?;
        self.verify_for(&assertion, expected_audience, now_nanos)?;
        Ok(assertion)
    }

    fn verify_for(
        &self,
        assertion: &ActorAssertion,
        expected_audience: &str,
        now_nanos: u64,
    ) -> Result<(), AssertionValidationError> {
        self.verify_proof(assertion)?;
        if !assertion
            .audience()
            .iter()
            .any(|entry| entry == expected_audience)
        {
            return Err(AssertionValidationError::AudienceMismatch {
                audience: expected_audience.to_owned(),
            });
        }
        if now_nanos < assertion.issued_at_nanos() {
            return Err(AssertionValidationError::NotYetValid {
                issued_at_nanos: assertion.issued_at_nanos(),
                now_nanos,
            });
        }
        if now_nanos >= assertion.expires_at_nanos() {
            return Err(AssertionValidationError::Expired {
                expires_at_nanos: assertion.expires_at_nanos(),
                now_nanos,
            });
        }
        Ok(())
    }

    fn verify_proof(&self, assertion: &ActorAssertion) -> Result<(), AssertionValidationError> {
        if assertion.issuer() != self.issuer {
            return Err(AssertionValidationError::IssuerMismatch {
                expected: self.issuer.clone(),
                actual: assertion.issuer().to_owned(),
            });
        }
        let key = hmac::Key::new(hmac::HMAC_SHA256, &self.signing_key);
        let expected = URL_SAFE_NO_PAD
            .decode(assertion.proof())
            .map_err(|_| AssertionValidationError::InvalidProof)?;
        hmac::verify(
            &key,
            self.signing_payload(&assertion.wire).as_bytes(),
            &expected,
        )
        .map_err(|_| AssertionValidationError::InvalidProof)
    }

    fn sign(&self, assertion: &AuthActorAssertion) -> String {
        let key = hmac::Key::new(hmac::HMAC_SHA256, &self.signing_key);
        let tag = hmac::sign(&key, self.signing_payload(assertion).as_bytes());
        URL_SAFE_NO_PAD.encode(tag.as_ref())
    }

    fn signing_payload(&self, assertion: &AuthActorAssertion) -> String {
        #[derive(Serialize)]
        struct SigningPayload<'a> {
            actor_kind: &'a str,
            assurance: &'a str,
            audience: &'a [String],
            claims: Option<&'a BTreeMap<String, serde_json::Value>>,
            expires_at_nanos: &'a str,
            issued_at_nanos: &'a str,
            issuer: &'a str,
            parent_provenance: Option<&'a str>,
            subject: &'a str,
        }

        serde_json::to_string(&SigningPayload {
            actor_kind: &assertion.actor_kind,
            assurance: &assertion.assurance,
            audience: &assertion.audience,
            claims: assertion.claims.as_ref(),
            expires_at_nanos: &assertion.expires_at_nanos,
            issued_at_nanos: &assertion.issued_at_nanos,
            issuer: &assertion.issuer,
            parent_provenance: assertion.parent_provenance.as_deref(),
            subject: &assertion.subject,
        })
        .expect("fixed ActorAssertion signing payload must serialize")
    }
}
