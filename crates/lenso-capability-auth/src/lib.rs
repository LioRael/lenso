//! Generated portable contract for the Auth Capability.

mod generated {
    include!("generated.rs");
}

pub use generated::*;

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
