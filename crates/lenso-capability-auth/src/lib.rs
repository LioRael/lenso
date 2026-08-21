//! Protocol-neutral authentication and typed Actor assertion primitives.
//!
//! This crate owns the Auth Capability and its domain semantics. The Kernel
//! only carries the resulting sealed extension and never interprets identity
//! or authorization meaning.

mod generated {
    include!("generated.rs");
}

pub use generated::*;

mod assertion;
mod credential;

pub use assertion::*;
pub use credential::*;

/// Stable extension key used for authenticated ActorAssertions.
pub const ACTOR_ASSERTION_EXTENSION: &str = "lenso.auth.actor-assertion";

/// The generated Auth Capability marker.
pub type AuthCapability = Auth;
/// The generated Auth request value.
pub type AuthRequest = AuthenticateRequest;
/// The generated Auth response value.
pub type AuthResponse = AuthenticateResponse;
/// The generated Auth Domain Error value.
pub type AuthError = AuthenticateError;
/// The generated assertion wire value.
pub type AuthActorAssertion = AuthenticateResponseAssertion;
/// The generated response discriminator.
pub type AuthResponseKind = AuthenticateResponseKind;

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use lenso_kernel::{CancellationToken, InvocationContext};

    use super::*;

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct UserActor {
        subject: String,
    }

    impl TypedActor for UserActor {
        fn from_assertion(assertion: &ActorAssertion) -> Result<Self, ActorProjectionError> {
            if assertion.actor_kind() != "user" {
                return Err(ActorProjectionError::UnexpectedActorKind {
                    expected: "user".to_owned(),
                    actual: assertion.actor_kind().to_owned(),
                });
            }
            Ok(Self {
                subject: assertion.subject().to_owned(),
            })
        }
    }

    #[test]
    fn configured_ingress_selection_returns_one_credential_without_protocol_knowledge() {
        let policy = CredentialSelectionPolicy::for_scheme("bearer");
        let selected = policy
            .select([
                CredentialEvidence::new("api-key", "wrong-scheme"),
                CredentialEvidence::new("bearer", "opaque-token"),
            ])
            .expect("one configured credential should be selected")
            .expect("the configured scheme is present");

        assert_eq!(selected.scheme(), "bearer");
        assert_eq!(selected.value(), "opaque-token");
        assert!(!format!("{selected:?}").contains("opaque-token"));
        assert_eq!(authenticate_request(None).credential, None);
        assert!(
            policy
                .select([CredentialEvidence::new("api-key", "not-selected")])
                .expect("non-matching credentials should be ignored")
                .is_none()
        );
        assert!(matches!(
            policy.select([
                CredentialEvidence::new("bearer", "first"),
                CredentialEvidence::new("bearer", "second"),
            ]),
            Err(CredentialSelectionError::MultipleConfiguredCredentials { scheme })
                if scheme == "bearer"
        ));
    }

    #[test]
    fn auth_outcomes_keep_absent_and_rejection_distinct_from_runtime_failure() {
        let absent = decode_auth_response(AuthenticateResponse {
            kind: AuthenticateResponseKind::Absent,
            assertion: None,
        })
        .expect("Absent is a valid Auth outcome");
        assert_eq!(absent, AuthOutcome::Absent);
        assert_eq!(AuthError::Invalid, AuthError::Invalid);
        assert!(matches!(
            AuthInvocationError::Runtime(lenso_kernel::RuntimeFailure::Unavailable {
                capability: AUTH_CAPABILITY_ID
            }),
            AuthInvocationError::Runtime(_)
        ));
        for (error, wire) in [
            (AuthError::Invalid, "\"invalid\""),
            (AuthError::Expired, "\"expired\""),
            (AuthError::Revoked, "\"revoked\""),
            (AuthError::Unsupported, "\"unsupported\""),
        ] {
            assert_eq!(
                encode_authenticate_error(&error).expect("rejection should encode"),
                wire
            );
            assert_eq!(
                decode_authenticate_error(wire).expect("rejection should decode"),
                error
            );
        }
    }

    #[test]
    fn authenticated_assertions_are_sealed_validated_and_attenuated() {
        let issuer = ActorAssertionIssuer::new("auth.users", b"shared-auth-key");
        let greeting_audience = audience("example.secure-greeting@1", "greet");
        let assertion = issuer.issue(
            "user-123",
            "user",
            "strong",
            [
                greeting_audience.clone(),
                audience("example.profile@1", "read"),
            ],
            Validity::new(10, 100),
            BTreeMap::new(),
        );

        let context = assertion
            .attach(
                InvocationContext::new(9, None, CancellationToken::new())
                    .with_caller_instance("ingress"),
            )
            .expect("the assertion should be attached once");
        let restored = ActorAssertion::from_context(&context)
            .expect("the sealed assertion should survive the context seam");
        let actor = issuer
            .project_context::<UserActor>(&context, &greeting_audience, 50)
            .expect("the target audience and validity should verify");
        assert_eq!(
            actor,
            UserActor {
                subject: "user-123".to_owned()
            }
        );
        assert_eq!(context.caller_instance(), Some("ingress"));
        assert!(matches!(
            issuer.project::<UserActor>(&restored, "example.other@1:read", 50),
            Err(ActorProjectionError::Assertion(
                AssertionValidationError::AudienceMismatch { .. }
            ))
        ));
        let other_issuer = ActorAssertionIssuer::new("auth.other", b"shared-auth-key");
        assert!(matches!(
            other_issuer.project::<UserActor>(&restored, &greeting_audience, 50),
            Err(ActorProjectionError::Assertion(
                AssertionValidationError::IssuerMismatch { .. }
            ))
        ));

        let delegated = issuer
            .attenuate(&restored, [greeting_audience.clone()], 80)
            .expect("delegation may narrow the audience and validity");
        assert_eq!(delegated.parent_provenance(), Some(restored.proof()));
        assert!(
            issuer
                .project::<UserActor>(&delegated, &greeting_audience, 81)
                .is_err()
        );
        assert!(matches!(
            issuer.project::<UserActor>(&restored, &greeting_audience, 100),
            Err(ActorProjectionError::Assertion(
                AssertionValidationError::Expired { .. }
            ))
        ));
        assert!(
            ActorAssertion::from_context(&InvocationContext::new(
                12,
                None,
                CancellationToken::new()
            ))
            .is_err()
        );

        let mut forged = restored.to_wire();
        forged.subject = "attacker".to_owned();
        assert!(matches!(
            issuer.decode_and_verify(forged, &greeting_audience, 50),
            Err(AssertionValidationError::InvalidProof)
        ));
    }
}
