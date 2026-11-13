use crate::{ActionError, ErrorCode};
use std::future::Future;

/// Awaits native asynchronous policies in order, skipping the second after any failure.
///
/// # Errors
/// Preserves the first failing policy's error unchanged.
pub async fn all_of_async<A, B, FA, FB>(first: A, second: B) -> Result<(), ActionError>
where
    A: FnOnce() -> FA + Send,
    B: FnOnce() -> FB + Send,
    FA: Future<Output = Result<(), ActionError>> + Send,
    FB: Future<Output = Result<(), ActionError>> + Send,
{
    first().await?;
    second().await
}

/// Tries an asynchronous alternative only after an access denial, preserving dependency failures.
///
/// # Errors
/// Preserves failures with the same denial precedence as [`any_of`].
pub async fn any_of_async<A, B, FA, FB>(first: A, second: B) -> Result<(), ActionError>
where
    A: FnOnce() -> FA + Send,
    B: FnOnce() -> FB + Send,
    FA: Future<Output = Result<(), ActionError>> + Send,
    FB: Future<Output = Result<(), ActionError>> + Send,
{
    let first = match first().await {
        Ok(()) => return Ok(()),
        Err(error)
            if matches!(
                error.code(),
                ErrorCode::Forbidden | ErrorCode::Unauthenticated
            ) =>
        {
            error
        }
        Err(error) => return Err(error),
    };
    alternative(first, second().await)
}

fn alternative(first: ActionError, second: Result<(), ActionError>) -> Result<(), ActionError> {
    match second {
        Ok(()) => Ok(()),
        Err(error)
            if error.code() == ErrorCode::Unauthenticated
                && first.code() == ErrorCode::Forbidden =>
        {
            Err(first)
        }
        Err(error) => Err(error),
    }
}

/// Checks both native policies in order, skipping the second after any failure.
///
/// # Errors
/// Preserves the first failing policy's error unchanged.
pub fn all_of(
    first: impl FnOnce() -> Result<(), ActionError>,
    second: impl FnOnce() -> Result<(), ActionError>,
) -> Result<(), ActionError> {
    first()?;
    second()
}

/// Tries a second policy only after an access denial, never after an implementation failure.
///
/// # Errors
/// Preserves dependency failures. When both deny, `forbidden` takes precedence over `unauthenticated`.
pub fn any_of(
    first: impl FnOnce() -> Result<(), ActionError>,
    second: impl FnOnce() -> Result<(), ActionError>,
) -> Result<(), ActionError> {
    let first = match first() {
        Ok(()) => return Ok(()),
        Err(error)
            if matches!(
                error.code(),
                ErrorCode::Forbidden | ErrorCode::Unauthenticated
            ) =>
        {
            error
        }
        Err(error) => return Err(error),
    };
    alternative(first, second())
}

/// A caller established by trusted authentication code, never by request body fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Principal {
    /// Stable identity within the application's identity domain.
    pub subject: String,
    /// Explicit grants supplied by the authentication integration.
    pub scopes: Vec<String>,
}

/// Requires an authenticated principal.
///
/// # Errors
/// Returns `unauthenticated` when no principal exists.
pub fn require_principal(principal: Option<&Principal>) -> Result<&Principal, ActionError> {
    principal.ok_or_else(|| ActionError::new(ErrorCode::Unauthenticated, "Authentication required"))
}

/// Requires an explicit scope; no wildcard expansion or administrator bypass is implied.
///
/// # Errors
/// Returns `unauthenticated` or `forbidden` when access is denied.
pub fn require_scope(principal: Option<&Principal>, scope: &str) -> Result<(), ActionError> {
    if require_principal(principal)?
        .scopes
        .iter()
        .any(|grant| grant == scope)
    {
        Ok(())
    } else {
        Err(ActionError::new(ErrorCode::Forbidden, "Permission denied"))
    }
}

/// Requires the caller's subject to match the resource owner.
///
/// # Errors
/// Returns `unauthenticated` or `forbidden` when access is denied.
pub fn require_owner(principal: Option<&Principal>, owner: &str) -> Result<(), ActionError> {
    if require_principal(principal)?.subject == owner {
        Ok(())
    } else {
        Err(ActionError::new(ErrorCode::Forbidden, "Permission denied"))
    }
}
