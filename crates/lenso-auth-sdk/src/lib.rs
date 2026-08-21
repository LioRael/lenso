//! Portable Auth semantics shared by ingress Adapters and target Modules.

use std::{collections::BTreeMap, fmt};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use lenso_capability_auth::{
    AuthActorAssertion, AuthRequest, AuthResponse, AuthResponseKind, AuthenticateRequestCredential,
};
use lenso_kernel::{InvocationContext, InvocationContextError, SealedInvocationExtension};
use serde::Serialize;
use sha2::Sha256;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

/// Stable extension key used for authenticated Actor assertions.
pub const ACTOR_ASSERTION_EXTENSION: &str = "lenso.auth.actor-assertion";

/// Protocol-neutral evidence already selected by an ingress Adapter.
#[derive(Clone)]
pub struct CredentialEvidence {
    wire: AuthenticateRequestCredential,
}

impl CredentialEvidence {
    /// Creates evidence after protocol-specific extraction and selection.
    pub fn new(scheme: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            wire: AuthenticateRequestCredential {
                scheme: scheme.into(),
                value: value.into(),
            },
        }
    }

    /// Returns the credential scheme.
    pub fn scheme(&self) -> &str {
        &self.wire.scheme
    }

    /// Returns the opaque credential material.
    pub fn value(&self) -> &str {
        &self.wire.value
    }
}

impl fmt::Debug for CredentialEvidence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialEvidence")
            .field("scheme", &self.scheme())
            .field("value", &"<redacted>")
            .finish()
    }
}

/// Builds the generated Auth request from Adapter-selected evidence.
pub fn authenticate_request(evidence: Option<CredentialEvidence>) -> AuthRequest {
    AuthRequest {
        credential: evidence.map(|evidence| evidence.wire),
    }
}

/// The non-error outcomes of the Auth Capability.
#[derive(Clone, Debug, PartialEq)]
pub enum AuthOutcome {
    /// No credential was selected for this ingress path.
    Absent,
    /// Authenticated evidence with a short-lived assertion.
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

/// Parsed RFC 3339 assertion validity interval.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Validity {
    issued_at: OffsetDateTime,
    expires_at: OffsetDateTime,
}

impl Validity {
    /// Creates a non-empty validity interval.
    pub fn new(
        issued_at: OffsetDateTime,
        expires_at: OffsetDateTime,
    ) -> Result<Self, AssertionValidationError> {
        if issued_at >= expires_at {
            return Err(AssertionValidationError::InvalidValidity);
        }
        Ok(Self {
            issued_at,
            expires_at,
        })
    }
}

/// Source of wall-clock time used consistently across Rust and Bun runtimes.
pub trait AssertionClock: fmt::Debug {
    /// Returns the current UTC instant.
    fn now(&self) -> OffsetDateTime;
}

/// Fixed clock for deterministic providers and tests.
#[derive(Clone, Copy, Debug)]
pub struct FixedClock(OffsetDateTime);

impl FixedClock {
    /// Creates a fixed clock.
    pub const fn new(now: OffsetDateTime) -> Self {
        Self(now)
    }
}

impl AssertionClock for FixedClock {
    fn now(&self) -> OffsetDateTime {
        self.0
    }
}

/// An Auth-issued assertion that requires verification before projection.
#[derive(Clone, Debug, PartialEq)]
pub struct ActorAssertion {
    wire: AuthActorAssertion,
    issued_at: OffsetDateTime,
    expires_at: OffsetDateTime,
}

impl ActorAssertion {
    fn from_wire(wire: AuthActorAssertion) -> Result<Self, AuthResponseError> {
        let issued_at = OffsetDateTime::parse(&wire.issued_at, &Rfc3339)
            .map_err(|_| AuthResponseError::InvalidAssertionWire)?;
        let expires_at = OffsetDateTime::parse(&wire.expires_at, &Rfc3339)
            .map_err(|_| AuthResponseError::InvalidAssertionWire)?;
        if wire.issuer.is_empty()
            || wire.subject.is_empty()
            || wire.actor_kind.is_empty()
            || wire.assurance.is_empty()
            || wire.audience.is_empty()
            || wire.audience.iter().any(String::is_empty)
            || wire.proof.is_empty()
            || issued_at >= expires_at
        {
            return Err(AuthResponseError::InvalidAssertionWire);
        }
        Ok(Self {
            wire,
            issued_at,
            expires_at,
        })
    }

