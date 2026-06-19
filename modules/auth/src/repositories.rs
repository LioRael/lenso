use crate::models::{AuthSession, AuthSessionRecord, AuthUser, AuthUserId};
use crate::resolver::session_token_hash;
use chrono::{DateTime, Utc};
use platform_core::{AppError, AppResult, DbPool, ErrorCode};

#[async_trait::async_trait]
pub trait AuthUserRepository: std::fmt::Debug + Send + Sync {
    async fn insert(&self, user: &AuthUser) -> AppResult<()>;
    async fn find_by_id(&self, user_id: &AuthUserId) -> AppResult<Option<AuthUser>>;
    async fn list(&self, limit: i64, cursor: Option<&str>) -> AppResult<Vec<AuthUser>>;
    async fn find_session_by_id(&self, session_id: &str) -> AppResult<Option<AuthSessionRecord>>;
    async fn list_sessions(
        &self,
        limit: i64,
        cursor: Option<&str>,
    ) -> AppResult<Vec<AuthSessionRecord>>;
    async fn revoke_session_by_id(
        &self,
        session_id: &str,
        revoked_at: DateTime<Utc>,
    ) -> AppResult<bool>;
}

#[derive(Debug, Clone)]
pub struct PostgresAuthUserRepository {
    pool: DbPool,
}

impl PostgresAuthUserRepository {
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn create_dev_session(
        &self,
        user_id: AuthUserId,
        session_id: String,
        token: String,
        created_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> AppResult<AuthSession> {
        let mut tx = self.pool.begin().await.map_err(map_sql_error)?;

        sqlx::query(
            r#"
            insert into auth.users (id, created_at, disabled_at)
            values ($1, $2, null)
            on conflict (id) do nothing
            "#,
        )
        .bind(&user_id.0)
        .bind(created_at)
        .execute(&mut *tx)
        .await
        .map_err(map_sql_error)?;

        let disabled_at = sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
            "select disabled_at from auth.users where id = $1",
        )
        .bind(&user_id.0)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_sql_error)?;

        if disabled_at.is_some() {
            return Err(AppError::new(ErrorCode::Forbidden, "Auth user is disabled"));
        }

        sqlx::query(
            r#"
            insert into auth.sessions (id, user_id, token_hash, created_at, expires_at, revoked_at)
            values ($1, $2, $3, $4, $5, null)
            "#,
        )
        .bind(&session_id)
        .bind(&user_id.0)
        .bind(session_token_hash(&token))
        .bind(created_at)
        .bind(expires_at)
        .execute(&mut *tx)
        .await
        .map_err(map_sql_error)?;

        tx.commit().await.map_err(map_sql_error)?;

        Ok(AuthSession {
            id: session_id,
            user_id,
            token,
            expires_at,
        })
    }

    pub async fn revoke_session_token(
        &self,
        token: &str,
        revoked_at: DateTime<Utc>,
    ) -> AppResult<bool> {
        let result = sqlx::query(
            r#"
            update auth.sessions
            set revoked_at = $2
            where token_hash = $1
              and revoked_at is null
            "#,
        )
        .bind(session_token_hash(token))
        .bind(revoked_at)
        .execute(&self.pool)
        .await
        .map_err(map_sql_error)?;

        Ok(result.rows_affected() > 0)
    }
}

#[async_trait::async_trait]
impl AuthUserRepository for PostgresAuthUserRepository {
    async fn insert(&self, user: &AuthUser) -> AppResult<()> {
        sqlx::query(
            r#"
            insert into auth.users (id, created_at, disabled_at)
            values ($1, $2, $3)
            "#,
        )
        .bind(&user.id.0)
        .bind(user.created_at)
        .bind(user.disabled_at)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(map_sql_error)
    }

