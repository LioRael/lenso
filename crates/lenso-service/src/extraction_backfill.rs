use crate::{
    ExtractionPlan, ExtractionRun, ExtractionRunStatus, extraction_input_digest,
    extraction_plan_integrity_is_valid, extraction_run_integrity_is_valid,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const EXTRACTION_BACKFILL_PROTOCOL: &str = "lenso.extraction-backfill.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ExtractionBackfillBoundary {
    TrustworthyCursor {
        cursor: String,
        source_high_water_mark: String,
    },
    BoundedWritePause {
        source_high_water_mark: String,
    },
    Missing,
}

impl ExtractionBackfillBoundary {
    fn high_water_mark(&self) -> Option<&str> {
        match self {
            Self::TrustworthyCursor {
                source_high_water_mark,
                ..
            }
            | Self::BoundedWritePause {
                source_high_water_mark,
            } => Some(source_high_water_mark),
            Self::Missing => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionBackfillRecord {
    pub stable_id: String,
    pub record_digest: String,
    pub value: Value,
}

impl ExtractionBackfillRecord {
    #[must_use]
    pub fn new(stable_id: impl Into<String>, value: Value) -> Self {
        let stable_id = stable_id.into();
        let record_digest = digest(&(&stable_id, &value));
        Self {
            stable_id,
            record_digest,
            value,
        }
    }

    fn integrity_is_valid(&self) -> bool {
        self.record_digest == digest(&(&self.stable_id, &self.value))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionBackfillScope {
    pub plan_id: String,
    pub plan_digest: String,
    pub source_owner_id: String,
    pub source_store_id: String,
    pub destination_store_id: String,
    pub table_mappings: Vec<String>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionBackfillStatus {
    Planned,
    InProgress,
    Succeeded,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionBackfillRequest {
    pub batch_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_destination_checkpoint: Option<String>,
    pub records: Vec<ExtractionBackfillRecord>,
    pub final_batch: bool,
}

impl ExtractionBackfillRequest {
    #[must_use]
    pub fn new(
        batch_id: impl Into<String>,
        expected_destination_checkpoint: Option<String>,
        records: Vec<ExtractionBackfillRecord>,
    ) -> Self {
        Self {
            batch_id: batch_id.into(),
            expected_destination_checkpoint,
            records,
            final_batch: false,
        }
    }

    #[must_use]
    pub fn final_batch(mut self) -> Self {
        self.final_batch = true;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionBackfillBatchReceipt {
    pub batch_id: String,
    pub batch_digest: String,
    pub previous_destination_checkpoint: Option<String>,
    pub destination_checkpoint: String,
    pub first_stable_id: Option<String>,
    pub last_stable_id: Option<String>,
    pub copied_count: u64,
    pub duplicate_count: u64,
    pub source_high_water_mark: String,
    pub source_authority_unchanged: bool,
    pub candidate_authoritative: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionBackfillProgress {
    pub copied_count: u64,
    pub remaining_lag: u64,
    pub source_high_water_mark: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination_checkpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_after_stable_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionBackfillEvidence {
    pub kind: String,
    pub subject: String,
    pub digest: String,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionBackfillEffects {
    pub reads_plan_scoped_source_data: bool,
    pub copies_destination_data: bool,
    pub mutates_source_data: bool,
    pub changes_authority: bool,
    pub emits_business_effects: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionBackfillRun {
    pub protocol: String,
    pub run_id: String,
    pub run_digest: String,
    pub revision: u64,
    pub status: ExtractionBackfillStatus,
    pub scope: ExtractionBackfillScope,
    pub boundary: ExtractionBackfillBoundary,
    pub progress: ExtractionBackfillProgress,
    #[serde(default)]
    pub destination_records: Vec<ExtractionBackfillRecord>,
    #[serde(default)]
    pub receipts: Vec<ExtractionBackfillBatchReceipt>,
    #[serde(default)]
    pub evidence: Vec<ExtractionBackfillEvidence>,
    #[serde(default)]
    pub next_actions: Vec<String>,
    pub linked_authority_remains_authoritative: bool,
    pub candidate_authoritative: bool,
    pub effects: ExtractionBackfillEffects,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionBackfillErrorCode {
    PlanInvalid,
    DestinationExpansionIncomplete,
    BackfillCursorMissing,
    BackfillBatchUnordered,
    BackfillCheckpointStale,
    BackfillBatchChanged,
    BackfillRecordChanged,
    BackfillRunInvalid,
    BackfillPersistenceFailed,
}

impl ExtractionBackfillErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PlanInvalid => "plan_invalid",
            Self::DestinationExpansionIncomplete => "destination_expansion_incomplete",
            Self::BackfillCursorMissing => "backfill_cursor_missing",
            Self::BackfillBatchUnordered => "backfill_batch_unordered",
            Self::BackfillCheckpointStale => "backfill_checkpoint_stale",
            Self::BackfillBatchChanged => "backfill_batch_changed",
            Self::BackfillRecordChanged => "backfill_record_changed",
            Self::BackfillRunInvalid => "backfill_run_invalid",
            Self::BackfillPersistenceFailed => "backfill_persistence_failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionBackfillError {
    pub code: ExtractionBackfillErrorCode,
    pub message: String,
    pub next_actions: Vec<String>,
}

impl fmt::Display for ExtractionBackfillError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ExtractionBackfillError {}

pub fn start_extraction_backfill(
    plan: &ExtractionPlan,
    expansion: &ExtractionRun,
    boundary: ExtractionBackfillBoundary,
) -> Result<ExtractionBackfillRun, ExtractionBackfillError> {
    if !extraction_plan_integrity_is_valid(plan) {
        return Err(error(
            ExtractionBackfillErrorCode::PlanInvalid,
            "Extraction Plan integrity validation failed before backfill.",
            "Regenerate and review the Extraction Plan.",
        ));
    }
    if !extraction_run_integrity_is_valid(expansion)
        || expansion.current_phase.status != ExtractionRunStatus::Succeeded
        || expansion.plan.plan_id != plan.plan_id
        || expansion.plan.plan_digest != plan.plan_digest
    {
        return Err(error(
            ExtractionBackfillErrorCode::DestinationExpansionIncomplete,
            "Destination expansion evidence is incomplete or belongs to another plan.",
            "Finish destination expansion for this exact Extraction Plan.",
        ));
    }
    let Some(source_high_water_mark) = boundary.high_water_mark().map(str::to_owned) else {
        return Err(error(
            ExtractionBackfillErrorCode::BackfillCursorMissing,
            "Online backfill requires a trustworthy cursor or a bounded write pause.",
            "Declare a trustworthy extraction cursor or enter the protected write-pause phase.",
        ));
    };
    if source_high_water_mark.is_empty() {
        return Err(error(
            ExtractionBackfillErrorCode::BackfillCursorMissing,
            "The source high-water mark must not be empty.",
            "Capture a durable source high-water mark before copying data.",
        ));
    }
    let mut table_mappings = plan
        .data_mapping
        .tables
        .iter()
        .map(|mapping| format!("{}->{}", mapping.source_table, mapping.destination_table))
        .collect::<Vec<_>>();
    table_mappings.sort();
    let scope = ExtractionBackfillScope {
        plan_id: plan.plan_id.clone(),
        plan_digest: plan.plan_digest.clone(),
        source_owner_id: plan.expected_authority.owner_id.clone(),
        source_store_id: format!("linked:{}", plan.expected_authority.owner_id),
        destination_store_id: plan.proposed_service.store.store_id.clone(),
        table_mappings,
    };
    let identity = digest(&(&scope, &boundary));
    let mut run = ExtractionBackfillRun {
        protocol: EXTRACTION_BACKFILL_PROTOCOL.to_owned(),
        run_id: format!("extraction-backfill:{identity}"),
        run_digest: String::new(),
        revision: 1,
        status: ExtractionBackfillStatus::Planned,
        scope,
        boundary,
        progress: ExtractionBackfillProgress {
            copied_count: 0,
            remaining_lag: 1,
            source_high_water_mark: source_high_water_mark.clone(),
            destination_checkpoint: None,
            next_after_stable_id: None,
        },
        destination_records: Vec::new(),
        receipts: Vec::new(),
        evidence: vec![ExtractionBackfillEvidence {
            kind: "source_boundary".to_owned(),
            subject: source_high_water_mark.clone(),
            digest: extraction_input_digest(source_high_water_mark.as_bytes()),
            detail: "Backfill is bounded by plan-scoped source evidence.".to_owned(),
        }],
        next_actions: vec!["Copy the next deterministic Postgres batch and persist its receipt atomically with the destination checkpoint.".to_owned()],
        linked_authority_remains_authoritative: true,
        candidate_authoritative: false,
        effects: ExtractionBackfillEffects::default(),
    };
    refresh_digest(&mut run);
    Ok(run)
}

pub fn apply_extraction_backfill_batch(
    mut run: ExtractionBackfillRun,
    request: ExtractionBackfillRequest,
) -> Result<ExtractionBackfillRun, ExtractionBackfillError> {
    if !extraction_backfill_integrity_is_valid(&run) {
        return Err(error(
            ExtractionBackfillErrorCode::BackfillRunInvalid,
            "Backfill Run integrity validation failed.",
            "Resume from the last integrity-valid durable revision.",
        ));
    }
    let batch_digest = digest(&request);
    if let Some(receipt) = run
        .receipts
        .iter()
        .find(|receipt| receipt.batch_id == request.batch_id)
    {
        if receipt.batch_digest == batch_digest {
            return Ok(run);
        }
        return Err(error(
            ExtractionBackfillErrorCode::BackfillBatchChanged,
            "A committed batch id was reused with different contents.",
            "Resume with the original batch or allocate the next ordered batch id.",
        ));
    }
    if request.expected_destination_checkpoint != run.progress.destination_checkpoint {
        return Err(error(
            ExtractionBackfillErrorCode::BackfillCheckpointStale,
            "The requested destination checkpoint is stale.",
            "Reload the durable Backfill Run and resume from its current checkpoint.",
        ));
    }
    let ordered_ids = request
        .records
        .iter()
        .map(|record| record.stable_id.as_str())
        .collect::<Vec<_>>();
    if ordered_ids
        .windows(2)
        .any(|pair| !stable_id_is_strictly_before(pair[0], pair[1]))
        || request
            .records
            .iter()
            .any(|record| !record.integrity_is_valid())
    {
        return Err(error(
            ExtractionBackfillErrorCode::BackfillBatchUnordered,
            "Backfill records must have unique stable identities in deterministic ascending order.",
            "Sort the source query by the declared stable identity and rebuild the batch.",
        ));
    }
    if run
        .progress
        .next_after_stable_id
        .as_deref()
        .is_some_and(|last| {
            request.records.first().is_some_and(|record| {
                !stable_id_is_strictly_before(last, record.stable_id.as_str())
            })
        })
    {
        return Err(error(
            ExtractionBackfillErrorCode::BackfillBatchUnordered,
            "The next batch does not advance beyond the durable stable-identity checkpoint.",
            "Resume the source query strictly after nextAfterStableId.",
        ));
    }
    let mut destination = run
        .destination_records
        .iter()
        .cloned()
        .map(|record| (record.stable_id.clone(), record))
        .collect::<BTreeMap<_, _>>();
    let mut copied_count = 0_u64;
    let mut duplicate_count = 0_u64;
    for record in &request.records {
        match destination.get(&record.stable_id) {
            Some(existing) if existing == record => duplicate_count += 1,
            Some(_) => {
                return Err(error(
                    ExtractionBackfillErrorCode::BackfillRecordChanged,
                    "A stable source identity changed after it was checkpointed.",
                    "Capture a fresh source boundary and regenerate the affected batch.",
                ));
            }
            None => {
                destination.insert(record.stable_id.clone(), record.clone());
                copied_count += 1;
            }
        }
    }
    let destination_checkpoint = format!(
        "backfill-checkpoint:{}",
        digest(&(
            run.run_id.as_str(),
            run.progress.destination_checkpoint.as_deref(),
            request.batch_id.as_str(),
            batch_digest.as_str(),
        ))
    );
    let first_stable_id = request
        .records
        .first()
        .map(|record| record.stable_id.clone());
    let last_stable_id = request
        .records
        .last()
        .map(|record| record.stable_id.clone());
    let receipt = ExtractionBackfillBatchReceipt {
        batch_id: request.batch_id,
        batch_digest,
        previous_destination_checkpoint: run.progress.destination_checkpoint.clone(),
        destination_checkpoint: destination_checkpoint.clone(),
        first_stable_id,
        last_stable_id: last_stable_id.clone(),
        copied_count,
        duplicate_count,
        source_high_water_mark: run.progress.source_high_water_mark.clone(),
        source_authority_unchanged: true,
        candidate_authoritative: false,
    };
    run.destination_records = destination.into_values().collect();
    run.receipts.push(receipt.clone());
    run.progress.copied_count += copied_count;
    run.progress.destination_checkpoint = Some(destination_checkpoint);
    run.progress.next_after_stable_id = last_stable_id;
    run.progress.remaining_lag = u64::from(!request.final_batch);
    run.status = if request.final_batch {
        ExtractionBackfillStatus::Succeeded
    } else {
        ExtractionBackfillStatus::InProgress
    };
    run.effects.reads_plan_scoped_source_data = true;
    run.effects.copies_destination_data |= copied_count > 0;
    run.evidence.push(ExtractionBackfillEvidence {
        kind: "durable_batch_receipt".to_owned(),
        subject: receipt.batch_id.clone(),
        digest: receipt.batch_digest.clone(),
        detail: format!(
            "Copied {} records and observed {} already-checkpointed records.",
            receipt.copied_count, receipt.duplicate_count
        ),
    });
    run.next_actions = if request.final_batch {
        vec!["Reconcile the candidate Store against this exact source high-water mark and destination checkpoint.".to_owned()]
    } else {
        vec!["Persist this receipt, then request the next ordered source batch after nextAfterStableId.".to_owned()]
    };
    run.revision += 1;
    refresh_digest(&mut run);
    Ok(run)
}

/// Atomically persists destination records, the batch receipt, and the next
/// checkpoint in PostgreSQL. The run row is locked and compared by digest so a
/// restarted or concurrent orchestrator cannot overwrite newer progress.
pub async fn apply_postgres_extraction_backfill_batch(
    pool: &sqlx::PgPool,
    run: ExtractionBackfillRun,
    request: ExtractionBackfillRequest,
) -> Result<ExtractionBackfillRun, ExtractionBackfillError> {
    let mut transaction = pool.begin().await.map_err(persistence_error)?;
    sqlx::query("create schema if not exists lenso_extraction")
        .execute(&mut *transaction)
        .await
        .map_err(persistence_error)?;
    sqlx::query(
        r#"
        create table if not exists lenso_extraction.backfill_runs (
            run_id text primary key,
            revision bigint not null,
            run_digest text not null,
            run_json jsonb not null,
            updated_at timestamptz not null default now()
        )
        "#,
    )
    .execute(&mut *transaction)
    .await
    .map_err(persistence_error)?;
    sqlx::query(
        r#"
        create table if not exists lenso_extraction.backfill_records (
            run_id text not null references lenso_extraction.backfill_runs(run_id),
            stable_id text not null,
            record_digest text not null,
            record_json jsonb not null,
            primary key (run_id, stable_id)
        )
        "#,
    )
    .execute(&mut *transaction)
    .await
    .map_err(persistence_error)?;
    sqlx::query("select pg_advisory_xact_lock(hashtext($1))")
        .bind(&run.run_id)
        .execute(&mut *transaction)
        .await
        .map_err(persistence_error)?;

    let stored = sqlx::query_as::<_, (i64, String, serde_json::Value)>(
        "select revision, run_digest, run_json from lenso_extraction.backfill_runs where run_id = $1 for update",
    )
    .bind(&run.run_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(persistence_error)?;
    let durable_run = if let Some((_, stored_digest, stored_json)) = stored {
        let stored_run: ExtractionBackfillRun =
            serde_json::from_value(stored_json).map_err(|source| {
                error(
                    ExtractionBackfillErrorCode::BackfillPersistenceFailed,
                    format!("Stored Backfill Run is unreadable: {source}"),
                    "Repair or restore the last integrity-valid durable Run.",
                )
            })?;
        if stored_digest != run.run_digest {
            let replayed = apply_extraction_backfill_batch(run.clone(), request.clone())?;
            let expected_receipt = replayed
                .receipts
                .iter()
                .find(|receipt| receipt.batch_id == request.batch_id);
            let stored_receipt = stored_run
                .receipts
                .iter()
                .find(|receipt| receipt.batch_id == request.batch_id);
            if expected_receipt == stored_receipt && stored_receipt.is_some() {
                transaction.commit().await.map_err(persistence_error)?;
                return Ok(stored_run);
            }
            return Err(error(
                ExtractionBackfillErrorCode::BackfillCheckpointStale,
                "A newer durable PostgreSQL checkpoint already exists for this Backfill Run.",
                "Reload the durable Run and resume from its current checkpoint.",
            ));
        }
        stored_run
    } else {
        sqlx::query(
            "insert into lenso_extraction.backfill_runs (run_id, revision, run_digest, run_json) values ($1, $2, $3, $4)",
        )
        .bind(&run.run_id)
        .bind(i64::try_from(run.revision).unwrap_or(i64::MAX))
        .bind(&run.run_digest)
        .bind(serde_json::to_value(&run).map_err(|source| {
            error(
                ExtractionBackfillErrorCode::BackfillPersistenceFailed,
                format!("Backfill Run could not serialize: {source}"),
                "Persist an integrity-valid Backfill Run.",
            )
        })?)
        .execute(&mut *transaction)
        .await
        .map_err(persistence_error)?;
        run
    };
    let previous_revision = durable_run.revision;
    let next = apply_extraction_backfill_batch(durable_run, request)?;
    for record in &next.destination_records {
        let persisted = sqlx::query(
            r#"
            insert into lenso_extraction.backfill_records (run_id, stable_id, record_digest, record_json)
            values ($1, $2, $3, $4)
            on conflict (run_id, stable_id) do update
            set record_digest = excluded.record_digest, record_json = excluded.record_json
            where lenso_extraction.backfill_records.record_digest = excluded.record_digest
            "#,
        )
        .bind(&next.run_id)
        .bind(&record.stable_id)
        .bind(&record.record_digest)
        .bind(&record.value)
        .execute(&mut *transaction)
        .await
        .map_err(persistence_error)?;
        if persisted.rows_affected() != 1 {
            return Err(error(
                ExtractionBackfillErrorCode::BackfillRecordChanged,
                format!(
                    "Durable candidate record {} differs from the checkpointed Backfill Run.",
                    record.stable_id
                ),
                "Reconcile the durable record ledger before advancing the checkpoint.",
            ));
        }
    }
    let updated = sqlx::query(
        r#"
        update lenso_extraction.backfill_runs
        set revision = $2, run_digest = $3, run_json = $4, updated_at = now()
        where run_id = $1 and revision = $5
        "#,
    )
    .bind(&next.run_id)
    .bind(i64::try_from(next.revision).unwrap_or(i64::MAX))
    .bind(&next.run_digest)
    .bind(serde_json::to_value(&next).map_err(|source| {
        error(
            ExtractionBackfillErrorCode::BackfillPersistenceFailed,
            format!("Backfill Run could not serialize: {source}"),
            "Persist an integrity-valid Backfill Run.",
        )
    })?)
    .bind(i64::try_from(previous_revision).unwrap_or(i64::MAX))
    .execute(&mut *transaction)
    .await
    .map_err(persistence_error)?;
    if updated.rows_affected() != 1 {
        return Err(error(
            ExtractionBackfillErrorCode::BackfillCheckpointStale,
            "The durable PostgreSQL checkpoint changed during this batch.",
            "Reload the durable Run and retry from its current checkpoint.",
        ));
    }
    transaction.commit().await.map_err(persistence_error)?;
    Ok(next)
}

/// Reload the last transactionally committed run after process restart or a
/// lost client response.
pub async fn load_postgres_extraction_backfill(
    pool: &sqlx::PgPool,
    run_id: &str,
) -> Result<Option<ExtractionBackfillRun>, ExtractionBackfillError> {
    let exists = sqlx::query_scalar::<_, Option<String>>(
        "select to_regclass('lenso_extraction.backfill_runs')::text",
    )
    .fetch_one(pool)
    .await
    .map_err(persistence_error)?
    .is_some();
    if !exists {
        return Ok(None);
    }
    let value = sqlx::query_scalar::<_, serde_json::Value>(
        "select run_json from lenso_extraction.backfill_runs where run_id = $1",
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await
    .map_err(persistence_error)?;
    value
        .map(|value| {
            serde_json::from_value(value).map_err(|source| {
                error(
                    ExtractionBackfillErrorCode::BackfillPersistenceFailed,
                    format!("Stored Backfill Run is unreadable: {source}"),
                    "Repair or restore the last integrity-valid durable Run.",
                )
            })
        })
        .transpose()
}

/// Read one plan-scoped source batch and copy it into the planned candidate
/// Postgres table before atomically advancing the durable checkpoint.
pub async fn copy_postgres_extraction_service_data_batch(
    source_pool: &sqlx::PgPool,
    destination_pool: &sqlx::PgPool,
    plan: &ExtractionPlan,
    run: ExtractionBackfillRun,
    batch_id: impl Into<String>,
    limit: i64,
) -> Result<ExtractionBackfillRun, ExtractionBackfillError> {
    if !extraction_plan_integrity_is_valid(plan)
        || run.scope.plan_id != plan.plan_id
        || run.scope.plan_digest != plan.plan_digest
    {
        return Err(error(
            ExtractionBackfillErrorCode::BackfillRunInvalid,
            "Backfill Run does not belong to the supplied Extraction Plan.",
            "Load the exact plan-scoped Run before reading Service Data.",
        ));
    }
    if plan.data_mapping.tables.len() != 1 {
        return Err(error(
            ExtractionBackfillErrorCode::BackfillRunInvalid,
            "A Postgres Backfill Run must be scoped to exactly one table mapping.",
            "Create one durable plan-scoped Backfill Run per table mapping.",
        ));
    }
    let mapping = plan.data_mapping.tables.first().ok_or_else(|| {
        error(
            ExtractionBackfillErrorCode::BackfillRunInvalid,
            "Extraction Plan has no Postgres table mapping.",
            "Regenerate the plan with an owned source and destination table.",
        )
    })?;
    let cursor = mapping
        .cursors
        .iter()
        .find(|cursor| cursor.trustworthy)
        .ok_or_else(|| {
            error(
                ExtractionBackfillErrorCode::BackfillCursorMissing,
                "Extraction Plan has no trustworthy Postgres cursor.",
                "Enter the bounded write-pause phase or regenerate cursor evidence.",
            )
        })?;
    let source_table = quoted_relation(&mapping.source_table)?;
    let destination_table = quoted_relation(&mapping.destination_table)?;
    let cursor_column = quoted_identifier(&cursor.column)?;
    let cursor_name = cursor.column.as_str();
    let after_cursor = format!(
        "(jsonb_populate_record(null::{source_table}, jsonb_build_object('{cursor_name}', $1::text))).{cursor_column}"
    );
    let high_water_cursor = format!(
        "(jsonb_populate_record(null::{source_table}, jsonb_build_object('{cursor_name}', $2::text))).{cursor_column}"
    );
    let after = run
        .progress
        .next_after_stable_id
        .clone()
        .unwrap_or_default();
    let rows = sqlx::query_as::<_, (String, serde_json::Value)>(sqlx::AssertSqlSafe(format!(
        "select {cursor_column}::text, to_jsonb(source_row) from {source_table} source_row where ($1 = '' or {cursor_column} > {after_cursor}) and {cursor_column} <= {high_water_cursor} order by {cursor_column} limit $3"
    )))
    .bind(&after)
    .bind(&run.progress.source_high_water_mark)
    .bind(limit.max(1))
    .fetch_all(source_pool)
    .await
    .map_err(persistence_error)?;
    let mut transaction = destination_pool.begin().await.map_err(persistence_error)?;
    let mut records = Vec::with_capacity(rows.len());
    for (stable_id, value) in rows {
        let existing = sqlx::query_scalar::<_, serde_json::Value>(sqlx::AssertSqlSafe(format!(
            "select to_jsonb(destination_row) from {destination_table} destination_row where {cursor_column}::text = $1"
        )))
        .bind(&stable_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(persistence_error)?;
        if let Some(existing) = existing {
            if existing != value {
                return Err(error(
                    ExtractionBackfillErrorCode::BackfillRecordChanged,
                    format!("Candidate record {stable_id} differs from the plan-scoped source."),
                    "Reconcile the conflicting candidate record before resuming.",
                ));
            }
        } else {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "insert into {destination_table} select * from jsonb_populate_record(null::{destination_table}, $1)"
            )))
            .bind(&value)
            .execute(&mut *transaction)
            .await
            .map_err(persistence_error)?;
        }
        records.push(ExtractionBackfillRecord::new(stable_id, value));
    }
    transaction.commit().await.map_err(persistence_error)?;
    let final_batch = records
        .last()
        .is_none_or(|record| record.stable_id == run.progress.source_high_water_mark)
        || i64::try_from(records.len()).unwrap_or(i64::MAX) < limit.max(1);
    let mut request = ExtractionBackfillRequest::new(
        batch_id,
        run.progress.destination_checkpoint.clone(),
        records,
    );
    request.final_batch = final_batch;
    apply_postgres_extraction_backfill_batch(destination_pool, run, request).await
}

fn quoted_relation(value: &str) -> Result<String, ExtractionBackfillError> {
    value
        .split('.')
        .map(quoted_identifier)
        .collect::<Result<Vec<_>, _>>()
        .map(|parts| parts.join("."))
}

fn quoted_identifier(value: &str) -> Result<String, ExtractionBackfillError> {
    if value.is_empty()
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(error(
            ExtractionBackfillErrorCode::BackfillRunInvalid,
            format!("Unsafe Postgres identifier `{value}` in Extraction Plan."),
            "Regenerate the plan from validated schema ownership evidence.",
        ));
    }
    Ok(format!("\"{value}\""))
}

fn persistence_error(source: sqlx::Error) -> ExtractionBackfillError {
    error(
        ExtractionBackfillErrorCode::BackfillPersistenceFailed,
        format!("PostgreSQL backfill persistence failed: {source}"),
        "Restore PostgreSQL availability and resume from the last durable checkpoint.",
    )
}

#[must_use]
pub fn extraction_backfill_integrity_is_valid(run: &ExtractionBackfillRun) -> bool {
    if run.protocol != EXTRACTION_BACKFILL_PROTOCOL
        || !run.linked_authority_remains_authoritative
        || run.candidate_authoritative
        || run.effects.mutates_source_data
        || run.effects.changes_authority
        || run.effects.emits_business_effects
        || run.progress.source_high_water_mark != run.boundary.high_water_mark().unwrap_or_default()
        || run
            .destination_records
            .iter()
            .any(|record| !record.integrity_is_valid())
        || run
            .destination_records
            .windows(2)
            .any(|pair| !stable_id_is_strictly_before(&pair[0].stable_id, &pair[1].stable_id))
    {
        return false;
    }
    let receipt_ids = run
        .receipts
        .iter()
        .map(|receipt| receipt.batch_id.as_str())
        .collect::<BTreeSet<_>>();
    receipt_ids.len() == run.receipts.len() && run.run_digest == run_digest(run)
}

fn stable_id_is_strictly_before(left: &str, right: &str) -> bool {
    match (left.parse::<i128>(), right.parse::<i128>()) {
        (Ok(left), Ok(right)) => left < right,
        _ => left < right,
    }
}

fn refresh_digest(run: &mut ExtractionBackfillRun) {
    run.run_digest = run_digest(run);
}

fn run_digest(run: &ExtractionBackfillRun) -> String {
    let mut value = run.clone();
    value.run_digest.clear();
    digest(&value)
}

fn digest(value: &impl Serialize) -> String {
    let bytes = serde_json::to_vec(value).expect("Extraction backfill values must serialize");
    extraction_input_digest(&bytes)
}

fn error(
    code: ExtractionBackfillErrorCode,
    message: impl Into<String>,
    next_action: impl Into<String>,
) -> ExtractionBackfillError {
    ExtractionBackfillError {
        code,
        message: message.into(),
        next_actions: vec![next_action.into()],
    }
}
