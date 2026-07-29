use async_trait::async_trait;
use lenso_service::system_plane::{
    EnrollmentCapabilityGrant, EnrollmentOffer, EnrollmentPolicyGrant, EnrollmentReceipt,
    EnrollmentSignature, EnrollmentSignatureAlgorithm, EnrollmentSignatureVerifier,
    EnrollmentSigner, enrollment_offer_digest, enrollment_receipt_digest, sign_enrollment_receipt,
    verify_enrollment_offer,
};
use platform_core::Migration;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;

pub const SYSTEM_PLANE_MIGRATIONS: &[Migration] = &[
    Migration {
        name: "system-plane/0001_create_enrollment_grants",
        sql: include_str!("../migrations/0001_create_enrollment_grants.sql"),
    },
    Migration {
        name: "system-plane/0002_create_enrollment_ceremonies",
        sql: include_str!("../migrations/0002_create_enrollment_ceremonies.sql"),
    },
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnrollmentGrant {
    pub system_id: String,
    pub managed_service_id: String,
    pub managed_service_principal: String,
    pub managed_service_revision: String,
    pub console_service_principal: String,
    pub offer_digest: String,
    pub receipt_digest: String,
    pub grant_revision: u64,
    pub authorization_epoch: u64,
    pub expires_at_unix_ms: u64,
    pub capabilities: Vec<EnrollmentCapabilityGrant>,
    pub policy: EnrollmentPolicyGrant,
}

impl EnrollmentGrant {
    #[must_use]
    pub fn system_sandbox(
        managed_service_id: impl Into<String>,
        console_service_principal: impl Into<String>,
        authorization_epoch: u64,
        expires_at_unix_ms: u64,
    ) -> Self {
        let managed_service_id = managed_service_id.into();
        Self {
            system_id: "system-sandbox".to_owned(),
            managed_service_principal: format!("service:{managed_service_id}"),
            managed_service_revision: "system-sandbox".to_owned(),
            managed_service_id,
            console_service_principal: console_service_principal.into(),
            offer_digest: format!("sha256:{}", "0".repeat(64)),
            receipt_digest: format!("sha256:{}", "1".repeat(64)),
            grant_revision: 1,
            authorization_epoch,
            expires_at_unix_ms,
            capabilities: Vec::new(),
            policy: EnrollmentPolicyGrant {
                policy_id: "system-sandbox".to_owned(),
                policy_revision: "1".to_owned(),
                policy_digest: format!("sha256:{}", "2".repeat(64)),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnrollmentRecord {
    pub grant: EnrollmentGrant,
    pub revoked_at_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrollmentAuthorization {
    pub system_id: String,
    pub managed_service_id: String,
    pub managed_service_principal: String,
    pub managed_service_revision: String,
    pub console_service_principal: String,
    pub receipt_digest: String,
    pub grant_revision: u64,
    pub authorization_epoch: u64,
    pub expires_at_unix_ms: u64,
    pub capabilities: Vec<EnrollmentCapabilityGrant>,
    pub policy: EnrollmentPolicyGrant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnrollmentErrorCode {
    InvalidGrant,
    InvalidDecision,
    SignatureRejected,
    NonceReused,
    AlreadyEnrolled,
    NotEnrolled,
    PrincipalMismatch,
    Revoked,
    Expired,
    StaleAuthorizationEpoch,
    StoreUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrollmentAcceptance {
    pub managed_service_id: String,
    pub managed_service_principal: String,
    pub managed_service_revision: String,
    pub grant_revision: u64,
    pub authorization_epoch: u64,
    pub expires_at_unix_ms: u64,
    pub capabilities: Vec<EnrollmentCapabilityGrant>,
    pub policy: EnrollmentPolicyGrant,
}

#[derive(Debug)]
pub struct EnrollmentCeremony {
    store: PostgresEnrollmentStore,
    console_verifier: Arc<dyn EnrollmentSignatureVerifier>,
    service_signer: Arc<dyn EnrollmentSigner>,
}

impl EnrollmentCeremony {
    #[must_use]
    pub fn new(
        store: PostgresEnrollmentStore,
        console_verifier: Arc<dyn EnrollmentSignatureVerifier>,
        service_signer: Arc<dyn EnrollmentSigner>,
    ) -> Self {
        Self {
            store,
            console_verifier,
            service_signer,
        }
    }

    pub async fn accept(
        &self,
        offer: &EnrollmentOffer,
        acceptance: &EnrollmentAcceptance,
        now_unix_ms: u64,
    ) -> Result<EnrollmentReceipt, EnrollmentError> {
        let offer_digest =
            verify_enrollment_offer(offer, self.console_verifier.as_ref(), now_unix_ms).map_err(
                |_| {
                    error(
                        EnrollmentErrorCode::SignatureRejected,
                        "Enrollment Offer signature, lifetime, or canonical content was rejected",
                    )
                },
            )?;
        validate_acceptance(offer, acceptance, now_unix_ms)?;
        let receipt = sign_enrollment_receipt(
            EnrollmentReceipt {
                protocol: String::new(),
                offer_digest,
                system_id: offer.system_id.clone(),
                managed_service_id: acceptance.managed_service_id.clone(),
                managed_service_principal: acceptance.managed_service_principal.clone(),
                managed_service_revision: acceptance.managed_service_revision.clone(),
                console_service_principal: offer.console_service_principal.clone(),
                nonce: offer.nonce.clone(),
                issued_at_unix_ms: now_unix_ms,
                expires_at_unix_ms: acceptance.expires_at_unix_ms,
                grant_revision: acceptance.grant_revision,
                authorization_epoch: acceptance.authorization_epoch,
                granted_capabilities: acceptance.capabilities.clone(),
                granted_policy: acceptance.policy.clone(),
                signature: EnrollmentSignature {
                    algorithm: EnrollmentSignatureAlgorithm::Ed25519,
                    key_id: self.service_signer.key_id().to_owned(),
                    subject_digest: String::new(),
                    value: String::new(),
                },
            },
            self.service_signer.as_ref(),
        )
        .map_err(|_| {
            error(
                EnrollmentErrorCode::SignatureRejected,
                "Managed Service could not sign the Enrollment Receipt",
            )
        })?;
        self.store.persist_receipt(offer, &receipt).await
    }
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
        let mut transaction = self.pool.begin().await.map_err(store_error)?;
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
                      authorization_epoch, expires_at_unix_ms, revoked_at_unix_ms,
                      system_id, managed_service_principal, managed_service_revision,
                      offer_digest, receipt_digest, capabilities, policy
            "#,
        )
        .bind(managed_service_id)
        .bind(expected_epoch)
        .bind(revoked_at)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(store_error)?;
        let record: EnrollmentRecord = match row {
            Some(row) => row.try_into()?,
            None => {
                return Err(error(
                    EnrollmentErrorCode::StaleAuthorizationEpoch,
                    "Enrollment is missing, revoked, or has advanced beyond the expected authorization epoch",
                ));
            }
        };
        append_audit(&mut transaction, "enrollment_revoked", &record).await?;
        transaction.commit().await.map_err(store_error)?;
        Ok(record)
    }

    async fn record(&self, managed_service_id: &str) -> Result<EnrollmentRecord, EnrollmentError> {
        let row = sqlx::query_as::<_, EnrollmentRow>(
            r#"
            select managed_service_id, console_service_principal, grant_revision,
                   authorization_epoch, expires_at_unix_ms, revoked_at_unix_ms,
                   system_id, managed_service_principal, managed_service_revision,
                   offer_digest, receipt_digest, capabilities, policy
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

    async fn persist_receipt(
        &self,
        offer: &EnrollmentOffer,
        receipt: &EnrollmentReceipt,
    ) -> Result<EnrollmentReceipt, EnrollmentError> {
        let receipt_digest = enrollment_receipt_digest(receipt);
        let mut transaction = self.pool.begin().await.map_err(store_error)?;
        sqlx::query(
            "select pg_advisory_xact_lock(hashtextextended($1, 0)), pg_advisory_xact_lock(hashtextextended($2, 1))",
        )
        .bind(&receipt.managed_service_id)
        .bind(&receipt.nonce)
        .execute(&mut *transaction)
        .await
        .map_err(store_error)?;
        if let Some(existing) = sqlx::query_scalar::<_, serde_json::Value>(
            "select receipt from platform.system_plane_enrollment_receipts where offer_digest = $1",
        )
        .bind(&receipt.offer_digest)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(store_error)?
        {
            let existing = serde_json::from_value(existing).map_err(serialization_error)?;
            transaction.commit().await.map_err(store_error)?;
            return Ok(existing);
        }
        let nonce_owner = sqlx::query_scalar::<_, String>(
            "select offer_digest from platform.system_plane_enrollment_receipts where nonce = $1",
        )
        .bind(&receipt.nonce)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(store_error)?;
        if nonce_owner.is_some_and(|digest| digest != receipt.offer_digest) {
            return Err(error(
                EnrollmentErrorCode::NonceReused,
                "Enrollment nonce was already consumed by a different Offer",
            ));
        }
        let grant = EnrollmentGrant {
            system_id: receipt.system_id.clone(),
            managed_service_id: receipt.managed_service_id.clone(),
            managed_service_principal: receipt.managed_service_principal.clone(),
            managed_service_revision: receipt.managed_service_revision.clone(),
            console_service_principal: receipt.console_service_principal.clone(),
            offer_digest: enrollment_offer_digest(offer),
            receipt_digest: receipt_digest.clone(),
            grant_revision: receipt.grant_revision,
            authorization_epoch: receipt.authorization_epoch,
            expires_at_unix_ms: receipt.expires_at_unix_ms,
            capabilities: receipt.granted_capabilities.clone(),
            policy: receipt.granted_policy.clone(),
        };
        validate_grant(&grant)?;
        let existing = sqlx::query_as::<_, (String, i64, i64, Option<i64>)>(
            r#"
            select system_id, grant_revision, authorization_epoch, revoked_at_unix_ms
            from platform.system_plane_enrollment_grants
            where managed_service_id = $1
            for update
            "#,
        )
        .bind(&grant.managed_service_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(store_error)?;
        let event_kind = match existing {
            None => {
                insert_grant(&mut transaction, &grant).await?;
                "enrollment_accepted"
            }
            Some((_, _, _, None)) => {
                return Err(error(
                    EnrollmentErrorCode::AlreadyEnrolled,
                    "The managed Service already has an active enrollment; revoke it before transfer",
                ));
            }
            Some((current_system_id, current_revision, current_epoch, Some(_))) => {
                if grant.system_id != current_system_id
                    || grant.grant_revision <= to_u64(current_revision, "grant revision")?
                    || grant.authorization_epoch <= to_u64(current_epoch, "authorization epoch")?
                {
                    return Err(error(
                        EnrollmentErrorCode::StaleAuthorizationEpoch,
                        "Signed enrollment transfer must preserve System identity and advance Grant revision and authorization epoch",
                    ));
                }
                replace_revoked_grant(&mut transaction, &grant, current_epoch).await?;
                "enrollment_transferred"
            }
        };
        let receipt_json = serde_json::to_value(receipt).map_err(serialization_error)?;
        sqlx::query(
            r#"
            insert into platform.system_plane_enrollment_receipts (
                receipt_digest, offer_digest, nonce, managed_service_id, receipt
            ) values ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(&receipt_digest)
        .bind(&receipt.offer_digest)
        .bind(&receipt.nonce)
        .bind(&receipt.managed_service_id)
        .bind(&receipt_json)
        .execute(&mut *transaction)
        .await
        .map_err(store_error)?;
        sqlx::query(
            r#"
            insert into platform.system_plane_enrollment_audit (
                managed_service_id, event_kind, receipt_digest, authorization_epoch, evidence
            ) values ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(&receipt.managed_service_id)
        .bind(event_kind)
        .bind(&receipt_digest)
        .bind(to_i64(receipt.authorization_epoch, "authorization epoch")?)
        .bind(&receipt_json)
        .execute(&mut *transaction)
        .await
        .map_err(store_error)?;
        transaction.commit().await.map_err(store_error)?;
        Ok(receipt.clone())
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
    system_id: String,
    managed_service_id: String,
    managed_service_principal: String,
    managed_service_revision: String,
    console_service_principal: String,
    offer_digest: String,
    receipt_digest: String,
    grant_revision: i64,
    authorization_epoch: i64,
    expires_at_unix_ms: i64,
    revoked_at_unix_ms: Option<i64>,
    capabilities: serde_json::Value,
    policy: serde_json::Value,
}

impl TryFrom<EnrollmentRow> for EnrollmentRecord {
    type Error = EnrollmentError;

    fn try_from(row: EnrollmentRow) -> Result<Self, Self::Error> {
        Ok(Self {
            grant: EnrollmentGrant {
                system_id: row.system_id,
                managed_service_id: row.managed_service_id,
                managed_service_principal: row.managed_service_principal,
                managed_service_revision: row.managed_service_revision,
                console_service_principal: row.console_service_principal,
                offer_digest: row.offer_digest,
                receipt_digest: row.receipt_digest,
                grant_revision: to_u64(row.grant_revision, "grant revision")?,
                authorization_epoch: to_u64(row.authorization_epoch, "authorization epoch")?,
                expires_at_unix_ms: to_u64(row.expires_at_unix_ms, "expiry")?,
                capabilities: serde_json::from_value(row.capabilities)
                    .map_err(serialization_error)?,
                policy: serde_json::from_value(row.policy).map_err(serialization_error)?,
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
        system_id: record.grant.system_id,
        managed_service_id: record.grant.managed_service_id,
        managed_service_principal: record.grant.managed_service_principal,
        managed_service_revision: record.grant.managed_service_revision,
        console_service_principal: record.grant.console_service_principal,
        receipt_digest: record.grant.receipt_digest,
        grant_revision: record.grant.grant_revision,
        authorization_epoch: record.grant.authorization_epoch,
        expires_at_unix_ms: record.grant.expires_at_unix_ms,
        capabilities: record.grant.capabilities,
        policy: record.grant.policy,
    })
}

fn validate_grant(grant: &EnrollmentGrant) -> Result<(), EnrollmentError> {
    if grant.system_id.trim().is_empty()
        || grant.managed_service_id.trim().is_empty()
        || grant.managed_service_principal.trim().is_empty()
        || grant.managed_service_revision.trim().is_empty()
        || grant.console_service_principal.trim().is_empty()
        || !canonical_digest(&grant.offer_digest)
        || !canonical_digest(&grant.receipt_digest)
        || grant.grant_revision == 0
        || grant.expires_at_unix_ms == 0
    {
        return Err(error(
            EnrollmentErrorCode::InvalidGrant,
            "Enrollment Grant requires identities, signed artifact digests, positive revision, and positive expiry",
        ));
    }
    to_i64(grant.grant_revision, "grant revision")?;
    to_i64(grant.authorization_epoch, "authorization epoch")?;
    to_i64(grant.expires_at_unix_ms, "expiry")?;
    Ok(())
}

fn validate_acceptance(
    offer: &EnrollmentOffer,
    acceptance: &EnrollmentAcceptance,
    now_unix_ms: u64,
) -> Result<(), EnrollmentError> {
    if acceptance.managed_service_id.trim().is_empty()
        || acceptance.managed_service_principal.trim().is_empty()
        || acceptance.managed_service_revision.trim().is_empty()
        || acceptance.grant_revision == 0
        || acceptance.expires_at_unix_ms <= now_unix_ms
        || acceptance.expires_at_unix_ms > offer.expires_at_unix_ms
    {
        return Err(error(
            EnrollmentErrorCode::InvalidDecision,
            "Enrollment acceptance requires Service identity, positive revision, and an expiry bounded by the Offer",
        ));
    }
    if acceptance.capabilities.iter().any(|granted| {
        !offer.requested_capabilities.iter().any(|requested| {
            requested.contract_id == granted.contract_id
                && requested.schema_digest == granted.schema_digest
                && granted.feature_ids.is_subset(&requested.feature_ids)
        })
    }) || acceptance.policy != offer.requested_policy
    {
        return Err(error(
            EnrollmentErrorCode::InvalidDecision,
            "Enrollment acceptance cannot widen the requested capabilities or substitute policy",
        ));
    }
    Ok(())
}

fn canonical_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    })
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

fn serialization_error(_source: serde_json::Error) -> EnrollmentError {
    error(
        EnrollmentErrorCode::StoreUnavailable,
        "Enrollment Store contains invalid ceremony evidence",
    )
}

async fn insert_grant(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    grant: &EnrollmentGrant,
) -> Result<(), EnrollmentError> {
    sqlx::query(
        r#"
        insert into platform.system_plane_enrollment_grants (
            system_id, managed_service_id, managed_service_principal,
            managed_service_revision, console_service_principal, offer_digest,
            receipt_digest, grant_revision, authorization_epoch, expires_at_unix_ms,
            capabilities, policy
        ) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        "#,
    )
    .bind(&grant.system_id)
    .bind(&grant.managed_service_id)
    .bind(&grant.managed_service_principal)
    .bind(&grant.managed_service_revision)
    .bind(&grant.console_service_principal)
    .bind(&grant.offer_digest)
    .bind(&grant.receipt_digest)
    .bind(to_i64(grant.grant_revision, "grant revision")?)
    .bind(to_i64(grant.authorization_epoch, "authorization epoch")?)
    .bind(to_i64(grant.expires_at_unix_ms, "expiry")?)
    .bind(serde_json::to_value(&grant.capabilities).map_err(serialization_error)?)
    .bind(serde_json::to_value(&grant.policy).map_err(serialization_error)?)
    .execute(&mut **transaction)
    .await
    .map_err(store_error)?;
    Ok(())
}

async fn replace_revoked_grant(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    grant: &EnrollmentGrant,
    expected_authorization_epoch: i64,
) -> Result<(), EnrollmentError> {
    let updated = sqlx::query(
        r#"
        update platform.system_plane_enrollment_grants
        set system_id = $3,
            managed_service_principal = $4,
            managed_service_revision = $5,
            console_service_principal = $6,
            offer_digest = $7,
            receipt_digest = $8,
            grant_revision = $9,
            authorization_epoch = $10,
            expires_at_unix_ms = $11,
            capabilities = $12,
            policy = $13,
            revoked_at_unix_ms = null,
            updated_at = now()
        where managed_service_id = $1
          and authorization_epoch = $2
          and revoked_at_unix_ms is not null
        "#,
    )
    .bind(&grant.managed_service_id)
    .bind(expected_authorization_epoch)
    .bind(&grant.system_id)
    .bind(&grant.managed_service_principal)
    .bind(&grant.managed_service_revision)
    .bind(&grant.console_service_principal)
    .bind(&grant.offer_digest)
    .bind(&grant.receipt_digest)
    .bind(to_i64(grant.grant_revision, "grant revision")?)
    .bind(to_i64(grant.authorization_epoch, "authorization epoch")?)
    .bind(to_i64(grant.expires_at_unix_ms, "expiry")?)
    .bind(serde_json::to_value(&grant.capabilities).map_err(serialization_error)?)
    .bind(serde_json::to_value(&grant.policy).map_err(serialization_error)?)
    .execute(&mut **transaction)
    .await
    .map_err(store_error)?;
    if updated.rows_affected() != 1 {
        return Err(error(
            EnrollmentErrorCode::StaleAuthorizationEpoch,
            "Enrollment authority changed while accepting the signed transfer",
        ));
    }
    Ok(())
}

async fn append_audit(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    event_kind: &str,
    record: &EnrollmentRecord,
) -> Result<(), EnrollmentError> {
    let evidence = serde_json::to_value(record).map_err(serialization_error)?;
    sqlx::query(
        r#"
        insert into platform.system_plane_enrollment_audit (
            managed_service_id, event_kind, receipt_digest, authorization_epoch, evidence
        ) values ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(&record.grant.managed_service_id)
    .bind(event_kind)
    .bind(&record.grant.receipt_digest)
    .bind(to_i64(
        record.grant.authorization_epoch,
        "authorization epoch",
    )?)
    .bind(evidence)
    .execute(&mut **transaction)
    .await
    .map_err(store_error)?;
    Ok(())
}

fn error(code: EnrollmentErrorCode, message: impl Into<String>) -> EnrollmentError {
    EnrollmentError {
        code,
        message: message.into(),
    }
}
