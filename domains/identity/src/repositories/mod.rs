use crate::models::user::{User, UserId};
use chrono::{DateTime, Utc};
use platform_core::{
    AppError, AppResult, DbPool, ErrorCode, EventEnvelope, OutboxEvent, OutboxPublisher,
};

#[async_trait::async_trait]
pub trait UserRepository: std::fmt::Debug + Send + Sync {
    async fn insert(&self, user: &User) -> AppResult<()>;
    async fn insert_with_outbox(&self, user: &User, event: &EventEnvelope) -> AppResult<()>;
    async fn find_by_id(&self, user_id: &UserId) -> AppResult<Option<User>>;
    async fn find_by_email(&self, email: &str) -> AppResult<Option<User>>;
}

#[derive(Debug, Clone)]
pub struct PostgresUserRepository {
    pool: DbPool,
}

impl PostgresUserRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl UserRepository for PostgresUserRepository {
    async fn insert(&self, user: &User) -> AppResult<()> {
        sqlx::query(
            r#"
            insert into identity.users (id, email, display_name, created_at, updated_at)
            values ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(&user.id.0)
        .bind(&user.email)
        .bind(&user.display_name)
        .bind(user.created_at)
        .bind(user.updated_at)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(map_sql_error)
    }

    async fn insert_with_outbox(&self, user: &User, event: &EventEnvelope) -> AppResult<()> {
        let mut tx = self.pool.begin().await.map_err(map_sql_error)?;

        sqlx::query(
            r#"
            insert into identity.users (id, email, display_name, created_at, updated_at)
            values ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(&user.id.0)
        .bind(&user.email)
        .bind(&user.display_name)
        .bind(user.created_at)
        .bind(user.updated_at)
        .execute(&mut *tx)
        .await
        .map_err(map_sql_error)?;

        OutboxPublisher
            .publish_in_tx(&mut tx, &OutboxEvent::from_envelope("user", event))
            .await?;

        tx.commit().await.map_err(map_sql_error)
    }

    async fn find_by_id(&self, user_id: &UserId) -> AppResult<Option<User>> {
        sqlx::query_as::<_, UserRow>(
            r#"
            select id, email, display_name, created_at, updated_at
            from identity.users
            where id = $1
            "#,
        )
        .bind(&user_id.0)
        .fetch_optional(&self.pool)
        .await
        .map(|row| row.map(user_from_row))
        .map_err(map_sql_error)
    }

    async fn find_by_email(&self, email: &str) -> AppResult<Option<User>> {
        sqlx::query_as::<_, UserRow>(
            r#"
            select id, email, display_name, created_at, updated_at
            from identity.users
            where email = $1
            "#,
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await
        .map(|row| row.map(user_from_row))
        .map_err(map_sql_error)
    }
}

type UserRow = (String, String, Option<String>, DateTime<Utc>, DateTime<Utc>);

fn user_from_row(row: UserRow) -> User {
    let (id, email, display_name, created_at, updated_at) = row;
    User {
        id: UserId(id),
        email,
        display_name,
        created_at,
        updated_at,
    }
}

fn map_sql_error(source: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(database_error) = &source {
        if database_error.constraint() == Some("users_email_key") {
            return AppError::new(ErrorCode::Conflict, "A user with this email already exists")
                .with_source(source);
        }
    }

    AppError::new(ErrorCode::Internal, "Internal server error").with_source(source)
}
