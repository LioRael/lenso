use platform_core::{ActorContext, ActorResolutionRequest, ActorResolver, AppResult, DbPool};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::sync::Arc;

const SESSION_COOKIE: &str = "lenso_session";

#[derive(Debug, Clone)]
pub struct AuthActorResolver {
    pool: DbPool,
    fallback: Arc<dyn ActorResolver>,
}

impl AuthActorResolver {
    #[must_use]
    pub fn new(pool: DbPool, fallback: Arc<dyn ActorResolver>) -> Self {
        Self { pool, fallback }
    }

    async fn resolve_session_token(&self, token: &str) -> AppResult<Option<String>> {
        let token_hash = session_token_hash(token);
        sqlx::query_scalar::<_, String>(
            r#"
            select users.id
            from auth.sessions sessions
            join auth.users users on users.id = sessions.user_id
            where sessions.token_hash = $1
              and sessions.expires_at > now()
              and sessions.revoked_at is null
              and users.disabled_at is null
            limit 1
            "#,
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|source| {
            platform_core::AppError::new(
                platform_core::ErrorCode::Internal,
                "Failed to resolve auth session",
            )
            .with_source(source)
        })
    }
}

#[async_trait::async_trait]
impl ActorResolver for AuthActorResolver {
    async fn resolve_actor(&self, request: ActorResolutionRequest) -> ActorContext {
        let tokens = session_tokens(&request);
        for token in tokens {
            match self.resolve_session_token(&token).await {
                Ok(Some(user_id)) => {
                    return ActorContext::User {
                        user_id,
                        scopes: Vec::new(),
                    };
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(error = ?error, "failed to resolve auth session");
                }
            }
        }

        self.fallback.resolve_actor(request).await
    }
}

pub fn session_token_hash(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    let mut encoded = String::with_capacity("sha256:".len() + digest.len() * 2);
    encoded.push_str("sha256:");
    for byte in digest {
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

pub fn first_session_token(request: &ActorResolutionRequest) -> Option<String> {
    session_tokens(request).into_iter().next()
}

fn session_tokens(request: &ActorResolutionRequest) -> Vec<String> {
    let mut tokens = Vec::new();
    if let Some(token) = request
        .authorization
        .as_deref()
        .and_then(bearer_token)
        .map(ToOwned::to_owned)
    {
        tokens.push(token);
    }
    if let Some(token) = request.cookie.as_deref().and_then(session_cookie) {
        tokens.push(token);
    }
    tokens
}

fn bearer_token(header: &str) -> Option<&str> {
    header
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .filter(|token| !token.starts_with("dev-user:") && !token.starts_with("dev-service:"))
}

fn session_cookie(header: &str) -> Option<String> {
    header.split(';').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        (name == SESSION_COOKIE)
            .then(|| value.trim())
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_token_hash_is_sha256_hex() {
        assert_eq!(
            session_token_hash("session-secret"),
            "sha256:135fa7d67146ca540f17e51e101d45de5b1edde8ea8a13f9c7b93b71aa21f8a4"
        );
    }

    #[test]
    fn extracts_bearer_and_cookie_session_tokens() {
        let request = ActorResolutionRequest {
            authorization: Some("Bearer bearer-token".to_owned()),
            cookie: Some("theme=dark; lenso_session=cookie-token".to_owned()),
        };

        assert_eq!(
            session_tokens(&request),
            vec!["bearer-token".to_owned(), "cookie-token".to_owned()]
        );
    }

    #[test]
    fn leaves_dev_bearer_tokens_for_fallback_resolver() {
        let request = ActorResolutionRequest {
            authorization: Some("Bearer dev-user:user_123".to_owned()),
            cookie: None,
        };

        assert!(session_tokens(&request).is_empty());
    }
}