    /// Returns issuer provenance.
    pub fn issuer(&self) -> &str {
        &self.wire.issuer
    }
    /// Returns the authenticated subject.
    pub fn subject(&self) -> &str {
        &self.wire.subject
    }
    /// Returns the actor kind.
    pub fn actor_kind(&self) -> &str {
        &self.wire.actor_kind
    }
    /// Returns assertion audiences.
    pub fn audience(&self) -> &[String] {
        &self.wire.audience
    }
    /// Returns the signed proof.
    pub fn proof(&self) -> &str {
        &self.wire.proof
    }
    /// Returns delegation provenance when present.
    pub fn parent_provenance(&self) -> Option<&str> {
        self.wire.parent_provenance.as_deref()
    }
    /// Returns a portable wire copy.
    pub fn to_wire(&self) -> AuthActorAssertion {
        self.wire.clone()
    }

    /// Attaches this assertion to the portable invocation context.
    pub fn attach(
        &self,
        context: InvocationContext,
    ) -> Result<InvocationContext, InvocationContextError> {
        let value = serde_json::to_vec(&self.wire)
            .expect("generated ActorAssertion wire value must serialize");
        context.with_sealed_extension(SealedInvocationExtension::signed(
            ACTOR_ASSERTION_EXTENSION,
            self.issuer(),
            self.audience().iter().cloned(),
            value,
            self.proof(),
        ))
    }

    fn from_context(context: &InvocationContext) -> Result<Self, AuthResponseError> {
        let extension = context
            .sealed_extension(ACTOR_ASSERTION_EXTENSION)
            .ok_or(AuthResponseError::InvalidAssertionWire)?;
        let wire = serde_json::from_slice(extension.value())
            .map_err(|_| AuthResponseError::InvalidAssertionWire)?;
        let assertion = Self::from_wire(wire)?;
        if assertion.issuer() != extension.issuer()
            || assertion.audience() != extension.audience()
            || assertion.proof() != extension.proof()
        {
            return Err(AuthResponseError::InvalidAssertionWire);
        }
        Ok(assertion)
    }
}

/// A target-owned projection from a verified generic assertion.
pub trait TypedActor: Sized {
    /// Builds the target's actor type.
    fn from_assertion(assertion: &ActorAssertion) -> Result<Self, ActorProjectionError>;
}

/// Projection failure at a target boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActorProjectionError {
    /// Generic assertion validation failed.
    Assertion(AssertionValidationError),
    /// The target expected another actor kind.
    UnexpectedActorKind { expected: String, actual: String },
}

impl From<AssertionValidationError> for ActorProjectionError {
    fn from(error: AssertionValidationError) -> Self {
        Self::Assertion(error)
    }
}

/// Generic assertion validation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssertionValidationError {
    /// Issuer does not match the configured Auth Module.
    IssuerMismatch { expected: String, actual: String },
    /// The cryptographic proof is invalid.
    InvalidProof,
    /// The interval is empty or reversed.
    InvalidValidity,
    /// The assertion is not valid yet.
    NotYetValid,
    /// The assertion has expired.
    Expired,
    /// The assertion does not cover this target.
    AudienceMismatch { audience: String },
    /// Delegation widened authority.
    DelegationWidensAuthority,
    /// Delegation widened validity.
    DelegationWidensValidity,
}

