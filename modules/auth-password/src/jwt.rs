use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

/// JWT claims carried in every password-auth token when `token_strategy` is `"jwt"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject — the authenticated user id.
    pub sub: String,
    /// Issuer — defaults to `"lenso"`.
    pub iss: String,
    /// Audience — defaults to `"lenso"`.
    pub aud: String,
    /// Issued-at timestamp (seconds since epoch).
    pub iat: u64,
    /// Expiration timestamp (seconds since epoch).
    pub exp: u64,
}

/// Derived JWT configuration with owned values suitable for resolver construction.
#[derive(Debug, Clone)]
pub struct JwtConfig {
    pub secret: String,
    pub issuer: String,
    pub audience: String,
    pub ttl_hours: u32,
}

/// Create a signed JWT for the given user id.
///
/// The token carries standard claims (`sub`, `iss`, `aud`, `iat`, `exp`) and
/// is signed with HMAC-SHA256.
pub fn create_token(user_id: &str, config: &JwtConfig, now: DateTime<Utc>) -> String {
    let iat = now.timestamp() as u64;
    let exp = (now + Duration::hours(i64::from(config.ttl_hours))).timestamp() as u64;
    let claims = Claims {
        sub: user_id.to_owned(),
        iss: config.issuer.clone(),
        aud: config.audience.clone(),
        iat,
        exp,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.secret.as_bytes()),
    )
    .expect("JWT encoding is infallible with HMAC-SHA256")
}

/// Verify a JWT and extract its claims.
///
/// Returns `None` if the token is malformed, expired, or fails validation.
pub fn verify_token(token: &str, config: &JwtConfig) -> Option<Claims> {
    let mut validation = Validation::default();
    validation.set_issuer(&[&config.issuer]);
    validation.set_audience(&[&config.audience]);

    decode::<Claims>(
        token,
        &DecodingKey::from_secret(config.secret.as_bytes()),
        &validation,
    )
    .map(|data| data.claims)
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> JwtConfig {
        JwtConfig {
            secret: "test-secret-key-for-jwt-validation".to_owned(),
            issuer: "lenso".to_owned(),
            audience: "lenso".to_owned(),
            ttl_hours: 1,
        }
    }

    #[test]
    fn roundtrip_jwt_token() {
        let config = test_config();
        let now = Utc::now();
        let token = create_token("usr_abc123", &config, now);

        let claims = verify_token(&token, &config).expect("token should verify");
        assert_eq!(claims.sub, "usr_abc123");
        assert_eq!(claims.iss, "lenso");
        assert_eq!(claims.aud, "lenso");
    }

    #[test]
    fn rejects_wrong_secret() {
        let config = test_config();
        let now = Utc::now();
        let token = create_token("usr_abc123", &config, now);

        let mut wrong_config = config.clone();
        wrong_config.secret = "wrong-secret".to_owned();

        assert!(verify_token(&token, &wrong_config).is_none());
    }

    #[test]
    fn rejects_expired_token() {
        let config = test_config();
        let past = Utc::now() - Duration::hours(2);
        let token = create_token("usr_abc123", &config, past);

        assert!(verify_token(&token, &config).is_none());
    }

    #[test]
    fn rejects_wrong_audience() {
        let config = test_config();
        let now = Utc::now();
        let token = create_token("usr_abc123", &config, now);

        let mut wrong_config = config.clone();
        wrong_config.audience = "wrong-audience".to_owned();

        assert!(verify_token(&token, &wrong_config).is_none());
    }
}