    async fn find_by_id(&self, user_id: &AuthUserId) -> AppResult<Option<AuthUser>> {
        sqlx::query_as::<_, UserRow>(
            r#"
            select id, created_at, disabled_at
            from auth.users
            where id = $1
            "#,
        )
        .bind(&user_id.0)
        .fetch_optional(&self.pool)
        .await
        .map(|row| row.map(user_from_row))
        .map_err(map_sql_error)
    }

    async fn list(&self, limit: i64, cursor: Option<&str>) -> AppResult<Vec<AuthUser>> {
        let rows = match cursor {
            Some(after) => {
                sqlx::query_as::<_, UserRow>(
                    r#"
                    select id, created_at, disabled_at
                    from auth.users
                    where id > $1
                    order by id asc
                    limit $2
                    "#,
                )
                .bind(after)
                .bind(limit)
                .fetch_all(&self.pool)
                .await
            }
            None => {
                sqlx::query_as::<_, UserRow>(
                    r#"
                    select id, created_at, disabled_at
                    from auth.users
                    order by id asc
                    limit $1
                    "#,
                )
                .bind(limit)
                .fetch_all(&self.pool)
                .await
            }
        }
        .map_err(map_sql_error)?;

        Ok(rows.into_iter().map(user_from_row).collect())
    }

    async fn find_session_by_id(&self, session_id: &str) -> AppResult<Option<AuthSessionRecord>> {
        sqlx::query_as::<_, SessionRow>(
            r#"
            select id, user_id, created_at, expires_at, revoked_at
            from auth.sessions
            where id = $1
            "#,
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await
        .map(|row| row.map(session_from_row))
        .map_err(map_sql_error)
    }

    async fn list_sessions(
        &self,
        limit: i64,
        cursor: Option<&str>,
    ) -> AppResult<Vec<AuthSessionRecord>> {
        let rows = match cursor {
            Some(after) => {
                sqlx::query_as::<_, SessionRow>(
                    r#"
                    select id, user_id, created_at, expires_at, revoked_at
                    from auth.sessions
                    where id > $1
                    order by id asc
                    limit $2
                    "#,
                )
                .bind(after)
                .bind(limit)
                .fetch_all(&self.pool)
                .await
            }
            None => {
                sqlx::query_as::<_, SessionRow>(
                    r#"
                    select id, user_id, created_at, expires_at, revoked_at
                    from auth.sessions
                    order by id asc
                    limit $1
                    "#,
                )
                .bind(limit)
                .fetch_all(&self.pool)
                .await
            }
        }
        .map_err(map_sql_error)?;

        Ok(rows.into_iter().map(session_from_row).collect())
    }

    async fn revoke_session_by_id(
        &self,
        session_id: &str,
        revoked_at: DateTime<Utc>,
    ) -> AppResult<bool> {
        let result = sqlx::query(
            r#"
            update auth.sessions
            set revoked_at = $2
            where id = $1
              and revoked_at is null
            "#,
        )
        .bind(session_id)
        .bind(revoked_at)
        .execute(&self.pool)
        .await
        .map_err(map_sql_error)?;

        Ok(result.rows_affected() > 0)
    }
}

type UserRow = (String, DateTime<Utc>, Option<DateTime<Utc>>);
type SessionRow = (
    String,
    String,
    DateTime<Utc>,
    DateTime<Utc>,
    Option<DateTime<Utc>>,
);

fn user_from_row(row: UserRow) -> AuthUser {
    let (id, created_at, disabled_at) = row;
    AuthUser {
        id: AuthUserId(id),
        created_at,
        disabled_at,
    }
}

fn session_from_row(row: SessionRow) -> AuthSessionRecord {
    let (id, user_id, created_at, expires_at, revoked_at) = row;
    AuthSessionRecord {
        id,
        user_id: AuthUserId(user_id),
        created_at,
        expires_at,
        revoked_at,
    }
}

fn map_sql_error(source: sqlx::Error) -> AppError {
    AppError::new(ErrorCode::Internal, "Internal server error").with_source(source)
}
