use crate::{
    CompatibilityCategory, ContractSemanticKind, SystemV2Graph, SystemV2GraphRelationship,
    system_v2_graph,
};
use lenso_contracts::{
    AdminSurface, ModuleHttpMethod, ModuleManifest, ModuleManifestLintSeverity, ModuleSource,
    StoryDisplaySource, lint_module_manifest,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const EXTRACTION_READINESS_REPORT_PROTOCOL: &str = "lenso.extraction-readiness-report.v1";
pub const EXTRACTION_READINESS_ANALYZER_VERSION: &str = "lenso.extraction-readiness.v2";
const EXTRACTION_READINESS_SCHEMA_ID: &str =
    "https://contracts.lenso.local/extraction/lenso.extraction-readiness-report.v1.schema.json";
const LARGE_DATA_VOLUME_ROWS: u64 = 1_000_000;
const LARGE_DATA_VOLUME_BYTES: u64 = 1_073_741_824;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionBoundaryReferenceKind {
    CrossModuleImport,
    InProcessBoundaryCall,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionBoundaryReference {
    pub kind: ExtractionBoundaryReferenceKind,
    pub from_module: String,
    pub to_module: String,
    pub symbol: String,
    pub evidence_reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionBoundaryEvidence {
    pub complete: bool,
    #[serde(default)]
    pub evidence_references: Vec<String>,
    #[serde(default)]
    pub references: Vec<ExtractionBoundaryReference>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionContractKind {
    Service,
    Event,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionContractDirection {
    Provides,
    Consumes,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionEvidenceStatus {
    Present,
    Missing,
    Ambiguous,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionContractEvidence {
    pub subject: String,
    pub kind: ExtractionContractKind,
    pub direction: ExtractionContractDirection,
    pub status: ExtractionEvidenceStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract_id: Option<String>,
    #[serde(default)]
    pub evidence_references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionConsumerCompatibilityEvidence {
    pub consumer_id: String,
    pub contract_id: String,
    pub classification: CompatibilityCategory,
    #[serde(default)]
    pub evidence_references: Vec<String>,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ExtractionDataEvidenceSource {
    StaticDeclaration,
    LiveStoreObservation {
        observation_id: String,
        store: String,
        read_only: bool,
    },
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionDataAccessKind {
    Read,
    Write,
    ReadWrite,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionCursorEvidence {
    pub column: String,
    pub high_water_mark: String,
    pub trustworthy: bool,
    #[serde(default)]
    pub evidence_references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionDataVolumeEvidence {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approximate_rows: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approximate_bytes: Option<u64>,
    #[serde(default)]
    pub evidence_references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionDataTableEvidence {
    pub table: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_module: Option<String>,
    pub source: ExtractionDataEvidenceSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<ExtractionDataVolumeEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<ExtractionCursorEvidence>,
    #[serde(default)]
    pub evidence_references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionMigrationEvidence {
    pub migration: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_module: Option<String>,
    pub source: ExtractionDataEvidenceSource,
    #[serde(default)]
    pub evidence_references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionDataAccessEvidence {
    pub accessor_module: String,
    pub table: String,
    pub access: ExtractionDataAccessKind,
    pub source: ExtractionDataEvidenceSource,
    #[serde(default)]
    pub evidence_references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionTransactionEvidence {
    pub transaction: String,
    #[serde(default)]
    pub participating_modules: Vec<String>,
    pub source: ExtractionDataEvidenceSource,
    #[serde(default)]
    pub evidence_references: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionServiceDataEvidence {
    pub complete: bool,
    #[serde(default)]
    pub evidence_references: Vec<String>,
    #[serde(default)]
    pub tables: Vec<ExtractionDataTableEvidence>,
    #[serde(default)]
    pub migrations: Vec<ExtractionMigrationEvidence>,
    #[serde(default)]
    pub access_paths: Vec<ExtractionDataAccessEvidence>,
    #[serde(default)]
    pub transactions: Vec<ExtractionTransactionEvidence>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionReadinessEvidence {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boundary: Option<ExtractionBoundaryEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contracts: Option<Vec<ExtractionContractEvidence>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_consumers: Option<Vec<ExtractionConsumerCompatibilityEvidence>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_data: Option<ExtractionServiceDataEvidence>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionReadinessIssueCode {
    ActiveConsumerBlocked,
    ActiveConsumerBreaking,
    ActiveConsumerCompatibilityMissing,
    ActiveConsumerEvidenceAmbiguous,
    ActiveConsumerNeedsAttention,
    AdminSurfacePresent,
    BoundaryClean,
    BoundaryEvidenceAmbiguous,
    BoundaryEvidenceIncomplete,
    BoundaryEvidenceMissing,
    BoundaryEvidenceTargetMismatch,
    ConsoleSurfacePresent,
    ConsumersCompatible,
    ContractEvidenceAmbiguous,
    ContractEvidenceMissing,
    ContractIdentityMismatch,
    ContractsComplete,
    CrossModuleTableAccess,
    CrossModuleImport,
    DataVolumeLarge,
    ExtractionCursorMissing,
    ExtractionCursorUsable,
    InProcessBoundaryCall,
    LiveStoreObservationNotReadOnly,
    LiveStoreObservationPresent,
    ManifestInvalid,
    ManifestNeedsAttention,
    MigrationOwnershipUnresolved,
    RequiredEventContractMissing,
    RequiredServiceContractMissing,
    RuntimeSurfacePresent,
    ServiceDataEvidenceIncomplete,
    ServiceDataEvidenceMissing,
    ServiceDataReady,
    StorySurfacePresent,
    SystemEvidenceInvalid,
    TableOwnershipUnresolved,
    TargetModuleMissing,
    TargetModuleNotLinked,
    TransactionBoundaryUnresolved,
    TransactionSpansServiceBoundary,
    WorkflowSurfacePresent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionReadinessFinding {
    pub classification: CompatibilityCategory,
    pub code: ExtractionReadinessIssueCode,
    pub subject: String,
    pub message: String,
    pub evidence_references: Vec<String>,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionReadinessSurfaceSummary {
    #[serde(default)]
    pub http_routes: Vec<String>,
    #[serde(default)]
    pub event_handlers: Vec<String>,
    #[serde(default)]
    pub runtime_functions: Vec<String>,
    #[serde(default)]
    pub schedules: Vec<String>,
    #[serde(default)]
    pub workflows: Vec<String>,
    #[serde(default)]
    pub admin: Vec<String>,
    #[serde(default)]
    pub console: Vec<String>,
    #[serde(default)]
    pub stories: Vec<String>,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionReadinessEffects {
    pub writes_repository_files: bool,
    pub starts_workloads: bool,
    pub moves_data: bool,
    pub changes_authority: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionReadinessReport {
    pub protocol: String,
    pub analyzer_version: String,
    pub target_module: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_owner: Option<String>,
    pub classification: CompatibilityCategory,
    pub ready: bool,
    #[serde(default)]
    pub issue_codes: Vec<ExtractionReadinessIssueCode>,
    #[serde(default)]
    pub contract_evidence: Vec<ExtractionContractEvidence>,
    #[serde(default)]
    pub active_consumers: Vec<ExtractionConsumerCompatibilityEvidence>,
    pub surfaces: ExtractionReadinessSurfaceSummary,
    #[serde(default)]
    pub service_data: ExtractionServiceDataEvidence,
    pub findings: Vec<ExtractionReadinessFinding>,
    pub effects: ExtractionReadinessEffects,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RequiredContractSubject<'a> {
    subject: String,
    kind: ExtractionContractKind,
    direction: ExtractionContractDirection,
    expected_contract_id: Option<&'a str>,
}

#[must_use]
pub fn evaluate_extraction_readiness(
    module: &ModuleManifest,
    system: &Value,
    evidence: &ExtractionReadinessEvidence,
) -> ExtractionReadinessReport {
    let mut findings = Vec::new();
    let surfaces = surface_summary(module);
    collect_manifest_findings(module, &mut findings);
    collect_surface_findings(&surfaces, &mut findings);

    let system_id = system
        .get("systemId")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let graph = match system_v2_graph(system) {
        Ok(graph) => Some(graph),
        Err(mut issues) => {
            issues.sort_by(|left, right| (&left.path, &left.code).cmp(&(&right.path, &right.code)));
            for issue in issues {
                push_finding(
                    &mut findings,
                    CompatibilityCategory::Blocked,
                    ExtractionReadinessIssueCode::SystemEvidenceInvalid,
                    issue.path.clone(),
                    format!(
                        "System evidence is invalid ({}): {}",
                        issue.code, issue.message
                    ),
                    vec![format!("system:{}", issue.path)],
                    vec![issue.next_action],
                );
            }
            None
        }
    };
    let target_owner = graph
        .as_ref()
        .and_then(|graph| collect_target_owner(module, graph, &mut findings));

    collect_boundary_findings(module, evidence.boundary.as_ref(), &mut findings);
    let contract_evidence = normalized_contract_evidence(evidence.contracts.as_deref());
    let contract_ids =
        collect_contract_findings(module, contract_evidence.as_deref(), &mut findings);
    let active_consumers = normalized_consumer_evidence(evidence.active_consumers.as_deref());
    collect_consumer_findings(
        graph.as_ref(),
        &contract_ids,
        active_consumers.as_deref(),
        &mut findings,
    );
    let service_data = normalized_service_data(evidence.service_data.as_ref());
    collect_service_data_findings(
        module,
        evidence.service_data.as_ref(),
        &service_data,
        &mut findings,
    );

    normalize_findings(&mut findings);
    let classification = findings
        .iter()
        .map(|finding| finding.classification)
        .max_by_key(|classification| classification_rank(*classification))
        .unwrap_or(CompatibilityCategory::Safe);
    let mut issue_codes = findings
        .iter()
        .filter(|finding| finding.classification != CompatibilityCategory::Safe)
        .map(|finding| finding.code)
        .collect::<Vec<_>>();
    issue_codes.sort();
    issue_codes.dedup();

    ExtractionReadinessReport {
        protocol: EXTRACTION_READINESS_REPORT_PROTOCOL.to_owned(),
        analyzer_version: EXTRACTION_READINESS_ANALYZER_VERSION.to_owned(),
        target_module: module.name.clone(),
        system_id,
        target_owner,
        classification,
        ready: matches!(
            classification,
            CompatibilityCategory::Safe | CompatibilityCategory::NeedsAttention
        ),
        issue_codes,
        contract_evidence: contract_evidence.unwrap_or_default(),
        active_consumers: active_consumers.unwrap_or_default(),
        surfaces,
        service_data,
        findings,
        effects: ExtractionReadinessEffects::default(),
    }
}

fn normalized_contract_evidence(
    evidence: Option<&[ExtractionContractEvidence]>,
) -> Option<Vec<ExtractionContractEvidence>> {
    evidence.map(|evidence| {
        let mut normalized = evidence.to_vec();
        for contract in &mut normalized {
            normalize_strings(&mut contract.evidence_references);
        }
        normalized.sort();
        normalized
    })
}

fn normalized_consumer_evidence(
    evidence: Option<&[ExtractionConsumerCompatibilityEvidence]>,
) -> Option<Vec<ExtractionConsumerCompatibilityEvidence>> {
    evidence.map(|evidence| {
        let mut normalized = evidence.to_vec();
        for consumer in &mut normalized {
            normalize_strings(&mut consumer.evidence_references);
        }
        normalized.sort();
        normalized
    })
}

fn normalized_service_data(
    evidence: Option<&ExtractionServiceDataEvidence>,
) -> ExtractionServiceDataEvidence {
    let mut normalized = evidence.cloned().unwrap_or_default();
    normalize_strings(&mut normalized.evidence_references);
    for table in &mut normalized.tables {
        normalize_strings(&mut table.evidence_references);
        if let Some(volume) = &mut table.volume {
            normalize_strings(&mut volume.evidence_references);
        }
        if let Some(cursor) = &mut table.cursor {
            normalize_strings(&mut cursor.evidence_references);
        }
    }
    for migration in &mut normalized.migrations {
        normalize_strings(&mut migration.evidence_references);
    }
    for access in &mut normalized.access_paths {
        normalize_strings(&mut access.evidence_references);
    }
    for transaction in &mut normalized.transactions {
        normalize_strings(&mut transaction.participating_modules);
        normalize_strings(&mut transaction.evidence_references);
    }
    normalized.tables.sort();
    normalized.tables.dedup();
    normalized.migrations.sort();
    normalized.migrations.dedup();
    normalized.access_paths.sort();
    normalized.access_paths.dedup();
    normalized.transactions.sort();
    normalized.transactions.dedup();
    normalized
}

fn collect_service_data_findings(
    module: &ModuleManifest,
    supplied: Option<&ExtractionServiceDataEvidence>,
    data: &ExtractionServiceDataEvidence,
    findings: &mut Vec<ExtractionReadinessFinding>,
) {
    let finding_start = findings.len();
    let Some(_) = supplied else {
        push_finding(
            findings,
            CompatibilityCategory::Blocked,
            ExtractionReadinessIssueCode::ServiceDataEvidenceMissing,
            "service_data".to_owned(),
            "Service Data evidence is missing, so Postgres ownership and extraction safety are unknown.",
            vec!["extraction-evidence:serviceData".to_owned()],
            vec!["Supply complete table, migration, access-path, transaction, volume, and cursor evidence before planning extraction.".to_owned()],
        );
        return;
    };
    if !data.complete {
        push_finding(
            findings,
            CompatibilityCategory::Blocked,
            ExtractionReadinessIssueCode::ServiceDataEvidenceIncomplete,
            "service_data".to_owned(),
            "Service Data analysis is incomplete, so missing Postgres coupling cannot be treated as safe.",
            evidence_references_or(&data.evidence_references, "extraction-evidence:serviceData"),
            vec!["Complete the Service Data analysis and rerun extraction readiness.".to_owned()],
        );
    }

    collect_live_store_findings(data, findings);

    let table_ownership = collect_table_ownership(data, findings);
    collect_migration_ownership(data, findings);
    collect_table_access_findings(module, data, &table_ownership, findings);
    collect_transaction_findings(module, data, findings);
    collect_volume_and_cursor_findings(module, data, &table_ownership, findings);

    if findings[finding_start..]
        .iter()
        .all(|finding| finding.classification != CompatibilityCategory::Blocked)
    {
        push_finding(
            findings,
            CompatibilityCategory::Safe,
            ExtractionReadinessIssueCode::ServiceDataReady,
            "service_data".to_owned(),
            "Service Data ownership, access paths, and transaction boundaries are safe enough to plan extraction.",
            evidence_references_or(
                &data.evidence_references,
                "extraction-evidence:serviceData",
            ),
            vec!["Carry the reported tables, migrations, volume, and cursor evidence into the Extraction Plan.".to_owned()],
        );
    }
}

fn collect_live_store_findings(
    data: &ExtractionServiceDataEvidence,
    findings: &mut Vec<ExtractionReadinessFinding>,
) {
    let mut observations = BTreeMap::<(String, String, bool), BTreeSet<String>>::new();
    for (source, references) in data
        .tables
        .iter()
        .map(|item| (&item.source, &item.evidence_references))
        .chain(
            data.migrations
                .iter()
                .map(|item| (&item.source, &item.evidence_references)),
        )
        .chain(
            data.access_paths
                .iter()
                .map(|item| (&item.source, &item.evidence_references)),
        )
        .chain(
            data.transactions
                .iter()
                .map(|item| (&item.source, &item.evidence_references)),
        )
    {
        if let ExtractionDataEvidenceSource::LiveStoreObservation {
            observation_id,
            store,
            read_only,
        } = source
        {
            observations
                .entry((observation_id.clone(), store.clone(), *read_only))
                .or_default()
                .extend(references.iter().cloned());
        }
    }
    for ((observation_id, store, read_only), references) in observations {
        let subject = format!("service_data.observation.{observation_id}");
        if read_only {
            push_finding(
                findings,
                CompatibilityCategory::Safe,
                ExtractionReadinessIssueCode::LiveStoreObservationPresent,
                subject,
                format!(
                    "Read-only live Store observation `{observation_id}` from `{store}` is reported separately from static declarations."
                ),
                evidence_references_or_set(&references, "extraction-evidence:serviceData"),
                vec!["Use the observation as planning evidence only; readiness analysis does not mutate the Store.".to_owned()],
            );
        } else {
            push_finding(
                findings,
                CompatibilityCategory::Blocked,
                ExtractionReadinessIssueCode::LiveStoreObservationNotReadOnly,
                subject,
                format!(
                    "Live Store observation `{observation_id}` from `{store}` is not declared read-only."
                ),
                evidence_references_or_set(&references, "extraction-evidence:serviceData"),
                vec!["Replace it with evidence collected through a read-only Store observation path.".to_owned()],
            );
        }
    }
}

fn collect_table_ownership(
    data: &ExtractionServiceDataEvidence,
    findings: &mut Vec<ExtractionReadinessFinding>,
) -> BTreeMap<String, Option<String>> {
    let mut records = BTreeMap::<String, (BTreeSet<String>, bool, BTreeSet<String>)>::new();
    for table in &data.tables {
        let record = records.entry(table.table.clone()).or_default();
        match table
            .owner_module
            .as_deref()
            .filter(|owner| !owner.trim().is_empty())
        {
            Some(owner) => {
                record.0.insert(owner.to_owned());
            }
            None => record.1 = true,
        }
        record.2.extend(table.evidence_references.iter().cloned());
    }
    records
        .into_iter()
        .map(|(table, (owners, has_unresolved, references))| {
            let owner = if !has_unresolved && owners.len() == 1 {
                owners.first().cloned()
            } else {
                push_finding(
                    findings,
                    CompatibilityCategory::Blocked,
                    ExtractionReadinessIssueCode::TableOwnershipUnresolved,
                    format!("service_data.table.{table}"),
                    if owners.is_empty() {
                        format!("Postgres table `{table}` has no declared Module owner.")
                    } else {
                        format!(
                            "Postgres table `{table}` has unresolved ownership evidence: {}.",
                            owners.into_iter().collect::<Vec<_>>().join(", ")
                        )
                    },
                    evidence_references_or_set(
                        &references,
                        &format!("extraction-evidence:table/{table}"),
                    ),
                    vec![
                        "Assign the table to exactly one Module before planning extraction."
                            .to_owned(),
                    ],
                );
                None
            };
            (table, owner)
        })
        .collect()
}

fn collect_migration_ownership(
    data: &ExtractionServiceDataEvidence,
    findings: &mut Vec<ExtractionReadinessFinding>,
) {
    let mut records = BTreeMap::<String, (BTreeSet<String>, bool, BTreeSet<String>)>::new();
    for migration in &data.migrations {
        let record = records.entry(migration.migration.clone()).or_default();
        match migration
            .owner_module
            .as_deref()
            .filter(|owner| !owner.trim().is_empty())
        {
            Some(owner) => {
                record.0.insert(owner.to_owned());
            }
            None => record.1 = true,
        }
        record
            .2
            .extend(migration.evidence_references.iter().cloned());
    }
    for (migration, (owners, has_unresolved, references)) in records {
        if !has_unresolved && owners.len() == 1 {
            continue;
        }
        push_finding(
            findings,
            CompatibilityCategory::Blocked,
            ExtractionReadinessIssueCode::MigrationOwnershipUnresolved,
            format!("service_data.migration.{migration}"),
            if owners.is_empty() {
                format!("Postgres migration `{migration}` has no declared Module owner.")
            } else {
                format!(
                    "Postgres migration `{migration}` has unresolved ownership evidence: {}.",
                    owners.into_iter().collect::<Vec<_>>().join(", ")
                )
            },
            evidence_references_or_set(
                &references,
                &format!("extraction-evidence:migration/{migration}"),
            ),
            vec!["Assign the migration to exactly one Module and preserve that schema lifecycle in the candidate Service.".to_owned()],
        );
    }
}

fn collect_table_access_findings(
    module: &ModuleManifest,
    data: &ExtractionServiceDataEvidence,
    table_ownership: &BTreeMap<String, Option<String>>,
    findings: &mut Vec<ExtractionReadinessFinding>,
) {
    for access in &data.access_paths {
        let subject = format!(
            "service_data.access.{}.{}",
            access.accessor_module, access.table
        );
        let Some(Some(owner)) = table_ownership.get(&access.table) else {
            push_finding(
                findings,
                CompatibilityCategory::Blocked,
                ExtractionReadinessIssueCode::TableOwnershipUnresolved,
                subject,
                format!(
                    "Access path from Module `{}` reaches table `{}` without trustworthy ownership evidence.",
                    access.accessor_module, access.table
                ),
                evidence_references_or(
                    &access.evidence_references,
                    &format!("extraction-evidence:access/{}", access.table),
                ),
                vec!["Declare the table owner before evaluating this access path.".to_owned()],
            );
            continue;
        };
        if owner != &access.accessor_module
            && (owner == &module.name || access.accessor_module == module.name)
        {
            push_finding(
                findings,
                CompatibilityCategory::Blocked,
                ExtractionReadinessIssueCode::CrossModuleTableAccess,
                subject,
                format!(
                    "Module `{}` directly {}-accesses table `{}` owned by Module `{owner}`.",
                    access.accessor_module,
                    data_access_label(access.access),
                    access.table
                ),
                evidence_references_or(
                    &access.evidence_references,
                    &format!("extraction-evidence:access/{}", access.table),
                ),
                vec!["Replace direct cross-Module table access with an approved Service or Event Contract before extraction.".to_owned()],
            );
        }
    }
}

fn collect_transaction_findings(
    module: &ModuleManifest,
    data: &ExtractionServiceDataEvidence,
    findings: &mut Vec<ExtractionReadinessFinding>,
) {
    for transaction in &data.transactions {
        let subject = format!("service_data.transaction.{}", transaction.transaction);
        if transaction.participating_modules.is_empty() {
            push_finding(
                findings,
                CompatibilityCategory::Blocked,
                ExtractionReadinessIssueCode::TransactionBoundaryUnresolved,
                subject,
                format!(
                    "Transaction `{}` has no trustworthy participating Module ownership evidence.",
                    transaction.transaction
                ),
                evidence_references_or(
                    &transaction.evidence_references,
                    &format!("extraction-evidence:transaction/{}", transaction.transaction),
                ),
                vec!["Attribute every table touched by the transaction to its Module owner before extraction.".to_owned()],
            );
            continue;
        }
        if transaction
            .participating_modules
            .iter()
            .any(|participant| participant == &module.name)
            && transaction.participating_modules.len() > 1
        {
            push_finding(
                findings,
                CompatibilityCategory::Blocked,
                ExtractionReadinessIssueCode::TransactionSpansServiceBoundary,
                subject,
                format!(
                    "Transaction `{}` spans the proposed Service boundary across Modules: {}.",
                    transaction.transaction,
                    transaction.participating_modules.join(", ")
                ),
                evidence_references_or(
                    &transaction.evidence_references,
                    &format!("extraction-evidence:transaction/{}", transaction.transaction),
                ),
                vec!["Split the transaction into Service-local transactions coordinated through Outbox delivery, idempotent consumption, and explicit progress or compensation.".to_owned()],
            );
        }
    }
}

fn collect_volume_and_cursor_findings(
    module: &ModuleManifest,
    data: &ExtractionServiceDataEvidence,
    table_ownership: &BTreeMap<String, Option<String>>,
    findings: &mut Vec<ExtractionReadinessFinding>,
) {
    for (table, owner) in table_ownership {
        if owner.as_deref() != Some(module.name.as_str()) {
            continue;
        }
        let records = data
            .tables
            .iter()
            .filter(|candidate| candidate.table == *table)
            .collect::<Vec<_>>();
        let mut max_rows = None;
        let mut max_bytes = None;
        let mut volume_references = BTreeSet::new();
        let mut cursor_references = BTreeSet::new();
        let mut usable_cursors = Vec::new();
        for record in records {
            if let Some(volume) = &record.volume {
                max_rows = max_rows.max(volume.approximate_rows);
                max_bytes = max_bytes.max(volume.approximate_bytes);
                volume_references.extend(record.evidence_references.iter().cloned());
                volume_references.extend(volume.evidence_references.iter().cloned());
            }
            if let Some(cursor) = &record.cursor {
                cursor_references.extend(record.evidence_references.iter().cloned());
                cursor_references.extend(cursor.evidence_references.iter().cloned());
                if cursor.trustworthy
                    && !cursor.column.trim().is_empty()
                    && !cursor.high_water_mark.trim().is_empty()
                {
                    usable_cursors.push(cursor);
                }
            }
        }
        let large = max_rows.is_some_and(|rows| rows >= LARGE_DATA_VOLUME_ROWS)
            || max_bytes.is_some_and(|bytes| bytes >= LARGE_DATA_VOLUME_BYTES);
        if large {
            push_finding(
                findings,
                CompatibilityCategory::NeedsAttention,
                ExtractionReadinessIssueCode::DataVolumeLarge,
                format!("service_data.table.{table}.volume"),
                format!(
                    "Large data volume for `{table}` informs backfill and bounded-write-pause risk ({}; large means at least {LARGE_DATA_VOLUME_ROWS} rows or {LARGE_DATA_VOLUME_BYTES} bytes).",
                    data_volume_label(max_rows, max_bytes),
                ),
                evidence_references_or_set(
                    &volume_references,
                    &format!("extraction-evidence:table/{table}/volume"),
                ),
                vec!["Size resumable backfill batches, reconciliation, and the bounded write pause from this evidence; volume alone does not block readiness.".to_owned()],
            );
        }
        usable_cursors.sort();
        if let Some(cursor) = usable_cursors.first() {
            push_finding(
                findings,
                CompatibilityCategory::Safe,
                ExtractionReadinessIssueCode::ExtractionCursorUsable,
                format!("service_data.table.{table}.cursor"),
                format!(
                    "Table `{table}` has trustworthy extraction cursor `{}` at high-water mark `{}`.",
                    cursor.column, cursor.high_water_mark
                ),
                evidence_references_or_set(
                    &cursor_references,
                    &format!("extraction-evidence:table/{table}/cursor"),
                ),
                vec!["Pin this cursor and high-water mark in the Extraction Plan.".to_owned()],
            );
        } else {
            push_finding(
                findings,
                CompatibilityCategory::NeedsAttention,
                ExtractionReadinessIssueCode::ExtractionCursorMissing,
                format!("service_data.table.{table}.cursor"),
                format!(
                    "Table `{table}` has no trustworthy extraction cursor or high-water mark; a full copy must occur during the bounded write pause."
                ),
                evidence_references_or_set(
                    &cursor_references,
                    &format!("extraction-evidence:table/{table}/cursor"),
                ),
                vec!["Plan a full table copy and reconciliation while Module writes are paused, or supply a trustworthy cursor.".to_owned()],
            );
        }
    }
}

fn data_volume_label(rows: Option<u64>, bytes: Option<u64>) -> String {
    let mut values = Vec::new();
    if let Some(rows) = rows {
        values.push(format!("approximately {rows} rows"));
    }
    if let Some(bytes) = bytes {
        values.push(format!("approximately {bytes} bytes"));
    }
    values.join(", ")
}

fn evidence_references_or(references: &[String], fallback: &str) -> Vec<String> {
    if references.is_empty() {
        vec![fallback.to_owned()]
    } else {
        references.to_vec()
    }
}

fn evidence_references_or_set(references: &BTreeSet<String>, fallback: &str) -> Vec<String> {
    if references.is_empty() {
        vec![fallback.to_owned()]
    } else {
        references.iter().cloned().collect()
    }
}

fn normalize_strings(values: &mut Vec<String>) {
    values.retain(|value| !value.trim().is_empty());
    values.sort();
    values.dedup();
}

fn collect_target_owner(
    module: &ModuleManifest,
    graph: &SystemV2Graph,
    findings: &mut Vec<ExtractionReadinessFinding>,
) -> Option<String> {
    debug_assert_eq!(graph.semantic_kind, ContractSemanticKind::MixedSystem);
    let module_nodes = graph
        .nodes
        .iter()
        .filter(|node| node.kind == "module" && node.id == module.name)
        .collect::<Vec<_>>();
    let Some(module_node) = module_nodes.first() else {
        push_finding(
            findings,
            CompatibilityCategory::Blocked,
            ExtractionReadinessIssueCode::TargetModuleMissing,
            format!("system.module.{}", module.name),
            format!(
                "Target Module `{}` is not declared in the System graph.",
                module.name
            ),
            vec!["system:modules".to_owned(), "module:manifest".to_owned()],
            vec![
                "Declare the linked Module under the System Host before analyzing extraction."
                    .to_owned(),
            ],
        );
        return None;
    };
    let Some(owner) = module_node.owner.as_deref() else {
        push_finding(
            findings,
            CompatibilityCategory::Blocked,
            ExtractionReadinessIssueCode::TargetModuleMissing,
            format!("system.module.{}", module.name),
            "Target Module ownership is missing from the System graph.",
            vec![format!("system:module/{}", module.name)],
            vec!["Declare exactly one Host owner for the linked Module.".to_owned()],
        );
        return None;
    };
    let owner_kind = graph
        .nodes
        .iter()
        .find(|node| node.id == owner && node.owner.is_none())
        .map(|node| node.kind.as_str());
    if owner_kind != Some("host") {
        push_finding(
            findings,
            CompatibilityCategory::Blocked,
            ExtractionReadinessIssueCode::TargetModuleNotLinked,
            format!("system.module.{}", module.name),
            format!(
                "Target Module `{}` is owned by `{owner}` as {}, not by the linked Host.",
                module.name,
                owner_kind.unwrap_or("an unknown topology kind")
            ),
            vec![format!("system:module/{}", module.name)],
            vec!["Select a Host-owned linked Module; Provider and Autonomous Service semantics are unchanged by extraction analysis.".to_owned()],
        );
    }
    Some(owner.to_owned())
}

fn collect_manifest_findings(
    module: &ModuleManifest,
    findings: &mut Vec<ExtractionReadinessFinding>,
) {
    for lint in lint_module_manifest(ModuleSource::Linked, module) {
        let (classification, code) = match lint.severity {
            ModuleManifestLintSeverity::Ok => continue,
            ModuleManifestLintSeverity::Warning => (
                CompatibilityCategory::NeedsAttention,
                ExtractionReadinessIssueCode::ManifestNeedsAttention,
            ),
            ModuleManifestLintSeverity::Error => (
                CompatibilityCategory::Blocked,
                ExtractionReadinessIssueCode::ManifestInvalid,
            ),
        };
        push_finding(
            findings,
            classification,
            code,
            lint.subject.clone(),
            lint.message,
            vec![format!("module:manifest/{}", lint.subject)],
            vec![lint.suggestion],
        );
    }
}

fn collect_boundary_findings(
    module: &ModuleManifest,
    boundary: Option<&ExtractionBoundaryEvidence>,
    findings: &mut Vec<ExtractionReadinessFinding>,
) {
    let Some(boundary) = boundary else {
        push_finding(
            findings,
            CompatibilityCategory::Blocked,
            ExtractionReadinessIssueCode::BoundaryEvidenceMissing,
            "boundary.analysis",
            "No source-boundary analysis evidence was supplied.",
            vec!["analyzer:boundary".to_owned()],
            vec!["Run a supported source analyzer for the target Module and supply its complete evidence.".to_owned()],
        );
        return;
    };
    if !boundary.complete {
        push_finding(
            findings,
            CompatibilityCategory::Blocked,
            ExtractionReadinessIssueCode::BoundaryEvidenceIncomplete,
            "boundary.analysis",
            "Source-boundary analysis did not complete.",
            non_empty_references(&boundary.evidence_references, "analyzer:boundary"),
            vec!["Resolve analyzer errors and rerun the complete target-Module scan.".to_owned()],
        );
    }
    for reference in &boundary.references {
        if reference.evidence_reference.trim().is_empty() {
            push_finding(
                findings,
                CompatibilityCategory::Blocked,
                ExtractionReadinessIssueCode::BoundaryEvidenceAmbiguous,
                reference.symbol.clone(),
                "A source-boundary finding has no verifiable evidence reference.",
                vec!["analyzer:boundary".to_owned()],
                vec![
                    "Attach a repository-relative file and symbol reference to this finding."
                        .to_owned(),
                ],
            );
        }
        if reference.from_module != module.name && reference.to_module != module.name {
            push_finding(
                findings,
                CompatibilityCategory::Blocked,
                ExtractionReadinessIssueCode::BoundaryEvidenceTargetMismatch,
                reference.symbol.clone(),
                format!(
                    "Boundary evidence for `{}` does not involve target Module `{}`.",
                    reference.symbol, module.name
                ),
                non_empty_references(
                    std::slice::from_ref(&reference.evidence_reference),
                    "analyzer:boundary",
                ),
                vec![
                    "Regenerate boundary evidence for exactly the requested target Module."
                        .to_owned(),
                ],
            );
            continue;
        }
        let (code, message, action) = match reference.kind {
            ExtractionBoundaryReferenceKind::CrossModuleImport => (
                ExtractionReadinessIssueCode::CrossModuleImport,
                format!(
                    "Cross-Module import `{}` couples `{}` to `{}` in-process.",
                    reference.symbol, reference.from_module, reference.to_module
                ),
                "Remove the import and preserve the interaction through an approved Service or Event Contract.",
            ),
            ExtractionBoundaryReferenceKind::InProcessBoundaryCall => (
                ExtractionReadinessIssueCode::InProcessBoundaryCall,
                format!(
                    "In-process boundary call `{}` crosses from `{}` to `{}`.",
                    reference.symbol, reference.from_module, reference.to_module
                ),
                "Replace the boundary call with an approved Service or Event Contract before extraction.",
            ),
        };
        push_finding(
            findings,
            CompatibilityCategory::Blocked,
            code,
            reference.symbol.clone(),
            message,
            non_empty_references(
                std::slice::from_ref(&reference.evidence_reference),
                "analyzer:boundary",
            ),
            vec![action.to_owned()],
        );
    }
    if boundary.complete && boundary.references.is_empty() {
        push_finding(
            findings,
            CompatibilityCategory::Safe,
            ExtractionReadinessIssueCode::BoundaryClean,
            "boundary.analysis",
            "No cross-Module imports or in-process boundary calls were detected.",
            non_empty_references(&boundary.evidence_references, "analyzer:boundary"),
            vec!["No boundary remediation is required.".to_owned()],
        );
    }
}

fn collect_contract_findings(
    module: &ModuleManifest,
    evidence: Option<&[ExtractionContractEvidence]>,
    findings: &mut Vec<ExtractionReadinessFinding>,
) -> BTreeSet<String> {
    let required = required_contract_subjects(module);
    let Some(evidence) = evidence else {
        push_finding(
            findings,
            CompatibilityCategory::Blocked,
            ExtractionReadinessIssueCode::ContractEvidenceMissing,
            "contracts.analysis",
            "No Service or Event Contract evidence was supplied.",
            vec!["analyzer:contracts".to_owned()],
            vec!["Resolve required Module surfaces to versioned contract artifacts and rerun readiness analysis.".to_owned()],
        );
        for subject in required {
            push_missing_contract_finding(
                findings,
                &subject,
                "Contract evidence is missing for this declared Module surface.",
            );
        }
        return BTreeSet::new();
    };

    let mut present_contract_ids = BTreeSet::new();
    let by_subject = evidence.iter().fold(
        BTreeMap::<&str, Vec<&ExtractionContractEvidence>>::new(),
        |mut grouped, item| {
            grouped.entry(item.subject.as_str()).or_default().push(item);
            grouped
        },
    );
    let mut complete = true;
    for subject in required {
        let matches = by_subject
            .get(subject.subject.as_str())
            .map(Vec::as_slice)
            .unwrap_or_default();
        let [item] = matches else {
            complete = false;
            if matches.is_empty() {
                push_missing_contract_finding(
                    findings,
                    &subject,
                    "No contract evidence covers this declared Module surface.",
                );
            } else {
                push_finding(
                    findings,
                    CompatibilityCategory::Blocked,
                    ExtractionReadinessIssueCode::ContractEvidenceAmbiguous,
                    subject.subject.clone(),
                    "More than one contract-evidence entry covers this Module surface.",
                    vec![format!("module:{}", subject.subject)],
                    vec!["Provide exactly one authoritative contract-evidence entry for this surface.".to_owned()],
                );
            }
            continue;
        };
        if item.kind != subject.kind || item.direction != subject.direction {
            complete = false;
            push_finding(
                findings,
                CompatibilityCategory::Blocked,
                ExtractionReadinessIssueCode::ContractEvidenceAmbiguous,
                subject.subject.clone(),
                "Contract kind or direction does not match the declared Module surface.",
                non_empty_references(
                    &item.evidence_references,
                    &format!("module:{}", subject.subject),
                ),
                vec![
                    "Regenerate contract evidence with the required kind and direction.".to_owned(),
                ],
            );
            continue;
        }
        match item.status {
            ExtractionEvidenceStatus::Missing => {
                complete = false;
                push_missing_contract_finding(
                    findings,
                    &subject,
                    "The required versioned contract artifact is missing.",
                );
            }
            ExtractionEvidenceStatus::Ambiguous => {
                complete = false;
                push_finding(
                    findings,
                    CompatibilityCategory::Blocked,
                    ExtractionReadinessIssueCode::ContractEvidenceAmbiguous,
                    subject.subject.clone(),
                    "Required contract evidence is ambiguous.",
                    non_empty_references(&item.evidence_references, &format!("module:{}", subject.subject)),
                    vec!["Select one authoritative versioned contract artifact and rerun readiness analysis.".to_owned()],
                );
            }
            ExtractionEvidenceStatus::Present => {
                let contract_id = item
                    .contract_id
                    .as_deref()
                    .filter(|id| !id.trim().is_empty());
                if contract_id.is_none() || item.evidence_references.is_empty() {
                    complete = false;
                    push_finding(
                        findings,
                        CompatibilityCategory::Blocked,
                        ExtractionReadinessIssueCode::ContractEvidenceAmbiguous,
                        subject.subject.clone(),
                        "Present contract evidence must name a contract and its artifact reference.",
                        non_empty_references(
                            &item.evidence_references,
                            &format!("module:{}", subject.subject),
                        ),
                        vec![
                            "Attach the stable contract identity and authoritative artifact path."
                                .to_owned(),
                        ],
                    );
                    continue;
                }
                let contract_id = contract_id.expect("checked above");
                if subject
                    .expected_contract_id
                    .is_some_and(|expected| expected != contract_id)
                {
                    complete = false;
                    push_finding(
                        findings,
                        CompatibilityCategory::Blocked,
                        ExtractionReadinessIssueCode::ContractIdentityMismatch,
                        subject.subject.clone(),
                        format!(
                            "Contract `{contract_id}` does not match declared Event `{}`.",
                            subject.expected_contract_id.unwrap_or_default()
                        ),
                        item.evidence_references.clone(),
                        vec!["Use the Event Contract whose identity matches the manifest event declaration.".to_owned()],
                    );
                } else {
                    present_contract_ids.insert(contract_id.to_owned());
                }
            }
        }
    }
    if complete {
        let references = evidence
            .iter()
            .flat_map(|item| item.evidence_references.iter().cloned())
            .collect::<Vec<_>>();
        push_finding(
            findings,
            CompatibilityCategory::Safe,
            ExtractionReadinessIssueCode::ContractsComplete,
            "contracts.analysis",
            "Every declared HTTP and Event surface has authoritative contract evidence.",
            non_empty_references(&references, "analyzer:contracts"),
            vec!["Preserve these contract identities during extraction.".to_owned()],
        );
    }
    present_contract_ids
}

fn collect_consumer_findings(
    graph: Option<&SystemV2Graph>,
    contract_ids: &BTreeSet<String>,
    evidence: Option<&[ExtractionConsumerCompatibilityEvidence]>,
    findings: &mut Vec<ExtractionReadinessFinding>,
) {
    let Some(evidence) = evidence else {
        push_finding(
            findings,
            CompatibilityCategory::Blocked,
            ExtractionReadinessIssueCode::ActiveConsumerCompatibilityMissing,
            "consumers.analysis",
            "No active Consumer compatibility evidence was supplied.",
            vec!["analyzer:consumers".to_owned()],
            vec!["Resolve active Consumers from the System graph and evaluate each pinned Contract Version.".to_owned()],
        );
        return;
    };
    let mut grouped =
        BTreeMap::<(&str, &str), Vec<&ExtractionConsumerCompatibilityEvidence>>::new();
    for item in evidence {
        grouped
            .entry((item.consumer_id.as_str(), item.contract_id.as_str()))
            .or_default()
            .push(item);
    }
    let mut complete = true;
    for ((consumer_id, contract_id), items) in &grouped {
        let [item] = items.as_slice() else {
            complete = false;
            push_finding(
                findings,
                CompatibilityCategory::Blocked,
                ExtractionReadinessIssueCode::ActiveConsumerEvidenceAmbiguous,
                format!("consumer:{consumer_id}:{contract_id}"),
                "Active Consumer compatibility evidence is duplicated.",
                vec!["analyzer:consumers".to_owned()],
                vec![
                    "Provide one compatibility result per active Consumer and Contract Version."
                        .to_owned(),
                ],
            );
            continue;
        };
        if item.evidence_references.is_empty() {
            complete = false;
            push_finding(
                findings,
                CompatibilityCategory::Blocked,
                ExtractionReadinessIssueCode::ActiveConsumerEvidenceAmbiguous,
                format!("consumer:{consumer_id}:{contract_id}"),
                "Active Consumer compatibility result has no evidence reference.",
                vec!["analyzer:consumers".to_owned()],
                vec!["Attach the System relationship and compatibility result used for this Consumer.".to_owned()],
            );
            continue;
        }
        let (code, message) = match item.classification {
            CompatibilityCategory::Safe => continue,
            CompatibilityCategory::NeedsAttention => (
                ExtractionReadinessIssueCode::ActiveConsumerNeedsAttention,
                "Active Consumer compatibility needs review before Cutover.",
            ),
            CompatibilityCategory::Breaking => {
                complete = false;
                (
                    ExtractionReadinessIssueCode::ActiveConsumerBreaking,
                    "Active Consumer is incompatible with the required Contract Version.",
                )
            }
            CompatibilityCategory::Blocked => {
                complete = false;
                (
                    ExtractionReadinessIssueCode::ActiveConsumerBlocked,
                    "Active Consumer compatibility could not be verified.",
                )
            }
        };
        push_finding(
            findings,
            item.classification,
            code,
            format!("consumer:{consumer_id}:{contract_id}"),
            message,
            item.evidence_references.clone(),
            vec![non_empty_action(&item.next_action)],
        );
    }

    if let Some(graph) = graph {
        for relationship in relevant_consumer_relationships(graph, contract_ids) {
            let consumer_id = relationship.from.trim_start_matches("consumer:");
            let contract_id = relationship
                .contract_id
                .as_deref()
                .and_then(|contract| contract.split('@').next())
                .unwrap_or_default();
            if !grouped.contains_key(&(consumer_id, contract_id)) {
                complete = false;
                push_finding(
                    findings,
                    CompatibilityCategory::Blocked,
                    ExtractionReadinessIssueCode::ActiveConsumerCompatibilityMissing,
                    format!("consumer:{consumer_id}:{contract_id}"),
                    "System graph declares an active Consumer without compatibility evidence.",
                    vec![format!("system:consumer/{consumer_id}")],
                    vec!["Evaluate this Consumer against the pinned Contract Version and attach the result.".to_owned()],
                );
            }
        }
    }

    if complete {
        let references = evidence
            .iter()
            .flat_map(|item| item.evidence_references.iter().cloned())
            .collect::<Vec<_>>();
        push_finding(
            findings,
            CompatibilityCategory::Safe,
            ExtractionReadinessIssueCode::ConsumersCompatible,
            "consumers.analysis",
            "All supplied active Consumer compatibility results are safe or reviewable.",
            non_empty_references(&references, "analyzer:consumers"),
            vec!["Pin these Consumer results in the Extraction Plan.".to_owned()],
        );
    }
}

fn relevant_consumer_relationships<'a>(
    graph: &'a SystemV2Graph,
    contract_ids: &BTreeSet<String>,
) -> Vec<&'a SystemV2GraphRelationship> {
    graph
        .relationships
        .iter()
        .filter(|relationship| {
            relationship.kind == "consumes"
                && relationship
                    .contract_id
                    .as_deref()
                    .and_then(|contract| contract.split('@').next())
                    .is_some_and(|contract| contract_ids.contains(contract))
        })
        .collect()
}

fn required_contract_subjects(module: &ModuleManifest) -> Vec<RequiredContractSubject<'_>> {
    let mut subjects = module
        .http_routes
        .iter()
        .map(|route| RequiredContractSubject {
            subject: format!("http:{} {}", http_method_label(route.method), route.path),
            kind: ExtractionContractKind::Service,
            direction: ExtractionContractDirection::Provides,
            expected_contract_id: None,
        })
        .collect::<Vec<_>>();
    if let Some(events) = &module.events {
        subjects.extend(
            events
                .handlers
                .iter()
                .map(|handler| RequiredContractSubject {
                    subject: format!("event-handler:{}", handler.name),
                    kind: ExtractionContractKind::Event,
                    direction: ExtractionContractDirection::Consumes,
                    expected_contract_id: Some(handler.event_name.as_str()),
                }),
        );
    }
    subjects.sort_by(|left, right| left.subject.cmp(&right.subject));
    subjects
}

fn push_missing_contract_finding(
    findings: &mut Vec<ExtractionReadinessFinding>,
    subject: &RequiredContractSubject<'_>,
    message: &str,
) {
    let (code, label) = match subject.kind {
        ExtractionContractKind::Service => (
            ExtractionReadinessIssueCode::RequiredServiceContractMissing,
            "Service",
        ),
        ExtractionContractKind::Event => (
            ExtractionReadinessIssueCode::RequiredEventContractMissing,
            "Event",
        ),
    };
    push_finding(
        findings,
        CompatibilityCategory::Blocked,
        code,
        subject.subject.clone(),
        format!("{message} Required kind: {label} Contract."),
        vec![format!("module:{}", subject.subject)],
        vec![format!(
            "Publish and reference an authoritative versioned {label} Contract for this surface."
        )],
    );
}

fn surface_summary(module: &ModuleManifest) -> ExtractionReadinessSurfaceSummary {
    let mut summary = ExtractionReadinessSurfaceSummary {
        http_routes: module
            .http_routes
            .iter()
            .map(|route| format!("{} {}", http_method_label(route.method), route.path))
            .collect(),
        event_handlers: module
            .events
            .iter()
            .flat_map(|events| events.handlers.iter())
            .map(|handler| format!("{} <- {}", handler.name, handler.event_name))
            .collect(),
        runtime_functions: module
            .runtime
            .iter()
            .flat_map(|runtime| runtime.functions.iter())
            .map(|function| format!("{}@v{}", function.name, function.version))
            .collect(),
        schedules: module
            .runtime
            .iter()
            .flat_map(|runtime| runtime.schedules.iter())
            .map(|schedule| format!("{} -> {}", schedule.name, schedule.function_name))
            .collect(),
        workflows: module
            .runtime
            .iter()
            .flat_map(|runtime| runtime.workflows.iter())
            .map(|workflow| format!("{}@{}", workflow.name, workflow.version))
            .collect(),
        admin: module
            .admin
            .iter()
            .map(|admin| match admin {
                AdminSurface::Schema(_) => "schema".to_owned(),
                AdminSurface::DeclarativeCustom(_) => "declarative_custom".to_owned(),
                AdminSurface::EmbeddedCustom(_) => "embedded_custom".to_owned(),
                _ => "unknown".to_owned(),
            })
            .collect(),
        console: module
            .console
            .iter()
            .map(|surface| format!("surface:{}@{}", surface.name, surface.route))
            .chain(
                module
                    .console_slots
                    .iter()
                    .map(|slot| format!("slot:{}@v{}", slot.id, slot.version)),
            )
            .chain(module.console_contributions.iter().map(|contribution| {
                format!(
                    "contribution:{}@v{}",
                    contribution.target, contribution.target_version
                )
            }))
            .collect(),
        stories: module
            .story_display
            .iter()
            .map(|story| match &story.source {
                StoryDisplaySource::ExecutionName { name } => {
                    format!("execution:{name} -> {}", story.display_name)
                }
                StoryDisplaySource::HttpRequest { method, path } => {
                    format!("http:{method} {path} -> {}", story.display_name)
                }
            })
            .collect(),
    };
    for values in [
        &mut summary.http_routes,
        &mut summary.event_handlers,
        &mut summary.runtime_functions,
        &mut summary.schedules,
        &mut summary.workflows,
        &mut summary.admin,
        &mut summary.console,
        &mut summary.stories,
    ] {
        values.sort();
        values.dedup();
    }
    summary
}

fn collect_surface_findings(
    surfaces: &ExtractionReadinessSurfaceSummary,
    findings: &mut Vec<ExtractionReadinessFinding>,
) {
    let runtime = surfaces
        .runtime_functions
        .iter()
        .chain(&surfaces.schedules)
        .cloned()
        .collect::<Vec<_>>();
    push_surface_finding(
        findings,
        ExtractionReadinessIssueCode::RuntimeSurfacePresent,
        "runtime",
        &runtime,
        "Runtime functions and schedules must retain their identities and execution ownership.",
        "Carry these runtime declarations into the candidate Service and extraction drain plan.",
    );
    push_surface_finding(
        findings,
        ExtractionReadinessIssueCode::WorkflowSurfacePresent,
        "workflows",
        &surfaces.workflows,
        "Durable Workflow declarations require explicit ownership and in-flight-instance handling.",
        "Preserve pinned Workflow Definitions and plan how active instances drain before Cutover.",
    );
    push_surface_finding(
        findings,
        ExtractionReadinessIssueCode::AdminSurfacePresent,
        "admin",
        &surfaces.admin,
        "Admin declarations are part of the Module's operator-facing identity.",
        "Preserve admin declarations and authorization requirements in the candidate Service.",
    );
    push_surface_finding(
        findings,
        ExtractionReadinessIssueCode::ConsoleSurfacePresent,
        "console",
        &surfaces.console,
        "Runtime Console declarations are part of the Module's operator-facing identity.",
        "Preserve Console routes, packages, slots, and contributions during extraction.",
    );
    push_surface_finding(
        findings,
        ExtractionReadinessIssueCode::StorySurfacePresent,
        "stories",
        &surfaces.stories,
        "Runtime Story display declarations must remain stable across extraction.",
        "Preserve Story display names and titles and include them in behavior comparison.",
    );
}

fn push_surface_finding(
    findings: &mut Vec<ExtractionReadinessFinding>,
    code: ExtractionReadinessIssueCode,
    subject: &str,
    surfaces: &[String],
    message: &str,
    action: &str,
) {
    if surfaces.is_empty() {
        return;
    }
    push_finding(
        findings,
        CompatibilityCategory::NeedsAttention,
        code,
        format!("module.{subject}"),
        message,
        surfaces
            .iter()
            .map(|surface| format!("module:{subject}/{surface}"))
            .collect(),
        vec![action.to_owned()],
    );
}

fn push_finding(
    findings: &mut Vec<ExtractionReadinessFinding>,
    classification: CompatibilityCategory,
    code: ExtractionReadinessIssueCode,
    subject: impl Into<String>,
    message: impl Into<String>,
    evidence_references: Vec<String>,
    next_actions: Vec<String>,
) {
    findings.push(ExtractionReadinessFinding {
        classification,
        code,
        subject: subject.into(),
        message: message.into(),
        evidence_references,
        next_actions,
    });
}

fn normalize_findings(findings: &mut Vec<ExtractionReadinessFinding>) {
    for finding in findings.iter_mut() {
        finding
            .evidence_references
            .retain(|reference| !reference.trim().is_empty());
        finding.evidence_references.sort();
        finding.evidence_references.dedup();
        finding
            .next_actions
            .retain(|action| !action.trim().is_empty());
        finding.next_actions.sort();
        finding.next_actions.dedup();
    }
    findings.sort_by(|left, right| {
        classification_rank(right.classification)
            .cmp(&classification_rank(left.classification))
            .then_with(|| left.code.cmp(&right.code))
            .then_with(|| left.subject.cmp(&right.subject))
            .then_with(|| left.message.cmp(&right.message))
    });
    findings.dedup();
}

const fn classification_rank(classification: CompatibilityCategory) -> u8 {
    match classification {
        CompatibilityCategory::Safe => 0,
        CompatibilityCategory::NeedsAttention => 1,
        CompatibilityCategory::Breaking => 2,
        CompatibilityCategory::Blocked => 3,
    }
}

fn non_empty_references(references: &[String], fallback: &str) -> Vec<String> {
    let mut references = references
        .iter()
        .filter(|reference| !reference.trim().is_empty())
        .cloned()
        .collect::<Vec<_>>();
    if references.is_empty() {
        references.push(fallback.to_owned());
    }
    references
}

fn non_empty_action(action: &str) -> String {
    if action.trim().is_empty() {
        "Resolve this Consumer compatibility finding before extraction.".to_owned()
    } else {
        action.to_owned()
    }
}

const fn http_method_label(method: ModuleHttpMethod) -> &'static str {
    match method {
        ModuleHttpMethod::Get => "GET",
        ModuleHttpMethod::Post => "POST",
        ModuleHttpMethod::Put => "PUT",
        ModuleHttpMethod::Patch => "PATCH",
        ModuleHttpMethod::Delete => "DELETE",
        _ => "OTHER",
    }
}

#[must_use]
pub fn render_extraction_readiness_report(report: &ExtractionReadinessReport) -> String {
    let mut output = vec![
        format!("Extraction readiness: {}", report.target_module),
        format!(
            "Result: {} ({})",
            classification_label(report.classification),
            if report.ready { "ready" } else { "not ready" }
        ),
        format!(
            "System: {}",
            report.system_id.as_deref().unwrap_or("unknown")
        ),
        format!(
            "Linked owner: {}",
            report.target_owner.as_deref().unwrap_or("unknown")
        ),
        "Effects: read-only; writesRepositoryFiles=false; startsWorkloads=false; movesData=false; changesAuthority=false".to_owned(),
    ];
    output.extend(render_service_data(&report.service_data));
    output.push("Findings:".to_owned());
    for finding in &report.findings {
        output.push(format!(
            "- [{}] {} {}: {}",
            classification_label(finding.classification),
            issue_code_label(finding.code),
            finding.subject,
            finding.message
        ));
        for reference in &finding.evidence_references {
            output.push(format!("  evidence: {reference}"));
        }
        for action in &finding.next_actions {
            output.push(format!("  next: {action}"));
        }
    }
    output.push("Declared surfaces:".to_owned());
    for (label, values) in [
        ("http", &report.surfaces.http_routes),
        ("events", &report.surfaces.event_handlers),
        ("runtime", &report.surfaces.runtime_functions),
        ("schedules", &report.surfaces.schedules),
        ("workflows", &report.surfaces.workflows),
        ("admin", &report.surfaces.admin),
        ("console", &report.surfaces.console),
        ("stories", &report.surfaces.stories),
    ] {
        output.push(format!(
            "- {label}: {}",
            if values.is_empty() {
                "none".to_owned()
            } else {
                values.join(", ")
            }
        ));
    }
    output.push(String::new());
    output.join("\n")
}

fn render_service_data(data: &ExtractionServiceDataEvidence) -> Vec<String> {
    let mut output = vec![
        "Service data:".to_owned(),
        format!(
            "- evidence: {}",
            if data.complete {
                "complete"
            } else {
                "missing_or_incomplete"
            }
        ),
    ];
    if data.tables.is_empty() {
        output.push("- tables: none".to_owned());
    } else {
        for table in &data.tables {
            output.push(format!(
                "- table {}: owner={}; source={}; volume={}; cursor={}",
                table.table,
                table.owner_module.as_deref().unwrap_or("unresolved"),
                data_source_label(&table.source),
                table
                    .volume
                    .as_ref()
                    .map(|volume| {
                        data_volume_label(volume.approximate_rows, volume.approximate_bytes)
                    })
                    .filter(|label| !label.is_empty())
                    .unwrap_or_else(|| "unknown".to_owned()),
                table.cursor.as_ref().map_or_else(
                    || "none".to_owned(),
                    |cursor| {
                        format!(
                            "{}@{} ({})",
                            cursor.column,
                            cursor.high_water_mark,
                            if cursor.trustworthy {
                                "trustworthy"
                            } else {
                                "untrusted"
                            }
                        )
                    },
                )
            ));
        }
    }
    if data.migrations.is_empty() {
        output.push("- migrations: none".to_owned());
    } else {
        for migration in &data.migrations {
            output.push(format!(
                "- migration {}: owner={}; source={}",
                migration.migration,
                migration.owner_module.as_deref().unwrap_or("unresolved"),
                data_source_label(&migration.source)
            ));
        }
    }
    if data.access_paths.is_empty() {
        output.push("- access paths: none".to_owned());
    } else {
        for access in &data.access_paths {
            output.push(format!(
                "- access {} -> {}: {}; source={}",
                access.accessor_module,
                access.table,
                data_access_label(access.access),
                data_source_label(&access.source)
            ));
        }
    }
    if data.transactions.is_empty() {
        output.push("- transactions: none".to_owned());
    } else {
        for transaction in &data.transactions {
            output.push(format!(
                "- transaction {}: modules={}; source={}",
                transaction.transaction,
                if transaction.participating_modules.is_empty() {
                    "unresolved".to_owned()
                } else {
                    transaction.participating_modules.join(", ")
                },
                data_source_label(&transaction.source)
            ));
        }
    }
    output
}

fn data_source_label(source: &ExtractionDataEvidenceSource) -> String {
    match source {
        ExtractionDataEvidenceSource::StaticDeclaration => "static_declaration".to_owned(),
        ExtractionDataEvidenceSource::LiveStoreObservation {
            observation_id,
            store,
            read_only,
        } => format!(
            "live_store_observation:{observation_id}@{store} ({})",
            if *read_only {
                "read_only"
            } else {
                "not_read_only"
            }
        ),
    }
}

fn data_access_label(access: ExtractionDataAccessKind) -> &'static str {
    match access {
        ExtractionDataAccessKind::Read => "read",
        ExtractionDataAccessKind::Write => "write",
        ExtractionDataAccessKind::ReadWrite => "read_write",
    }
}

pub fn extraction_readiness_report_json(
    report: &ExtractionReadinessReport,
) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(report).map(|rendered| format!("{rendered}\n"))
}

#[must_use]
pub fn extraction_readiness_report_schema() -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(ExtractionReadinessReport))
        .expect("Extraction Readiness Report schema must serialize");
    let object = schema
        .as_object_mut()
        .expect("Extraction Readiness Report schema must be an object");
    object.insert(
        "$id".to_owned(),
        Value::String(EXTRACTION_READINESS_SCHEMA_ID.to_owned()),
    );
    object.insert(
        "title".to_owned(),
        Value::String("Lenso Extraction Readiness Report v1".to_owned()),
    );
    schema["properties"]["protocol"] = json!({
        "type": "string",
        "const": EXTRACTION_READINESS_REPORT_PROTOCOL
    });
    schema["properties"]["analyzerVersion"] = json!({
        "type": "string",
        "const": EXTRACTION_READINESS_ANALYZER_VERSION
    });
    schema["properties"]["issueCodes"]["uniqueItems"] = Value::Bool(true);
    for field in [
        "writesRepositoryFiles",
        "startsWorkloads",
        "movesData",
        "changesAuthority",
    ] {
        schema["$defs"]["ExtractionReadinessEffects"]["properties"][field] = json!({
            "type": "boolean",
            "const": false
        });
    }
    schema
}

fn classification_label(classification: CompatibilityCategory) -> &'static str {
    match classification {
        CompatibilityCategory::Safe => "safe",
        CompatibilityCategory::NeedsAttention => "needs_attention",
        CompatibilityCategory::Breaking => "breaking",
        CompatibilityCategory::Blocked => "blocked",
    }
}

fn issue_code_label(code: ExtractionReadinessIssueCode) -> &'static str {
    match code {
        ExtractionReadinessIssueCode::ActiveConsumerBlocked => "active_consumer_blocked",
        ExtractionReadinessIssueCode::ActiveConsumerBreaking => "active_consumer_breaking",
        ExtractionReadinessIssueCode::ActiveConsumerCompatibilityMissing => {
            "active_consumer_compatibility_missing"
        }
        ExtractionReadinessIssueCode::ActiveConsumerEvidenceAmbiguous => {
            "active_consumer_evidence_ambiguous"
        }
        ExtractionReadinessIssueCode::ActiveConsumerNeedsAttention => {
            "active_consumer_needs_attention"
        }
        ExtractionReadinessIssueCode::AdminSurfacePresent => "admin_surface_present",
        ExtractionReadinessIssueCode::BoundaryClean => "boundary_clean",
        ExtractionReadinessIssueCode::BoundaryEvidenceAmbiguous => "boundary_evidence_ambiguous",
        ExtractionReadinessIssueCode::BoundaryEvidenceIncomplete => "boundary_evidence_incomplete",
        ExtractionReadinessIssueCode::BoundaryEvidenceMissing => "boundary_evidence_missing",
        ExtractionReadinessIssueCode::BoundaryEvidenceTargetMismatch => {
            "boundary_evidence_target_mismatch"
        }
        ExtractionReadinessIssueCode::ConsoleSurfacePresent => "console_surface_present",
        ExtractionReadinessIssueCode::ConsumersCompatible => "consumers_compatible",
        ExtractionReadinessIssueCode::ContractEvidenceAmbiguous => "contract_evidence_ambiguous",
        ExtractionReadinessIssueCode::ContractEvidenceMissing => "contract_evidence_missing",
        ExtractionReadinessIssueCode::ContractIdentityMismatch => "contract_identity_mismatch",
        ExtractionReadinessIssueCode::ContractsComplete => "contracts_complete",
        ExtractionReadinessIssueCode::CrossModuleTableAccess => "cross_module_table_access",
        ExtractionReadinessIssueCode::CrossModuleImport => "cross_module_import",
        ExtractionReadinessIssueCode::DataVolumeLarge => "data_volume_large",
        ExtractionReadinessIssueCode::ExtractionCursorMissing => "extraction_cursor_missing",
        ExtractionReadinessIssueCode::ExtractionCursorUsable => "extraction_cursor_usable",
        ExtractionReadinessIssueCode::InProcessBoundaryCall => "in_process_boundary_call",
        ExtractionReadinessIssueCode::LiveStoreObservationNotReadOnly => {
            "live_store_observation_not_read_only"
        }
        ExtractionReadinessIssueCode::LiveStoreObservationPresent => {
            "live_store_observation_present"
        }
        ExtractionReadinessIssueCode::ManifestInvalid => "manifest_invalid",
        ExtractionReadinessIssueCode::ManifestNeedsAttention => "manifest_needs_attention",
        ExtractionReadinessIssueCode::MigrationOwnershipUnresolved => {
            "migration_ownership_unresolved"
        }
        ExtractionReadinessIssueCode::RequiredEventContractMissing => {
            "required_event_contract_missing"
        }
        ExtractionReadinessIssueCode::RequiredServiceContractMissing => {
            "required_service_contract_missing"
        }
        ExtractionReadinessIssueCode::RuntimeSurfacePresent => "runtime_surface_present",
        ExtractionReadinessIssueCode::ServiceDataEvidenceIncomplete => {
            "service_data_evidence_incomplete"
        }
        ExtractionReadinessIssueCode::ServiceDataEvidenceMissing => "service_data_evidence_missing",
        ExtractionReadinessIssueCode::ServiceDataReady => "service_data_ready",
        ExtractionReadinessIssueCode::StorySurfacePresent => "story_surface_present",
        ExtractionReadinessIssueCode::SystemEvidenceInvalid => "system_evidence_invalid",
        ExtractionReadinessIssueCode::TableOwnershipUnresolved => "table_ownership_unresolved",
        ExtractionReadinessIssueCode::TargetModuleMissing => "target_module_missing",
        ExtractionReadinessIssueCode::TargetModuleNotLinked => "target_module_not_linked",
        ExtractionReadinessIssueCode::TransactionBoundaryUnresolved => {
            "transaction_boundary_unresolved"
        }
        ExtractionReadinessIssueCode::TransactionSpansServiceBoundary => {
            "transaction_spans_service_boundary"
        }
        ExtractionReadinessIssueCode::WorkflowSurfacePresent => "workflow_surface_present",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lenso_contracts::{
        AdminSchema, ConsoleArea, ConsolePackage, ConsoleSurface, EntitySchema,
        EventHandlerDeclaration, EventSurface, FieldSchema, FieldType, ModuleHttpRoute,
        RuntimeFunctionDeclaration, RuntimeSurface, ScheduledFunctionDeclaration,
        StoryDisplayDescriptor, WorkflowDataContract, WorkflowDefinition, WorkflowStepDeclaration,
    };
    fn manifest() -> ModuleManifest {
        ModuleManifest::builder("support-ticket")
            .capabilities(vec!["support.tickets.read".to_owned()])
            .http_routes(vec![ModuleHttpRoute {
                method: ModuleHttpMethod::Get,
                path: "/tickets/{id}".to_owned(),
                capability: Some("support.tickets.read".to_owned()),
                display_name: Some("Get ticket".to_owned()),
                story_title: Some("Support ticket opened".to_owned()),
                operation: None,
            }])
            .events(EventSurface {
                handlers: vec![
                    EventHandlerDeclaration {
                        name: "apply_sla_update".to_owned(),
                        event_name: "support.sla-updated.v1".to_owned(),
                        operation: None,
                    },
                    EventHandlerDeclaration {
                        name: "record_audit".to_owned(),
                        event_name: "support.audit-recorded.v1".to_owned(),
                        operation: None,
                    },
                ],
            })
            .runtime(RuntimeSurface {
                functions: vec![RuntimeFunctionDeclaration {
                    name: "support-ticket.reindex.v1".to_owned(),
                    version: 1,
                    queue: "support-ticket".to_owned(),
                    input_schema: Some("support-ticket.reindex.v1".to_owned()),
                    retry_policy: None,
                    operation: None,
                }],
                schedules: vec![ScheduledFunctionDeclaration {
                    name: "support-ticket-reindex".to_owned(),
                    function_name: "support-ticket.reindex.v1".to_owned(),
                    cron: "0 * * * *".to_owned(),
                    input: json!({}),
                }],
                workflows: vec![WorkflowDefinition::new(
                    "support-ticket",
                    "ticket_triage",
                    "v1",
                    WorkflowDataContract::new("support.ticket-triage-input", "v1"),
                    WorkflowDataContract::new("support.ticket-triage-result", "v1"),
                    vec![WorkflowStepDeclaration::new("classify")],
                )],
            })
            .admin(AdminSchema {
                entities: vec![EntitySchema {
                    name: "tickets".to_owned(),
                    label: "Tickets".to_owned(),
                    fields: vec![FieldSchema {
                        name: "id".to_owned(),
                        label: "ID".to_owned(),
                        field_type: FieldType::String,
                        nullable: false,
                    }],
                    read_capability: "support.tickets.read".to_owned(),
                }],
            })
            .console(vec![ConsoleSurface {
                name: "support-tickets".to_owned(),
                label: "Support tickets".to_owned(),
                area: ConsoleArea::Data,
                route: "/support/tickets".to_owned(),
                package: ConsolePackage {
                    name: "@lenso/support-ticket-console".to_owned(),
                    export: "supportTicketConsoleModule".to_owned(),
                },
                icon: None,
                required_capabilities: vec!["support.tickets.read".to_owned()],
                navigation: None,
            }])
            .story_display(vec![StoryDisplayDescriptor {
                source: StoryDisplaySource::ExecutionName {
                    name: "support-ticket.reindex.v1".to_owned(),
                },
                display_name: "Reindex support tickets".to_owned(),
                story_title: Some("Support ticket maintenance".to_owned()),
            }])
            .build()
    }

    fn system() -> Value {
        json!({
            "protocol": "lenso.system.v2",
            "systemId": "support-system",
            "host": { "hostId": "support-host", "modules": ["support-ticket"] },
            "providers": [{
                "providerId": "notification-provider",
                "modules": ["notification-gateway"]
            }],
            "autonomousServices": [{
                "serviceId": "support-sla-service",
                "modules": ["support-sla"],
                "workloads": [{ "workloadId": "support-sla-api", "role": "api" }]
            }],
            "contracts": [{
                "contractId": "support.sla-updated.v1",
                "version": "v1",
                "producerKind": "autonomous_service",
                "producerId": "support-sla-service",
                "artifact": {
                    "format": "json_schema",
                    "path": "contracts/events/support.sla-updated.v1.schema.json"
                },
                "tenancyMode": "required"
            }],
            "consumers": [{
                "consumerId": "support-ticket-sla-updates",
                "ownerKind": "host",
                "ownerId": "support-host",
                "contractId": "support.sla-updated.v1",
                "tenancyMode": "required"
            }]
        })
    }

    fn corrected_service_data() -> ExtractionServiceDataEvidence {
        ExtractionServiceDataEvidence {
            complete: true,
            evidence_references: vec!["analyzer:postgres/support-ticket".to_owned()],
            tables: vec![
                ExtractionDataTableEvidence {
                    table: "support.tickets".to_owned(),
                    owner_module: Some("support-ticket".to_owned()),
                    source: ExtractionDataEvidenceSource::StaticDeclaration,
                    volume: None,
                    cursor: None,
                    evidence_references: vec![
                        "modules/support-ticket/migrations/0001_tickets.sql".to_owned(),
                    ],
                },
                ExtractionDataTableEvidence {
                    table: "support.tickets".to_owned(),
                    owner_module: Some("support-ticket".to_owned()),
                    source: ExtractionDataEvidenceSource::LiveStoreObservation {
                        observation_id: "support-store-2026-07-19".to_owned(),
                        store: "host-postgres".to_owned(),
                        read_only: true,
                    },
                    volume: Some(ExtractionDataVolumeEvidence {
                        approximate_rows: Some(25_000_000),
                        approximate_bytes: Some(17_179_869_184),
                        evidence_references: vec!["postgres:pg_class/support.tickets".to_owned()],
                    }),
                    cursor: Some(ExtractionCursorEvidence {
                        column: "id".to_owned(),
                        high_water_mark: "25000000".to_owned(),
                        trustworthy: true,
                        evidence_references: vec!["postgres:max(support.tickets.id)".to_owned()],
                    }),
                    evidence_references: vec![
                        "postgres:observation/support-store-2026-07-19".to_owned(),
                    ],
                },
            ],
            migrations: vec![ExtractionMigrationEvidence {
                migration: "0001_create_support_tickets".to_owned(),
                owner_module: Some("support-ticket".to_owned()),
                source: ExtractionDataEvidenceSource::StaticDeclaration,
                evidence_references: vec![
                    "modules/support-ticket/migrations/0001_tickets.sql".to_owned(),
                ],
            }],
            access_paths: vec![ExtractionDataAccessEvidence {
                accessor_module: "support-ticket".to_owned(),
                table: "support.tickets".to_owned(),
                access: ExtractionDataAccessKind::ReadWrite,
                source: ExtractionDataEvidenceSource::StaticDeclaration,
                evidence_references: vec!["modules/support-ticket/src/store.rs:14".to_owned()],
            }],
            transactions: vec![ExtractionTransactionEvidence {
                transaction: "support-ticket-update".to_owned(),
                participating_modules: vec!["support-ticket".to_owned()],
                source: ExtractionDataEvidenceSource::StaticDeclaration,
                evidence_references: vec!["modules/support-ticket/src/store.rs:41".to_owned()],
            }],
        }
    }

    fn corrected_evidence() -> ExtractionReadinessEvidence {
        ExtractionReadinessEvidence {
            boundary: Some(ExtractionBoundaryEvidence {
                complete: true,
                evidence_references: vec!["analyzer:rust/support-ticket".to_owned()],
                references: Vec::new(),
            }),
            contracts: Some(vec![
                ExtractionContractEvidence {
                    subject: "http:GET /tickets/{id}".to_owned(),
                    kind: ExtractionContractKind::Service,
                    direction: ExtractionContractDirection::Provides,
                    status: ExtractionEvidenceStatus::Present,
                    contract_id: Some("support-ticket-http.v1".to_owned()),
                    evidence_references: vec![
                        "contracts/openapi/support-ticket.v1.yaml".to_owned(),
                    ],
                },
                ExtractionContractEvidence {
                    subject: "event-handler:apply_sla_update".to_owned(),
                    kind: ExtractionContractKind::Event,
                    direction: ExtractionContractDirection::Consumes,
                    status: ExtractionEvidenceStatus::Present,
                    contract_id: Some("support.sla-updated.v1".to_owned()),
                    evidence_references: vec![
                        "contracts/events/support.sla-updated.v1.schema.json".to_owned(),
                    ],
                },
                ExtractionContractEvidence {
                    subject: "event-handler:record_audit".to_owned(),
                    kind: ExtractionContractKind::Event,
                    direction: ExtractionContractDirection::Consumes,
                    status: ExtractionEvidenceStatus::Present,
                    contract_id: Some("support.audit-recorded.v1".to_owned()),
                    evidence_references: vec![
                        "contracts/events/support.audit-recorded.v1.schema.json".to_owned(),
                    ],
                },
            ]),
            active_consumers: Some(vec![ExtractionConsumerCompatibilityEvidence {
                consumer_id: "support-ticket-sla-updates".to_owned(),
                contract_id: "support.sla-updated.v1".to_owned(),
                classification: CompatibilityCategory::Safe,
                evidence_references: vec!["system:consumer/support-ticket-sla-updates".to_owned()],
                next_action: "No action needed.".to_owned(),
            }]),
            service_data: Some(corrected_service_data()),
        }
    }

    #[test]
    fn blocked_and_corrected_reports_are_deterministic_and_fail_closed() {
        let module = manifest();
        let mut blocked = corrected_evidence();
        blocked.boundary = Some(ExtractionBoundaryEvidence {
            complete: true,
            evidence_references: vec!["analyzer:rust/support-ticket".to_owned()],
            references: vec![
                ExtractionBoundaryReference {
                    kind: ExtractionBoundaryReferenceKind::CrossModuleImport,
                    from_module: "support-ticket".to_owned(),
                    to_module: "support-sla".to_owned(),
                    symbol: "support_sla::internal::SlaPolicy".to_owned(),
                    evidence_reference: "modules/support-ticket/src/lib.rs:12".to_owned(),
                },
                ExtractionBoundaryReference {
                    kind: ExtractionBoundaryReferenceKind::InProcessBoundaryCall,
                    from_module: "support-ticket".to_owned(),
                    to_module: "support-sla".to_owned(),
                    symbol: "support_sla::public::evaluate".to_owned(),
                    evidence_reference: "modules/support-ticket/src/service.rs:41".to_owned(),
                },
            ],
        });
        blocked.contracts.as_mut().expect("contracts")[0].status =
            ExtractionEvidenceStatus::Missing;
        blocked.contracts.as_mut().expect("contracts")[0].contract_id = None;
        blocked.contracts.as_mut().expect("contracts")[2].status =
            ExtractionEvidenceStatus::Missing;
        blocked.contracts.as_mut().expect("contracts")[2].contract_id = None;
        blocked.active_consumers.as_mut().expect("consumers")[0].classification =
            CompatibilityCategory::Breaking;
        blocked.active_consumers.as_mut().expect("consumers")[0].next_action =
            "Migrate the Consumer to support.sla-updated.v1.".to_owned();
        let service_data = blocked.service_data.as_mut().expect("service data");
        service_data.tables.extend([
            ExtractionDataTableEvidence {
                table: "support.sla_policies".to_owned(),
                owner_module: Some("support-sla".to_owned()),
                source: ExtractionDataEvidenceSource::StaticDeclaration,
                volume: None,
                cursor: None,
                evidence_references: vec!["modules/support-sla/migrations/0001.sql".to_owned()],
            },
            ExtractionDataTableEvidence {
                table: "support.audit_events".to_owned(),
                owner_module: None,
                source: ExtractionDataEvidenceSource::StaticDeclaration,
                volume: None,
                cursor: None,
                evidence_references: vec!["migrations/0009_support_audit.sql".to_owned()],
            },
        ]);
        service_data.migrations.push(ExtractionMigrationEvidence {
            migration: "0009_support_audit".to_owned(),
            owner_module: None,
            source: ExtractionDataEvidenceSource::StaticDeclaration,
            evidence_references: vec!["migrations/0009_support_audit.sql".to_owned()],
        });
        service_data
            .access_paths
            .push(ExtractionDataAccessEvidence {
                accessor_module: "support-ticket".to_owned(),
                table: "support.sla_policies".to_owned(),
                access: ExtractionDataAccessKind::Read,
                source: ExtractionDataEvidenceSource::StaticDeclaration,
                evidence_references: vec!["modules/support-ticket/src/sla.rs:28".to_owned()],
            });
        service_data
            .transactions
            .push(ExtractionTransactionEvidence {
                transaction: "ticket-and-sla-update".to_owned(),
                participating_modules: vec!["support-sla".to_owned(), "support-ticket".to_owned()],
                source: ExtractionDataEvidenceSource::StaticDeclaration,
                evidence_references: vec!["modules/support-ticket/src/sla.rs:52".to_owned()],
            });

        let first = evaluate_extraction_readiness(&module, &system(), &blocked);
        let second = evaluate_extraction_readiness(&module, &system(), &blocked);
        assert_eq!(first, second);
        let mut reordered = blocked.clone();
        let data = reordered.service_data.as_mut().expect("service data");
        data.tables.reverse();
        data.migrations.reverse();
        data.access_paths.reverse();
        data.transactions.reverse();
        data.evidence_references.reverse();
        assert_eq!(
            first,
            evaluate_extraction_readiness(&module, &system(), &reordered)
        );
        assert_eq!(first.classification, CompatibilityCategory::Blocked);
        assert!(!first.ready);
        for code in [
            ExtractionReadinessIssueCode::CrossModuleImport,
            ExtractionReadinessIssueCode::InProcessBoundaryCall,
            ExtractionReadinessIssueCode::RequiredServiceContractMissing,
            ExtractionReadinessIssueCode::RequiredEventContractMissing,
            ExtractionReadinessIssueCode::ActiveConsumerBreaking,
            ExtractionReadinessIssueCode::CrossModuleTableAccess,
            ExtractionReadinessIssueCode::TableOwnershipUnresolved,
            ExtractionReadinessIssueCode::MigrationOwnershipUnresolved,
            ExtractionReadinessIssueCode::TransactionSpansServiceBoundary,
        ] {
            assert!(first.issue_codes.contains(&code), "missing {code:?}");
        }
        assert_eq!(first.effects, ExtractionReadinessEffects::default());

        let corrected = evaluate_extraction_readiness(&module, &system(), &corrected_evidence());
        assert_eq!(
            corrected.classification,
            CompatibilityCategory::NeedsAttention,
            "{:#?}",
            corrected.findings
        );
        assert!(corrected.ready);
        assert!(!corrected.surfaces.runtime_functions.is_empty());
        assert!(!corrected.surfaces.workflows.is_empty());
        assert!(!corrected.surfaces.admin.is_empty());
        assert!(!corrected.surfaces.console.is_empty());
        assert!(!corrected.surfaces.stories.is_empty());
        assert_eq!(corrected.service_data.tables.len(), 2);
        assert!(
            corrected
                .issue_codes
                .contains(&ExtractionReadinessIssueCode::DataVolumeLarge)
        );
        assert!(corrected.findings.iter().any(|finding| {
            finding.code == ExtractionReadinessIssueCode::LiveStoreObservationPresent
                && finding.message.contains("Read-only")
        }));
        assert!(
            !corrected
                .issue_codes
                .contains(&ExtractionReadinessIssueCode::CrossModuleImport)
        );
    }

    #[test]
    fn missing_or_ambiguous_analysis_evidence_blocks_readiness() {
        let module = manifest();
        let report = evaluate_extraction_readiness(
            &module,
            &system(),
            &ExtractionReadinessEvidence::default(),
        );
        assert_eq!(report.classification, CompatibilityCategory::Blocked);
        for code in [
            ExtractionReadinessIssueCode::BoundaryEvidenceMissing,
            ExtractionReadinessIssueCode::ContractEvidenceMissing,
            ExtractionReadinessIssueCode::ActiveConsumerCompatibilityMissing,
            ExtractionReadinessIssueCode::ServiceDataEvidenceMissing,
        ] {
            assert!(report.issue_codes.contains(&code));
        }
    }

    #[test]
    fn missing_cursor_requires_bounded_pause_full_copy_without_blocking_readiness() {
        let mut evidence = corrected_evidence();
        for table in &mut evidence.service_data.as_mut().expect("service data").tables {
            table.cursor = None;
        }
        let report = evaluate_extraction_readiness(&manifest(), &system(), &evidence);
        assert!(report.ready);
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.code == ExtractionReadinessIssueCode::ExtractionCursorMissing)
            .expect("missing cursor should be reported");
        assert!(finding.message.contains("full copy"));
        assert!(finding.message.contains("bounded write pause"));
    }

    #[test]
    fn live_store_observations_must_be_read_only() {
        let mut evidence = corrected_evidence();
        for table in &mut evidence.service_data.as_mut().expect("service data").tables {
            if let ExtractionDataEvidenceSource::LiveStoreObservation { read_only, .. } =
                &mut table.source
            {
                *read_only = false;
            }
        }
        let report = evaluate_extraction_readiness(&manifest(), &system(), &evidence);
        assert!(!report.ready);
        assert!(
            report
                .issue_codes
                .contains(&ExtractionReadinessIssueCode::LiveStoreObservationNotReadOnly)
        );
        assert_eq!(report.effects, ExtractionReadinessEffects::default());
    }

    #[test]
    fn report_schema_accepts_public_json_and_v1_reader_ignores_future_fields() {
        let report = evaluate_extraction_readiness(&manifest(), &system(), &corrected_evidence());
        let value = serde_json::to_value(&report).expect("report should serialize");
        let validator = jsonschema::validator_for(&extraction_readiness_report_schema())
            .expect("report schema should compile");
        assert!(validator.is_valid(&value));

        let mut future = value;
        future["futureField"] = json!(true);
        let decoded: ExtractionReadinessReport =
            serde_json::from_value(future).expect("v1 reader should ignore future fields");
        assert_eq!(decoded.protocol, EXTRACTION_READINESS_REPORT_PROTOCOL);

        let mut older = serde_json::to_value(&report).expect("report should serialize");
        older
            .as_object_mut()
            .expect("report should be an object")
            .remove("serviceData");
        older
            .as_object_mut()
            .expect("report should be an object")
            .remove("contractEvidence");
        older
            .as_object_mut()
            .expect("report should be an object")
            .remove("activeConsumers");
        let decoded: ExtractionReadinessReport =
            serde_json::from_value(older).expect("v1 reader should default added data summary");
        assert_eq!(
            decoded.service_data,
            ExtractionServiceDataEvidence::default()
        );
        assert!(decoded.contract_evidence.is_empty());
        assert!(decoded.active_consumers.is_empty());
    }

    #[test]
    fn human_and_json_renderers_project_the_same_report() {
        let report = evaluate_extraction_readiness(&manifest(), &system(), &corrected_evidence());
        let human = render_extraction_readiness_report(&report);
        assert!(human.contains("Extraction readiness: support-ticket"));
        assert!(human.contains("Result: needs_attention (ready)"));
        assert!(human.contains("writesRepositoryFiles=false"));
        assert!(human.contains("live_store_observation"));
        assert!(human.contains("support.tickets"));
        let json = extraction_readiness_report_json(&report).expect("report should render");
        let decoded: ExtractionReadinessReport =
            serde_json::from_str(&json).expect("JSON output should be readable");
        assert_eq!(decoded, report);
    }

    #[test]
    fn provider_system_is_rejected_without_reinterpreting_provider_semantics() {
        let provider: Value = serde_json::from_str(crate::LEGACY_SYSTEM_V1_FIXTURE_JSON)
            .expect("Provider System fixture should parse");
        let check = crate::check_contract_artifact_value(&provider)
            .expect("Provider System semantics should remain valid");
        assert_eq!(check.semantic_kind, ContractSemanticKind::ProviderSystem);

        let report = evaluate_extraction_readiness(&manifest(), &provider, &corrected_evidence());
        assert_eq!(report.classification, CompatibilityCategory::Blocked);
        assert!(
            report
                .issue_codes
                .contains(&ExtractionReadinessIssueCode::SystemEvidenceInvalid)
        );
    }
}
