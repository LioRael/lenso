use crate::config::{AuthPasswordConfig, TokenStrategy};
use crate::jwt;
use crate::password::{
    hash_password, new_session_token, normalize_identifier, validate_password, verify_password,
};
use auth::public::{self, AuthSession, AuthUserId};
use chrono::{DateTime, Utc};
use platform_core::{AppError, AppResult, DbPool, ErrorCode};

const PASSWORD_PROVIDER: &str = "password";

#[derive(Debug, Clone)]
pub struct PasswordAuthRepository {
    pool: DbPool,
}

/// Token returned by register/login, varying by strategy.
#[derive(Debug)]
pub enum AuthToken {
    Session(AuthSession),
    Jwt {
        user_id: String,
        token: String,
        expires_at: DateTime<Utc>,
    },
}

impl PasswordAuthRepository {
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn register(
        &self,
        identifier: &str,
        password: &str,
        user_id: String,
        identity_id: String,
        session_id: String,
        now: DateTime<Utc>,
        expires_at: DateTime<Utc>,
        config: &AuthPasswordConfig,
    ) -> AppResult<AuthToken> {
        let normalized_identifier = normalize_identifier(identifier)?;
        validate_password(password)?;
        let password_hash = hash_password(password, config)?;

        match config.token_strategy {
            TokenStrategy::Session => {
                let token = new_session_token();
                let mut tx = self.pool.begin().await.map_err(map_sql_error)?;
                let identity = public::create_user_identity_in_tx(
                    &mut tx,
                    AuthUserId(user_id),
                    identity_id,
                    PASSWORD_PROVIDER,
                    &normalized_identifier,
                    now,
                )
                .await?;

                sqlx::query(
                    r#"
                    insert into auth_password.credentials (identity_id, password_hash, created_at, updated_at)
                    values ($1, $2, $3, $3)
                    "#,
                )
                .bind(&identity.id)
                .bind(password_hash)
                .bind(now)
                .execute(&mut *tx)
                .await
                .map_err(map_sql_error)?;

                let session = public::create_session_in_tx(
                    &mut tx,
                    &identity.user_id,
                    session_id,
                    token,
                    now,
                    expires_at,
                )
                .await?;

                tx.commit().await.map_err(map_sql_error)?;
                Ok(AuthToken::Session(session))
            }
            TokenStrategy::Jwt => {
                let jwt_config = config.jwt_config()?.ok_or_else(|| {
                    AppError::new(ErrorCode::Internal, "JWT configuration is required")
                })?;
                let mut tx = self.pool.begin().await.map_err(map_sql_error)?;
                let identity = public::create_user_identity_in_tx(
                    &mut tx,
                    AuthUserId(user_id),
                    identity_id,
                    PASSWORD_PROVIDER,
                    &normalized_identifier,
                    now,
                )
                .await?;

                sqlx::query(
                    r#"
                    insert into auth_password.credentials (identity_id, password_hash, created_at, updated_at)
                    values ($1, $2, $3, $3)
                    "#,
                )
                .bind(&identity.id)
                .bind(password_hash)
                .bind(now)
                .execute(&mut *tx)
                .await
                .map_err(map_sql_error)?;

                tx.commit().await.map_err(map_sql_error)?;

                let user_id_str = identity.user_id.0.clone();
                let token = jwt::create_token(&user_id_str, &jwt_config, now);

                Ok(AuthToken::Jwt {
                    user_id: user_id_str,
                    token,
                    expires_at,
                })
            }
        }
    }

    pub async fn login(
        &self,
        identifier: &str,
        password: &str,
        session_id: String,
        now: DateTime<Utc>,
        expires_at: DateTime<Utc>,
        config: &AuthPasswordConfig,
    ) -> AppResult<AuthToken> {
        let normalized_identifier = normalize_identifier(identifier)?;
        validate_password(password)?;

        let Some(identity) =
            public::find_active_identity(&self.pool, PASSWORD_PROVIDER, &normalized_identifier)
                .await?
        else {
            return Err(invalid_credentials());
        };

        let Some(password_hash) = sqlx::query_scalar::<_, String>(
            r#"
            select password_hash
            from auth_password.credentials
            where identity_id = $1
            "#,
        )
        .bind(&identity.id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sql_error)?
        else {
            return Err(invalid_credentials());
        };

        if !verify_password(&password_hash, password)? {
            return Err(invalid_credentials());
        }

        match config.token_strategy {
            TokenStrategy::Session => {
                let session = public::create_session(
                    &self.pool,
                    &identity.user_id,
                    session_id,
                    new_session_token(),
                    now,
                    expires_at,
                )
                .await?;
                Ok(AuthToken::Session(session))
            }
            TokenStrategy::Jwt => {
                let jwt_config = config.jwt_config()?.ok_or_else(|| {
                    AppError::new(ErrorCode::Internal, "JWT configuration is required")
                })?;
                let user_id_str = identity.user_id.0.clone();
                let token = jwt::create_token(&user_id_str, &jwt_config, now);
                Ok(AuthToken::Jwt {
                    user_id: user_id_str,
                    token,
                    expires_at,
                })
            }
        }
    }
}

fn invalid_credentials() -> AppError {
    AppError::new(ErrorCode::Unauthorized, "Invalid identifier or password")
}

fn map_sql_error(source: sqlx::Error) -> AppError {
    AppError::new(ErrorCode::Internal, "Internal server error").with_source(source)
}