/// Issuer and verifier configured for one Auth Module.
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
    /// Creates an issuer/verifier from App-selected secret material.
    pub fn new(issuer: impl Into<String>, signing_key: impl AsRef<[u8]>) -> Self {
        Self {
            issuer: issuer.into(),
            signing_key: signing_key.as_ref().to_vec(),
        }
    }

    /// Issues a signed assertion.
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
            expires_at: format_timestamp(validity.expires_at),
            issued_at: format_timestamp(validity.issued_at),
            issuer: self.issuer.clone(),
            parent_provenance: None,
            proof: String::new(),
            subject: subject.into(),
        };
        wire.proof = self.sign(&wire);
        ActorAssertion {
            wire,
            issued_at: validity.issued_at,
            expires_at: validity.expires_at,
        }
    }

    /// Narrows an assertion to an existing audience and earlier expiry.
    pub fn attenuate(
        &self,
        parent: &ActorAssertion,
        audience: impl IntoIterator<Item = String>,
        expires_at: OffsetDateTime,
    ) -> Result<ActorAssertion, AssertionValidationError> {
        self.verify_proof(parent)?;
        let audience = audience.into_iter().collect::<Vec<_>>();
        if audience
            .iter()
            .any(|entry| !parent.audience().contains(entry))
        {
            return Err(AssertionValidationError::DelegationWidensAuthority);
        }
        if expires_at > parent.expires_at || expires_at <= parent.issued_at {
            return Err(AssertionValidationError::DelegationWidensValidity);
        }
        let mut wire = parent.to_wire();
        wire.audience = audience;
        wire.expires_at = format_timestamp(expires_at);
        wire.parent_provenance = Some(parent.proof().to_owned());
        wire.proof = self.sign(&wire);
        Ok(ActorAssertion {
            wire,
            issued_at: parent.issued_at,
            expires_at,
        })
    }

    /// Verifies and projects the assertion carried by a target context.
    pub fn project_context<T: TypedActor>(
        &self,
        context: &InvocationContext,
        capability_id: &str,
        operation: &str,
        clock: &dyn AssertionClock,
    ) -> Result<T, ActorProjectionError> {
        let assertion = ActorAssertion::from_context(context)
            .map_err(|_| AssertionValidationError::InvalidProof)?;
        self.verify_for(&assertion, &audience(capability_id, operation), clock.now())?;
        T::from_assertion(&assertion)
    }

    fn verify_for(
        &self,
        assertion: &ActorAssertion,
        expected_audience: &str,
        now: OffsetDateTime,
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
        if now < assertion.issued_at {
            return Err(AssertionValidationError::NotYetValid);
        }
        if now >= assertion.expires_at {
            return Err(AssertionValidationError::Expired);
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
        let expected = URL_SAFE_NO_PAD
            .decode(assertion.proof())
            .map_err(|_| AssertionValidationError::InvalidProof)?;
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.signing_key)
            .expect("HMAC accepts keys of any size");
        mac.update(self.signing_payload(&assertion.wire).as_bytes());
        mac.verify_slice(&expected)
            .map_err(|_| AssertionValidationError::InvalidProof)
    }

    fn sign(&self, assertion: &AuthActorAssertion) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.signing_key)
            .expect("HMAC accepts keys of any size");
        mac.update(self.signing_payload(assertion).as_bytes());
        URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    }

    fn signing_payload(&self, assertion: &AuthActorAssertion) -> String {
        #[derive(Serialize)]
        struct SigningPayload<'a> {
            actor_kind: &'a str,
            assurance: &'a str,
            audience: &'a [String],
            claims: Option<&'a BTreeMap<String, serde_json::Value>>,
            expires_at: &'a str,
            issued_at: &'a str,
            issuer: &'a str,
            parent_provenance: Option<&'a str>,
            subject: &'a str,
        }
        serde_json::to_string(&SigningPayload {
            actor_kind: &assertion.actor_kind,
            assurance: &assertion.assurance,
            audience: &assertion.audience,
            claims: assertion.claims.as_ref(),
            expires_at: &assertion.expires_at,
            issued_at: &assertion.issued_at,
            issuer: &assertion.issuer,
            parent_provenance: assertion.parent_provenance.as_deref(),
            subject: &assertion.subject,
        })
        .expect("fixed assertion payload must serialize")
    }
}

fn format_timestamp(timestamp: OffsetDateTime) -> String {
    timestamp
        .format(&Rfc3339)
        .expect("OffsetDateTime always formats as RFC 3339")
}

#[cfg(test)]
mod tests {
    use super::*;
    use lenso_kernel::{CancellationToken, InvocationContext};
    use time::Duration;

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct UserActor(String);

    impl TypedActor for UserActor {
        fn from_assertion(assertion: &ActorAssertion) -> Result<Self, ActorProjectionError> {
            if assertion.actor_kind() != "user" {
                return Err(ActorProjectionError::UnexpectedActorKind {
                    expected: "user".to_owned(),
                    actual: assertion.actor_kind().to_owned(),
                });
            }
            Ok(Self(assertion.subject().to_owned()))
        }
    }

    #[test]
    fn assertions_use_rfc3339_and_are_verified_at_the_target_boundary() {
        let now = OffsetDateTime::from_unix_timestamp(1_800_000_000).expect("valid timestamp");
        let issuer = ActorAssertionIssuer::new("auth.users", b"shared-auth-key");
        let assertion = issuer.issue(
            "user-123",
            "user",
            "strong",
            [audience("example.secure@1", "read")],
            Validity::new(now - Duration::seconds(1), now + Duration::minutes(1))
                .expect("valid interval"),
            BTreeMap::new(),
        );
        let context = assertion
            .attach(InvocationContext::new(1, None, CancellationToken::new()))
            .expect("assertion should attach");
        let actor = issuer
            .project_context::<UserActor>(
                &context,
                "example.secure@1",
                "read",
                &FixedClock::new(now),
            )
            .expect("target-bound assertion should project");

        assert_eq!(actor, UserActor("user-123".to_owned()));
        assert!(assertion.to_wire().issued_at.contains('T'));
        assert!(
            !format!(
                "{:?}",
                authenticate_request(Some(CredentialEvidence::new("bearer", "secret")))
            )
            .contains("secret")
        );
        assert!(
            issuer
                .project_context::<UserActor>(
                    &context,
                    "example.other@1",
                    "read",
                    &FixedClock::new(now),
                )
                .is_err()
        );
    }
}
