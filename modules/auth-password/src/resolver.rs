use crate::jwt::{self, JwtConfig};
use platform_core::{ActorContext, ActorResolutionRequest, ActorResolver};
use std::sync::Arc;

/// Actor resolver that validates JWT tokens issued by the auth-password module
/// when `token_strategy` is `"jwt"`.
///
/// Chain position: after `AuthActorResolver`, before the final fallback.
/// Session tokens (prefixed `sess_`) are skipped — they belong to the session resolver.
#[derive(Debug, Clone)]
pub struct JwtActorResolver {
    config: JwtConfig,
    fallback: Arc<dyn ActorResolver>,
}

impl JwtActorResolver {
    #[must_use]
    pub fn new(config: JwtConfig, fallback: Arc<dyn ActorResolver>) -> Self {
        Self { config, fallback }
    }
}

#[async_trait::async_trait]
impl ActorResolver for JwtActorResolver {
    async fn resolve_actor(&self, request: ActorResolutionRequest) -> ActorContext {
        if let Some(token) = bearer_jwt(&request) {
            if let Some(claims) = jwt::verify_token(&token, &self.config) {
                return ActorContext::User {
                    user_id: claims.sub,
                    scopes: Vec::new(),
                };
            }
        }

        self.fallback.resolve_actor(request).await
    }
}

/// Extract a Bearer token that looks like a JWT candidate.
///
/// Skips:
/// - Empty tokens
/// - `dev-user:` / `dev-service:` dev tokens (handled by DevActorResolver)
/// - `sess_`-prefixed session tokens (handled by AuthActorResolver)
fn bearer_jwt(request: &ActorResolutionRequest) -> Option<String> {
    request
        .authorization
        .as_deref()
        .and_then(|header| header.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .filter(|token| !token.starts_with("dev-user:") && !token.starts_with("dev-service:"))
        .filter(|token| !token.starts_with("sess_"))
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_bearer_jwt() {
        let request = ActorResolutionRequest {
            authorization: Some("Bearer eyJhbGciOiJIUzI1NiJ9.test".to_owned()),
            cookie: None,
        };
        assert_eq!(
            bearer_jwt(&request),
            Some("eyJhbGciOiJIUzI1NiJ9.test".to_owned())
        );
    }

    #[test]
    fn skips_session_tokens() {
        let request = ActorResolutionRequest {
            authorization: Some("Bearer sess_abc123".to_owned()),
            cookie: None,
        };
        assert!(bearer_jwt(&request).is_none());
    }

    #[test]
    fn skips_dev_tokens() {
        let request = ActorResolutionRequest {
            authorization: Some("Bearer dev-user:test".to_owned()),
            cookie: None,
        };
        assert!(bearer_jwt(&request).is_none());
    }

    #[test]
    fn returns_none_when_no_authorization() {
        let request = ActorResolutionRequest {
            authorization: None,
            cookie: None,
        };
        assert!(bearer_jwt(&request).is_none());
    }
}
