use async_trait::async_trait;
use platform_core::Migration;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;

pub const SYSTEM_PLANE_MIGRATIONS: &[Migration] = &[Migration {
    name: "system-plane/0001_create_enrollment_grants",
    sql: include_str!("../migrations/0001_create_enrollment_grants.sql"),
}];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnrollmentGrant {
    pub managed_service_id: String,
    pub console_service_principal: String,
    pub grant_revision: u64,
    pub authorization_epoch: u64,
    pub expires_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnrollmentRecord {
    pub grant: EnrollmentGrant,
    pub revoked_at_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrollmentAuthorization {
    pub managed_service_id: String,
    pub console_service_principal: String,
    pub grant_revision: u64,
    pub authorization_epoch: u64,
    pub expires_at_unix_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnrollmentErrorCode {
    InvalidGrant,
    AlreadyEnrolled,
    NotEnrolled,
    PrincipalMismatch,
    Revoked,
    Expired,
    StaleAuthorizationEpoch,
    StoreUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct EnrollmentError {
    pub code: EnrollmentErrorCode,
    pub message: String,
}

#[async_trait]
pub trait EnrollmentAuthorizer: std::fmt::Debug + Send + Sync {
    async fn authorize(
        &self,
        managed_service_id: &str,
        console_service_principal: &str,
        now_unix_ms: u64,
    ) -> Result<EnrollmentAuthorization, EnrollmentError>;
}

#[derive(Debug, Clone)]
pub struct PostgresEnrollmentStore {
    pool: PgPool,
}

impl PostgresEnrollmentStore {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn enroll(
        &self,
        grant: &EnrollmentGrant,
    ) -> Result<EnrollmentRecord, EnrollmentError> {
        validate_grant(grant)?;
        let result = sqlx::query(
            r#"
            insert into platform.system_plane_enrollment_grants (
                managed_service_id, console_service_principal, grant_revision,
                authorization_epoch, expires_at_unix_ms
            ) values ($1, $2, $3, $4, $5)
            on conflict (managed_service_id) do nothing
            "#,
        )
        .bind(&grant.managed_service_id)
        .bind(&grant.console_service_principal)
        .bind(to_i64(grant.grant_revision, "grant revision")?)
        .bind(to_i64(grant.authorization_epoch, "authorization epoch")?)
        .bind(to_i64(grant.expires_at_unix_ms, "expiry")?)
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        if result.rows_affected() != 1 {
            return Err(error(
                EnrollmentErrorCode::AlreadyEnrolled,
                "The managed Service already has an enrollment record; use explicit transfer after revocation",
            ));
        }
        Ok(EnrollmentRecord {
            grant: grant.clone(),
            revoked_at_unix_ms: None,
        })
    }

    pub async fn revoke(
        &self,
        managed_service_id: &str,
        expected_authorization_epoch: u64,
        revoked_at_unix_ms: u64,
    ) -> Result<EnrollmentRecord, EnrollmentError> {
        if managed_service_id.trim().is_empty() || revoked_at_unix_ms == 0 {
            return Err(error(
                EnrollmentErrorCode::InvalidGrant,
                "Revocation requires a managed Service identity and positive timestamp",
            ));
        }
        let expected_epoch = to_i64(expected_authorization_epoch, "authorization epoch")?;
        let revoked_at = to_i64(revoked_at_unix_ms, "revocation timestamp")?;
        let row = sqlx::query_as::<_, EnrollmentRow>(
            r#"
            update platform.system_plane_enrollment_grants
            set revoked_at_unix_ms = $3,
                authorization_epoch = authorization_epoch + 1,
                updated_at = now()
            where managed_service_id = $1
              and authorization_epoch = $2
              and revoked_at_unix_ms is null
            returning managed_service_id, console_service_principal, grant_revision,
                      authorization_epoch, expires_at_unix_ms, revoked_at_unix_ms
            "#,
        )
        .bind(managed_service_id)
        .bind(expected_epoch)
        .bind(revoked_at)
        .fetch_optional(&self.pool)
        .await
        .map_err(store_error)?;
        match row {
            Some(row) => row.try_into(),
            None => Err(error(
                EnrollmentErrorCode::StaleAuthorizationEpoch,
                "Enrollment is missing, revoked, or has advanced beyond the expected authorization epoch",
            )),
        }
    }

    pub async fn transfer(
        &self,
        grant: &EnrollmentGrant,
        expected_authorization_epoch: u64,
    ) -> Result<EnrollmentRecord, EnrollmentError> {
        validate_grant(grant)?;
        let expected_epoch = to_i64(expected_authorization_epoch, "authorization epoch")?;
        if grant.authorization_epoch <= expected_authorization_epoch {
            return Err(error(
                EnrollmentErrorCode::InvalidGrant,
                "Transfer must advance the authorization epoch",
            ));
        }
        let row = sqlx::query_as::<_, EnrollmentRow>(
            r#"
            update platform.system_plane_enrollment_grants
            set console_service_principal = $3,
                grant_revision = $4,
                authorization_epoch = $5,
                expires_at_unix_ms = $6,
                revoked_at_unix_ms = null,
                updated_at = now()
            where managed_service_id = $1
              and authorization_epoch = $2
              and revoked_at_unix_ms is not null
            returning managed_service_id, console_service_principal, grant_revision,
                      authorization_epoch, expires_at_unix_ms, revoked_at_unix_ms
            "#,
        )
        .bind(&grant.managed_service_id)
        .bind(expected_epoch)
        .bind(&grant.console_service_principal)
        .bind(to_i64(grant.grant_revision, "grant revision")?)
        .bind(to_i64(grant.authorization_epoch, "authorization epoch")?)
        .bind(to_i64(grant.expires_at_unix_ms, "expiry")?)
        .fetch_optional(&self.pool)
        .await
        .map_err(store_error)?;
        match row {
            Some(row) => row.try_into(),
            None => Err(error(
                EnrollmentErrorCode::StaleAuthorizationEpoch,
                "Transfer requires the exact revoked enrollment authorization epoch",
            )),
        }
    }

    async fn record(&self, managed_service_id: &str) -> Result<EnrollmentRecord, EnrollmentError> {
        let row = sqlx::query_as::<_, EnrollmentRow>(
            r#"
            select managed_service_id, console_service_principal, grant_revision,
                   authorization_epoch, expires_at_unix_ms, revoked_at_unix_ms
            from platform.system_plane_enrollment_grants
            where managed_service_id = $1
            "#,
        )
        .bind(managed_service_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(store_error)?
        .ok_or_else(|| {
            error(
                EnrollmentErrorCode::NotEnrolled,
                "The managed Service has no enrollment grant",
            )
        })?;
        row.try_into()
    }
}

#[async_trait]
impl EnrollmentAuthorizer for PostgresEnrollmentStore {
    async fn authorize(
        &self,
        managed_service_id: &str,
        console_service_principal: &str,
        now_unix_ms: u64,
    ) -> Result<EnrollmentAuthorization, EnrollmentError> {
        authorize_record(
            self.record(managed_service_id).await?,
            console_service_principal,
            now_unix_ms,
        )
    }
}

#[derive(Debug, Clone)]
pub struct SystemSandboxEnrollmentAuthorizer {
    record: Arc<EnrollmentRecord>,
}

impl SystemSandboxEnrollmentAuthorizer {
    pub fn new(environment: &str, grant: EnrollmentGrant) -> Result<Self, EnrollmentError> {
        if !matches!(environment, "local" | "development" | "test") {
            return Err(error(
                EnrollmentErrorCode::InvalidGrant,
                "System Sandbox enrollment is forbidden outside local development and tests",
            ));
        }
        validate_grant(&grant)?;
        Ok(Self {
            record: Arc::new(EnrollmentRecord {
                grant,
                revoked_at_unix_ms: None,
            }),
        })
    }
}

#[async_trait]
impl EnrollmentAuthorizer for SystemSandboxEnrollmentAuthorizer {
    async fn authorize(
        &self,
        managed_service_id: &str,
        console_service_principal: &str,
        now_unix_ms: u64,
    ) -> Result<EnrollmentAuthorization, EnrollmentError> {
        if self.record.grant.managed_service_id != managed_service_id {
            return Err(error(
                EnrollmentErrorCode::NotEnrolled,
                "The enrollment grant belongs to a different managed Service",
            ));
        }
        authorize_record(
            self.record.as_ref().clone(),
            console_service_principal,
            now_unix_ms,
        )
    }
}

#[derive(sqlx::FromRow)]
struct EnrollmentRow {
    managed_service_id: String,
    console_service_principal: String,
    grant_revision: i64,
    authorization_epoch: i64,
    expires_at_unix_ms: i64,
    revoked_at_unix_ms: Option<i64>,
}

impl TryFrom<EnrollmentRow> for EnrollmentRecord {
    type Error = EnrollmentError;

    fn try_from(row: EnrollmentRow) -> Result<Self, Self::Error> {
        Ok(Self {
            grant: EnrollmentGrant {
                managed_service_id: row.managed_service_id,
                console_service_principal: row.console_service_principal,
                grant_revision: to_u64(row.grant_revision, "grant revision")?,
                authorization_epoch: to_u64(row.authorization_epoch, "authorization epoch")?,
                expires_at_unix_ms: to_u64(row.expires_at_unix_ms, "expiry")?,
            },
            revoked_at_unix_ms: row
                .revoked_at_unix_ms
                .map(|value| to_u64(value, "revocation timestamp"))
                .transpose()?,
        })
    }
}

fn authorize_record(
    record: EnrollmentRecord,
    principal: &str,
    now_unix_ms: u64,
) -> Result<EnrollmentAuthorization, EnrollmentError> {
    if record.revoked_at_unix_ms.is_some() {
        return Err(error(
            EnrollmentErrorCode::Revoked,
            "The managed Service enrollment has been revoked",
        ));
    }
    if record.grant.console_service_principal != principal {
        return Err(error(
            EnrollmentErrorCode::PrincipalMismatch,
            "Authenticated Service Principal does not match the active enrollment grant",
        ));
    }
    if record.grant.expires_at_unix_ms <= now_unix_ms {
        return Err(error(
            EnrollmentErrorCode::Expired,
            "The managed Service enrollment grant has expired",
        ));
    }
    Ok(EnrollmentAuthorization {
        managed_service_id: record.grant.managed_service_id,
        console_service_principal: record.grant.console_service_principal,
        grant_revision: record.grant.grant_revision,
        authorization_epoch: record.grant.authorization_epoch,
        expires_at_unix_ms: record.grant.expires_at_unix_ms,
    })
}

fn validate_grant(grant: &EnrollmentGrant) -> Result<(), EnrollmentError> {
    if grant.managed_service_id.trim().is_empty()
        || grant.console_service_principal.trim().is_empty()
        || grant.grant_revision == 0
        || grant.expires_at_unix_ms == 0
    {
        return Err(error(
            EnrollmentErrorCode::InvalidGrant,
            "Enrollment grant requires Service identities, positive revision, and positive expiry",
        ));
    }
    to_i64(grant.grant_revision, "grant revision")?;
    to_i64(grant.authorization_epoch, "authorization epoch")?;
    to_i64(grant.expires_at_unix_ms, "expiry")?;
    Ok(())
}

fn to_i64(value: u64, field: &str) -> Result<i64, EnrollmentError> {
    i64::try_from(value).map_err(|_| {
        error(
            EnrollmentErrorCode::InvalidGrant,
            format!("Enrollment {field} exceeds the supported storage range"),
        )
    })
}

fn to_u64(value: i64, field: &str) -> Result<u64, EnrollmentError> {
    u64::try_from(value).map_err(|_| {
        error(
            EnrollmentErrorCode::StoreUnavailable,
            format!("Stored enrollment {field} is invalid"),
        )
    })
}

fn store_error(_source: sqlx::Error) -> EnrollmentError {
    error(
        EnrollmentErrorCode::StoreUnavailable,
        "Enrollment Store operation failed",
    )
}

fn error(code: EnrollmentErrorCode, message: impl Into<String>) -> EnrollmentError {
    EnrollmentError {
        code,
        message: message.into(),
    }
}
