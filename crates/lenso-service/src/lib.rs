use axum::{Json, Router, routing::get};
use prost::Message;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use utoipa::ToSchema;

mod call_policy;
mod delegated_context;
mod delivery_console_projection;
mod direct_grpc;
mod direct_http;
mod endpoint_resolution;
mod event_envelope;
mod extraction_authority_commit;
mod extraction_backfill;
mod extraction_console_projection;
mod extraction_plan;
mod extraction_provisional_cutover;
mod extraction_quiescence;
mod extraction_readiness;
mod extraction_reconciliation;
mod extraction_run;
mod extraction_scaffold;
mod extraction_verification;
mod ga_support;
mod production_delivery;
mod reliability_report;
mod story_segment;
mod workload_identity;

pub use call_policy::{
    CallPolicyCircuitBreaker, CallPolicyClock, CallPolicyConcurrency, CallPolicyDeclaration,
    CallPolicyEvent, CallPolicyEvidence, CallPolicyFailure, CallPolicyFallback, CallPolicyOverload,
    CallPolicyPermit, CallPolicyRuntime, CallPolicyTerminalOutcome, CallPolicyValidationIssue,
    ManualCallPolicyClock, SystemCallPolicyClock,
};
pub use delegated_context::{
    AuthenticatedServiceContext, DelegatedActorCredentialRequest, DelegatedContextError,
    DelegatedContextErrorCode, DelegatedContextProvider, IdentityDecisionEvidence,
    IdentityDecisionRecorder, JsonlIdentityDecisionRecorder, MemoryIdentityDecisionRecorder,
    ServiceContext, ServiceContextAdmission, ServiceContextPolicy,
    SystemSandboxDelegatedContextProvider, TenantCredentialRequest,
};
pub use delivery_console_projection::{
    DELIVERY_ARTIFACT_BATCH_PROTOCOL, DELIVERY_CONSOLE_PROJECTION_PROTOCOL,
    DeliveryConsoleAdapterDrift, DeliveryConsoleArtifacts, DeliveryConsoleConfiguration,
    DeliveryConsoleDeployment, DeliveryConsoleEdge, DeliveryConsoleIssue, DeliveryConsolePolicy,
    DeliveryConsoleProjection, DeliveryConsoleRelease, DeliveryConsoleSecretReference,
    DeliveryConsoleState, DeliveryConsoleSupplyChainWorkload, DeliveryConsoleTimelineEntry,
    delivery_artifact_batch_subject, load_delivery_console_projection, project_delivery_console,
    record_delivery_artifact, record_delivery_artifacts,
};

pub mod support_grpc_v1 {
    tonic::include_proto!("lenso.services.support.v1");
}

pub use direct_grpc::{
    DirectGrpcAdmission, DirectGrpcAdmissionError, DirectGrpcBindings, DirectGrpcCallError,
    DirectGrpcClient, DirectGrpcEvidence, DirectGrpcOperation, DirectGrpcResponse,
    DirectGrpcServerPolicy, GrpcIdempotency, generate_direct_grpc_bindings,
    parse_protobuf_call_policies,
};
pub use direct_http::{
    DirectHttpBindings, DirectHttpCall, DirectHttpCallError, DirectHttpClient, DirectHttpEvidence,
    DirectHttpOperation, DirectHttpRequest, DirectHttpResponse, DirectHttpServerBinding,
    HttpIdempotency, RetryDecision, generate_direct_http_bindings,
};

pub use endpoint_resolution::{
    Endpoint, EndpointResolutionError, EndpointResolutionErrorCode, EndpointResolver,
    EndpointState, LastValidEndpointResolver, LocalProcessEndpointResolver, ServiceReference,
    StaticEndpointResolver,
};
pub use event_envelope::{
    CloudEvent, EVENT_CONTRACT_ARTIFACT_PROTOCOL, EVENT_ENVELOPE_PROTOCOL, EventContent,
    EventContext, EventContractGenerationError, EventEnvelope, EventEnvelopeIssue,
    EventEnvelopeIssueCode, GeneratedEventContract,
    evaluate_generated_event_contract_compatibility, event_envelope_from_cloudevent,
    generate_event_contract, validate_event_envelope, validate_event_envelope_value,
};
pub use extraction_authority_commit::{
    EXTRACTION_AUTHORITY_COMMIT_PROTOCOL, EXTRACTION_CANDIDATE_HEALTH_PROTOCOL, ExtractionApproval,
    ExtractionApprovalVerifier, ExtractionAuthorityCommitError, ExtractionAuthorityCommitErrorCode,
    ExtractionAuthorityCommitInputs, ExtractionAuthorityCommitReceipt,
    ExtractionAuthorityCommitResult, ExtractionAuthorityCommitRevalidation,
    ExtractionAuthorityCommitStatus, ExtractionCandidateHealthEvidence,
    ExtractionFastRollbackError, ExtractionFastRollbackIssueCode,
    ExtractionReverseMigrationEvidence, ExtractionTopologyState, commit_extraction_authority,
    commit_extraction_authority_postgres, extraction_candidate_health_integrity_is_valid,
    initialize_extraction_topology_state, record_autonomous_mutation,
    request_fast_extraction_rollback,
};
pub use extraction_backfill::{
    EXTRACTION_BACKFILL_PROTOCOL, ExtractionBackfillBatchReceipt, ExtractionBackfillBoundary,
    ExtractionBackfillEffects, ExtractionBackfillError, ExtractionBackfillErrorCode,
    ExtractionBackfillEvidence, ExtractionBackfillProgress, ExtractionBackfillRecord,
    ExtractionBackfillRequest, ExtractionBackfillRun, ExtractionBackfillScope,
    ExtractionBackfillStatus, apply_extraction_backfill_batch,
    apply_postgres_extraction_backfill_batch, copy_postgres_extraction_service_data_batch,
    extraction_backfill_integrity_is_valid, load_postgres_extraction_backfill,
    start_extraction_backfill,
};
pub use extraction_console_projection::{
    EXTRACTION_CONSOLE_PROJECTION_PROTOCOL, ExtractionConsoleApprovalBoundary,
    ExtractionConsoleArtifacts, ExtractionConsoleAuthority, ExtractionConsoleBlocker,
    ExtractionConsoleEvidence, ExtractionConsoleProjection, ExtractionConsoleState,
    ExtractionConsoleTimelineEntry, load_extraction_artifact, load_extraction_console_projection,
    project_extraction_console, record_extraction_artifact,
};
pub use extraction_plan::{
    EXTRACTION_PLAN_GENERATOR_VERSION, EXTRACTION_PLAN_PROTOCOL, ExtractionApprovalBoundary,
    ExtractionAuthorityKind, ExtractionContractArtifactFormat, ExtractionCopyMode,
    ExtractionDataMapping, ExtractionEvidenceDigest, ExtractionExpectedAuthority,
    ExtractionGeneratedClientPlan, ExtractionInputPin, ExtractionInputPinKind,
    ExtractionMigrationMapping, ExtractionPlan, ExtractionPlanContractVersion, ExtractionPlanDiff,
    ExtractionPlanDiffEntry, ExtractionPlanEffects, ExtractionPlanGenerationError,
    ExtractionPlanGenerationIssueCode, ExtractionPlanInputs, ExtractionPlanIssueCode,
    ExtractionPlanPhase, ExtractionPlanPhaseKind, ExtractionPlanRejection, ExtractionServicePlan,
    ExtractionServiceReferencePlan, ExtractionStaleInput, ExtractionStorePlan,
    ExtractionTableMapping, ExtractionWorkloadPlan, ExtractionWorkloadRole,
    dry_run_extraction_plan, ensure_extraction_plan_fresh, extraction_input_digest,
    extraction_plan_integrity_is_valid, extraction_plan_json, extraction_plan_schema,
    generate_extraction_plan, render_extraction_plan,
};
pub use extraction_provisional_cutover::{
    EXTRACTION_PROVISIONAL_CUTOVER_PROTOCOL, ExtractionCutoverEvidence, ExtractionCutoverReceipt,
    ExtractionLinkedRollbackValidation, ExtractionProvisionalCutoverError,
    ExtractionProvisionalCutoverInputs, ExtractionProvisionalCutoverIssueCode,
    ExtractionProvisionalCutoverRun, ExtractionProvisionalCutoverStatus, ExtractionTrafficRoute,
    complete_provisional_rollback_validation, extraction_provisional_cutover_integrity_is_valid,
    fail_provisional_cutover, start_provisional_cutover, verify_provisional_cutover,
};
pub use extraction_quiescence::{
    EXTRACTION_QUIESCENCE_PROTOCOL, ExtractionDrainSnapshot, ExtractionQuiescenceEffects,
    ExtractionQuiescenceEvidence, ExtractionQuiescenceIssue, ExtractionQuiescenceIssueCode,
    ExtractionQuiescenceRun, ExtractionQuiescenceStartError, ExtractionQuiescenceStatus,
    cancel_extraction_quiescence, complete_extraction_quiescence,
    extraction_quiescence_integrity_is_valid, record_extraction_drain, start_extraction_quiescence,
};
pub use extraction_readiness::{
    EXTRACTION_READINESS_ANALYZER_VERSION, EXTRACTION_READINESS_REPORT_PROTOCOL,
    ExtractionBoundaryEvidence, ExtractionBoundaryReference, ExtractionBoundaryReferenceKind,
    ExtractionConsumerCompatibilityEvidence, ExtractionContractDirection,
    ExtractionContractEvidence, ExtractionContractKind, ExtractionCursorEvidence,
    ExtractionDataAccessEvidence, ExtractionDataAccessKind, ExtractionDataEvidenceSource,
    ExtractionDataTableEvidence, ExtractionDataVolumeEvidence, ExtractionEvidenceStatus,
    ExtractionMigrationEvidence, ExtractionReadinessEffects, ExtractionReadinessEvidence,
    ExtractionReadinessFinding, ExtractionReadinessIssueCode, ExtractionReadinessReport,
    ExtractionReadinessSurfaceSummary, ExtractionServiceDataEvidence,
    ExtractionTransactionEvidence, evaluate_extraction_readiness, extraction_readiness_report_json,
    extraction_readiness_report_schema, render_extraction_readiness_report,
};
pub use extraction_reconciliation::{
    EXTRACTION_RECONCILIATION_PROTOCOL, ExtractionBusinessInvariant, ExtractionNormalizedField,
    ExtractionReconciliationEffects, ExtractionReconciliationEvidence,
    ExtractionReconciliationInputs, ExtractionReconciliationIssue,
    ExtractionReconciliationIssueCode, ExtractionReconciliationReadError,
    ExtractionReconciliationResult, ExtractionReconciliationStatus, ExtractionRelationshipCount,
    ExtractionSourceSnapshot, extraction_reconciliation_integrity_is_valid,
    reconcile_extraction_data, reconcile_postgres_extraction_service_data,
};
pub use extraction_run::{
    DESTINATION_EXPANSION_PHASE_ID, EXTRACTION_OPERATION_RECEIPT_PROTOCOL, EXTRACTION_RUN_PROTOCOL,
    ExtractionExpandMigration, ExtractionExpansionOperation, ExtractionExpansionOperationKind,
    ExtractionExpansionWorkload, ExtractionMigrationArtifact, ExtractionOperationOutcome,
    ExtractionOperationReceipt, ExtractionRun, ExtractionRunAdvanceError,
    ExtractionRunAdvanceErrorCode, ExtractionRunEffects, ExtractionRunError,
    ExtractionRunErrorCode, ExtractionRunEvidence, ExtractionRunEvidenceKind,
    ExtractionRunExpectedState, ExtractionRunInputs, ExtractionRunMode, ExtractionRunPhase,
    ExtractionRunStartError, ExtractionRunStartErrorCode, ExtractionRunStatus,
    ExtractionWorkloadFailure, ExtractionWorkloadFailureCode, ExtractionWorkloadRequest,
    advance_destination_expansion, build_extraction_operation_receipt,
    dry_run_destination_expansion, extraction_operation_receipt_integrity_is_valid,
    extraction_run_integrity_is_valid, extraction_run_json, extraction_run_schema,
    record_destination_expansion_receipt, render_extraction_run, start_destination_expansion,
    validate_expand_first_postgres_sql,
};
pub use extraction_scaffold::{
    EXTRACTION_SCAFFOLD_GENERATOR_VERSION, EXTRACTION_SCAFFOLD_PROTOCOL,
    ExtractionGeneratedBinding, ExtractionGeneratedBindingKind, ExtractionPreservedIdentity,
    ExtractionScaffold, ExtractionScaffoldApplyError, ExtractionScaffoldApplyErrorCode,
    ExtractionScaffoldApplyResult, ExtractionScaffoldArtifact, ExtractionScaffoldBindingRole,
    ExtractionScaffoldEffects, ExtractionScaffoldFile, ExtractionScaffoldFileKind,
    ExtractionScaffoldGenerationError, ExtractionScaffoldGenerationIssueCode,
    ExtractionScaffoldInputs, ExtractionScaffoldIssue, ExtractionScaffoldIssueCode,
    apply_extraction_scaffold, dry_run_extraction_scaffold, extraction_scaffold_integrity_is_valid,
    extraction_scaffold_json, extraction_scaffold_schema, generate_extraction_scaffold,
    render_extraction_scaffold_patch, validate_extraction_scaffold,
};
pub use extraction_verification::{
    EXTRACTION_VERIFICATION_PROTOCOL, ExtractionBehaviorObservation,
    ExtractionCompatibilityEvidence, ExtractionPolicyEvidence, ExtractionVerificationEffects,
    ExtractionVerificationEvidence, ExtractionVerificationInputs, ExtractionVerificationIssue,
    ExtractionVerificationIssueCode, ExtractionVerificationResult, ExtractionVerificationStatus,
    extraction_verification_integrity_is_valid, verify_extraction_behavior,
};
pub use ga_support::*;
pub use production_delivery::*;
pub use reliability_report::{
    ActiveDegradedMode, RELIABILITY_REPORT_PROTOCOL, ReliabilityCheck, ReliabilityCheckState,
    ReliabilityEnforcementBoundary, ReliabilityHealthResult, ReliabilityIssueCode,
    ReliabilityReport, ReliabilityServiceState,
};
pub use story_segment::{
    STORY_SEGMENT_FEED_PROTOCOL, StorySegment, StorySegmentContract, StorySegmentFeed,
    StorySegmentOperation, StorySegmentSource, StorySegmentWorkflow,
};
pub use workload_identity::{
    AuthenticatedServicePrincipal, AuthenticatedTransportBinding, SpiffeWorkloadIdentityConfig,
    SpiffeWorkloadIdentityProvider, SystemSandboxWorkloadIdentityProvider, WorkloadCredential,
    WorkloadCredentialRequest, WorkloadIdentityError, WorkloadIdentityErrorCode,
    WorkloadIdentityEvidence, WorkloadIdentityProvider, WorkloadIdentityRotationEvidence,
    WorkloadIdentityVerification,
};

pub use lenso_contracts::ModuleManifest;

pub const SERVICE_CONTRACT_PROTOCOL: &str = "lenso.service.v1";
pub const AUTONOMOUS_SERVICE_PROTOCOL: &str = "lenso.service.v2";
pub const COMMON_CONTEXT_PROTOCOL: &str = "lenso.context.v1";
pub const SERVICE_PACKAGE_PROTOCOL: &str = "lenso.service-package.v1";
pub const SERVICE_WORKSPACE_PROTOCOL: &str = "lenso.service-workspace.v1";
pub const SERVICE_RELEASE_PLAN_PROTOCOL: &str = "lenso.service-release-plan.v1";
pub const SERVICE_SYSTEM_PROTOCOL: &str = "lenso.system.v1";
pub const SYSTEM_V2_PROTOCOL: &str = "lenso.system.v2";
pub const MODULE_CONTRACT_PROTOCOL: &str = "lenso.module.v1";
pub const MODULE_RELEASE_PROTOCOL: &str = "lenso.module-release.v1";
pub const SERVICE_CONTRACT_SCHEMA_JSON: &str =
    include_str!("../schemas/lenso-service.v1.schema.json");
pub const SERVICE_V2_CONTRACT_SCHEMA_JSON: &str =
    include_str!("../schemas/lenso-service.v2.schema.json");
pub const COMMON_CONTEXT_V1_SCHEMA_JSON: &str =
    include_str!("../schemas/lenso-context.v1.schema.json");
pub const EVENT_ENVELOPE_V1_SCHEMA_JSON: &str =
    include_str!("../schemas/lenso-event-envelope.v1.schema.json");
pub const SERVICE_PACKAGE_SCHEMA_JSON: &str =
    include_str!("../schemas/lenso-service-package.v1.schema.json");
pub const SERVICE_WORKSPACE_SCHEMA_JSON: &str =
    include_str!("../schemas/lenso-service-workspace.v1.schema.json");
pub const SERVICE_SYSTEM_SCHEMA_JSON: &str = include_str!("../schemas/lenso-system.v1.schema.json");
pub const SYSTEM_V2_CONTRACT_SCHEMA_JSON: &str =
    include_str!("../schemas/lenso-system.v2.schema.json");
pub const MODULE_CONTRACT_SCHEMA_JSON: &str =
    include_str!("../schemas/lenso-module.v1.schema.json");
pub const MODULE_RELEASE_SCHEMA_JSON: &str =
    include_str!("../schemas/lenso-module-release.v1.schema.json");
pub const LEGACY_SERVICE_V1_FIXTURE_JSON: &str =
    include_str!("../fixtures/contracts/v1/service-provider.json");
pub const LEGACY_SYSTEM_V1_FIXTURE_JSON: &str =
    include_str!("../fixtures/contracts/v1/system-provider.json");
pub const AUTONOMOUS_SERVICE_V2_FIXTURE_JSON: &str =
    include_str!("../fixtures/contracts/v2/autonomous-service.json");
pub const DIRECT_HTTP_OPENAPI_V1_FIXTURE_YAML: &str =
    include_str!("../fixtures/contracts/v2/support-http.v1.yaml");
pub const DIRECT_GRPC_PROTO_V1_FIXTURE: &str =
    include_str!("../fixtures/contracts/v2/support-grpc.v1.proto");
pub const SUPPORT_EVENT_SCHEMA_JSON: &str =
    include_str!("../fixtures/contracts/v2/support-ticket-opened.v1.schema.json");
pub const DIRECT_GRPC_DESCRIPTOR_V1: &[u8] =
    tonic::include_file_descriptor_set!("support_descriptor");
pub const MIXED_SYSTEM_V2_FIXTURE_JSON: &str =
    include_str!("../fixtures/contracts/v2/mixed-system.json");
pub const COMMON_CONTEXT_V1_FIXTURE_JSON: &str =
    include_str!("../fixtures/contracts/v1/common-context.json");
pub const COMMON_CONTEXT_GLOSSARY_MARKDOWN: &str =
    include_str!("../docs/common-context-contracts.md");
pub const REQUEST_RESPONSE_COMPATIBILITY_MARKDOWN: &str =
    include_str!("../docs/request-response-compatibility.md");
pub const REQUEST_RESPONSE_COMPATIBILITY_SAFE_FIXTURE_JSON: &str =
    include_str!("../fixtures/compatibility/request-response/safe.json");
pub const REQUEST_RESPONSE_COMPATIBILITY_NEEDS_ATTENTION_FIXTURE_JSON: &str =
    include_str!("../fixtures/compatibility/request-response/needs-attention.json");
pub const REQUEST_RESPONSE_COMPATIBILITY_BREAKING_FIXTURE_JSON: &str =
    include_str!("../fixtures/compatibility/request-response/breaking.json");
pub const REQUEST_RESPONSE_COMPATIBILITY_BLOCKED_FIXTURE_JSON: &str =
    include_str!("../fixtures/compatibility/request-response/blocked.json");
pub const EVENT_COMPATIBILITY_SAFE_FIXTURE_JSON: &str =
    include_str!("../fixtures/compatibility/event/safe.json");
pub const EVENT_COMPATIBILITY_NEEDS_ATTENTION_FIXTURE_JSON: &str =
    include_str!("../fixtures/compatibility/event/needs-attention.json");
pub const EVENT_COMPATIBILITY_BREAKING_FIXTURE_JSON: &str =
    include_str!("../fixtures/compatibility/event/breaking.json");
pub const EVENT_COMPATIBILITY_BLOCKED_FIXTURE_JSON: &str =
    include_str!("../fixtures/compatibility/event/blocked.json");
pub const CONFIG_COMPATIBILITY_SAFE_FIXTURE_JSON: &str =
    include_str!("../fixtures/compatibility/config/safe.json");
pub const CONFIG_COMPATIBILITY_NEEDS_ATTENTION_FIXTURE_JSON: &str =
    include_str!("../fixtures/compatibility/config/needs-attention.json");
pub const CONFIG_COMPATIBILITY_BREAKING_FIXTURE_JSON: &str =
    include_str!("../fixtures/compatibility/config/breaking.json");
pub const CONFIG_COMPATIBILITY_BLOCKED_FIXTURE_JSON: &str =
    include_str!("../fixtures/compatibility/config/blocked.json");
pub const RELIABILITY_COMPATIBILITY_SAFE_FIXTURE_JSON: &str =
    include_str!("../fixtures/compatibility/reliability/safe.json");
pub const RELIABILITY_COMPATIBILITY_NEEDS_ATTENTION_FIXTURE_JSON: &str =
    include_str!("../fixtures/compatibility/reliability/needs-attention.json");
pub const RELIABILITY_COMPATIBILITY_BREAKING_FIXTURE_JSON: &str =
    include_str!("../fixtures/compatibility/reliability/breaking.json");
pub const RELIABILITY_COMPATIBILITY_BLOCKED_FIXTURE_JSON: &str =
    include_str!("../fixtures/compatibility/reliability/blocked.json");
pub const CONTRACT_COMPATIBILITY_MARKDOWN: &str = include_str!("../docs/contract-compatibility.md");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompatibilityFixture {
    pub name: &'static str,
    pub json: &'static str,
}

pub const REQUEST_RESPONSE_COMPATIBILITY_FIXTURES: &[CompatibilityFixture] = &[
    CompatibilityFixture {
        name: "safe",
        json: REQUEST_RESPONSE_COMPATIBILITY_SAFE_FIXTURE_JSON,
    },
    CompatibilityFixture {
        name: "needs_attention",
        json: REQUEST_RESPONSE_COMPATIBILITY_NEEDS_ATTENTION_FIXTURE_JSON,
    },
    CompatibilityFixture {
        name: "breaking",
        json: REQUEST_RESPONSE_COMPATIBILITY_BREAKING_FIXTURE_JSON,
    },
    CompatibilityFixture {
        name: "blocked",
        json: REQUEST_RESPONSE_COMPATIBILITY_BLOCKED_FIXTURE_JSON,
    },
];

pub const EVENT_COMPATIBILITY_FIXTURES: &[CompatibilityFixture] = &[
    CompatibilityFixture {
        name: "safe",
        json: EVENT_COMPATIBILITY_SAFE_FIXTURE_JSON,
    },
    CompatibilityFixture {
        name: "needs_attention",
        json: EVENT_COMPATIBILITY_NEEDS_ATTENTION_FIXTURE_JSON,
    },
    CompatibilityFixture {
        name: "breaking",
        json: EVENT_COMPATIBILITY_BREAKING_FIXTURE_JSON,
    },
    CompatibilityFixture {
        name: "blocked",
        json: EVENT_COMPATIBILITY_BLOCKED_FIXTURE_JSON,
    },
];
pub const CONFIG_COMPATIBILITY_FIXTURES: &[CompatibilityFixture] = &[
    CompatibilityFixture {
        name: "safe",
        json: CONFIG_COMPATIBILITY_SAFE_FIXTURE_JSON,
    },
    CompatibilityFixture {
        name: "needs_attention",
        json: CONFIG_COMPATIBILITY_NEEDS_ATTENTION_FIXTURE_JSON,
    },
    CompatibilityFixture {
        name: "breaking",
        json: CONFIG_COMPATIBILITY_BREAKING_FIXTURE_JSON,
    },
    CompatibilityFixture {
        name: "blocked",
        json: CONFIG_COMPATIBILITY_BLOCKED_FIXTURE_JSON,
    },
];
pub const RELIABILITY_COMPATIBILITY_FIXTURES: &[CompatibilityFixture] = &[
    CompatibilityFixture {
        name: "safe",
        json: RELIABILITY_COMPATIBILITY_SAFE_FIXTURE_JSON,
    },
    CompatibilityFixture {
        name: "needs_attention",
        json: RELIABILITY_COMPATIBILITY_NEEDS_ATTENTION_FIXTURE_JSON,
    },
    CompatibilityFixture {
        name: "breaking",
        json: RELIABILITY_COMPATIBILITY_BREAKING_FIXTURE_JSON,
    },
    CompatibilityFixture {
        name: "blocked",
        json: RELIABILITY_COMPATIBILITY_BLOCKED_FIXTURE_JSON,
    },
];

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CompatibilityCategory {
    Safe,
    NeedsAttention,
    Breaking,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractCompatibilityKind {
    EventContract,
    ConfigContract,
    ReliabilityContract,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatibilityReason {
    pub code: String,
    pub path: String,
    pub message: String,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractCompatibilityResult {
    pub category: CompatibilityCategory,
    pub contract_kind: ContractCompatibilityKind,
    pub contract_id: String,
    pub changed_version: String,
    pub affected_references: Vec<String>,
    pub reasons: Vec<CompatibilityReason>,
}

#[must_use]
pub fn evaluate_event_compatibility(input: &Value) -> ContractCompatibilityResult {
    let mut result = compatibility_result(input, ContractCompatibilityKind::EventContract);
    let before = valid_candidate(input, &mut result);
    let after = input.get("after");
    let formats = before
        .and_then(|v| v.get("format"))
        .zip(after.and_then(|v| v.get("format")));
    if !matches!(formats, Some((a, b)) if a == b && matches!(a.as_str(), Some("json_schema" | "protobuf")))
    {
        add_contract_reason(
            &mut result,
            CompatibilityCategory::Blocked,
            "event_artifact_unverifiable",
            "$.before.format",
            "Event artifacts must use the same supported transport-independent format.",
            "Provide canonical JSON Schema or Protobuf event artifacts; broker choice is not part of this comparison.",
        );
    } else if formats.is_some_and(|(a, _)| a == "json_schema") {
        compare_event_json_schema(
            &mut result,
            before.and_then(|v| v.get("schema")),
            after.and_then(|v| v.get("schema")),
        );
    } else {
        compare_event_protobuf(&mut result, before, after);
    }
    finish_contract_result(
        &mut result,
        "event_backward_compatible",
        "The business event remains compatible for the affected references.",
        "Publish the new event version and monitor Consumers.",
    );
    result
}

#[must_use]
pub fn evaluate_config_compatibility(input: &Value) -> ContractCompatibilityResult {
    let mut result = compatibility_result(input, ContractCompatibilityKind::ConfigContract);
    let before = valid_candidate(input, &mut result);
    let after = input.get("after");
    let old_fields = indexed_values(before.and_then(|v| v.get("fields")), "path");
    let new_fields = indexed_values(after.and_then(|v| v.get("fields")), "path");
    if old_fields.is_none() || new_fields.is_none() {
        add_contract_reason(
            &mut result,
            CompatibilityCategory::Blocked,
            "config_fields_unverifiable",
            "$.after.fields",
            "Config field declarations are missing or ambiguous.",
            "Provide unique field paths in both Config Contract versions.",
        );
    } else if let (Some(old_fields), Some(new_fields)) = (old_fields, new_fields) {
        if !old_fields.values().all(valid_config_field)
            || !new_fields.values().all(valid_config_field)
        {
            add_contract_reason(
                &mut result,
                CompatibilityCategory::Blocked,
                "config_field_declaration_unverifiable",
                "$.after.fields",
                "A Config field dimension is missing or unsupported.",
                "Provide required, sensitivity, scope, mutability, and activation metadata for every field.",
            );
        }
        for (path, old) in &old_fields {
            let Some(new) = new_fields.get(path) else {
                add_contract_reason(
                    &mut result,
                    CompatibilityCategory::Breaking,
                    "config_field_removed",
                    &format!("$.after.fields.{path}"),
                    "A declared configuration value was removed.",
                    "Restore the field or coordinate migration with affected operators.",
                );
                continue;
            };
            config_change(
                &mut result,
                path,
                old,
                new,
                "shape",
                "config_shape_changed",
                CompatibilityCategory::Breaking,
                "The configuration value shape changed.",
            );
            if !old
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                && new
                    .get("required")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            {
                add_contract_reason(
                    &mut result,
                    CompatibilityCategory::Breaking,
                    "config_required_value_added",
                    &format!("$.after.fields.{path}.required"),
                    "An optional configuration value became required.",
                    "Keep it optional or provide a migration and activation plan.",
                );
            }
            if old
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                && !new
                    .get("required")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            {
                add_contract_reason(
                    &mut result,
                    CompatibilityCategory::NeedsAttention,
                    "config_required_value_relaxed",
                    &format!("$.after.fields.{path}.required"),
                    "A required configuration value became optional.",
                    "Review default and absence semantics with affected operators.",
                );
            }
            if old.get("sensitive") == Some(&Value::Bool(true))
                && new.get("sensitive") == Some(&Value::Bool(false))
            {
                add_contract_reason(
                    &mut result,
                    CompatibilityCategory::Breaking,
                    "config_sensitivity_weakened",
                    &format!("$.after.fields.{path}.sensitive"),
                    "A sensitive value is no longer declared sensitive.",
                    "Restore sensitivity before exposing or persisting the value.",
                );
            }
            if old.get("sensitive") == Some(&Value::Bool(false))
                && new.get("sensitive") == Some(&Value::Bool(true))
            {
                add_contract_reason(
                    &mut result,
                    CompatibilityCategory::NeedsAttention,
                    "config_sensitivity_strengthened",
                    &format!("$.after.fields.{path}.sensitive"),
                    "A configuration value is newly declared sensitive.",
                    "Verify storage, display, logging, and secret-provider handling before activation.",
                );
            }
            config_change(
                &mut result,
                path,
                old,
                new,
                "scope",
                "config_scope_changed",
                CompatibilityCategory::Breaking,
                "The configuration ownership scope changed.",
            );
            if old.get("mutability").and_then(Value::as_str) == Some("mutable")
                && new.get("mutability").and_then(Value::as_str) == Some("immutable")
            {
                add_contract_reason(
                    &mut result,
                    CompatibilityCategory::Breaking,
                    "config_mutability_restricted",
                    &format!("$.after.fields.{path}.mutability"),
                    "A mutable value became immutable.",
                    "Preserve mutability or provide a replacement and migration path.",
                );
            } else {
                config_change(
                    &mut result,
                    path,
                    old,
                    new,
                    "mutability",
                    "config_mutability_changed",
                    CompatibilityCategory::NeedsAttention,
                    "Configuration mutability changed.",
                );
            }
            config_change(
                &mut result,
                path,
                old,
                new,
                "activation",
                "config_activation_changed",
                CompatibilityCategory::NeedsAttention,
                "Configuration activation requirements changed.",
            );
        }
        for (path, new) in new_fields
            .iter()
            .filter(|(path, _)| !old_fields.contains_key(*path))
        {
            let (category, code, message) = if new
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                (
                    CompatibilityCategory::Breaking,
                    "config_required_value_added",
                    "A new required configuration value was added.",
                )
            } else {
                (
                    CompatibilityCategory::Safe,
                    "config_optional_value_added",
                    "A new optional configuration value was added.",
                )
            };
            add_contract_reason(
                &mut result,
                category,
                code,
                &format!("$.after.fields.{path}"),
                message,
                "Provide the value when required and retain activation guidance for operators.",
            );
        }
    }
    finish_contract_result(
        &mut result,
        "config_backward_compatible",
        "The Config Contract remains compatible for the affected references.",
        "Publish the Config Contract and retain the activation evidence.",
    );
    result
}

#[must_use]
pub fn evaluate_reliability_compatibility(input: &Value) -> ContractCompatibilityResult {
    let mut result = compatibility_result(input, ContractCompatibilityKind::ReliabilityContract);
    let before = valid_candidate(input, &mut result);
    let after = input.get("after");
    if let (Some(old), Some(new)) = (
        before.and_then(Value::as_object),
        after.and_then(Value::as_object),
    ) {
        for key in ["availabilityTarget", "latencyTargetMs"] {
            if old.get(key) != new.get(key) {
                let tightened = reliability_target_tightened(key, old.get(key), new.get(key));
                let category = if tightened {
                    CompatibilityCategory::Breaking
                } else {
                    CompatibilityCategory::NeedsAttention
                };
                let code = if tightened {
                    "reliability_target_tightened".to_owned()
                } else {
                    format!("reliability_{}_changed", camel_to_snake(key))
                };
                add_contract_reason(
                    &mut result,
                    category,
                    &code,
                    &format!("$.after.{key}"),
                    "A whole-Service reliability target changed; declaration compatibility does not replace a runtime Reliability Report.",
                    "Review the target direction and runtime evidence with affected owners.",
                );
            }
        }
        for key in old.keys().chain(new.keys()).collect::<BTreeSet<_>>() {
            if key != "version"
                && !matches!(key.as_str(), "availabilityTarget" | "latencyTargetMs")
                && old.get(key) != new.get(key)
            {
                add_contract_reason(
                    &mut result,
                    CompatibilityCategory::NeedsAttention,
                    &format!("reliability_{}_changed", camel_to_snake(key)),
                    &format!("$.after.{key}"),
                    "A Reliability Contract declaration changed; evaluate the effective profile against runtime evidence.",
                    "Review the declared whole-Service expectation, Reliability Report, and affected owners.",
                );
            }
        }
    }
    finish_contract_result(
        &mut result,
        "reliability_declaration_compatible",
        "The Reliability Contract declaration is unchanged; runtime observations remain a separate report.",
        "Retain the runtime Reliability Report beside this declaration check.",
    );
    result
}

fn compatibility_result(
    input: &Value,
    kind: ContractCompatibilityKind,
) -> ContractCompatibilityResult {
    let mut affected_references = input
        .get("affectedReferences")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    affected_references.sort();
    affected_references.dedup();
    ContractCompatibilityResult {
        category: CompatibilityCategory::Safe,
        contract_kind: kind,
        contract_id: input
            .get("contractId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        changed_version: input
            .get("changedVersion")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        affected_references,
        reasons: Vec::new(),
    }
}

fn valid_candidate<'a>(
    input: &'a Value,
    result: &mut ContractCompatibilityResult,
) -> Option<&'a Value> {
    let before = input.get("before");
    let after = input.get("after");
    let before_version = before
        .and_then(|v| v.get("version"))
        .and_then(Value::as_str);
    let after_version = after.and_then(|v| v.get("version")).and_then(Value::as_str);
    if result.contract_id.is_empty()
        || result.changed_version.is_empty()
        || result.affected_references.is_empty()
    {
        add_contract_reason(
            result,
            CompatibilityCategory::Blocked,
            "contract_relationship_unverifiable",
            "$",
            "Contract identity, changed version, and affected references must be explicit.",
            "Resolve the contract identity and affected references before comparing versions.",
        );
    }
    if before_version.is_none()
        || after_version != Some(result.changed_version.as_str())
        || before_version == after_version
    {
        add_contract_reason(
            result,
            CompatibilityCategory::Blocked,
            "contract_version_unverifiable",
            "$.after.version",
            "Both versions must be explicit, different, and match changedVersion.",
            "Provide authoritative before and after contract versions.",
        );
    }
    before
}

fn compare_event_json_schema(
    result: &mut ContractCompatibilityResult,
    old: Option<&Value>,
    new: Option<&Value>,
) {
    let (Some(old), Some(new)) = (old, new) else {
        add_contract_reason(
            result,
            CompatibilityCategory::Blocked,
            "event_schema_unverifiable",
            "$.after.schema",
            "The event schemas are missing.",
            "Provide both canonical event schemas.",
        );
        return;
    };
    if old.get("type") != new.get("type") {
        add_contract_reason(
            result,
            CompatibilityCategory::Breaking,
            "event_type_changed",
            "$.after.schema.type",
            "The event payload type changed.",
            "Restore the payload type or introduce a coordinated event version.",
        );
    }
    let old_properties = old.get("properties").and_then(Value::as_object);
    let new_properties = new.get("properties").and_then(Value::as_object);
    if let (Some(old_properties), Some(new_properties)) = (old_properties, new_properties) {
        for (field, old_field) in old_properties {
            let Some(new_field) = new_properties.get(field) else {
                add_contract_reason(
                    result,
                    CompatibilityCategory::Breaking,
                    "event_field_removed",
                    &format!("$.after.schema.properties.{field}"),
                    "An event payload field was removed.",
                    "Restore the field or coordinate a new version with every affected Consumer.",
                );
                continue;
            };
            if old_field.get("type") != new_field.get("type") {
                add_contract_reason(
                    result,
                    CompatibilityCategory::Breaking,
                    "event_field_type_changed",
                    &format!("$.after.schema.properties.{field}"),
                    "An event payload field type changed.",
                    "Restore the type or add a new versioned field.",
                );
            } else if old_field != new_field {
                add_contract_reason(
                    result,
                    CompatibilityCategory::NeedsAttention,
                    "event_field_constraints_changed",
                    &format!("$.after.schema.properties.{field}"),
                    "Event field constraints changed and require semantic review.",
                    "Review Producer and Consumer behavior before publishing.",
                );
            }
        }
        let old_required = string_set(old.get("required"));
        let new_required = string_set(new.get("required"));
        for field in old_required.difference(&new_required) {
            add_contract_reason(
                result,
                CompatibilityCategory::Breaking,
                "event_required_field_became_optional",
                &format!("$.after.schema.properties.{field}"),
                "A field guaranteed to Consumers may now be omitted.",
                "Keep the event field required or coordinate a new event version.",
            );
        }
    } else if old != new {
        add_contract_reason(
            result,
            CompatibilityCategory::NeedsAttention,
            "event_schema_not_structurally_proven",
            "$.after.schema",
            "The event schema change cannot be proven compatible structurally.",
            "Review the schemas with affected Producers and Consumers.",
        );
    }
}

fn compare_event_protobuf(
    result: &mut ContractCompatibilityResult,
    old: Option<&Value>,
    new: Option<&Value>,
) {
    if !valid_protobuf_event_fields(old) || !valid_protobuf_event_fields(new) {
        add_contract_reason(
            result,
            CompatibilityCategory::Blocked,
            "event_protobuf_descriptor_unverifiable",
            "$.after.fields",
            "The Protobuf event field descriptors are incomplete or ambiguous.",
            "Provide unique positive field numbers with non-empty names and types.",
        );
        return;
    }
    let old_fields = protobuf_fields(old);
    let new_fields = protobuf_fields(new);
    let (Some(old_fields), Some(new_fields)) = (old_fields, new_fields) else {
        add_contract_reason(
            result,
            CompatibilityCategory::Blocked,
            "event_protobuf_descriptor_unverifiable",
            "$.after.fields",
            "The Protobuf event field descriptors are missing or invalid.",
            "Provide descriptor-based fields for both event versions.",
        );
        return;
    };
    for (number, old_field) in &old_fields {
        let Some(new_field) = new_fields.get(number) else {
            add_contract_reason(
                result,
                CompatibilityCategory::Breaking,
                "event_protobuf_field_removed",
                &format!("$.after.fields.{number}"),
                "A Protobuf event field number was removed.",
                "Restore or reserve the number and coordinate a new event version.",
            );
            continue;
        };
        if old_field.get("type") != new_field.get("type") {
            add_contract_reason(
                result,
                CompatibilityCategory::Breaking,
                "event_protobuf_field_type_changed",
                &format!("$.after.fields.{number}"),
                "A Protobuf event field number changed wire type.",
                "Restore the wire-compatible type or allocate a new field number.",
            );
        } else if old_field.get("name") != new_field.get("name") {
            add_contract_reason(
                result,
                CompatibilityCategory::NeedsAttention,
                "event_protobuf_field_renamed",
                &format!("$.after.fields.{number}"),
                "A Protobuf event field kept its number but changed source name.",
                "Review generated clients and JSON mappings before publishing.",
            );
        }
    }
}

fn valid_protobuf_event_fields(value: Option<&Value>) -> bool {
    let Some(fields) = value
        .and_then(|value| value.get("fields"))
        .and_then(Value::as_array)
    else {
        return false;
    };
    let mut numbers = BTreeSet::new();
    fields.iter().all(|field| {
        field
            .get("number")
            .and_then(Value::as_i64)
            .is_some_and(|number| number > 0 && numbers.insert(number))
            && field
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty())
            && field
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty())
    })
}

fn valid_config_field(field: &Value) -> bool {
    field.get("required").is_some_and(Value::is_boolean)
        && field.get("sensitive").is_some_and(Value::is_boolean)
        && matches!(
            field.get("scope").and_then(Value::as_str),
            Some("service" | "region" | "tenant")
        )
        && matches!(
            field.get("mutability").and_then(Value::as_str),
            Some("immutable" | "mutable")
        )
        && matches!(
            field.get("activation").and_then(Value::as_str),
            Some("hot" | "restart")
        )
}

fn indexed_values(value: Option<&Value>, key: &str) -> Option<BTreeMap<String, Value>> {
    let values = value?.as_array()?;
    let mut indexed = BTreeMap::new();
    for value in values {
        let identity = value.get(key)?.as_str()?.to_owned();
        if indexed.insert(identity, value.clone()).is_some() {
            return None;
        }
    }
    Some(indexed)
}

fn config_change(
    result: &mut ContractCompatibilityResult,
    path: &str,
    old: &Value,
    new: &Value,
    field: &str,
    code: &str,
    category: CompatibilityCategory,
    message: &str,
) {
    if old.get(field) != new.get(field) {
        add_contract_reason(
            result,
            category,
            code,
            &format!("$.after.fields.{path}.{field}"),
            message,
            "Review the change and provide an operator migration and activation plan.",
        );
    }
}

fn add_contract_reason(
    result: &mut ContractCompatibilityResult,
    category: CompatibilityCategory,
    code: &str,
    path: &str,
    message: &str,
    next_action: &str,
) {
    result.category = result.category.max(category);
    result.reasons.push(CompatibilityReason {
        code: code.to_owned(),
        path: path.to_owned(),
        message: message.to_owned(),
        next_action: next_action.to_owned(),
    });
}

fn finish_contract_result(
    result: &mut ContractCompatibilityResult,
    safe_code: &str,
    message: &str,
    next_action: &str,
) {
    if result.reasons.is_empty() {
        add_contract_reason(
            result,
            CompatibilityCategory::Safe,
            safe_code,
            "$",
            message,
            next_action,
        );
    }
    result.reasons.sort();
    result.reasons.dedup();
}

fn camel_to_snake(value: &str) -> String {
    value
        .chars()
        .enumerate()
        .fold(String::new(), |mut output, (index, ch)| {
            if ch.is_ascii_uppercase() {
                if index > 0 {
                    output.push('_');
                }
                output.push(ch.to_ascii_lowercase());
            } else {
                output.push(ch);
            }
            output
        })
}

fn reliability_target_tightened(key: &str, old: Option<&Value>, new: Option<&Value>) -> bool {
    match key {
        "availabilityTarget" => {
            let parse = |value: Option<&Value>| {
                value
                    .and_then(Value::as_str)
                    .and_then(|value| value.trim_end_matches('%').parse::<f64>().ok())
            };
            matches!((parse(old), parse(new)), (Some(old), Some(new)) if new > old)
        }
        "latencyTargetMs" => {
            matches!((old.and_then(Value::as_u64), new.and_then(Value::as_u64)), (Some(old), Some(new)) if new < old)
        }
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestResponseCompatibilityCategory {
    Safe,
    NeedsAttention,
    Breaking,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestResponseContractKind {
    ProviderProtocol,
    ServiceContract,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestResponseCompatibilityReason {
    pub code: String,
    pub path: String,
    pub message: String,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestResponseCompatibilityResult {
    pub category: RequestResponseCompatibilityCategory,
    pub contract_kind: RequestResponseContractKind,
    pub contract_id: String,
    pub changed_version: String,
    pub producers: Vec<String>,
    pub consumers: Vec<String>,
    pub reasons: Vec<RequestResponseCompatibilityReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestResponseCanonicalizationError {
    pub code: String,
    pub path: String,
    pub message: String,
    pub next_action: String,
}

/// Converts a raw OpenAPI JSON value into the canonical operation shape consumed by the
/// compatibility evaluator. YAML callers should deserialize to `serde_json::Value` first.
pub fn canonicalize_openapi_request_response(
    document: &Value,
) -> Result<Value, Vec<RequestResponseCanonicalizationError>> {
    if !document
        .get("openapi")
        .and_then(Value::as_str)
        .is_some_and(|version| version.starts_with("3.0.") || version.starts_with("3.1."))
    {
        return Err(vec![canonicalization_error(
            "openapi_protocol_unsupported",
            "$.openapi",
            "Only OpenAPI 3.0 and 3.1 documents are supported.",
            "Generate a complete OpenAPI 3.0 or 3.1 document before comparing contracts.",
        )]);
    }
    let mut reference_errors = Vec::new();
    validate_openapi_references(document, document, "$", &mut reference_errors);
    if !reference_errors.is_empty() {
        return Err(reference_errors);
    }
    let Some(version) = document
        .get("info")
        .and_then(|info| info.get("version"))
        .and_then(Value::as_str)
        .filter(|version| !version.is_empty())
    else {
        return Err(vec![canonicalization_error(
            "openapi_version_missing",
            "$.info.version",
            "OpenAPI info.version is required.",
            "Set info.version to the authoritative contract version.",
        )]);
    };
    let Some(paths) = document.get("paths").and_then(Value::as_object) else {
        return Err(vec![canonicalization_error(
            "openapi_paths_missing",
            "$.paths",
            "OpenAPI paths must be an object.",
            "Generate a complete OpenAPI document with request-response paths.",
        )]);
    };
    let mut operations = serde_json::Map::new();
    let mut errors = Vec::new();
    for (path, path_item) in paths {
        let Some(path_item) = path_item.as_object() else {
            continue;
        };
        for method in [
            "get", "put", "post", "delete", "patch", "head", "options", "trace",
        ] {
            let Some(operation) = path_item.get(method).and_then(Value::as_object) else {
                continue;
            };
            let operation_id = operation
                .get("operationId")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{} {}", method.to_ascii_uppercase(), path));
            let declared_parameters = path_item
                .get("parameters")
                .and_then(Value::as_array)
                .map_or(0, Vec::len)
                + operation
                    .get("parameters")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len);
            let parameters = canonicalize_openapi_parameters(document, path_item, operation);
            if declared_parameters > 0 && parameters.is_none() {
                errors.push(canonicalization_error(
                    "openapi_parameter_unverifiable",
                    format!("$.paths.{path}.{method}.parameters"),
                    "An OpenAPI parameter cannot be canonicalized.",
                    "Resolve parameter references and provide name, location, and schema metadata.",
                ));
                continue;
            }
            let request = operation
                .get("requestBody")
                .and_then(|body| {
                    let mut schema = canonicalize_openapi_content(document, body.get("content")?)?;
                    if let Some(object) = schema.as_object_mut() {
                        object.insert(
                            "x-lenso-request-body-required".to_owned(),
                            Value::Bool(
                                body.get("required")
                                    .and_then(Value::as_bool)
                                    .unwrap_or(false),
                            ),
                        );
                        if let Some(parameters) = &parameters {
                            object.insert("x-lenso-parameters".to_owned(), parameters.clone());
                        }
                    }
                    Some(schema)
                })
                .or(parameters);
            let declared_success_responses = operation
                .get("responses")
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
                .filter(|(status, _)| status.starts_with('2'))
                .count();
            let responses = operation
                .get("responses")
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
                .filter(|(status, _)| status.starts_with('2'))
                .filter_map(|(status, response)| {
                    canonicalize_openapi_response(document, response)
                        .map(|schema| (status.clone(), schema))
                })
                .collect::<serde_json::Map<_, _>>();
            if responses.len() != declared_success_responses {
                errors.push(canonicalization_error(
                    "openapi_response_unverifiable",
                    format!("$.paths.{path}.{method}.responses"),
                    "A successful OpenAPI response cannot be canonicalized.",
                    "Use one explicit media type per response and provide a valid schema.",
                ));
                continue;
            }
            let response_statuses = responses.keys().cloned().collect::<Vec<_>>();
            let response = (!responses.is_empty()).then(|| {
                serde_json::json!({
                    "type": "object",
                    "required": response_statuses,
                    "properties": responses,
                })
            });
            let (Some(request), Some(response)) = (request, response) else {
                errors.push(canonicalization_error(
                    "openapi_operation_schema_missing",
                    format!("$.paths.{path}.{method}"),
                    "A request-response operation needs request and success response schemas.",
                    "Declare requestBody and a 2xx response schema for this operation.",
                ));
                continue;
            };
            if operations.contains_key(&operation_id) {
                errors.push(canonicalization_error(
                    "openapi_operation_id_duplicate",
                    format!("$.paths.{path}.{method}.operationId"),
                    "OpenAPI operationId values must be unique.",
                    "Assign a unique stable operationId before comparing contracts.",
                ));
                continue;
            }
            operations.insert(
                operation_id,
                serde_json::json!({
                    "method": method,
                    "path": path,
                    "request": request,
                    "response": response,
                }),
            );
        }
    }
    if operations.is_empty() {
        errors.push(canonicalization_error(
            "openapi_operations_missing",
            "$.paths",
            "No verifiable request-response operations were found.",
            "Declare at least one operation with request and success response schemas.",
        ));
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(serde_json::json!({ "format": "openapi", "version": version, "operations": operations }))
}

/// Converts a binary Protobuf `FileDescriptorSet` into the canonical operation shape.
pub fn canonicalize_protobuf_request_response(
    version: &str,
    descriptor_bytes: &[u8],
) -> Result<Value, Vec<RequestResponseCanonicalizationError>> {
    if version.is_empty() {
        return Err(vec![canonicalization_error(
            "protobuf_version_missing",
            "$.version",
            "The Protobuf contract version is required.",
            "Provide the authoritative Protobuf contract version.",
        )]);
    }
    let descriptor = prost_types::FileDescriptorSet::decode(descriptor_bytes).map_err(|error| {
        vec![canonicalization_error(
            "protobuf_descriptor_invalid",
            "$",
            format!("Invalid FileDescriptorSet: {error}"),
            "Generate a binary descriptor set with protoc --descriptor_set_out.",
        )]
    })?;
    let descriptor_errors = validate_supported_protobuf_descriptor(&descriptor);
    if !descriptor_errors.is_empty() {
        return Err(descriptor_errors);
    }
    let mut messages = BTreeMap::new();
    for file in &descriptor.file {
        let package = file.package.as_deref().unwrap_or_default();
        collect_protobuf_messages(package, "", &file.message_type, &mut messages);
    }
    let mut operations = serde_json::Map::new();
    for file in &descriptor.file {
        let package = file.package.as_deref().unwrap_or_default();
        for service in &file.service {
            let service_name = service.name.as_deref().unwrap_or("Service");
            for method in &service.method {
                let method_name = method.name.as_deref().unwrap_or("Method");
                let operation_id = [package, service_name, method_name]
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join(".");
                let request_type = method
                    .input_type
                    .as_deref()
                    .map(|name| name.trim_start_matches('.'));
                let response_type = method
                    .output_type
                    .as_deref()
                    .map(|name| name.trim_start_matches('.'));
                let request = method.input_type.as_deref().and_then(|name| {
                    expand_protobuf_message(
                        name.trim_start_matches('.'),
                        &messages,
                        &mut BTreeSet::new(),
                    )
                });
                let response = method.output_type.as_deref().and_then(|name| {
                    expand_protobuf_message(
                        name.trim_start_matches('.'),
                        &messages,
                        &mut BTreeSet::new(),
                    )
                });
                let (Some(mut request), Some(mut response)) = (request, response) else {
                    return Err(vec![canonicalization_error(
                        "protobuf_message_unresolved",
                        format!("$.operations.{operation_id}"),
                        "RPC input or output message is missing from the descriptor set.",
                        "Include imports when generating the FileDescriptorSet and retry.",
                    )]);
                };
                let syntax = file.syntax.as_deref().unwrap_or("proto2");
                for (schema, message_name) in [
                    (&mut request, request_type.unwrap_or_default()),
                    (&mut response, response_type.unwrap_or_default()),
                ] {
                    if let Some(object) = schema.as_object_mut() {
                        object.insert("message".to_owned(), Value::String(message_name.to_owned()));
                        object.insert("syntax".to_owned(), Value::String(syntax.to_owned()));
                    }
                }
                operations.insert(
                    operation_id,
                    serde_json::json!({ "request": request, "response": response }),
                );
            }
        }
    }
    if operations.is_empty() {
        return Err(vec![canonicalization_error(
            "protobuf_operations_missing",
            "$",
            "No verifiable Protobuf RPC operations were found.",
            "Include at least one service RPC in the FileDescriptorSet.",
        )]);
    }
    Ok(serde_json::json!({ "format": "protobuf", "version": version, "operations": operations }))
}

fn canonicalize_openapi_content(document: &Value, content: &Value) -> Option<Value> {
    let content = content.as_object()?;
    if content.len() != 1 {
        return None;
    }
    let (media_type, media) = content.iter().next()?;
    let mut primary = resolve_openapi_schema(document, media.get("schema")?, &mut BTreeSet::new());
    if let Some(object) = primary.as_object_mut() {
        object.insert(
            "x-lenso-media-type".to_owned(),
            Value::String(media_type.clone()),
        );
        Some(primary)
    } else {
        Some(serde_json::json!({
            "allOf": [primary],
            "x-lenso-media-type": media_type,
        }))
    }
}

fn canonicalize_openapi_response(document: &Value, response: &Value) -> Option<Value> {
    let mut schema = match response.get("content") {
        None => serde_json::json!({
            "type": "null",
            "x-lenso-no-content": true,
        }),
        Some(content) if content.as_object().is_some_and(serde_json::Map::is_empty) => {
            serde_json::json!({
                "type": "null",
                "x-lenso-no-content": true,
            })
        }
        Some(content) => canonicalize_openapi_content(document, content)?,
    };
    let headers = response
        .get("headers")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .map(|(name, header)| {
            (
                name.clone(),
                resolve_openapi_schema(document, header, &mut BTreeSet::new()),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    if let Some(object) = schema.as_object_mut() {
        object.insert(
            "x-lenso-response-headers".to_owned(),
            Value::Object(headers),
        );
    }
    Some(schema)
}

fn validate_openapi_references(
    document: &Value,
    value: &Value,
    path: &str,
    errors: &mut Vec<RequestResponseCanonicalizationError>,
) {
    match value {
        Value::Object(object) => {
            if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
                let resolved = reference
                    .strip_prefix('#')
                    .and_then(|pointer| document.pointer(pointer));
                if resolved.is_none() {
                    errors.push(canonicalization_error(
                        "openapi_reference_unverifiable",
                        format!("{path}.$ref"),
                        format!("OpenAPI reference `{reference}` cannot be resolved locally."),
                        "Bundle external references and fix unresolved pointers before comparing contracts.",
                    ));
                }
            }
            for (key, child) in object {
                validate_openapi_references(document, child, &format!("{path}.{key}"), errors);
            }
        }
        Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                validate_openapi_references(document, child, &format!("{path}[{index}]"), errors);
            }
        }
        _ => {}
    }
}

fn canonicalize_openapi_parameters(
    document: &Value,
    path_item: &serde_json::Map<String, Value>,
    operation: &serde_json::Map<String, Value>,
) -> Option<Value> {
    let parameters = path_item
        .get("parameters")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .chain(
            operation
                .get("parameters")
                .and_then(Value::as_array)
                .into_iter()
                .flatten(),
        )
        .collect::<Vec<_>>();
    if parameters.is_empty() {
        return Some(serde_json::json!({ "type": "object", "properties": {} }));
    }
    let mut properties = serde_json::Map::new();
    let mut required = BTreeSet::new();
    for parameter in parameters {
        let parameter = resolve_openapi_schema(document, parameter, &mut BTreeSet::new());
        let name = parameter.get("name").and_then(Value::as_str)?;
        let schema = parameter.get("schema")?;
        let mut schema = resolve_openapi_schema(document, schema, &mut BTreeSet::new());
        if let Some(object) = schema.as_object_mut() {
            for key in ["in", "style", "explode", "allowEmptyValue", "allowReserved"] {
                if let Some(value) = parameter.get(key) {
                    object.insert(format!("x-lenso-parameter-{key}"), value.clone());
                }
            }
        }
        properties.insert(name.to_owned(), schema);
        if parameter
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            required.insert(name.to_owned());
        }
    }
    Some(serde_json::json!({
        "type": "object",
        "required": required,
        "properties": properties,
    }))
}

fn resolve_openapi_schema(document: &Value, schema: &Value, seen: &mut BTreeSet<String>) -> Value {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        if !seen.insert(reference.to_owned()) {
            return serde_json::json!({ "$ref": reference, "recursive": true });
        }
        let resolved = reference
            .strip_prefix('#')
            .and_then(|pointer| document.pointer(pointer))
            .map(|value| resolve_openapi_schema(document, value, seen));
        seen.remove(reference);
        if let Some(mut resolved) = resolved {
            if let (Some(target), Some(source)) = (resolved.as_object_mut(), schema.as_object()) {
                for (key, value) in source {
                    if key != "$ref" {
                        target.insert(key.clone(), resolve_openapi_schema(document, value, seen));
                    }
                }
            }
            return resolved;
        }
    }
    match schema {
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(|value| resolve_openapi_schema(document, value, seen))
                .collect(),
        ),
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| (key.clone(), resolve_openapi_schema(document, value, seen)))
                .collect(),
        ),
        _ => schema.clone(),
    }
}

fn collect_protobuf_messages(
    package: &str,
    parent: &str,
    descriptors: &[prost_types::DescriptorProto],
    messages: &mut BTreeMap<String, Value>,
) {
    for descriptor in descriptors {
        let name = descriptor.name.as_deref().unwrap_or("Message");
        let local_name = if parent.is_empty() {
            name.to_owned()
        } else {
            format!("{parent}.{name}")
        };
        let full_name = if package.is_empty() {
            local_name.clone()
        } else {
            format!("{package}.{local_name}")
        };
        let fields = descriptor.field.iter().map(|field| {
            let field_type = field.type_name.clone().unwrap_or_else(|| format!("{:?}", field.r#type.and_then(|value| prost_types::field_descriptor_proto::Type::try_from(value).ok())));
            let label = field.label.and_then(|value| prost_types::field_descriptor_proto::Label::try_from(value).ok()).map(|label| format!("{label:?}").to_ascii_lowercase()).unwrap_or_else(|| "optional".to_owned());
            serde_json::json!({ "number": field.number.unwrap_or_default(), "name": field.name.clone().unwrap_or_default(), "jsonName": field.json_name, "type": field_type, "label": label })
        }).collect::<Vec<_>>();
        messages.insert(full_name, serde_json::json!({ "fields": fields }));
        collect_protobuf_messages(package, &local_name, &descriptor.nested_type, messages);
    }
}

fn validate_supported_protobuf_descriptor(
    descriptor: &prost_types::FileDescriptorSet,
) -> Vec<RequestResponseCanonicalizationError> {
    let mut errors = Vec::new();
    for (file_index, file) in descriptor.file.iter().enumerate() {
        if !matches!(
            file.syntax.as_deref().unwrap_or("proto2"),
            "proto2" | "proto3"
        ) {
            errors.push(unsupported_protobuf_feature(
                format!("$.file[{file_index}].syntax"),
                "Protobuf editions or unknown syntax",
            ));
        }
        if !file.enum_type.is_empty() {
            errors.push(unsupported_protobuf_feature(
                format!("$.file[{file_index}].enumType"),
                "enum definitions",
            ));
        }
        validate_supported_protobuf_messages(
            &file.message_type,
            &format!("$.file[{file_index}].messageType"),
            &mut errors,
        );
        for (service_index, service) in file.service.iter().enumerate() {
            for (method_index, method) in service.method.iter().enumerate() {
                if method.client_streaming.unwrap_or(false)
                    || method.server_streaming.unwrap_or(false)
                    || method.options.as_ref().is_some_and(|options| {
                        options.deprecated.unwrap_or(false)
                            || !options.uninterpreted_option.is_empty()
                    })
                {
                    errors.push(unsupported_protobuf_feature(
                        format!(
                            "$.file[{file_index}].service[{service_index}].method[{method_index}]"
                        ),
                        "streaming RPCs or unsupported method options",
                    ));
                }
            }
        }
    }
    errors
}

fn validate_supported_protobuf_messages(
    messages: &[prost_types::DescriptorProto],
    path: &str,
    errors: &mut Vec<RequestResponseCanonicalizationError>,
) {
    for (message_index, message) in messages.iter().enumerate() {
        let message_path = format!("{path}[{message_index}]");
        if !message.enum_type.is_empty()
            || !message.oneof_decl.is_empty()
            || message.options.is_some()
        {
            errors.push(unsupported_protobuf_feature(
                &message_path,
                "enum, oneof, map, or message options",
            ));
        }
        for (field_index, field) in message.field.iter().enumerate() {
            let is_enum = field
                .r#type
                .and_then(|value| prost_types::field_descriptor_proto::Type::try_from(value).ok())
                == Some(prost_types::field_descriptor_proto::Type::Enum);
            if is_enum
                || field.oneof_index.is_some()
                || field.proto3_optional.unwrap_or(false)
                || field.default_value.is_some()
                || field.options.is_some()
            {
                errors.push(unsupported_protobuf_feature(
                    format!("{message_path}.field[{field_index}]"),
                    "enum, oneof, proto3 optional, defaults, or field options",
                ));
            }
        }
        validate_supported_protobuf_messages(
            &message.nested_type,
            &format!("{message_path}.nestedType"),
            errors,
        );
    }
}

fn unsupported_protobuf_feature(
    path: impl Into<String>,
    feature: &str,
) -> RequestResponseCanonicalizationError {
    canonicalization_error(
        "protobuf_feature_unverifiable",
        path,
        format!("Compatibility for {feature} is not yet structurally verifiable."),
        "Review this Protobuf change manually or use only currently supported descriptor features.",
    )
}

fn expand_protobuf_message(
    name: &str,
    messages: &BTreeMap<String, Value>,
    seen: &mut BTreeSet<String>,
) -> Option<Value> {
    let canonical_name = name.trim_start_matches('.');
    let message = messages.get(canonical_name)?;
    if !seen.insert(canonical_name.to_owned()) {
        return Some(serde_json::json!({
            "message": canonical_name,
            "recursive": true,
        }));
    }
    let mut expanded = message.clone();
    if let Some(fields) = expanded.get_mut("fields").and_then(Value::as_array_mut) {
        for field in fields {
            let Some(field_type) = field.get("type").and_then(Value::as_str) else {
                continue;
            };
            if let Some(schema) = expand_protobuf_message(field_type, messages, seen) {
                field["type"] = serde_json::json!({
                    "message": field_type.trim_start_matches('.'),
                    "schema": schema,
                });
            }
        }
    }
    seen.remove(canonical_name);
    Some(expanded)
}

fn canonicalization_error(
    code: impl Into<String>,
    path: impl Into<String>,
    message: impl Into<String>,
    next_action: impl Into<String>,
) -> RequestResponseCanonicalizationError {
    RequestResponseCanonicalizationError {
        code: code.into(),
        path: path.into(),
        message: message.into(),
        next_action: next_action.into(),
    }
}

/// Compares canonical OpenAPI or Protobuf request/response operation shapes.
///
/// The input is deliberately JSON so the public library and CLI can call the same evaluator.
/// `operations` is keyed by stable operation or RPC name. OpenAPI request/response values are
/// JSON Schemas; Protobuf values contain descriptor-like `fields` arrays keyed by field number.
#[must_use]
pub fn evaluate_request_response_compatibility(
    input: &Value,
) -> RequestResponseCompatibilityResult {
    evaluate_request_response_compatibility_inner(None, input)
}

#[must_use]
pub fn evaluate_request_response_compatibility_in_system(
    system: &Value,
    input: &Value,
) -> RequestResponseCompatibilityResult {
    evaluate_request_response_compatibility_inner(Some(system), input)
}

fn evaluate_request_response_compatibility_inner(
    system: Option<&Value>,
    input: &Value,
) -> RequestResponseCompatibilityResult {
    let raw_contract_kind = input.get("contractKind").and_then(Value::as_str);
    let contract_kind = match raw_contract_kind {
        Some("provider_protocol") => RequestResponseContractKind::ProviderProtocol,
        _ => RequestResponseContractKind::ServiceContract,
    };
    let prefix = match contract_kind {
        RequestResponseContractKind::ProviderProtocol => "provider_protocol_",
        RequestResponseContractKind::ServiceContract => "service_contract_",
    };
    let mut result = RequestResponseCompatibilityResult {
        category: RequestResponseCompatibilityCategory::Safe,
        contract_kind,
        contract_id: input
            .get("contractId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        changed_version: input
            .get("changedVersion")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        producers: Vec::new(),
        consumers: Vec::new(),
        reasons: Vec::new(),
    };
    if !matches!(
        raw_contract_kind,
        Some("provider_protocol" | "service_contract")
    ) {
        compatibility_reason(
            &mut result,
            RequestResponseCompatibilityCategory::Blocked,
            "relationship_unverifiable",
            "$.contractKind",
            "The request-response contract kind is missing or unsupported.",
            "Declare `provider_protocol` or `service_contract` explicitly.",
        );
    }
    if let Some(system) = system {
        resolve_system_v2_relationships(system, input, &mut result);
    }
    if result.contract_id.is_empty()
        || result.changed_version.is_empty()
        || result.producers.is_empty()
        || result.consumers.is_empty()
    {
        compatibility_reason(
            &mut result,
            RequestResponseCompatibilityCategory::Blocked,
            "relationship_unverifiable",
            "$",
            "Contract identity, changed version, Producer, and Consumer relationships must all be verifiable.",
            "Resolve the contract and System graph references before evaluating compatibility.",
        );
    }
    let before = input.get("before");
    let after = input.get("after");
    let before_version = before
        .and_then(|value| value.get("version"))
        .and_then(Value::as_str);
    let after_version = after
        .and_then(|value| value.get("version"))
        .and_then(Value::as_str);
    if before_version.is_none()
        || after_version.is_none()
        || after_version != Some(result.changed_version.as_str())
        || before_version == after_version
    {
        compatibility_reason(
            &mut result,
            RequestResponseCompatibilityCategory::Blocked,
            &format!("{prefix}artifact_version_unverifiable"),
            "$.after.version",
            "Both artifact versions must be explicit, different, and the candidate must match changedVersion.",
            "Provide authoritative before and after artifact versions and retry.",
        );
    }
    let before_format = before
        .and_then(|value| value.get("format"))
        .and_then(Value::as_str);
    let after_format = after
        .and_then(|value| value.get("format"))
        .and_then(Value::as_str);
    if before_format != after_format || !matches!(before_format, Some("openapi" | "protobuf")) {
        compatibility_reason(
            &mut result,
            RequestResponseCompatibilityCategory::Blocked,
            &format!("{prefix}artifact_unverifiable"),
            "$.before.format",
            "Both artifacts must use the same supported request-response format.",
            "Provide canonical OpenAPI or Protobuf artifacts in the same format.",
        );
    } else if let (Some(before_operations), Some(after_operations)) = (
        before
            .and_then(|value| value.get("operations"))
            .and_then(Value::as_object),
        after
            .and_then(|value| value.get("operations"))
            .and_then(Value::as_object),
    ) {
        for (operation, old) in before_operations {
            let Some(new) = after_operations.get(operation) else {
                compatibility_reason(
                    &mut result,
                    RequestResponseCompatibilityCategory::Breaking,
                    &format!("{prefix}operation_removed"),
                    &format!("$.after.operations.{operation}"),
                    "A request-response operation was removed.",
                    "Restore the operation or coordinate a new contract version with every Consumer.",
                );
                continue;
            };
            match before_format {
                Some("openapi") => {
                    compare_openapi_operation(&mut result, prefix, operation, old, new)
                }
                Some("protobuf") => {
                    compare_protobuf_operation(&mut result, prefix, operation, old, new)
                }
                _ => {}
            }
        }
    } else {
        compatibility_reason(
            &mut result,
            RequestResponseCompatibilityCategory::Blocked,
            &format!("{prefix}artifact_unverifiable"),
            "$.before.operations",
            "Both artifacts must expose canonical operations.",
            "Generate canonical operation shapes from both artifacts and retry.",
        );
    }
    if result.reasons.is_empty() {
        compatibility_reason(
            &mut result,
            RequestResponseCompatibilityCategory::Safe,
            &format!("{prefix}backward_compatible"),
            "$",
            "All known Producer and Consumer request-response relationships remain compatible.",
            "Publish the changed version and monitor known Consumers.",
        );
    }
    result.reasons.sort();
    result.reasons.dedup();
    result
}

fn resolve_system_v2_relationships(
    system: &Value,
    input: &Value,
    result: &mut RequestResponseCompatibilityResult,
) {
    if system_v2_graph(system).is_err() {
        return;
    }
    let Some(contract) = system
        .get("contracts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|contract| {
            contract.get("contractId").and_then(Value::as_str) == Some(&result.contract_id)
        })
    else {
        return;
    };
    let Some(producer_kind) = contract.get("producerKind").and_then(Value::as_str) else {
        return;
    };
    let Some(producer_id) = contract.get("producerId").and_then(Value::as_str) else {
        return;
    };
    let resolved_kind = if producer_kind == "provider" {
        RequestResponseContractKind::ProviderProtocol
    } else {
        RequestResponseContractKind::ServiceContract
    };
    let graph_version = contract.get("version").and_then(Value::as_str);
    let graph_format = contract
        .get("artifact")
        .and_then(|artifact| artifact.get("format"))
        .and_then(Value::as_str);
    let baseline_version = input
        .get("before")
        .and_then(|artifact| artifact.get("version"))
        .and_then(Value::as_str);
    let baseline_format = input
        .get("before")
        .and_then(|artifact| artifact.get("format"))
        .and_then(Value::as_str);
    if resolved_kind != result.contract_kind
        || graph_version != baseline_version
        || graph_format != baseline_format
    {
        compatibility_reason(
            result,
            RequestResponseCompatibilityCategory::Blocked,
            "relationship_unverifiable",
            "$.before",
            "The declared contract kind, baseline version, or format does not match the resolved System graph contract.",
            "Use the contract kind, version, and artifact format declared by the authoritative System graph.",
        );
        return;
    }
    result.producers = vec![format!("{producer_kind}:{producer_id}")];
    result.consumers = system
        .get("consumers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|consumer| {
            consumer.get("contractId").and_then(Value::as_str) == Some(&result.contract_id)
        })
        .filter_map(|consumer| {
            Some(format!(
                "{}:{}",
                consumer.get("ownerKind")?.as_str()?,
                consumer.get("ownerId")?.as_str()?
            ))
        })
        .collect();
    result.consumers.sort();
    result.consumers.dedup();
}

fn compare_openapi_operation(
    result: &mut RequestResponseCompatibilityResult,
    prefix: &str,
    operation: &str,
    old: &Value,
    new: &Value,
) {
    if old.get("method") != new.get("method") || old.get("path") != new.get("path") {
        compatibility_reason(
            result,
            RequestResponseCompatibilityCategory::Breaking,
            &format!("{prefix}operation_transport_changed"),
            &format!("$.after.operations.{operation}"),
            "The HTTP method or path for an existing operation changed.",
            "Restore the previous method and path or coordinate a new contract version with every Consumer.",
        );
    }
    compare_json_schema(
        result,
        prefix,
        operation,
        "request",
        old.get("request"),
        new.get("request"),
        true,
    );
    compare_json_schema(
        result,
        prefix,
        operation,
        "response",
        old.get("response"),
        new.get("response"),
        false,
    );
}

fn compare_json_schema(
    result: &mut RequestResponseCompatibilityResult,
    prefix: &str,
    operation: &str,
    direction: &str,
    old: Option<&Value>,
    new: Option<&Value>,
    request: bool,
) {
    let path = format!("$.after.operations.{operation}.{direction}");
    let (Some(old), Some(new)) = (old, new) else {
        compatibility_reason(
            result,
            RequestResponseCompatibilityCategory::Blocked,
            &format!("{prefix}{direction}_schema_unverifiable"),
            &path,
            "The request or response schema is missing.",
            "Provide both canonical schemas before evaluating compatibility.",
        );
        return;
    };
    if old.get("type") != new.get("type") {
        compatibility_reason(
            result,
            RequestResponseCompatibilityCategory::Breaking,
            &format!("{prefix}{direction}_type_changed"),
            &path,
            "The request or response type changed.",
            "Restore the previous type or coordinate a new contract version with affected Consumers.",
        );
    }
    if request
        && !old
            .get("x-lenso-request-body-required")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && new
            .get("x-lenso-request-body-required")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        compatibility_reason(
            result,
            RequestResponseCompatibilityCategory::Breaking,
            &format!("{prefix}request_body_became_required"),
            &path,
            "An optional request body became required.",
            "Keep the request body optional or coordinate the change with every Consumer.",
        );
    }
    let old_required = string_set(old.get("required"));
    let new_required = string_set(new.get("required"));
    let old_properties = old.get("properties").and_then(Value::as_object);
    let new_properties = new.get("properties").and_then(Value::as_object);
    if let (Some(old_properties), Some(new_properties)) = (old_properties, new_properties) {
        if request {
            for field in new_required.difference(&old_required) {
                compatibility_reason(
                    result,
                    RequestResponseCompatibilityCategory::Breaking,
                    &format!("{prefix}request_required_field_added"),
                    &format!("{path}.properties.{field}"),
                    "A new required request field rejects requests from existing Consumers.",
                    "Make the field optional or coordinate the required request change with every Consumer.",
                );
            }
        } else {
            for field in old_required.difference(&new_required) {
                compatibility_reason(
                    result,
                    RequestResponseCompatibilityCategory::Breaking,
                    &format!("{prefix}response_required_field_became_optional"),
                    &format!("{path}.properties.{field}"),
                    "A response field required by existing Consumers may now be omitted.",
                    "Keep the response field required or coordinate a new contract version with affected Consumers.",
                );
            }
            for field in old_properties
                .keys()
                .filter(|field| !new_properties.contains_key(*field))
            {
                compatibility_reason(
                    result,
                    RequestResponseCompatibilityCategory::Breaking,
                    &format!("{prefix}response_field_removed"),
                    &format!("{path}.properties.{field}"),
                    "A response field used by existing Consumers was removed.",
                    "Restore the response field or coordinate a new contract version with affected Consumers.",
                );
            }
        }
        for (field, old_field) in old_properties {
            if let Some(new_field) = new_properties.get(field) {
                if old_field.get("type") != new_field.get("type") {
                    compatibility_reason(
                        result,
                        RequestResponseCompatibilityCategory::Breaking,
                        &format!("{prefix}{direction}_field_type_changed"),
                        &format!("{path}.properties.{field}"),
                        "A field type changed.",
                        "Restore the previous field type or introduce a new field and version.",
                    );
                } else if old_field != new_field {
                    compatibility_reason(
                        result,
                        RequestResponseCompatibilityCategory::NeedsAttention,
                        &format!("{prefix}{direction}_field_constraints_changed"),
                        &format!("{path}.properties.{field}"),
                        "A field constraint changed and compatibility cannot be proven structurally.",
                        "Review enum, range, format, and composition constraints with affected owners.",
                    );
                }
            }
        }
        let old_constraints = schema_constraints(old);
        let new_constraints = schema_constraints(new);
        if old_constraints != new_constraints {
            compatibility_reason(
                result,
                RequestResponseCompatibilityCategory::NeedsAttention,
                &format!("{prefix}{direction}_constraints_changed"),
                &path,
                "Schema-level constraints changed and require semantic review.",
                "Review the changed constraints with affected Producer and Consumer owners.",
            );
        }
    } else if old != new {
        compatibility_reason(
            result,
            RequestResponseCompatibilityCategory::NeedsAttention,
            &format!("{prefix}{direction}_schema_not_structurally_proven"),
            &path,
            "The schema change cannot be proven compatible from object properties.",
            "Review the schema constraints with affected Producer and Consumer owners.",
        );
    }
}

fn compare_protobuf_operation(
    result: &mut RequestResponseCompatibilityResult,
    prefix: &str,
    operation: &str,
    old: &Value,
    new: &Value,
) {
    for direction in ["request", "response"] {
        let old_message = old.get(direction);
        let new_message = new.get(direction);
        for attribute in ["message", "syntax"] {
            if old_message.and_then(|value| value.get(attribute))
                != new_message.and_then(|value| value.get(attribute))
            {
                compatibility_reason(
                    result,
                    RequestResponseCompatibilityCategory::NeedsAttention,
                    &format!("{prefix}protobuf_{attribute}_changed"),
                    &format!("$.after.operations.{operation}.{direction}.{attribute}"),
                    "Protobuf message identity or language semantics changed.",
                    "Review generated clients and presence/default semantics with affected owners.",
                );
            }
        }
        let path = format!("$.after.operations.{operation}.{direction}.fields");
        let old_fields = protobuf_fields(old.get(direction));
        let new_fields = protobuf_fields(new.get(direction));
        let (Some(old_fields), Some(new_fields)) = (old_fields, new_fields) else {
            compatibility_reason(
                result,
                RequestResponseCompatibilityCategory::Blocked,
                &format!("{prefix}protobuf_descriptor_unverifiable"),
                &path,
                "The Protobuf field descriptors are missing or invalid.",
                "Generate descriptor-based canonical fields for both contract versions.",
            );
            continue;
        };
        for (number, old_field) in &old_fields {
            let Some(new_field) = new_fields.get(number) else {
                if direction == "response" {
                    compatibility_reason(
                        result,
                        RequestResponseCompatibilityCategory::Breaking,
                        &format!("{prefix}protobuf_response_field_removed"),
                        &format!("{path}.{number}"),
                        "A response field number was removed.",
                        "Restore or reserve the field and coordinate a new response contract version.",
                    );
                } else {
                    compatibility_reason(
                        result,
                        RequestResponseCompatibilityCategory::NeedsAttention,
                        &format!("{prefix}protobuf_request_field_removed"),
                        &format!("{path}.{number}"),
                        "A request field number was removed and its semantic handling cannot be proven.",
                        "Reserve the removed number and confirm all Producers tolerate the old wire field.",
                    );
                }
                continue;
            };
            if old_field.get("type") != new_field.get("type") {
                compatibility_reason(
                    result,
                    RequestResponseCompatibilityCategory::Breaking,
                    &format!("{prefix}protobuf_field_type_changed"),
                    &format!("{path}.{number}"),
                    "A Protobuf field number changed wire type.",
                    "Restore the wire-compatible type or allocate a new field number.",
                );
            }
            if old_field.get("name") != new_field.get("name") {
                compatibility_reason(
                    result,
                    RequestResponseCompatibilityCategory::NeedsAttention,
                    &format!("{prefix}protobuf_field_renamed"),
                    &format!("{path}.{number}"),
                    "A Protobuf field kept its number but changed source name.",
                    "Review generated clients and JSON mappings before publishing.",
                );
            }
            if old_field.get("label") != new_field.get("label") {
                let category = if new_field.get("label").and_then(Value::as_str) == Some("required")
                {
                    RequestResponseCompatibilityCategory::Breaking
                } else {
                    RequestResponseCompatibilityCategory::NeedsAttention
                };
                compatibility_reason(
                    result,
                    category,
                    &format!("{prefix}protobuf_field_label_changed"),
                    &format!("{path}.{number}"),
                    "A Protobuf field cardinality changed.",
                    "Restore the previous label or coordinate the cardinality change with affected owners.",
                );
            }
            if old_field.get("jsonName") != new_field.get("jsonName") {
                compatibility_reason(
                    result,
                    RequestResponseCompatibilityCategory::NeedsAttention,
                    &format!("{prefix}protobuf_json_name_changed"),
                    &format!("{path}.{number}"),
                    "A Protobuf field JSON name changed.",
                    "Review JSON-transcoded Consumers before publishing the changed contract.",
                );
            }
        }
        for (number, new_field) in &new_fields {
            if !old_fields.contains_key(number)
                && new_field.get("label").and_then(Value::as_str) == Some("required")
            {
                compatibility_reason(
                    result,
                    RequestResponseCompatibilityCategory::Breaking,
                    &format!("{prefix}protobuf_required_field_added"),
                    &format!("{path}.{number}"),
                    "A required Protobuf field was added.",
                    "Make the field optional or coordinate a new contract version with every affected owner.",
                );
            }
        }
    }
}

fn compatibility_reason(
    result: &mut RequestResponseCompatibilityResult,
    category: RequestResponseCompatibilityCategory,
    code: &str,
    path: &str,
    message: &str,
    next_action: &str,
) {
    result.category = result.category.max(category);
    result.reasons.push(RequestResponseCompatibilityReason {
        code: code.to_owned(),
        path: path.to_owned(),
        message: message.to_owned(),
        next_action: next_action.to_owned(),
    });
}

fn string_set(value: Option<&Value>) -> BTreeSet<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn schema_constraints(schema: &Value) -> BTreeMap<String, Value> {
    schema
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(key, _)| {
            !matches!(
                key.as_str(),
                "type"
                    | "properties"
                    | "required"
                    | "description"
                    | "default"
                    | "title"
                    | "examples"
            )
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn protobuf_fields(value: Option<&Value>) -> Option<BTreeMap<u64, Value>> {
    let fields = value?.get("fields")?.as_array()?;
    fields
        .iter()
        .map(|field| Some((field.get("number")?.as_u64()?, field.clone())))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoryContext {
    pub story_id: String,
    pub segment_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceContext {
    pub traceparent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracestate: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub baggage: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextClaimProof {
    pub verification_method: String,
    pub algorithm: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServicePrincipal {
    pub issuer: String,
    pub subject: String,
    pub audiences: Vec<String>,
    pub expires_at_unix_ms: u64,
    pub credential_id: String,
    pub proof: ContextClaimProof,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DelegatedActorContext {
    pub issuer: String,
    pub subject: String,
    pub audiences: Vec<String>,
    pub intent: String,
    pub permissions: Vec<String>,
    pub expires_at_unix_ms: u64,
    pub delegation_id: String,
    pub proof: ContextClaimProof,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TenantContext {
    pub issuer: String,
    pub tenant_id: String,
    pub actor_subject: String,
    pub delegation_id: String,
    pub audiences: Vec<String>,
    pub expires_at_unix_ms: u64,
    pub claim_id: String,
    pub proof: ContextClaimProof,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeadlineContext {
    pub expires_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdempotencyKeyContext {
    pub value: String,
    pub scope: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CausationContext {
    pub causation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegionContext {
    pub operating_region: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_domain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommonContextContract {
    pub protocol: String,
    pub story: StoryContext,
    pub trace: TraceContext,
    pub service_principal: ServicePrincipal,
    pub delegated_actor: DelegatedActorContext,
    pub tenant: TenantContext,
    pub deadline: DeadlineContext,
    pub idempotency_key: IdempotencyKeyContext,
    pub causation: CausationContext,
    pub region: RegionContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommonContextIssueCode {
    InvalidProtocol,
    InvalidStoryContext,
    InvalidTraceContext,
    InvalidServicePrincipal,
    InvalidDelegatedActorContext,
    InvalidTenantContext,
    InvalidDeadline,
    InvalidIdempotencyKey,
    InvalidCausation,
    InvalidRegion,
    UntrustedActorClaim,
    UntrustedTenantClaim,
    AudienceMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommonContextIssue {
    pub code: CommonContextIssueCode,
    pub path: String,
    pub message: String,
    pub next_action: String,
}

#[must_use]
pub fn validate_common_context_contract(
    contract: &CommonContextContract,
) -> Vec<CommonContextIssue> {
    validate_common_context_contract_value(
        &serde_json::to_value(contract).expect("CommonContextContract must serialize"),
    )
}

#[must_use]
pub fn validate_common_context_contract_for_audience(
    contract: &CommonContextContract,
    expected_audience: &str,
) -> Vec<CommonContextIssue> {
    let mut issues = validate_common_context_contract(contract);
    for (field, audiences) in [
        ("servicePrincipal", &contract.service_principal.audiences),
        ("delegatedActor", &contract.delegated_actor.audiences),
        ("tenant", &contract.tenant.audiences),
    ] {
        if !audiences
            .iter()
            .any(|audience| audience == expected_audience)
        {
            push_common_context_issue(
                &mut issues,
                CommonContextIssueCode::AudienceMismatch,
                format!("$.{field}.audiences"),
                format!("claim is not intended for audience `{expected_audience}`"),
                "Reject the context or obtain a claim issued for this receiving audience.",
            );
        }
    }
    issues
}

#[must_use]
pub fn validate_common_context_contract_value(value: &Value) -> Vec<CommonContextIssue> {
    let mut issues = Vec::new();
    if value.get("protocol").and_then(Value::as_str) != Some(COMMON_CONTEXT_PROTOCOL) {
        push_common_context_issue(
            &mut issues,
            CommonContextIssueCode::InvalidProtocol,
            "$.protocol",
            "protocol must be `lenso.context.v1`",
            "Set `protocol` to `lenso.context.v1`.",
        );
    }
    validate_required_strings(
        value,
        "story",
        &["storyId", "segmentId"],
        CommonContextIssueCode::InvalidStoryContext,
        &mut issues,
    );
    validate_required_strings(
        value,
        "trace",
        &["traceparent"],
        CommonContextIssueCode::InvalidTraceContext,
        &mut issues,
    );
    validate_verifiable_claim(
        value,
        "servicePrincipal",
        "subject",
        "credentialId",
        CommonContextIssueCode::InvalidServicePrincipal,
        &mut issues,
    );
    validate_required_strings(
        value,
        "delegatedActor",
        &["intent"],
        CommonContextIssueCode::InvalidDelegatedActorContext,
        &mut issues,
    );
    validate_verifiable_claim(
        value,
        "delegatedActor",
        "subject",
        "delegationId",
        CommonContextIssueCode::InvalidDelegatedActorContext,
        &mut issues,
    );
    if value
        .pointer("/delegatedActor/permissions")
        .and_then(Value::as_array)
        .is_none_or(|items| {
            items.is_empty()
                || items
                    .iter()
                    .any(|item| item.as_str().is_none_or(|text| text.trim().is_empty()))
        })
    {
        push_common_context_issue(
            &mut issues,
            CommonContextIssueCode::InvalidDelegatedActorContext,
            "$.delegatedActor.permissions",
            "permissions must contain non-empty delegated permissions",
            "Declare at least one permission narrowed for this delegation.",
        );
    }
    validate_verifiable_claim(
        value,
        "tenant",
        "tenantId",
        "claimId",
        CommonContextIssueCode::InvalidTenantContext,
        &mut issues,
    );
    validate_required_strings(
        value,
        "tenant",
        &["actorSubject", "delegationId"],
        CommonContextIssueCode::InvalidTenantContext,
        &mut issues,
    );
    validate_positive_integer(
        value,
        "deadline",
        "expiresAtUnixMs",
        CommonContextIssueCode::InvalidDeadline,
        &mut issues,
    );
    validate_required_strings(
        value,
        "idempotencyKey",
        &["value", "scope"],
        CommonContextIssueCode::InvalidIdempotencyKey,
        &mut issues,
    );
    validate_required_strings(
        value,
        "causation",
        &["causationId"],
        CommonContextIssueCode::InvalidCausation,
        &mut issues,
    );
    validate_required_strings(
        value,
        "region",
        &["operatingRegion"],
        CommonContextIssueCode::InvalidRegion,
        &mut issues,
    );

    if let Some(baggage) = value.pointer("/trace/baggage").and_then(Value::as_object) {
        let mut keys = baggage.keys().collect::<Vec<_>>();
        keys.sort();
        for key in keys {
            let normalized = key.to_ascii_lowercase();
            let words = normalized
                .split(|character: char| !character.is_ascii_alphanumeric())
                .collect::<Vec<_>>();
            let compact = words.join("");
            let (code, claim) = if words.contains(&"tenant") || compact.starts_with("tenant") {
                (
                    CommonContextIssueCode::UntrustedTenantClaim,
                    "tenant authorization",
                )
            } else if words.iter().any(|word| {
                matches!(
                    *word,
                    "actor"
                        | "auth"
                        | "user"
                        | "enduser"
                        | "permission"
                        | "permissions"
                        | "role"
                        | "delegation"
                        | "subject"
                        | "audience"
                )
            }) || [
                "actor",
                "authz",
                "userrole",
                "enduserid",
                "permission",
                "delegatedactor",
                "subject",
                "audience",
            ]
            .iter()
            .any(|prefix| compact.starts_with(prefix))
            {
                (
                    CommonContextIssueCode::UntrustedActorClaim,
                    "actor authorization",
                )
            } else {
                continue;
            };
            push_common_context_issue(
                &mut issues,
                code,
                format!("$.trace.baggage.{key}"),
                format!("OpenTelemetry Baggage must not supply {claim} claims"),
                "Remove the Baggage entry and use the signed, audience-bounded context claim.",
            );
        }
    }
    issues
}

fn validate_verifiable_claim(
    value: &Value,
    field: &str,
    subject: &str,
    claim_id: &str,
    code: CommonContextIssueCode,
    issues: &mut Vec<CommonContextIssue>,
) {
    validate_required_strings(value, field, &["issuer", subject, claim_id], code, issues);
    validate_required_strings(
        value,
        &format!("{field}/proof"),
        &["verificationMethod", "algorithm", "signature"],
        code,
        issues,
    );
    let path = format!("/{field}/audiences");
    if value
        .pointer(&path)
        .and_then(Value::as_array)
        .is_none_or(|items| {
            items.is_empty()
                || items
                    .iter()
                    .any(|item| item.as_str().is_none_or(|text| text.trim().is_empty()))
        })
    {
        push_common_context_issue(
            issues,
            code,
            format!("$.{field}.audiences"),
            "audiences must contain non-empty audience identifiers",
            "Declare at least one intended receiving Service or Workload audience.",
        );
    }
    validate_positive_integer(value, field, "expiresAtUnixMs", code, issues);
}

fn validate_required_strings(
    value: &Value,
    field: &str,
    names: &[&str],
    code: CommonContextIssueCode,
    issues: &mut Vec<CommonContextIssue>,
) {
    let json_path = field.replace('/', ".");
    for name in names {
        let pointer = format!("/{field}/{name}");
        if value
            .pointer(&pointer)
            .and_then(Value::as_str)
            .is_none_or(|text| text.trim().is_empty())
        {
            push_common_context_issue(
                issues,
                code,
                format!("$.{json_path}.{name}"),
                format!("{name} must be a non-empty string"),
                format!("Set a non-empty `{name}` value."),
            );
        }
    }
}

fn validate_positive_integer(
    value: &Value,
    field: &str,
    name: &str,
    code: CommonContextIssueCode,
    issues: &mut Vec<CommonContextIssue>,
) {
    let pointer = format!("/{field}/{name}");
    if value
        .pointer(&pointer)
        .and_then(Value::as_u64)
        .is_none_or(|number| number == 0)
    {
        push_common_context_issue(
            issues,
            code,
            format!("$.{field}.{name}"),
            format!("{name} must be a positive integer"),
            format!("Set `{name}` to an absolute Unix timestamp in milliseconds."),
        );
    }
}

fn push_common_context_issue(
    issues: &mut Vec<CommonContextIssue>,
    code: CommonContextIssueCode,
    path: impl Into<String>,
    message: impl Into<String>,
    next_action: impl Into<String>,
) {
    issues.push(CommonContextIssue {
        code,
        path: path.into(),
        message: message.into(),
        next_action: next_action.into(),
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractArtifactKind {
    Service,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractSemanticKind {
    Provider,
    ProviderSystem,
    AutonomousService,
    MixedSystem,
}

impl ContractSemanticKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Provider => "provider",
            Self::ProviderSystem => "provider_system",
            Self::AutonomousService => "autonomous_service",
            Self::MixedSystem => "mixed_system",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractOwner {
    Host,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSemantics {
    pub providers: Vec<String>,
    pub auth_owner: ContractOwner,
    pub proxy_policy_owner: ContractOwner,
    pub retry_owner: ContractOwner,
    pub runtime_queue_owner: ContractOwner,
    pub outbox_owner: ContractOwner,
    pub story_owner: ContractOwner,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractArtifactCheck {
    pub detected_protocol: String,
    pub artifact_kind: ContractArtifactKind,
    pub semantic_kind: ContractSemanticKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_semantics: Option<ProviderSemantics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub autonomous_service: Option<AutonomousServiceSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutonomousServiceSummary {
    pub service_id: String,
    pub workloads: Vec<String>,
    pub modules: Vec<String>,
    pub service_contracts: Vec<String>,
    pub event_contracts: Vec<String>,
    pub has_config_contract: bool,
    pub has_reliability_contract: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractArtifactCheckErrorCode {
    AmbiguousProtocol,
    UnsupportedProtocol,
    InvalidArtifact,
    UnknownField,
    InvalidProtocol,
    InvalidVersion,
    InvalidServiceIdentity,
    InvalidWorkloadIdentity,
    WorkloadOwnerMismatch,
    DuplicateWorkloadIdentity,
    InvalidWorkloadRole,
    InvalidModuleIdentity,
    DuplicateModuleIdentity,
    InvalidStoreIdentity,
    StoreOwnerMismatch,
    DuplicateStoreIdentity,
    InvalidTenancyMode,
    InvalidOperatingRegion,
    DuplicateOperatingRegion,
    InvalidContractIdentity,
    DuplicateContractIdentity,
    UnresolvedModuleReference,
    InvalidArtifactReference,
    UnresolvedArtifactReference,
    UnsupportedArtifactFormat,
    InvalidConfigContract,
    DuplicateConfigField,
    InvalidReliabilityContract,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractArtifactCheckError {
    pub code: ContractArtifactCheckErrorCode,
    pub path: String,
    pub message: String,
    pub next_action: String,
}

impl std::fmt::Display for ContractArtifactCheckError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let payload = serde_json::to_string(self).map_err(|_| std::fmt::Error)?;
        formatter.write_str(&payload)
    }
}

impl std::error::Error for ContractArtifactCheckError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContractFixture {
    pub name: &'static str,
    pub protocol: &'static str,
    pub semantic_kind: ContractSemanticKind,
    pub json: &'static str,
}

pub const LEGACY_CONTRACT_FIXTURES: &[ContractFixture] = &[
    ContractFixture {
        name: "service-provider-v1",
        protocol: SERVICE_CONTRACT_PROTOCOL,
        semantic_kind: ContractSemanticKind::Provider,
        json: LEGACY_SERVICE_V1_FIXTURE_JSON,
    },
    ContractFixture {
        name: "system-provider-v1",
        protocol: SERVICE_SYSTEM_PROTOCOL,
        semantic_kind: ContractSemanticKind::ProviderSystem,
        json: LEGACY_SYSTEM_V1_FIXTURE_JSON,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceDeploymentTarget {
    Kubernetes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceEnvironmentsFile {
    pub version: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environments: Vec<ServiceEnvironment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceEnvironment {
    pub name: String,
    pub service_name: String,
    pub target: ServiceDeploymentTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kube_context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_reference: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_track: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<KubernetesDeploymentConfig>,
}

impl ServiceEnvironment {
    #[must_use]
    pub fn kubernetes(name: impl Into<String>, service_name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            service_name: service_name.into(),
            target: ServiceDeploymentTarget::Kubernetes,
            namespace: None,
            kube_context: None,
            image: None,
            public_base_url: None,
            manifest_reference: None,
            release_track: None,
            config: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KubernetesDeploymentConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicas: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingress_host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_request: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_request: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_limit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_limit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autoscaling: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disruption_budget: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_policy: Option<bool>,
}

impl KubernetesDeploymentConfig {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    #[must_use]
    pub fn replicas(mut self, replicas: u32) -> Self {
        self.replicas = Some(replicas);
        self
    }

    #[must_use]
    pub fn ingress_host(mut self, ingress_host: impl Into<String>) -> Self {
        self.ingress_host = Some(ingress_host.into());
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceDeploymentState {
    Ready,
    Progressing,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceDeploymentDrift {
    InSync,
    HostAhead,
    ClusterAhead,
    ImageDrift,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceDeploymentsFile {
    pub version: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observations: Vec<ServiceDeploymentObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceDeploymentObservation {
    pub service_name: String,
    pub environment: String,
    pub target: ServiceDeploymentTarget,
    pub observed_at_unix_ms: u64,
    pub state: ServiceDeploymentState,
    pub drift: ServiceDeploymentDrift,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster: Option<KubernetesDeploymentObservation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<ServiceDeploymentHostObservation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<ServiceDeploymentCheck>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_action: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KubernetesDeploymentObservation {
    pub namespace: String,
    pub deployment: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ready_replicas: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desired_replicas: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_replicas: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_reference: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingress_host: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceDeploymentHostObservation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceDeploymentCheck {
    pub name: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceHealth {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ready_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liveness_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceProvider {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceCompatibility {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_protocol_version: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_host_features: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sdk_language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sdk_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceConfigField {
    pub key: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<Value>,
    #[serde(default)]
    pub secret: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceEnvField {
    pub name: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub example: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceLocalProcess {
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default = "default_service_auto_start")]
    pub auto_start: bool,
    #[serde(default = "default_service_ready_timeout_ms")]
    pub ready_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceWorkspace {
    pub protocol: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub services: Vec<ServiceWorkspaceService>,
}

impl ServiceWorkspace {
    #[must_use]
    pub fn new(services: Vec<ServiceWorkspaceService>) -> Self {
        Self {
            protocol: SERVICE_WORKSPACE_PROTOCOL.to_owned(),
            services,
        }
    }
}

#[must_use]
pub fn service_workspace_to_module_services(
    workspace: &ServiceWorkspace,
) -> ServiceWorkspaceModuleServicesFile {
    ServiceWorkspaceModuleServicesFile {
        version: 1,
        modules: workspace
            .services
            .iter()
            .map(|service| ServiceWorkspaceModuleServices {
                module_name: service.name.clone(),
                services: vec![ServiceWorkspaceProcess {
                    name: service.name.clone(),
                    command: service.command.clone(),
                    cwd: service.cwd.clone(),
                    ready_url: service.ready_url.clone(),
                    auto_start: service.auto_start,
                    ready_timeout_ms: service.ready_timeout_ms,
                }],
            })
            .collect(),
    }
}

#[must_use]
pub fn service_workspace_base_url(service: &ServiceWorkspaceService) -> Option<String> {
    service_base_url_from_ready_url(&service.ready_url)
        .or_else(|| service_base_url_from_manifest_url(&service.manifest))
}

#[must_use]
pub fn service_base_url_from_ready_url(ready_url: &str) -> Option<String> {
    service_base_url_from_url_suffix(ready_url, &["/status", "/ready", "/health", "/healthz"])
}

#[must_use]
pub fn service_base_url_from_manifest_url(manifest_url: &str) -> Option<String> {
    service_base_url_from_url_suffix(manifest_url, &["/manifest"])
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceWorkspaceService {
    pub name: String,
    pub lang: String,
    pub cwd: String,
    #[serde(default = "default_service_manifest")]
    pub manifest: String,
    pub command: String,
    pub ready_url: String,
    #[serde(default = "default_service_auto_start")]
    pub auto_start: bool,
    #[serde(default = "default_workspace_service_ready_timeout_ms")]
    pub ready_timeout_ms: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceWorkspaceProcess {
    pub name: String,
    pub command: String,
    pub cwd: String,
    pub ready_url: String,
    pub auto_start: bool,
    pub ready_timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceWorkspaceModuleServices {
    pub module_name: String,
    pub services: Vec<ServiceWorkspaceProcess>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceWorkspaceModuleServicesFile {
    pub version: u64,
    pub modules: Vec<ServiceWorkspaceModuleServices>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemV2Graph {
    pub artifact_protocol: String,
    pub semantic_kind: ContractSemanticKind,
    pub system_id: String,
    pub nodes: Vec<SystemV2GraphNode>,
    pub relationships: Vec<SystemV2GraphRelationship>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<SystemV2Issue>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemV2GraphNode {
    pub id: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemV2GraphRelationship {
    pub kind: String,
    pub from: String,
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemV2Issue {
    pub code: String,
    pub path: String,
    pub message: String,
    pub next_action: String,
}

/// Validates and canonicalizes a declarative mixed-topology System v2 artifact.
///
/// This projection is intentionally control-plane-only: it contains no endpoint resolution or
/// runtime dispatch behavior.
pub fn system_v2_graph(value: &Value) -> Result<SystemV2Graph, Vec<SystemV2Issue>> {
    let mut issues = Vec::new();
    let Some(object) = value.as_object() else {
        return Err(vec![system_v2_issue(
            "ambiguous_kind",
            "$",
            "System artifact must be an object.",
            "Provide a lenso.system.v2 JSON object with explicit topology kinds.",
        )]);
    };
    if object.get("protocol").and_then(Value::as_str) != Some(SYSTEM_V2_PROTOCOL) {
        issues.push(system_v2_issue(
            "unsupported_protocol",
            "$.protocol",
            "protocol must be `lenso.system.v2`",
            "Set protocol to `lenso.system.v2` or use the System v1 compatibility adapter.",
        ));
    }
    let system_id = required_system_v2_string(object.get("systemId"), "$.systemId", &mut issues);
    let mut nodes = Vec::new();
    let mut relationships = Vec::new();
    let mut owners = BTreeSet::new();
    let mut identity_kinds = BTreeMap::<String, BTreeSet<String>>::new();
    let mut module_owners = BTreeMap::<String, String>::new();

    if let Some(host) = object.get("host").and_then(Value::as_object) {
        let id = required_system_v2_string(host.get("hostId"), "$.host.hostId", &mut issues);
        if !id.is_empty() {
            owners.insert(("host".to_owned(), id.clone()));
            identity_kinds
                .entry(id.clone())
                .or_default()
                .insert("host".to_owned());
            nodes.push(SystemV2GraphNode {
                id: id.clone(),
                kind: "host".to_owned(),
                owner: None,
            });
            collect_system_v2_modules(
                host.get("modules"),
                "$.host.modules",
                &id,
                &mut module_owners,
                &mut nodes,
                &mut relationships,
                &mut issues,
            );
        }
    } else {
        issues.push(system_v2_issue(
            "missing_ownership",
            "$.host",
            "System v2 requires one explicit Host.",
            "Declare host.hostId and the Modules owned by the Host.",
        ));
    }
    let providers = system_v2_sorted_objects(
        object.get("providers"),
        "$.providers",
        "providerId",
        &mut issues,
    );
    let autonomous_services = system_v2_sorted_objects(
        object.get("autonomousServices"),
        "$.autonomousServices",
        "serviceId",
        &mut issues,
    );
    collect_system_v2_owners(
        &providers,
        "providers",
        "providerId",
        "provider",
        &mut owners,
        &mut identity_kinds,
        &mut module_owners,
        &mut nodes,
        &mut relationships,
        &mut issues,
    );
    collect_system_v2_owners(
        &autonomous_services,
        "autonomousServices",
        "serviceId",
        "autonomous_service",
        &mut owners,
        &mut identity_kinds,
        &mut module_owners,
        &mut nodes,
        &mut relationships,
        &mut issues,
    );

    for (index, service) in autonomous_services.iter().enumerate() {
        let owner = service
            .get("serviceId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let workloads = system_v2_sorted_objects(
            service.get("workloads"),
            &format!("$.autonomousServices[{index}].workloads"),
            "workloadId",
            &mut issues,
        );
        if workloads.is_empty() {
            issues.push(system_v2_issue(
                "missing_ownership",
                format!("$.autonomousServices[{index}].workloads"),
                "Autonomous Service workloads must be explicit.",
                "Declare every Workload under its owning Autonomous Service.",
            ));
        } else {
            for (workload_index, workload) in workloads.iter().enumerate() {
                let id = workload
                    .get("workloadId")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if id.is_empty() {
                    issues.push(system_v2_issue(
                        "missing_ownership",
                        format!(
                            "$.autonomousServices[{index}].workloads[{workload_index}].workloadId"
                        ),
                        "Workload identity is required.",
                        "Declare workloadId under its owning Autonomous Service.",
                    ));
                } else {
                    nodes.push(SystemV2GraphNode {
                        id: id.to_owned(),
                        kind: "workload".to_owned(),
                        owner: Some(owner.to_owned()),
                    });
                    relationships.push(SystemV2GraphRelationship {
                        kind: "owns".to_owned(),
                        from: owner.to_owned(),
                        to: id.to_owned(),
                        contract_id: None,
                    });
                }
            }
        }
    }

    for (identity, kinds) in &identity_kinds {
        if kinds.len() > 1 {
            issues.push(system_v2_issue(
                "ambiguous_kind",
                format!("$.identities.{identity}"),
                format!(
                    "Identity `{identity}` is declared with multiple kinds: {}.",
                    kinds.iter().cloned().collect::<Vec<_>>().join(", ")
                ),
                "Give every Host, Provider, and Autonomous Service a distinct stable identity.",
            ));
        }
    }

    let mut contracts = BTreeMap::<String, (String, String)>::new();
    let contract_items = system_v2_sorted_objects(
        object.get("contracts"),
        "$.contracts",
        "contractId",
        &mut issues,
    );
    for (index, item) in contract_items.iter().enumerate() {
        let contract_id = required_system_v2_string(
            item.get("contractId"),
            &format!("$.contracts[{index}].contractId"),
            &mut issues,
        );
        let producer_kind = item
            .get("producerKind")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let producer_id = item
            .get("producerId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !matches!(producer_kind, "provider" | "autonomous_service") {
            issues.push(system_v2_issue(
                "ambiguous_kind",
                format!("$.contracts[{index}].producerKind"),
                "Producer kind must be explicit.",
                "Use `provider` or `autonomous_service`.",
            ));
        } else if !owners.contains(&(producer_kind.to_owned(), producer_id.to_owned())) {
            issues.push(system_v2_issue(
                "unresolved_reference",
                format!("$.contracts[{index}].producerId"),
                "Producer reference does not resolve.",
                "Reference a declared Provider or Autonomous Service.",
            ));
        }
        let tenancy = item
            .get("tenancyMode")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let version = required_system_v2_string(
            item.get("version"),
            &format!("$.contracts[{index}].version"),
            &mut issues,
        );
        let artifact = item.get("artifact").and_then(Value::as_object);
        let artifact_valid = artifact
            .and_then(|artifact| artifact.get("format"))
            .and_then(Value::as_str)
            .is_some_and(|format| {
                matches!(
                    format,
                    "openapi" | "protobuf" | "json_schema" | "config" | "reliability"
                )
            })
            && artifact
                .and_then(|artifact| artifact.get("path"))
                .and_then(Value::as_str)
                .is_some_and(|path| !path.is_empty());
        if !artifact_valid {
            issues.push(system_v2_issue(
                "unresolved_reference",
                format!("$.contracts[{index}].artifact"),
                "Versioned contract artifact format and path are required.",
                "Declare a supported artifact.format and non-empty artifact.path.",
            ));
        }
        if contracts
            .insert(contract_id.clone(), (tenancy, version.clone()))
            .is_some()
        {
            issues.push(system_v2_issue(
                "ambiguous_kind",
                format!("$.contracts[{index}].contractId"),
                "Contract identity is declared more than once.",
                "Give every versioned contract a unique contractId.",
            ));
        }
        nodes.push(SystemV2GraphNode {
            id: format!("producer:{contract_id}"),
            kind: "producer".to_owned(),
            owner: Some(producer_id.to_owned()),
        });
        relationships.push(SystemV2GraphRelationship {
            kind: "produces".to_owned(),
            from: producer_id.to_owned(),
            to: format!("producer:{contract_id}"),
            contract_id: Some(format!("{contract_id}@{version}")),
        });
    }

    let consumer_items = system_v2_sorted_objects(
        object.get("consumers"),
        "$.consumers",
        "consumerId",
        &mut issues,
    );
    for (index, item) in consumer_items.iter().enumerate() {
        let consumer_id = required_system_v2_string(
            item.get("consumerId"),
            &format!("$.consumers[{index}].consumerId"),
            &mut issues,
        );
        let owner_kind = item
            .get("ownerKind")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let owner_id = item
            .get("ownerId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !matches!(owner_kind, "host" | "provider" | "autonomous_service") {
            issues.push(system_v2_issue(
                "ambiguous_kind",
                format!("$.consumers[{index}].ownerKind"),
                "Consumer owner kind is ambiguous.",
                "Use `host`, `provider`, or `autonomous_service`.",
            ));
        } else if !owners.contains(&(owner_kind.to_owned(), owner_id.to_owned())) {
            issues.push(system_v2_issue(
                "unresolved_reference",
                format!("$.consumers[{index}].ownerId"),
                "Consumer owner reference does not resolve.",
                "Reference a declared Host, Provider, or Autonomous Service.",
            ));
        }
        let contract_id = item
            .get("contractId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match contracts.get(contract_id) {
            None => issues.push(system_v2_issue(
                "unresolved_reference",
                format!("$.consumers[{index}].contractId"),
                "Consumer contract reference does not resolve.",
                "Reference a contractId declared in contracts.",
            )),
            Some((required, _version))
                if !system_v2_tenancy_compatible(
                    required,
                    item.get("tenancyMode")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                ) =>
            {
                issues.push(system_v2_issue(
                    "incompatible_tenancy",
                    format!("$.consumers[{index}].tenancyMode"),
                    "Consumer tenancy does not satisfy the Producer contract.",
                    "Align the Consumer tenancyMode with the Producer contract requirement.",
                ))
            }
            Some(_) => {}
        }
        nodes.push(SystemV2GraphNode {
            id: format!("consumer:{consumer_id}"),
            kind: "consumer".to_owned(),
            owner: Some(owner_id.to_owned()),
        });
        let contract_version = contracts
            .get(contract_id)
            .map(|(_, version)| version.as_str())
            .unwrap_or_default();
        relationships.push(SystemV2GraphRelationship {
            kind: "consumes".to_owned(),
            from: format!("consumer:{consumer_id}"),
            to: format!("producer:{contract_id}"),
            contract_id: Some(format!("{contract_id}@{contract_version}")),
        });
    }
    let mut node_id_kinds = BTreeMap::<&str, Vec<&str>>::new();
    for node in &nodes {
        node_id_kinds
            .entry(node.id.as_str())
            .or_default()
            .push(node.kind.as_str());
    }
    for (id, kinds) in node_id_kinds {
        if kinds.len() > 1 {
            issues.push(system_v2_issue(
                "ambiguous_kind",
                format!("$.nodes.{id}"),
                format!("Graph node identity `{id}` is declared more than once."),
                "Give every Host, Provider, Autonomous Service, Module, and Workload a unique identity.",
            ));
        }
    }
    if !issues.is_empty() {
        issues.sort_by(|a, b| (&a.path, &a.code).cmp(&(&b.path, &b.code)));
        return Err(issues);
    }
    nodes.sort();
    nodes.dedup();
    relationships.sort();
    relationships.dedup();
    Ok(SystemV2Graph {
        artifact_protocol: SYSTEM_V2_PROTOCOL.to_owned(),
        semantic_kind: ContractSemanticKind::MixedSystem,
        system_id,
        nodes,
        relationships,
        issues: Vec::new(),
    })
}

fn collect_system_v2_owners(
    items: &[&serde_json::Map<String, Value>],
    field: &str,
    id_field: &str,
    kind: &str,
    owners: &mut BTreeSet<(String, String)>,
    identity_kinds: &mut BTreeMap<String, BTreeSet<String>>,
    module_owners: &mut BTreeMap<String, String>,
    nodes: &mut Vec<SystemV2GraphNode>,
    relationships: &mut Vec<SystemV2GraphRelationship>,
    issues: &mut Vec<SystemV2Issue>,
) {
    for (index, item) in items.iter().enumerate() {
        let id = required_system_v2_string(
            item.get(id_field),
            &format!("$.{field}[{index}].{id_field}"),
            issues,
        );
        if id.is_empty() {
            continue;
        }
        owners.insert((kind.to_owned(), id.clone()));
        identity_kinds
            .entry(id.clone())
            .or_default()
            .insert(kind.to_owned());
        nodes.push(SystemV2GraphNode {
            id: id.clone(),
            kind: kind.to_owned(),
            owner: None,
        });
        collect_system_v2_modules(
            item.get("modules"),
            &format!("$.{field}[{index}].modules"),
            &id,
            module_owners,
            nodes,
            relationships,
            issues,
        );
    }
}

fn system_v2_sorted_objects<'a>(
    value: Option<&'a Value>,
    path: &str,
    identity_field: &str,
    issues: &mut Vec<SystemV2Issue>,
) -> Vec<&'a serde_json::Map<String, Value>> {
    let Some(items) = value.and_then(Value::as_array) else {
        issues.push(system_v2_issue(
            "missing_ownership",
            path,
            "A non-empty explicit topology collection is required.",
            "Declare this field as a non-empty array of explicitly typed objects.",
        ));
        return Vec::new();
    };
    if items.is_empty() {
        issues.push(system_v2_issue(
            "missing_ownership",
            path,
            "A non-empty explicit topology collection is required.",
            "Add at least one explicitly typed object.",
        ));
    }
    let mut objects = Vec::new();
    for (index, item) in items.iter().enumerate() {
        if let Some(object) = item.as_object() {
            objects.push(object);
        } else {
            issues.push(system_v2_issue(
                "ambiguous_kind",
                format!("{path}[{index}]"),
                "Topology entries must be objects with explicit kinds and identities.",
                "Replace this entry with the documented object shape.",
            ));
        }
    }
    objects.sort_by_key(|object| {
        (
            object
                .get(identity_field)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            serde_json::to_string(object).unwrap_or_default(),
        )
    });
    objects
}

fn collect_system_v2_modules(
    value: Option<&Value>,
    path: &str,
    owner: &str,
    module_owners: &mut BTreeMap<String, String>,
    nodes: &mut Vec<SystemV2GraphNode>,
    relationships: &mut Vec<SystemV2GraphRelationship>,
    issues: &mut Vec<SystemV2Issue>,
) {
    let Some(modules) = value.and_then(Value::as_array) else {
        issues.push(system_v2_issue(
            "missing_ownership",
            path,
            "Module ownership collection is required.",
            "Declare modules as an array under its explicit owner.",
        ));
        return;
    };
    if modules.is_empty() {
        issues.push(system_v2_issue(
            "missing_ownership",
            path,
            "Every topology owner must declare at least one Module.",
            "Add the Modules owned by this Host, Provider, or Autonomous Service.",
        ));
    }
    let mut modules = modules.iter().enumerate().collect::<Vec<_>>();
    modules.sort_by_key(|(_, module)| serde_json::to_string(module).unwrap_or_default());
    let mut seen = BTreeSet::new();
    for (index, module) in modules {
        let Some(id) = module.as_str().filter(|id| !id.is_empty()) else {
            issues.push(system_v2_issue(
                "missing_ownership",
                format!("{path}[{index}]"),
                "Module identity is required.",
                "Declare a non-empty Module identity under its owner.",
            ));
            continue;
        };
        if !seen.insert(id) {
            issues.push(system_v2_issue(
                "ambiguous_kind",
                format!("{path}[{index}]"),
                "Module identity is declared more than once for this owner.",
                "Keep each Module identity once under its owning topology node.",
            ));
        }
        if let Some(existing) = module_owners.insert(id.to_owned(), owner.to_owned())
            && existing != owner
        {
            issues.push(system_v2_issue(
                "ambiguous_kind",
                format!("{path}[{index}]"),
                "Module has more than one owner.",
                "Keep each Module under exactly one Host, Provider, or Autonomous Service.",
            ));
        }
        nodes.push(SystemV2GraphNode {
            id: id.to_owned(),
            kind: "module".to_owned(),
            owner: Some(owner.to_owned()),
        });
        relationships.push(SystemV2GraphRelationship {
            kind: "owns".to_owned(),
            from: owner.to_owned(),
            to: id.to_owned(),
            contract_id: None,
        });
    }
}

fn required_system_v2_string(
    value: Option<&Value>,
    path: &str,
    issues: &mut Vec<SystemV2Issue>,
) -> String {
    match value
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        Some(value) => value.to_owned(),
        None => {
            issues.push(system_v2_issue(
                "missing_ownership",
                path,
                "Explicit identity is required.",
                "Declare a stable non-empty identity.",
            ));
            String::new()
        }
    }
}

fn system_v2_tenancy_compatible(producer: &str, consumer: &str) -> bool {
    matches!(
        (producer, consumer),
        ("none", "none") | ("optional", "optional" | "required") | ("required", "required")
    )
}

fn system_v2_issue(
    code: impl Into<String>,
    path: impl Into<String>,
    message: impl Into<String>,
    next_action: impl Into<String>,
) -> SystemV2Issue {
    SystemV2Issue {
        code: code.into(),
        path: path.into(),
        message: message.into(),
        next_action: next_action.into(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSystem {
    pub protocol: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environments: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub services: Vec<ServiceSystemService>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<ServiceSystemModule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<ServiceSystemDependency>,
}

impl ServiceSystem {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            protocol: SERVICE_SYSTEM_PROTOCOL.to_owned(),
            name: name.into(),
            environments: Vec::new(),
            services: Vec::new(),
            modules: Vec::new(),
            dependencies: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSystemService {
    pub name: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSystemModule {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_to: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSystemDependency {
    pub from: String,
    pub capability: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSystemGraph {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environments: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub services: Vec<ServiceSystemGraphService>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<ServiceSystemGraphModule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<ServiceSystemGraphDependency>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<ServiceSystemGraphIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSystemGraphService {
    pub name: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSystemGraphModule {
    pub name: String,
    pub owner: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSystemGraphDependency {
    pub from: String,
    pub capability: String,
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSystemGraphIssue {
    pub code: String,
    pub message: String,
}

#[must_use]
pub fn service_system_graph(system: &ServiceSystem) -> ServiceSystemGraph {
    let services_by_name = system
        .services
        .iter()
        .map(|service| (service.name.as_str(), service))
        .collect::<BTreeMap<_, _>>();
    let modules_by_name = system
        .modules
        .iter()
        .map(|module| (module.name.as_str(), module))
        .collect::<BTreeMap<_, _>>();
    let mut module_owner = BTreeMap::new();
    let mut issues = Vec::new();
    for service in &system.services {
        for module_name in &service.modules {
            if !modules_by_name.contains_key(module_name.as_str()) {
                issues.push(ServiceSystemGraphIssue {
                    code: "module_not_declared".to_owned(),
                    message: format!(
                        "Service `{}` references undeclared module `{module_name}`.",
                        service.name
                    ),
                });
            }
            if let Some(existing) = module_owner.insert(module_name.as_str(), service.name.as_str())
            {
                issues.push(ServiceSystemGraphIssue {
                    code: "module_owned_twice".to_owned(),
                    message: format!(
                        "Module `{module_name}` is assigned to both `{existing}` and `{}`.",
                        service.name
                    ),
                });
            }
        }
    }
    for module in &system.modules {
        if let Some(service_name) = module
            .install_to
            .as_deref()
            .and_then(|install_to| install_to.strip_prefix("service:"))
            && !services_by_name.contains_key(service_name)
        {
            issues.push(ServiceSystemGraphIssue {
                code: "install_target_missing".to_owned(),
                message: format!(
                    "Module `{}` installs to missing service `{service_name}`.",
                    module.name
                ),
            });
        }
    }

    let capability_owners = service_system_capability_owners(system, &module_owner);
    let mut dependencies = Vec::new();
    for module in &system.modules {
        let from = service_system_module_owner(module, &module_owner);
        for capability in &module.dependencies {
            dependencies.push(service_system_dependency_edge(
                from,
                capability,
                capability_owners
                    .get(capability.as_str())
                    .map(Vec::as_slice),
            ));
        }
    }
    for dependency in &system.dependencies {
        if let Some(to) = dependency.to.as_deref() {
            let target_exists =
                services_by_name.contains_key(to) || modules_by_name.contains_key(to);
            let target_has_capability = service_system_target_owns_capability(
                to,
                &dependency.capability,
                &capability_owners,
                &modules_by_name,
            );
            dependencies.push(ServiceSystemGraphDependency {
                from: dependency.from.clone(),
                capability: dependency.capability.clone(),
                state: if !target_exists {
                    "unresolved".to_owned()
                } else if target_has_capability {
                    "resolved".to_owned()
                } else {
                    "missing_capability".to_owned()
                },
                to: Some(to.to_owned()),
            });
        } else {
            dependencies.push(service_system_dependency_edge(
                &dependency.from,
                &dependency.capability,
                capability_owners
                    .get(dependency.capability.as_str())
                    .map(Vec::as_slice),
            ));
        }
    }
    for dependency in &dependencies {
        if dependency.state != "resolved" {
            issues.push(ServiceSystemGraphIssue {
                code: format!("dependency_{}", dependency.state),
                message: format!(
                    "`{}` depends on `{}`, but it is {}.",
                    dependency.from, dependency.capability, dependency.state
                ),
            });
        }
    }

    ServiceSystemGraph {
        name: system.name.clone(),
        environments: system.environments.clone(),
        services: system
            .services
            .iter()
            .map(|service| ServiceSystemGraphService {
                name: service.name.clone(),
                target: service.target.clone(),
                modules: service.modules.clone(),
            })
            .collect(),
        modules: system
            .modules
            .iter()
            .map(|module| ServiceSystemGraphModule {
                name: module.name.clone(),
                owner: service_system_module_owner(module, &module_owner).to_owned(),
                capabilities: module.capabilities.clone(),
                dependencies: module.dependencies.clone(),
            })
            .collect(),
        dependencies,
        issues,
    }
}

fn service_system_install_owner(module: &ServiceSystemModule) -> Option<&str> {
    let install_to = module.install_to.as_deref()?;
    install_to.strip_prefix("service:").or(Some(install_to))
}

fn service_system_module_owner<'a>(
    module: &'a ServiceSystemModule,
    module_owner: &BTreeMap<&'a str, &'a str>,
) -> &'a str {
    module_owner
        .get(module.name.as_str())
        .copied()
        .or_else(|| service_system_install_owner(module))
        .unwrap_or("host")
}

fn service_system_capability_owners<'a>(
    system: &'a ServiceSystem,
    module_owner: &BTreeMap<&'a str, &'a str>,
) -> BTreeMap<&'a str, Vec<&'a str>> {
    let mut owners: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for module in &system.modules {
        let owner = service_system_module_owner(module, module_owner);
        for capability in &module.capabilities {
            owners.entry(capability.as_str()).or_default().push(owner);
        }
    }
    owners
}

fn service_system_target_owns_capability(
    target: &str,
    capability: &str,
    capability_owners: &BTreeMap<&str, Vec<&str>>,
    modules_by_name: &BTreeMap<&str, &ServiceSystemModule>,
) -> bool {
    capability_owners
        .get(capability)
        .is_some_and(|owners| owners.iter().any(|owner| *owner == target))
        || modules_by_name.get(target).is_some_and(|module| {
            module
                .capabilities
                .iter()
                .any(|provided| provided == capability)
        })
}

fn service_system_dependency_edge(
    from: &str,
    capability: &str,
    owners: Option<&[&str]>,
) -> ServiceSystemGraphDependency {
    let (state, to) = match owners {
        Some(owners) if owners.len() == 1 => ("resolved", Some(owners[0].to_owned())),
        Some(owners) if owners.len() > 1 => ("ambiguous", Some(owners.join(","))),
        _ => ("unresolved", None),
    };
    ServiceSystemGraphDependency {
        from: from.to_owned(),
        capability: capability.to_owned(),
        state: state.to_owned(),
        to,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceReleaseRisk {
    Safe,
    NeedsAttention,
    Breaking,
    Blocked,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceReleaseChangeSet {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceReleaseModuleChangeSet {
    pub module: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceReleaseDiff {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<ServiceReleaseModuleChangeSet>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub compatibility_changed: bool,
    #[serde(default)]
    pub config: ServiceReleaseChangeSet,
    #[serde(default)]
    pub env: ServiceReleaseChangeSet,
    #[serde(default)]
    pub modules: ServiceReleaseChangeSet,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operations: Vec<ServiceReleaseModuleChangeSet>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceReleaseManifestSummary {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub manifest_reference: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_reference: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_reference: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility_issue: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceReleasePolicyIssue {
    pub code: String,
    pub level: ServiceReleaseRisk,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceReleasePolicy {
    pub risk: ServiceReleaseRisk,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<ServiceReleasePolicyIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceReleasePlan {
    pub protocol: String,
    pub service: BTreeMap<String, String>,
    pub current: ServiceReleaseManifestSummary,
    pub candidate: ServiceReleaseManifestSummary,
    pub diff: ServiceReleaseDiff,
    pub policy: ServiceReleasePolicy,
    pub restart_required: bool,
    pub next_action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at_unix_ms: Option<u64>,
}

impl ServiceReleasePlan {
    #[must_use]
    pub fn new(
        service_name: impl Into<String>,
        current: ServiceReleaseManifestSummary,
        candidate: ServiceReleaseManifestSummary,
        diff: ServiceReleaseDiff,
    ) -> Self {
        let policy =
            evaluate_service_release_policy(&diff, candidate.compatibility_issue.as_deref());
        let mut service = BTreeMap::new();
        service.insert("name".to_owned(), service_name.into());
        Self {
            protocol: SERVICE_RELEASE_PLAN_PROTOCOL.to_owned(),
            service,
            current,
            candidate,
            restart_required: service_release_restart_required(&diff),
            next_action: service_release_next_action(policy.risk).to_owned(),
            diff,
            policy,
            created_at_unix_ms: None,
        }
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ServiceTenancyMode {
    None,
    Optional,
    Required,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum WorkloadRole {
    Api,
    Worker,
    Migration,
    Other(String),
}

impl WorkloadRole {
    pub const API: Self = Self::Api;
    pub const WORKER: Self = Self::Worker;
    pub const MIGRATION: Self = Self::Migration;

    #[must_use]
    pub fn new(role: impl Into<String>) -> Self {
        match role.into().as_str() {
            "api" => Self::Api,
            "worker" => Self::Worker,
            "migration" => Self::Migration,
            role => Self::Other(role.to_owned()),
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Api => "api",
            Self::Worker => "worker",
            Self::Migration => "migration",
            Self::Other(role) => role,
        }
    }
}

impl Serialize for WorkloadRole {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for WorkloadRole {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Self::new)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousServiceWorkload {
    pub workload_id: String,
    pub service_id: String,
    pub role: WorkloadRole,
}

impl AutonomousServiceWorkload {
    #[must_use]
    pub fn new(
        workload_id: impl Into<String>,
        service_id: impl Into<String>,
        role: WorkloadRole,
    ) -> Self {
        Self {
            workload_id: workload_id.into(),
            service_id: service_id.into(),
            role,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousServiceStore {
    pub store_id: String,
    pub service_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceArtifactFormat {
    Openapi,
    Protobuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventArtifactFormat {
    JsonSchema,
    Protobuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceArtifactReference {
    pub format: ServiceArtifactFormat,
    pub path: String,
}

impl ServiceArtifactReference {
    #[must_use]
    pub fn new(format: ServiceArtifactFormat, path: impl Into<String>) -> Self {
        Self {
            format,
            path: path.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventArtifactReference {
    pub format: EventArtifactFormat,
    pub path: String,
}

impl EventArtifactReference {
    #[must_use]
    pub fn new(format: EventArtifactFormat, path: impl Into<String>) -> Self {
        Self {
            format,
            path: path.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SchemaArtifactReference {
    pub path: String,
}

impl SchemaArtifactReference {
    #[must_use]
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CommonContextRequirement {
    Story,
    Trace,
    ServicePrincipal,
    DelegatedActor,
    Tenant,
    Deadline,
    IdempotencyKey,
    Causation,
    Region,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContractContextRequirements {
    pub protocol: String,
    pub required: Vec<CommonContextRequirement>,
}

impl ContractContextRequirements {
    #[must_use]
    pub fn new(required: Vec<CommonContextRequirement>) -> Self {
        Self {
            protocol: COMMON_CONTEXT_PROTOCOL.to_owned(),
            required,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceContractArtifact {
    pub contract_id: String,
    pub module_id: String,
    pub version: String,
    pub tenancy_mode: ServiceTenancyMode,
    pub artifact: ServiceArtifactReference,
    pub context: ContractContextRequirements,
}

impl ServiceContractArtifact {
    #[must_use]
    pub fn new(
        contract_id: impl Into<String>,
        module_id: impl Into<String>,
        version: impl Into<String>,
        tenancy_mode: ServiceTenancyMode,
        artifact: ServiceArtifactReference,
    ) -> Self {
        Self {
            contract_id: contract_id.into(),
            module_id: module_id.into(),
            version: version.into(),
            tenancy_mode,
            artifact,
            context: ContractContextRequirements::new(Vec::new()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventContractArtifact {
    pub contract_id: String,
    pub module_id: String,
    pub version: String,
    pub tenancy_mode: ServiceTenancyMode,
    pub artifact: EventArtifactReference,
    pub context: ContractContextRequirements,
}

impl EventContractArtifact {
    #[must_use]
    pub fn new(
        contract_id: impl Into<String>,
        module_id: impl Into<String>,
        version: impl Into<String>,
        tenancy_mode: ServiceTenancyMode,
        artifact: EventArtifactReference,
    ) -> Self {
        Self {
            contract_id: contract_id.into(),
            module_id: module_id.into(),
            version: version.into(),
            tenancy_mode,
            artifact,
            context: ContractContextRequirements::new(Vec::new()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigScope {
    Service,
    Region,
    Tenant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigMutability {
    Immutable,
    Mutable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigActivation {
    Hot,
    Restart,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigFieldContract {
    pub path: String,
    pub shape: String,
    pub sensitive: bool,
    pub scope: ConfigScope,
    pub mutability: ConfigMutability,
    pub activation: ConfigActivation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigContract {
    pub contract_id: String,
    pub version: String,
    pub artifact: SchemaArtifactReference,
    pub fields: Vec<ConfigFieldContract>,
}

impl ConfigContract {
    #[must_use]
    pub fn new(
        contract_id: impl Into<String>,
        version: impl Into<String>,
        artifact: SchemaArtifactReference,
        fields: Vec<ConfigFieldContract>,
    ) -> Self {
        Self {
            contract_id: contract_id.into(),
            version: version.into(),
            artifact,
            fields,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReliabilityProfile {
    Development,
    Standard,
    Critical,
}

impl Default for ReliabilityProfile {
    fn default() -> Self {
        Self::Standard
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReliabilityReadinessSemantics {
    Serving,
    Healthy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReliabilityLivenessSemantics {
    ProcessRunning,
    RuntimeOperational,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReliabilityProfileOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_backlog_limit: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_backlog_limit: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timer_lag_limit_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_exhaustion_limit: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compensation_pressure_limit: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_budget_consumed_limit_basis_points: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readiness: Option<ReliabilityReadinessSemantics>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liveness: Option<ReliabilityLivenessSemantics>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveReliabilityValues {
    pub availability_target_basis_points: u32,
    pub latency_target_ms: u64,
    pub queue_backlog_limit: u64,
    pub workflow_backlog_limit: u64,
    pub timer_lag_limit_ms: u64,
    pub retry_exhaustion_limit: u64,
    pub compensation_pressure_limit: u64,
    pub error_budget: String,
    pub error_budget_consumed_limit_basis_points: u32,
    pub readiness: ReliabilityReadinessSemantics,
    pub liveness: ReliabilityLivenessSemantics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReliabilityProfileDefaults {
    workflow_backlog_limit: u64,
    timer_lag_limit_ms: u64,
    retry_exhaustion_limit: u64,
    compensation_pressure_limit: u64,
    error_budget_consumed_limit_basis_points: u32,
    readiness: ReliabilityReadinessSemantics,
    liveness: ReliabilityLivenessSemantics,
}

impl ReliabilityProfile {
    const fn defaults(self) -> ReliabilityProfileDefaults {
        match self {
            Self::Development => ReliabilityProfileDefaults {
                workflow_backlog_limit: 1_000,
                timer_lag_limit_ms: 60_000,
                retry_exhaustion_limit: 100,
                compensation_pressure_limit: 100,
                error_budget_consumed_limit_basis_points: 10_000,
                readiness: ReliabilityReadinessSemantics::Serving,
                liveness: ReliabilityLivenessSemantics::ProcessRunning,
            },
            Self::Standard => ReliabilityProfileDefaults {
                workflow_backlog_limit: 250,
                timer_lag_limit_ms: 30_000,
                retry_exhaustion_limit: 25,
                compensation_pressure_limit: 25,
                error_budget_consumed_limit_basis_points: 10_000,
                readiness: ReliabilityReadinessSemantics::Serving,
                liveness: ReliabilityLivenessSemantics::RuntimeOperational,
            },
            Self::Critical => ReliabilityProfileDefaults {
                workflow_backlog_limit: 50,
                timer_lag_limit_ms: 5_000,
                retry_exhaustion_limit: 5,
                compensation_pressure_limit: 5,
                error_budget_consumed_limit_basis_points: 8_000,
                readiness: ReliabilityReadinessSemantics::Healthy,
                liveness: ReliabilityLivenessSemantics::RuntimeOperational,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReliabilityContract {
    pub contract_id: String,
    pub version: String,
    pub artifact: SchemaArtifactReference,
    pub profile: ReliabilityProfile,
    #[serde(default)]
    pub overrides: ReliabilityProfileOverrides,
    pub availability_target: String,
    pub latency_target_ms: u64,
    pub dependency_criticality: BTreeMap<String, String>,
    pub health_semantics: Vec<String>,
    pub degraded_modes: Vec<String>,
    #[serde(default)]
    pub degraded_mode_by_dependency: BTreeMap<String, String>,
    pub backlog_limit: u64,
    pub error_budget: String,
    pub rollout_safety: Vec<String>,
}

impl ReliabilityContract {
    #[must_use]
    pub fn new(
        contract_id: impl Into<String>,
        version: impl Into<String>,
        artifact: SchemaArtifactReference,
        availability_target: impl Into<String>,
        error_budget: impl Into<String>,
    ) -> Self {
        Self {
            contract_id: contract_id.into(),
            version: version.into(),
            artifact,
            profile: ReliabilityProfile::Standard,
            overrides: ReliabilityProfileOverrides::default(),
            availability_target: availability_target.into(),
            latency_target_ms: 0,
            dependency_criticality: BTreeMap::new(),
            health_semantics: Vec::new(),
            degraded_modes: Vec::new(),
            degraded_mode_by_dependency: BTreeMap::new(),
            backlog_limit: 0,
            error_budget: error_budget.into(),
            rollout_safety: Vec::new(),
        }
    }

    /// Resolves one deterministic runtime view from a validated declaration.
    #[must_use]
    pub fn effective_values(&self) -> Option<EffectiveReliabilityValues> {
        let defaults = self.profile.defaults();
        let availability_target_basis_points =
            parse_percentage_basis_points(&self.availability_target)?;
        Some(EffectiveReliabilityValues {
            availability_target_basis_points,
            latency_target_ms: self.latency_target_ms,
            queue_backlog_limit: self
                .overrides
                .queue_backlog_limit
                .unwrap_or(self.backlog_limit),
            workflow_backlog_limit: self
                .overrides
                .workflow_backlog_limit
                .unwrap_or(defaults.workflow_backlog_limit),
            timer_lag_limit_ms: self
                .overrides
                .timer_lag_limit_ms
                .unwrap_or(defaults.timer_lag_limit_ms),
            retry_exhaustion_limit: self
                .overrides
                .retry_exhaustion_limit
                .unwrap_or(defaults.retry_exhaustion_limit),
            compensation_pressure_limit: self
                .overrides
                .compensation_pressure_limit
                .unwrap_or(defaults.compensation_pressure_limit),
            error_budget: self.error_budget.clone(),
            error_budget_consumed_limit_basis_points: self
                .overrides
                .error_budget_consumed_limit_basis_points
                .unwrap_or(defaults.error_budget_consumed_limit_basis_points),
            readiness: self.overrides.readiness.unwrap_or(defaults.readiness),
            liveness: self.overrides.liveness.unwrap_or(defaults.liveness),
        })
    }
}

fn parse_percentage_basis_points(value: &str) -> Option<u32> {
    let percent = value.trim().strip_suffix('%')?.trim();
    let (whole, fraction) = percent.split_once('.').unwrap_or((percent, ""));
    if fraction.len() > 2
        || whole.is_empty()
        || !whole.chars().all(|c| c.is_ascii_digit())
        || !fraction.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    let whole = whole.parse::<u32>().ok()?;
    let fraction = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<u32>().ok()? * 10,
        2 => fraction.parse::<u32>().ok()?,
        _ => return None,
    };
    let basis_points = whole.checked_mul(100)?.checked_add(fraction)?;
    (basis_points <= 10_000).then_some(basis_points)
}

impl AutonomousServiceStore {
    #[must_use]
    pub fn new(store_id: impl Into<String>, service_id: impl Into<String>) -> Self {
        Self {
            store_id: store_id.into(),
            service_id: service_id.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousServiceContract {
    pub protocol: String,
    pub service_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub workloads: Vec<AutonomousServiceWorkload>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stores: Vec<AutonomousServiceStore>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub service_contracts: Vec<ServiceContractArtifact>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub event_contracts: Vec<EventContractArtifact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_contract: Option<ConfigContract>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reliability_contract: Option<ReliabilityContract>,
    pub tenancy_mode: ServiceTenancyMode,
    pub operating_regions: Vec<String>,
}

impl AutonomousServiceContract {
    #[must_use]
    pub fn new(
        service_id: impl Into<String>,
        workloads: Vec<AutonomousServiceWorkload>,
        tenancy_mode: ServiceTenancyMode,
        operating_regions: Vec<String>,
    ) -> Self {
        Self {
            protocol: AUTONOMOUS_SERVICE_PROTOCOL.to_owned(),
            service_id: service_id.into(),
            version: None,
            workloads,
            modules: Vec::new(),
            stores: Vec::new(),
            service_contracts: Vec::new(),
            event_contracts: Vec::new(),
            config_contract: None,
            reliability_contract: None,
            tenancy_mode,
            operating_regions,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousServiceIssueCode {
    UnknownField,
    InvalidProtocol,
    InvalidVersion,
    InvalidServiceIdentity,
    InvalidWorkloadIdentity,
    WorkloadOwnerMismatch,
    DuplicateWorkloadIdentity,
    InvalidWorkloadRole,
    InvalidModuleIdentity,
    DuplicateModuleIdentity,
    InvalidStoreIdentity,
    StoreOwnerMismatch,
    DuplicateStoreIdentity,
    InvalidTenancyMode,
    InvalidOperatingRegion,
    DuplicateOperatingRegion,
    InvalidContractIdentity,
    DuplicateContractIdentity,
    UnresolvedModuleReference,
    InvalidArtifactReference,
    UnresolvedArtifactReference,
    UnsupportedArtifactFormat,
    InvalidConfigContract,
    DuplicateConfigField,
    InvalidReliabilityContract,
}

impl From<AutonomousServiceIssueCode> for ContractArtifactCheckErrorCode {
    fn from(code: AutonomousServiceIssueCode) -> Self {
        match code {
            AutonomousServiceIssueCode::UnknownField => Self::UnknownField,
            AutonomousServiceIssueCode::InvalidProtocol => Self::InvalidProtocol,
            AutonomousServiceIssueCode::InvalidVersion => Self::InvalidVersion,
            AutonomousServiceIssueCode::InvalidServiceIdentity => Self::InvalidServiceIdentity,
            AutonomousServiceIssueCode::InvalidWorkloadIdentity => Self::InvalidWorkloadIdentity,
            AutonomousServiceIssueCode::WorkloadOwnerMismatch => Self::WorkloadOwnerMismatch,
            AutonomousServiceIssueCode::DuplicateWorkloadIdentity => {
                Self::DuplicateWorkloadIdentity
            }
            AutonomousServiceIssueCode::InvalidWorkloadRole => Self::InvalidWorkloadRole,
            AutonomousServiceIssueCode::InvalidModuleIdentity => Self::InvalidModuleIdentity,
            AutonomousServiceIssueCode::DuplicateModuleIdentity => Self::DuplicateModuleIdentity,
            AutonomousServiceIssueCode::InvalidStoreIdentity => Self::InvalidStoreIdentity,
            AutonomousServiceIssueCode::StoreOwnerMismatch => Self::StoreOwnerMismatch,
            AutonomousServiceIssueCode::DuplicateStoreIdentity => Self::DuplicateStoreIdentity,
            AutonomousServiceIssueCode::InvalidTenancyMode => Self::InvalidTenancyMode,
            AutonomousServiceIssueCode::InvalidOperatingRegion => Self::InvalidOperatingRegion,
            AutonomousServiceIssueCode::DuplicateOperatingRegion => Self::DuplicateOperatingRegion,
            AutonomousServiceIssueCode::InvalidContractIdentity => Self::InvalidContractIdentity,
            AutonomousServiceIssueCode::DuplicateContractIdentity => {
                Self::DuplicateContractIdentity
            }
            AutonomousServiceIssueCode::UnresolvedModuleReference => {
                Self::UnresolvedModuleReference
            }
            AutonomousServiceIssueCode::InvalidArtifactReference => Self::InvalidArtifactReference,
            AutonomousServiceIssueCode::UnresolvedArtifactReference => {
                Self::UnresolvedArtifactReference
            }
            AutonomousServiceIssueCode::UnsupportedArtifactFormat => {
                Self::UnsupportedArtifactFormat
            }
            AutonomousServiceIssueCode::InvalidConfigContract => Self::InvalidConfigContract,
            AutonomousServiceIssueCode::DuplicateConfigField => Self::DuplicateConfigField,
            AutonomousServiceIssueCode::InvalidReliabilityContract => {
                Self::InvalidReliabilityContract
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutonomousServiceIssue {
    pub code: AutonomousServiceIssueCode,
    pub path: String,
    pub message: String,
    pub next_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceContract {
    #[serde(default = "default_service_contract_protocol")]
    pub protocol: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<ServiceProvider>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility: Option<ServiceCompatibility>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config: Vec<ServiceConfigField>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<ServiceEnvField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<ServiceHealth>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_process: Option<ServiceLocalProcess>,
    pub modules: Vec<ModuleManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServicePackage {
    pub protocol: String,
    pub name: String,
    pub version: String,
    pub service_manifest: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<String>,
}

impl ServicePackage {
    #[must_use]
    pub fn new(name: impl Into<String>, version: impl Into<String>, modules: Vec<String>) -> Self {
        Self {
            protocol: SERVICE_PACKAGE_PROTOCOL.to_owned(),
            name: name.into(),
            version: version.into(),
            service_manifest: "lenso.service.json".to_owned(),
            modules,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleContract {
    pub protocol: String,
    pub name: String,
    pub version: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest: Option<ModuleManifest>,
}

impl ModuleContract {
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            protocol: MODULE_CONTRACT_PROTOCOL.to_owned(),
            name: name.into(),
            version: version.into(),
            source: source.into(),
            summary: None,
            capabilities: Vec::new(),
            dependencies: Vec::new(),
            manifest: None,
        }
    }

    #[must_use]
    pub fn manifest(mut self, manifest: ModuleManifest) -> Self {
        self.manifest = Some(manifest);
        self
    }

    #[must_use]
    pub fn capabilities(mut self, capabilities: Vec<String>) -> Self {
        self.capabilities = capabilities;
        self
    }

    #[must_use]
    pub fn dependencies(mut self, dependencies: Vec<String>) -> Self {
        self.dependencies = dependencies;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleReleaseProvider {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_package: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_manifest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleRelease {
    pub protocol: String,
    pub name: String,
    pub version: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<ModuleReleaseProvider>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
}

impl ModuleRelease {
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        provider_name: impl Into<String>,
    ) -> Self {
        Self {
            protocol: MODULE_RELEASE_PROTOCOL.to_owned(),
            name: name.into(),
            version: version.into(),
            source: "service".to_owned(),
            provider: Some(ModuleReleaseProvider {
                name: provider_name.into(),
                service_package: Some("lenso.service-package.json".to_owned()),
                service_manifest: None,
            }),
            summary: None,
            capabilities: Vec::new(),
            dependencies: Vec::new(),
        }
    }

    #[must_use]
    pub fn capabilities(mut self, capabilities: Vec<String>) -> Self {
        self.capabilities = capabilities;
        self
    }

    #[must_use]
    pub fn dependencies(mut self, dependencies: Vec<String>) -> Self {
        self.dependencies = dependencies;
        self
    }

    #[must_use]
    pub fn service_manifest(mut self, service_manifest: impl Into<String>) -> Self {
        if let Some(provider) = &mut self.provider {
            provider.service_package = None;
            provider.service_manifest = Some(service_manifest.into());
        }
        self
    }
}

impl ServiceContract {
    #[must_use]
    pub fn new(name: impl Into<String>, modules: Vec<ModuleManifest>) -> Self {
        Self {
            protocol: SERVICE_CONTRACT_PROTOCOL.to_owned(),
            name: name.into(),
            version: None,
            provider: None,
            compatibility: None,
            config: Vec::new(),
            env: Vec::new(),
            health: None,
            local_process: None,
            modules,
        }
    }

    #[must_use]
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    #[must_use]
    pub fn provider(mut self, provider: ServiceProvider) -> Self {
        self.provider = Some(provider);
        self
    }

    #[must_use]
    pub fn compatibility(mut self, compatibility: ServiceCompatibility) -> Self {
        self.compatibility = Some(compatibility);
        self
    }

    #[must_use]
    pub fn config(mut self, config: Vec<ServiceConfigField>) -> Self {
        self.config = config;
        self
    }

    #[must_use]
    pub fn env(mut self, env: Vec<ServiceEnvField>) -> Self {
        self.env = env;
        self
    }

    #[must_use]
    pub fn health(mut self, health: ServiceHealth) -> Self {
        self.health = Some(health);
        self
    }

    #[must_use]
    pub fn local_process(mut self, local_process: ServiceLocalProcess) -> Self {
        self.local_process = Some(local_process);
        self
    }
}

fn default_service_contract_protocol() -> String {
    SERVICE_CONTRACT_PROTOCOL.to_owned()
}

#[must_use]
pub fn evaluate_service_release_policy(
    diff: &ServiceReleaseDiff,
    compatibility_issue: Option<&str>,
) -> ServiceReleasePolicy {
    let mut issues = Vec::new();
    if let Some(issue) = compatibility_issue {
        issues.push(ServiceReleasePolicyIssue {
            code: "host_incompatible".to_owned(),
            level: ServiceReleaseRisk::Blocked,
            message: issue.to_owned(),
        });
    } else if diff.compatibility_changed {
        issues.push(ServiceReleasePolicyIssue {
            code: "compatibility_changed".to_owned(),
            level: ServiceReleaseRisk::NeedsAttention,
            message: "Service compatibility metadata changed; review host support before applying."
                .to_owned(),
        });
    }
    for module in &diff.modules.removed {
        issues.push(ServiceReleasePolicyIssue {
            code: "module_removed".to_owned(),
            level: ServiceReleaseRisk::Breaking,
            message: format!("Module `{module}` is removed by this release."),
        });
    }
    for env in &diff.env.added {
        issues.push(ServiceReleasePolicyIssue {
            code: "env_added".to_owned(),
            level: ServiceReleaseRisk::NeedsAttention,
            message: format!("Environment value `{env}` is newly required by this release."),
        });
    }
    for config in &diff.config.added {
        issues.push(ServiceReleasePolicyIssue {
            code: "config_added".to_owned(),
            level: ServiceReleaseRisk::NeedsAttention,
            message: format!("Runtime config `{config}` is newly declared by this release."),
        });
    }
    for change in &diff.capabilities {
        for capability in &change.removed {
            issues.push(ServiceReleasePolicyIssue {
                code: "capability_removed".to_owned(),
                level: ServiceReleaseRisk::Breaking,
                message: format!(
                    "Capability `{capability}` is removed from module `{}`.",
                    change.module
                ),
            });
        }
    }
    for change in &diff.operations {
        for operation in &change.removed {
            issues.push(ServiceReleasePolicyIssue {
                code: "operation_removed".to_owned(),
                level: ServiceReleaseRisk::Breaking,
                message: format!(
                    "Operation `{operation}` is removed from module `{}`.",
                    change.module
                ),
            });
        }
    }
    let risk = issues
        .iter()
        .map(|issue| issue.level)
        .max_by_key(|risk| service_release_risk_rank(*risk))
        .unwrap_or(ServiceReleaseRisk::Safe);
    ServiceReleasePolicy { risk, issues }
}

#[must_use]
pub fn service_release_restart_required(diff: &ServiceReleaseDiff) -> bool {
    diff.compatibility_changed
        || !diff.modules.added.is_empty()
        || !diff.modules.removed.is_empty()
        || !diff.env.added.is_empty()
        || !diff.env.removed.is_empty()
        || !diff.config.added.is_empty()
        || !diff.config.removed.is_empty()
        || diff
            .capabilities
            .iter()
            .any(|change| !change.added.is_empty() || !change.removed.is_empty())
        || diff
            .operations
            .iter()
            .any(|change| !change.added.is_empty() || !change.removed.is_empty())
}

#[must_use]
pub fn service_release_next_action(risk: ServiceReleaseRisk) -> &'static str {
    match risk {
        ServiceReleaseRisk::Safe => "Run `lenso service release apply <plan.json>` when ready.",
        ServiceReleaseRisk::NeedsAttention => {
            "Review required env/config, then run `lenso service release apply <plan.json>`."
        }
        ServiceReleaseRisk::Breaking => {
            "Review removed modules, capabilities, or operations before applying."
        }
        ServiceReleaseRisk::Blocked => "Fix blocked policy issues before applying this release.",
    }
}

#[must_use]
pub fn health_router() -> Router {
    Router::new()
        .route(
            "/lenso/service/v1/ready",
            get(|| async { Json(serde_json::json!({"ready": true})) }),
        )
        .route(
            "/lenso/service/v1/status",
            get(|| async { Json(serde_json::json!({"state": "ready"})) }),
        )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceContractIssue {
    pub path: String,
    pub message: String,
}

impl ServiceContractIssue {
    fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

/// Checks a versioned Provider or Autonomous Service artifact and projects its meaning.
///
/// The returned read model is separate from the source JSON so compatibility checks never
/// rewrite a legacy artifact or reinterpret Provider declarations as Autonomous Services.
pub fn check_contract_artifact_value(
    value: &Value,
) -> Result<ContractArtifactCheck, ContractArtifactCheckError> {
    let Some(object) = value.as_object() else {
        return Err(ambiguous_protocol_error(
            "artifact must be an object with an explicit versioned protocol",
        ));
    };
    let Some(protocol) = object
        .get("protocol")
        .and_then(Value::as_str)
        .filter(|protocol| !protocol.trim().is_empty())
    else {
        return Err(ambiguous_protocol_error(
            "artifact protocol is required to determine its semantic kind",
        ));
    };

    if protocol == SYSTEM_V2_PROTOCOL {
        if let Err(issues) = system_v2_graph(value) {
            let issue = &issues[0];
            return Err(ContractArtifactCheckError {
                code: ContractArtifactCheckErrorCode::InvalidArtifact,
                path: issue.path.clone(),
                message: issue.message.clone(),
                next_action: issue.next_action.clone(),
            });
        }
        return Ok(ContractArtifactCheck {
            detected_protocol: protocol.to_owned(),
            artifact_kind: ContractArtifactKind::System,
            semantic_kind: ContractSemanticKind::MixedSystem,
            provider_semantics: None,
            autonomous_service: None,
        });
    }

    if protocol == AUTONOMOUS_SERVICE_PROTOCOL {
        let issues = validate_autonomous_service_contract_value(value);
        if let Some(issue) = issues.first() {
            return Err(ContractArtifactCheckError {
                code: issue.code.into(),
                path: issue.path.clone(),
                message: issue.message.clone(),
                next_action: issue.next_action.clone(),
            });
        }
        let contract: AutonomousServiceContract =
            serde_json::from_value(value.clone()).map_err(|error| ContractArtifactCheckError {
                code: ContractArtifactCheckErrorCode::InvalidArtifact,
                path: "$".to_owned(),
                message: error.to_string(),
                next_action: "Fix the reported contract field and run the check again.".to_owned(),
            })?;
        return Ok(ContractArtifactCheck {
            detected_protocol: protocol.to_owned(),
            artifact_kind: ContractArtifactKind::Service,
            semantic_kind: ContractSemanticKind::AutonomousService,
            provider_semantics: None,
            autonomous_service: Some(AutonomousServiceSummary {
                service_id: contract.service_id,
                workloads: {
                    let mut workloads = contract
                        .workloads
                        .into_iter()
                        .map(|workload| workload.workload_id)
                        .collect::<Vec<_>>();
                    workloads.sort();
                    workloads
                },
                modules: sorted_unique(contract.modules),
                service_contracts: sorted_unique(
                    contract
                        .service_contracts
                        .into_iter()
                        .map(|contract| contract.contract_id),
                ),
                event_contracts: sorted_unique(
                    contract
                        .event_contracts
                        .into_iter()
                        .map(|contract| contract.contract_id),
                ),
                has_config_contract: contract.config_contract.is_some(),
                has_reliability_contract: contract.reliability_contract.is_some(),
            }),
        });
    }

    let (artifact_kind, semantic_kind, issues) = match protocol {
        SERVICE_CONTRACT_PROTOCOL => (
            ContractArtifactKind::Service,
            ContractSemanticKind::Provider,
            validate_service_contract_value(value),
        ),
        SERVICE_SYSTEM_PROTOCOL => (
            ContractArtifactKind::System,
            ContractSemanticKind::ProviderSystem,
            validate_service_system_value(value),
        ),
        _ => {
            return Err(ContractArtifactCheckError {
                code: ContractArtifactCheckErrorCode::UnsupportedProtocol,
                path: "$.protocol".to_owned(),
                message: format!("unsupported artifact protocol `{protocol}`"),
                next_action: "Use a supported protocol or upgrade Lenso for this artifact version."
                    .to_owned(),
            });
        }
    };

    if let Some(issue) = issues.first() {
        return Err(ContractArtifactCheckError {
            code: ContractArtifactCheckErrorCode::InvalidArtifact,
            path: issue.path.clone(),
            message: issue.message.clone(),
            next_action: "Fix the reported contract field and run the check again.".to_owned(),
        });
    }

    let mut providers: Vec<String> = match artifact_kind {
        ContractArtifactKind::Service => object
            .get("name")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .into_iter()
            .collect(),
        ContractArtifactKind::System => object
            .get("services")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|service| service.get("name").and_then(Value::as_str))
            .map(ToOwned::to_owned)
            .collect(),
    };
    providers.sort();
    providers.dedup();

    Ok(ContractArtifactCheck {
        detected_protocol: protocol.to_owned(),
        artifact_kind,
        semantic_kind,
        provider_semantics: Some(ProviderSemantics {
            providers,
            auth_owner: ContractOwner::Host,
            proxy_policy_owner: ContractOwner::Host,
            retry_owner: ContractOwner::Host,
            runtime_queue_owner: ContractOwner::Host,
            outbox_owner: ContractOwner::Host,
            story_owner: ContractOwner::Host,
        }),
        autonomous_service: None,
    })
}

/// Checks an artifact and resolves its owned contract files against a packaged path set.
pub fn check_contract_artifact_value_with_artifacts(
    value: &Value,
    available_paths: &BTreeSet<String>,
) -> Result<ContractArtifactCheck, ContractArtifactCheckError> {
    let check = check_contract_artifact_value(value)?;
    if check.semantic_kind == ContractSemanticKind::AutonomousService
        && let Some(issue) =
            validate_autonomous_service_artifact_references(value, available_paths).first()
    {
        return Err(ContractArtifactCheckError {
            code: issue.code.into(),
            path: issue.path.clone(),
            message: issue.message.clone(),
            next_action: issue.next_action.clone(),
        });
    }
    Ok(check)
}

fn sorted_unique(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut values = values.into_iter().collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

fn ambiguous_protocol_error(message: &str) -> ContractArtifactCheckError {
    ContractArtifactCheckError {
        code: ContractArtifactCheckErrorCode::AmbiguousProtocol,
        path: "$.protocol".to_owned(),
        message: message.to_owned(),
        next_action: "Set `protocol` to a supported Provider-era protocol or `lenso.service.v2`."
            .to_owned(),
    }
}

#[must_use]
pub fn validate_autonomous_service_contract(
    contract: &AutonomousServiceContract,
) -> Vec<AutonomousServiceIssue> {
    validate_autonomous_service_contract_value(
        &serde_json::to_value(contract).expect("AutonomousServiceContract must serialize"),
    )
}

#[must_use]
pub fn validate_autonomous_service_contract_value(value: &Value) -> Vec<AutonomousServiceIssue> {
    let mut issues = Vec::new();
    let Some(object) = value.as_object() else {
        push_autonomous_issue(
            &mut issues,
            AutonomousServiceIssueCode::InvalidServiceIdentity,
            "$",
            "service contract must be an object",
            "Use a JSON object for the Service declaration.",
        );
        return issues;
    };
    validate_unknown_fields(
        object,
        "$",
        &[
            "protocol",
            "serviceId",
            "version",
            "workloads",
            "modules",
            "stores",
            "serviceContracts",
            "eventContracts",
            "configContract",
            "reliabilityContract",
            "tenancyMode",
            "operatingRegions",
        ],
        &mut issues,
    );
    if object.get("protocol").and_then(Value::as_str) != Some(AUTONOMOUS_SERVICE_PROTOCOL) {
        push_autonomous_issue(
            &mut issues,
            AutonomousServiceIssueCode::InvalidProtocol,
            "$.protocol",
            "protocol must be `lenso.service.v2`",
            "Set `protocol` to `lenso.service.v2`.",
        );
    }
    let service_id = object
        .get("serviceId")
        .and_then(Value::as_str)
        .unwrap_or("");
    if service_id.trim().is_empty() {
        push_autonomous_issue(
            &mut issues,
            AutonomousServiceIssueCode::InvalidServiceIdentity,
            "$.serviceId",
            "serviceId must be a non-empty string",
            "Assign one stable logical Service identity.",
        );
    }
    if object.get("version").is_some_and(|version| {
        version
            .as_str()
            .is_none_or(|version| version.trim().is_empty())
    }) {
        push_autonomous_issue(
            &mut issues,
            AutonomousServiceIssueCode::InvalidVersion,
            "$.version",
            "version must be a non-empty string when present",
            "Set a non-empty Service version or remove the optional field.",
        );
    }
    let mut workload_ids = BTreeSet::new();
    match object.get("workloads").and_then(Value::as_array) {
        Some(workloads) if !workloads.is_empty() => {
            for (index, workload) in workloads.iter().enumerate() {
                let path = format!("$.workloads[{index}]");
                if let Some(object) = workload.as_object() {
                    validate_unknown_fields(
                        object,
                        &path,
                        &["workloadId", "serviceId", "role"],
                        &mut issues,
                    );
                }
                let id = workload
                    .get("workloadId")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if id.trim().is_empty() {
                    push_autonomous_issue(
                        &mut issues,
                        AutonomousServiceIssueCode::InvalidWorkloadIdentity,
                        format!("{path}.workloadId"),
                        "workloadId must be a non-empty string",
                        "Assign a unique identity to this Workload.",
                    );
                } else if !workload_ids.insert(id) {
                    push_autonomous_issue(
                        &mut issues,
                        AutonomousServiceIssueCode::DuplicateWorkloadIdentity,
                        format!("{path}.workloadId"),
                        "workloadId must be unique within the Service",
                        "Rename this Workload so each workloadId is unique.",
                    );
                }
                if workload.get("serviceId").and_then(Value::as_str) != Some(service_id) {
                    push_autonomous_issue(
                        &mut issues,
                        AutonomousServiceIssueCode::WorkloadOwnerMismatch,
                        format!("{path}.serviceId"),
                        "Workload owner must match the enclosing serviceId",
                        "Set the Workload serviceId to the enclosing Service identity.",
                    );
                }
                if workload
                    .get("role")
                    .and_then(Value::as_str)
                    .is_none_or(|role| role.trim().is_empty())
                {
                    push_autonomous_issue(
                        &mut issues,
                        AutonomousServiceIssueCode::InvalidWorkloadRole,
                        format!("{path}.role"),
                        "role must be a non-empty string",
                        "Use `api`, `worker`, `migration`, or a stable extension role.",
                    );
                }
            }
        }
        _ => push_autonomous_issue(
            &mut issues,
            AutonomousServiceIssueCode::InvalidWorkloadIdentity,
            "$.workloads",
            "workloads must contain at least one Workload",
            "Declare at least one API, Worker, Migration, or extension Workload.",
        ),
    }
    validate_owned_identities(
        object.get("stores"),
        "stores",
        "storeId",
        service_id,
        AutonomousServiceIssueCode::InvalidStoreIdentity,
        AutonomousServiceIssueCode::StoreOwnerMismatch,
        AutonomousServiceIssueCode::DuplicateStoreIdentity,
        &mut issues,
    );
    validate_unique_strings(
        object.get("modules"),
        "modules",
        AutonomousServiceIssueCode::InvalidModuleIdentity,
        AutonomousServiceIssueCode::DuplicateModuleIdentity,
        &mut issues,
    );
    let module_ids = object
        .get("modules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    validate_contract_artifacts(
        object.get("serviceContracts"),
        "serviceContracts",
        &["openapi", "protobuf"],
        true,
        &module_ids,
        &mut issues,
    );
    validate_contract_artifacts(
        object.get("eventContracts"),
        "eventContracts",
        &["json_schema", "protobuf"],
        true,
        &module_ids,
        &mut issues,
    );
    validate_config_contract(object.get("configContract"), &mut issues);
    validate_reliability_contract(object.get("reliabilityContract"), &mut issues);
    match object.get("tenancyMode").and_then(Value::as_str) {
        Some("none" | "optional" | "required") => {}
        _ => push_autonomous_issue(
            &mut issues,
            AutonomousServiceIssueCode::InvalidTenancyMode,
            "$.tenancyMode",
            "tenancyMode must be `none`, `optional`, or `required`",
            "Choose one supported Tenancy Mode.",
        ),
    }
    validate_unique_strings(
        object.get("operatingRegions"),
        "operatingRegions",
        AutonomousServiceIssueCode::InvalidOperatingRegion,
        AutonomousServiceIssueCode::DuplicateOperatingRegion,
        &mut issues,
    );
    if object
        .get("operatingRegions")
        .and_then(Value::as_array)
        .is_none_or(Vec::is_empty)
    {
        push_autonomous_issue(
            &mut issues,
            AutonomousServiceIssueCode::InvalidOperatingRegion,
            "$.operatingRegions",
            "at least one Operating Region is required",
            "Declare at least one logical Operating Region.",
        );
    }
    issues
}

/// Resolves every owned contract artifact against paths packaged by a caller such as the CLI.
#[must_use]
pub fn validate_autonomous_service_artifact_references(
    value: &Value,
    available_paths: &BTreeSet<String>,
) -> Vec<AutonomousServiceIssue> {
    let mut issues = Vec::new();
    for field in ["serviceContracts", "eventContracts"] {
        for (index, contract) in value
            .get(field)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            validate_available_artifact_path(
                contract
                    .get("artifact")
                    .and_then(|artifact| artifact.get("path")),
                format!("$.{field}[{index}].artifact.path"),
                available_paths,
                &mut issues,
            );
        }
    }
    for field in ["configContract", "reliabilityContract"] {
        if let Some(contract) = value.get(field) {
            validate_available_artifact_path(
                contract
                    .get("artifact")
                    .and_then(|artifact| artifact.get("path")),
                format!("$.{field}.artifact.path"),
                available_paths,
                &mut issues,
            );
        }
    }
    issues
}

fn validate_available_artifact_path(
    value: Option<&Value>,
    path: String,
    available_paths: &BTreeSet<String>,
    issues: &mut Vec<AutonomousServiceIssue>,
) {
    if let Some(reference) = value.and_then(Value::as_str)
        && is_repository_relative_artifact_path(reference)
        && !available_paths.contains(reference)
    {
        push_autonomous_issue(
            issues,
            AutonomousServiceIssueCode::UnresolvedArtifactReference,
            path,
            format!("artifact `{reference}` is not present in the package"),
            "Package the referenced artifact or correct its repository-relative path.",
        );
    }
}

fn validate_contract_artifacts(
    value: Option<&Value>,
    field: &str,
    formats: &[&str],
    has_tenancy: bool,
    module_ids: &BTreeSet<&str>,
    issues: &mut Vec<AutonomousServiceIssue>,
) {
    let Some(values) = value.and_then(Value::as_array) else {
        if value.is_some() {
            push_autonomous_issue(
                issues,
                AutonomousServiceIssueCode::InvalidContractIdentity,
                format!("$.{field}"),
                "contract declarations must be an array",
                "Declare contract artifacts as an array.",
            );
        }
        return;
    };
    let mut identities = BTreeSet::new();
    for (index, value) in values.iter().enumerate() {
        let base = format!("$.{field}[{index}]");
        let Some(object) = value.as_object() else {
            push_autonomous_issue(
                issues,
                AutonomousServiceIssueCode::InvalidContractIdentity,
                &base,
                "contract declaration must be an object",
                "Declare a versioned contract object.",
            );
            continue;
        };
        let mut allowed = vec!["contractId", "moduleId", "version", "artifact", "context"];
        if has_tenancy {
            allowed.push("tenancyMode");
        }
        validate_unknown_fields(object, &base, &allowed, issues);
        let contract_id = object
            .get("contractId")
            .and_then(Value::as_str)
            .unwrap_or("");
        let version = object.get("version").and_then(Value::as_str).unwrap_or("");
        if contract_id.trim().is_empty() || version.trim().is_empty() {
            let field_name = if contract_id.trim().is_empty() {
                "contractId"
            } else {
                "version"
            };
            push_autonomous_issue(
                issues,
                AutonomousServiceIssueCode::InvalidContractIdentity,
                format!("{base}.{field_name}"),
                "contractId and version must be non-empty strings",
                "Assign a stable contractId and Contract Version.",
            );
        }
        let artifact = object.get("artifact");
        let format = artifact
            .and_then(|value| value.get("format"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if !formats.contains(&format) {
            push_autonomous_issue(
                issues,
                AutonomousServiceIssueCode::UnsupportedArtifactFormat,
                format!("{base}.artifact.format"),
                format!("unsupported artifact format `{format}`"),
                format!("Use one supported format: {}.", formats.join(", ")),
            );
        }
        let module_id = object.get("moduleId").and_then(Value::as_str).unwrap_or("");
        if !module_ids.contains(module_id) {
            push_autonomous_issue(
                issues,
                AutonomousServiceIssueCode::UnresolvedModuleReference,
                format!("{base}.moduleId"),
                "moduleId does not resolve to an owned Module",
                "Reference one Module identity declared in `modules`.",
            );
        }
        if !contract_id.is_empty() && !identities.insert(contract_id) {
            push_autonomous_issue(
                issues,
                AutonomousServiceIssueCode::DuplicateContractIdentity,
                format!("{base}.contractId"),
                "contractId must be unique within its contract kind",
                "Remove or rename the duplicate contractId.",
            );
        }
        if artifact
            .and_then(|value| value.get("path"))
            .and_then(Value::as_str)
            .is_none_or(|path| !is_repository_relative_artifact_path(path))
        {
            push_autonomous_issue(
                issues,
                AutonomousServiceIssueCode::InvalidArtifactReference,
                format!("{base}.artifact.path"),
                "artifact path must be a non-empty string",
                "Reference a packaged contract artifact using a repository-relative path.",
            );
        }
        if let Some(artifact) = artifact.and_then(Value::as_object) {
            validate_unknown_fields(
                artifact,
                &format!("{base}.artifact"),
                &["format", "path"],
                issues,
            );
        }
        if has_tenancy
            && !matches!(
                object.get("tenancyMode").and_then(Value::as_str),
                Some("none" | "optional" | "required")
            )
        {
            push_autonomous_issue(
                issues,
                AutonomousServiceIssueCode::InvalidTenancyMode,
                format!("{base}.tenancyMode"),
                "tenancyMode must be `none`, `optional`, or `required`",
                "Choose one supported Tenancy Mode for this Service Contract.",
            );
        }
        validate_context_requirements(object.get("context"), &base, issues);
    }
}

fn validate_context_requirements(
    value: Option<&Value>,
    base: &str,
    issues: &mut Vec<AutonomousServiceIssue>,
) {
    let supported = [
        "story",
        "trace",
        "service_principal",
        "delegated_actor",
        "tenant",
        "deadline",
        "idempotency_key",
        "causation",
        "region",
    ];
    let valid = value.and_then(Value::as_object).is_some_and(|context| {
        context.get("protocol").and_then(Value::as_str) == Some(COMMON_CONTEXT_PROTOCOL)
            && context
                .get("required")
                .and_then(Value::as_array)
                .is_some_and(|required| {
                    let values = required
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<BTreeSet<_>>();
                    values.len() == required.len()
                        && values.iter().all(|value| supported.contains(value))
                })
    });
    if !valid {
        push_autonomous_issue(
            issues,
            AutonomousServiceIssueCode::InvalidContractIdentity,
            format!("{base}.context"),
            "context must reference `lenso.context.v1` with unique supported requirements",
            "Declare the common context required by this contract.",
        );
    }
    if let Some(context) = value.and_then(Value::as_object) {
        validate_unknown_fields(
            context,
            &format!("{base}.context"),
            &["protocol", "required"],
            issues,
        );
    }
}

fn is_repository_relative_artifact_path(path: &str) -> bool {
    let path = std::path::Path::new(path);
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && !path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
        && !path.to_string_lossy().contains("://")
}

fn validate_config_contract(value: Option<&Value>, issues: &mut Vec<AutonomousServiceIssue>) {
    let Some(value) = value else {
        return;
    };
    let Some(object) = value.as_object() else {
        push_autonomous_issue(
            issues,
            AutonomousServiceIssueCode::InvalidConfigContract,
            "$.configContract",
            "configContract must be an object",
            "Declare one versioned Config Contract object.",
        );
        return;
    };
    validate_unknown_fields(
        object,
        "$.configContract",
        &["contractId", "version", "artifact", "fields"],
        issues,
    );
    validate_service_owned_contract_header(object, "$.configContract", issues);
    if object
        .get("version")
        .and_then(Value::as_str)
        .is_none_or(|v| v.trim().is_empty())
    {
        push_autonomous_issue(
            issues,
            AutonomousServiceIssueCode::InvalidConfigContract,
            "$.configContract.version",
            "Config Contract version must be a non-empty string",
            "Assign a stable Config Contract Version.",
        );
    }
    let Some(fields) = object.get("fields").and_then(Value::as_array) else {
        push_autonomous_issue(
            issues,
            AutonomousServiceIssueCode::InvalidConfigContract,
            "$.configContract.fields",
            "Config Contract fields must be an array",
            "Declare configuration field requirements.",
        );
        return;
    };
    let mut paths = BTreeSet::new();
    for (index, field) in fields.iter().enumerate() {
        let base = format!("$.configContract.fields[{index}]");
        let Some(object) = field.as_object() else {
            push_autonomous_issue(
                issues,
                AutonomousServiceIssueCode::InvalidConfigContract,
                &base,
                "Config field must be an object",
                "Declare all Config field requirements in an object.",
            );
            continue;
        };
        validate_unknown_fields(
            object,
            &base,
            &[
                "path",
                "shape",
                "sensitive",
                "scope",
                "mutability",
                "activation",
            ],
            issues,
        );
        let path = object.get("path").and_then(Value::as_str).unwrap_or("");
        let valid = !path.trim().is_empty()
            && object
                .get("shape")
                .and_then(Value::as_str)
                .is_some_and(|v| !v.trim().is_empty())
            && object.get("sensitive").is_some_and(Value::is_boolean)
            && matches!(
                object.get("scope").and_then(Value::as_str),
                Some("service" | "region" | "tenant")
            )
            && matches!(
                object.get("mutability").and_then(Value::as_str),
                Some("immutable" | "mutable")
            )
            && matches!(
                object.get("activation").and_then(Value::as_str),
                Some("hot" | "restart")
            );
        if !valid {
            push_autonomous_issue(
                issues,
                AutonomousServiceIssueCode::InvalidConfigContract,
                &base,
                "Config field must declare path, shape, sensitivity, scope, mutability, and activation",
                "Complete every Config Contract field declaration using supported values.",
            );
        }
        if !path.is_empty() && !paths.insert(path) {
            push_autonomous_issue(
                issues,
                AutonomousServiceIssueCode::DuplicateConfigField,
                format!("{base}.path"),
                "Config field path must be unique",
                "Remove the duplicate Config field path.",
            );
        }
    }
}

fn validate_reliability_contract(value: Option<&Value>, issues: &mut Vec<AutonomousServiceIssue>) {
    let Some(value) = value else {
        return;
    };
    let Some(object) = value.as_object() else {
        push_autonomous_issue(
            issues,
            AutonomousServiceIssueCode::InvalidReliabilityContract,
            "$.reliabilityContract",
            "reliabilityContract must be an object",
            "Declare one versioned Reliability Contract object.",
        );
        return;
    };
    validate_unknown_fields(
        object,
        "$.reliabilityContract",
        &[
            "contractId",
            "version",
            "artifact",
            "profile",
            "overrides",
            "availabilityTarget",
            "latencyTargetMs",
            "dependencyCriticality",
            "healthSemantics",
            "degradedModes",
            "degradedModeByDependency",
            "backlogLimit",
            "errorBudget",
            "rolloutSafety",
        ],
        issues,
    );
    validate_service_owned_contract_header(object, "$.reliabilityContract", issues);
    let dependencies = object
        .get("dependencyCriticality")
        .and_then(Value::as_object);
    let degraded_mode_by_dependency = object
        .get("degradedModeByDependency")
        .and_then(Value::as_object);
    let degradable_dependencies = dependencies
        .into_iter()
        .flat_map(|dependencies| dependencies.iter())
        .filter_map(|(dependency, criticality)| {
            (criticality.as_str() == Some("degradable")).then_some(dependency.as_str())
        })
        .collect::<BTreeSet<_>>();
    let mapped_degradable_dependencies = degraded_mode_by_dependency
        .into_iter()
        .flat_map(|dependencies| dependencies.iter())
        .filter_map(|(dependency, mode)| {
            mode.as_str()
                .is_some_and(|mode| !mode.trim().is_empty())
                .then_some(dependency.as_str())
        })
        .collect::<BTreeSet<_>>();
    if object
        .get("version")
        .and_then(Value::as_str)
        .is_none_or(|v| v.trim().is_empty())
        || !matches!(
            object.get("profile").and_then(Value::as_str),
            Some("development" | "standard" | "critical")
        )
        || !valid_reliability_overrides(object.get("overrides"))
        || object
            .get("availabilityTarget")
            .and_then(Value::as_str)
            .and_then(parse_percentage_basis_points)
            .is_none()
        || !object.get("latencyTargetMs").is_some_and(Value::is_u64)
        || !object
            .get("dependencyCriticality")
            .and_then(Value::as_object)
            .is_some_and(|dependencies| {
                dependencies.iter().all(|(dependency, value)| {
                    !dependency.trim().is_empty()
                        && matches!(value.as_str(), Some("critical" | "degradable" | "optional"))
                })
            })
        || !is_string_array(object.get("healthSemantics"))
        || !is_string_array(object.get("degradedModes"))
        || !object
            .get("degradedModeByDependency")
            .and_then(Value::as_object)
            .is_some_and(|modes| {
                modes.iter().all(|(dependency, mode)| {
                    mode.as_str().is_some_and(|mode| !mode.trim().is_empty())
                        && dependencies.is_some_and(|dependencies| {
                            dependencies.get(dependency).and_then(Value::as_str)
                                == Some("degradable")
                        })
                })
            })
        || degradable_dependencies != mapped_degradable_dependencies
        || !object.get("backlogLimit").is_some_and(Value::is_u64)
        || object
            .get("errorBudget")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        || !is_string_array(object.get("rolloutSafety"))
    {
        push_autonomous_issue(
            issues,
            AutonomousServiceIssueCode::InvalidReliabilityContract,
            "$.reliabilityContract",
            "Reliability Contract must declare availability, latency, dependencies, health, degradation, backlog, error budget, and rollout safety",
            "Declare whole-Service reliability expectations using supported values.",
        );
    }
}

fn valid_reliability_overrides(value: Option<&Value>) -> bool {
    let Some(overrides) = value.and_then(Value::as_object) else {
        return false;
    };
    const NUMERIC_FIELDS: &[&str] = &[
        "queueBacklogLimit",
        "workflowBacklogLimit",
        "timerLagLimitMs",
        "retryExhaustionLimit",
        "compensationPressureLimit",
    ];
    const ALLOWED_FIELDS: &[&str] = &[
        "queueBacklogLimit",
        "workflowBacklogLimit",
        "timerLagLimitMs",
        "retryExhaustionLimit",
        "compensationPressureLimit",
        "errorBudgetConsumedLimitBasisPoints",
        "readiness",
        "liveness",
    ];
    overrides
        .keys()
        .all(|field| ALLOWED_FIELDS.contains(&field.as_str()))
        && NUMERIC_FIELDS
            .iter()
            .all(|field| overrides.get(*field).is_none_or(Value::is_u64))
        && overrides
            .get("errorBudgetConsumedLimitBasisPoints")
            .is_none_or(|value| value.as_u64().is_some_and(|value| value <= 10_000))
        && overrides
            .get("readiness")
            .is_none_or(|value| matches!(value.as_str(), Some("serving" | "healthy")))
        && overrides.get("liveness").is_none_or(|value| {
            matches!(
                value.as_str(),
                Some("process_running" | "runtime_operational")
            )
        })
}

fn is_string_array(value: Option<&Value>) -> bool {
    value.and_then(Value::as_array).is_some_and(|values| {
        values
            .iter()
            .all(|value| value.as_str().is_some_and(|value| !value.trim().is_empty()))
    })
}

fn validate_service_owned_contract_header(
    object: &serde_json::Map<String, Value>,
    base: &str,
    issues: &mut Vec<AutonomousServiceIssue>,
) {
    for field in ["contractId", "version"] {
        if object
            .get(field)
            .and_then(Value::as_str)
            .is_none_or(|value| value.trim().is_empty())
        {
            push_autonomous_issue(
                issues,
                AutonomousServiceIssueCode::InvalidContractIdentity,
                format!("{base}.{field}"),
                format!("{field} must be a non-empty string"),
                "Assign a stable contract identity and version.",
            );
        }
    }
    if object
        .get("artifact")
        .and_then(|artifact| artifact.get("path"))
        .and_then(Value::as_str)
        .is_none_or(|path| !is_repository_relative_artifact_path(path))
    {
        push_autonomous_issue(
            issues,
            AutonomousServiceIssueCode::InvalidArtifactReference,
            format!("{base}.artifact.path"),
            "artifact path must be repository-relative without traversal",
            "Reference a packaged contract artifact using a repository-relative path.",
        );
    }
    if let Some(artifact) = object.get("artifact").and_then(Value::as_object) {
        validate_unknown_fields(artifact, &format!("{base}.artifact"), &["path"], issues);
    }
}

fn push_autonomous_issue(
    issues: &mut Vec<AutonomousServiceIssue>,
    code: AutonomousServiceIssueCode,
    path: impl Into<String>,
    message: impl Into<String>,
    next_action: impl Into<String>,
) {
    issues.push(AutonomousServiceIssue {
        code,
        path: path.into(),
        message: message.into(),
        next_action: next_action.into(),
    });
}

fn validate_unknown_fields(
    object: &serde_json::Map<String, Value>,
    path: &str,
    allowed: &[&str],
    issues: &mut Vec<AutonomousServiceIssue>,
) {
    let mut unknown = object
        .keys()
        .filter(|key| !allowed.contains(&key.as_str()))
        .collect::<Vec<_>>();
    unknown.sort();
    for field in unknown {
        push_autonomous_issue(
            issues,
            AutonomousServiceIssueCode::UnknownField,
            format!("{path}.{field}"),
            format!("unknown field `{field}`"),
            "Remove the field or upgrade to a contract version that declares it.",
        );
    }
}

fn validate_unique_strings(
    value: Option<&Value>,
    field: &str,
    invalid: AutonomousServiceIssueCode,
    duplicate: AutonomousServiceIssueCode,
    issues: &mut Vec<AutonomousServiceIssue>,
) {
    let Some(values) = value.and_then(Value::as_array) else {
        return;
    };
    let mut seen = BTreeSet::new();
    for (index, value) in values.iter().enumerate() {
        let path = format!("$.{field}[{index}]");
        let Some(identity) = value
            .as_str()
            .filter(|identity| !identity.trim().is_empty())
        else {
            push_autonomous_issue(
                issues,
                invalid,
                path,
                format!("{field} identity must be a non-empty string"),
                format!("Assign a non-empty {field} identity."),
            );
            continue;
        };
        if !seen.insert(identity) {
            push_autonomous_issue(
                issues,
                duplicate,
                path,
                format!("{field} identities must be unique"),
                format!("Remove or rename the duplicate {field} identity."),
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_owned_identities(
    value: Option<&Value>,
    field: &str,
    identity_field: &str,
    service_id: &str,
    invalid: AutonomousServiceIssueCode,
    owner_mismatch: AutonomousServiceIssueCode,
    duplicate: AutonomousServiceIssueCode,
    issues: &mut Vec<AutonomousServiceIssue>,
) {
    let Some(values) = value.and_then(Value::as_array) else {
        return;
    };
    let mut seen = BTreeSet::new();
    for (index, value) in values.iter().enumerate() {
        let base = format!("$.{field}[{index}]");
        if let Some(object) = value.as_object() {
            validate_unknown_fields(object, &base, &[identity_field, "serviceId"], issues);
        }
        let identity = value
            .get(identity_field)
            .and_then(Value::as_str)
            .unwrap_or("");
        if identity.trim().is_empty() {
            push_autonomous_issue(
                issues,
                invalid,
                format!("{base}.{identity_field}"),
                "identity must be a non-empty string",
                "Assign a stable logical identity.",
            );
        }
        if value.get("serviceId").and_then(Value::as_str) != Some(service_id) {
            push_autonomous_issue(
                issues,
                owner_mismatch,
                format!("{base}.serviceId"),
                "owner must match the enclosing serviceId",
                "Set serviceId to the enclosing Service identity.",
            );
        }
        if !identity.is_empty() && !seen.insert(identity) {
            push_autonomous_issue(
                issues,
                duplicate,
                format!("{base}.{identity_field}"),
                "identity must be unique within the Service",
                "Rename the duplicate identity.",
            );
        }
    }
}

#[must_use]
pub fn validate_service_contract_value(value: &Value) -> Vec<ServiceContractIssue> {
    let Some(object) = value.as_object() else {
        return vec![ServiceContractIssue::new(
            "$",
            "service contract must be an object",
        )];
    };

    let mut issues = Vec::new();
    if let Some(protocol) = object.get("protocol") {
        match protocol.as_str() {
            Some(SERVICE_CONTRACT_PROTOCOL) => {}
            Some(_) => issues.push(ServiceContractIssue::new(
                "$.protocol",
                format!("protocol must be `{SERVICE_CONTRACT_PROTOCOL}`"),
            )),
            None => issues.push(ServiceContractIssue::new(
                "$.protocol",
                "field must be a non-empty string",
            )),
        }
    }
    require_non_empty_string(object.get("name"), "$.name", &mut issues);
    if let Some(version) = object.get("version") {
        require_non_empty_string(Some(version), "$.version", &mut issues);
    }
    validate_provider(object.get("provider"), &mut issues);
    validate_named_fields_array(object.get("config"), "$.config", "key", &mut issues);
    validate_named_fields_array(object.get("env"), "$.env", "name", &mut issues);
    validate_string_array(
        object
            .get("requiredEnv")
            .or_else(|| object.get("required_env")),
        "$.requiredEnv",
        &mut issues,
    );
    validate_compatibility(object.get("compatibility"), &mut issues);
    validate_local_process(
        object
            .get("localProcess")
            .or_else(|| object.get("local_process")),
        "$.localProcess",
        &mut issues,
    );
    validate_install(object.get("install"), &mut issues);
    validate_modules(object.get("modules"), &mut issues);
    issues
}

#[must_use]
pub fn validate_service_package_value(value: &Value) -> Vec<ServiceContractIssue> {
    let Some(object) = value.as_object() else {
        return vec![ServiceContractIssue::new(
            "$",
            "service package must be an object",
        )];
    };

    let mut issues = Vec::new();
    match object.get("protocol").and_then(Value::as_str) {
        Some(SERVICE_PACKAGE_PROTOCOL) => {}
        Some(_) => issues.push(ServiceContractIssue::new(
            "$.protocol",
            format!("protocol must be `{SERVICE_PACKAGE_PROTOCOL}`"),
        )),
        None => issues.push(ServiceContractIssue::new(
            "$.protocol",
            "field must be a non-empty string",
        )),
    }
    require_non_empty_string(object.get("name"), "$.name", &mut issues);
    require_non_empty_string(object.get("version"), "$.version", &mut issues);
    require_non_empty_string(
        object
            .get("serviceManifest")
            .or_else(|| object.get("service_manifest")),
        "$.serviceManifest",
        &mut issues,
    );
    validate_service_package_modules(object.get("modules"), &mut issues);
    issues
}

#[must_use]
pub fn validate_service_workspace_value(value: &Value) -> Vec<ServiceContractIssue> {
    let Some(object) = value.as_object() else {
        return vec![ServiceContractIssue::new(
            "$",
            "service workspace must be an object",
        )];
    };

    let mut issues = Vec::new();
    match object.get("protocol").and_then(Value::as_str) {
        Some(SERVICE_WORKSPACE_PROTOCOL) => {}
        Some(_) => issues.push(ServiceContractIssue::new(
            "$.protocol",
            format!("protocol must be `{SERVICE_WORKSPACE_PROTOCOL}`"),
        )),
        None => issues.push(ServiceContractIssue::new(
            "$.protocol",
            "field must be a non-empty string",
        )),
    }
    validate_workspace_services(object.get("services"), &mut issues);
    issues
}

#[must_use]
pub fn validate_service_system_value(value: &Value) -> Vec<ServiceContractIssue> {
    let Some(object) = value.as_object() else {
        return vec![ServiceContractIssue::new(
            "$",
            "service system must be an object",
        )];
    };

    let mut issues = Vec::new();
    match object.get("protocol").and_then(Value::as_str) {
        Some(SERVICE_SYSTEM_PROTOCOL) => {}
        Some(_) => issues.push(ServiceContractIssue::new(
            "$.protocol",
            format!("protocol must be `{SERVICE_SYSTEM_PROTOCOL}`"),
        )),
        None => issues.push(ServiceContractIssue::new(
            "$.protocol",
            "field must be a non-empty string",
        )),
    }
    require_non_empty_string(object.get("name"), "$.name", &mut issues);
    validate_string_array(object.get("environments"), "$.environments", &mut issues);
    validate_system_services(object.get("services"), &mut issues);
    validate_system_modules(object.get("modules"), &mut issues);
    validate_system_dependencies(object.get("dependencies"), &mut issues);
    issues
}

#[must_use]
pub fn validate_module_contract_value(value: &Value) -> Vec<ServiceContractIssue> {
    let Some(object) = value.as_object() else {
        return vec![ServiceContractIssue::new(
            "$",
            "module contract must be an object",
        )];
    };

    let mut issues = Vec::new();
    match object.get("protocol").and_then(Value::as_str) {
        Some(MODULE_CONTRACT_PROTOCOL) => {}
        Some(_) => issues.push(ServiceContractIssue::new(
            "$.protocol",
            format!("protocol must be `{MODULE_CONTRACT_PROTOCOL}`"),
        )),
        None => issues.push(ServiceContractIssue::new(
            "$.protocol",
            "field must be a non-empty string",
        )),
    }
    require_non_empty_string(object.get("name"), "$.name", &mut issues);
    require_non_empty_string(object.get("version"), "$.version", &mut issues);
    validate_module_artifact_source(object.get("source"), "$.source", &mut issues);
    validate_string_array(object.get("capabilities"), "$.capabilities", &mut issues);
    validate_string_array(object.get("dependencies"), "$.dependencies", &mut issues);
    if let Some(manifest) = object.get("manifest")
        && !manifest.is_object()
    {
        issues.push(ServiceContractIssue::new(
            "$.manifest",
            "manifest must be an object",
        ));
    }
    issues
}

#[must_use]
pub fn validate_module_release_value(value: &Value) -> Vec<ServiceContractIssue> {
    let Some(object) = value.as_object() else {
        return vec![ServiceContractIssue::new(
            "$",
            "module release must be an object",
        )];
    };

    let mut issues = Vec::new();
    match object.get("protocol").and_then(Value::as_str) {
        Some(MODULE_RELEASE_PROTOCOL) => {}
        Some(_) => issues.push(ServiceContractIssue::new(
            "$.protocol",
            format!("protocol must be `{MODULE_RELEASE_PROTOCOL}`"),
        )),
        None => issues.push(ServiceContractIssue::new(
            "$.protocol",
            "field must be a non-empty string",
        )),
    }
    require_non_empty_string(object.get("name"), "$.name", &mut issues);
    require_non_empty_string(object.get("version"), "$.version", &mut issues);
    let source = object.get("source").and_then(Value::as_str);
    validate_module_artifact_source(object.get("source"), "$.source", &mut issues);
    match source {
        Some("service") => validate_module_release_provider(object.get("provider"), &mut issues),
        Some("linked" | "bundled") if object.get("provider").is_some() => {
            validate_module_release_provider(object.get("provider"), &mut issues);
        }
        _ => {}
    }
    validate_string_array(object.get("capabilities"), "$.capabilities", &mut issues);
    validate_string_array(object.get("dependencies"), "$.dependencies", &mut issues);
    issues
}

fn validate_module_artifact_source(
    value: Option<&Value>,
    path: &str,
    issues: &mut Vec<ServiceContractIssue>,
) {
    match value.and_then(Value::as_str) {
        Some("service" | "linked" | "bundled") => {}
        Some(_) => issues.push(ServiceContractIssue::new(
            path,
            "source must be `service`, `linked`, or `bundled`",
        )),
        None => issues.push(ServiceContractIssue::new(
            path,
            "field must be a non-empty string",
        )),
    }
}

fn validate_module_release_provider(value: Option<&Value>, issues: &mut Vec<ServiceContractIssue>) {
    let Some(value) = value else {
        issues.push(ServiceContractIssue::new(
            "$.provider",
            "provider must be an object",
        ));
        return;
    };
    let Some(object) = value.as_object() else {
        issues.push(ServiceContractIssue::new(
            "$.provider",
            "provider must be an object",
        ));
        return;
    };
    require_non_empty_string(object.get("name"), "$.provider.name", issues);
    if object
        .get("servicePackage")
        .or_else(|| object.get("service_package"))
        .or_else(|| object.get("serviceManifest"))
        .or_else(|| object.get("service_manifest"))
        .and_then(Value::as_str)
        .map(str::trim)
        .is_none_or(str::is_empty)
    {
        issues.push(ServiceContractIssue::new(
            "$.provider.servicePackage",
            "field must be a non-empty string",
        ));
    }
}

fn validate_provider(value: Option<&Value>, issues: &mut Vec<ServiceContractIssue>) {
    let Some(value) = value else {
        return;
    };
    if !value.is_object() {
        issues.push(ServiceContractIssue::new(
            "$.provider",
            "provider must be an object",
        ));
        return;
    }
    require_non_empty_string(value.get("name"), "$.provider.name", issues);
}

fn validate_compatibility(value: Option<&Value>, issues: &mut Vec<ServiceContractIssue>) {
    let Some(value) = value else {
        return;
    };
    let Some(object) = value.as_object() else {
        issues.push(ServiceContractIssue::new(
            "$.compatibility",
            "compatibility must be an object",
        ));
        return;
    };
    validate_string_array(
        object
            .get("requiredHostFeatures")
            .or_else(|| object.get("required_host_features")),
        "$.compatibility.requiredHostFeatures",
        issues,
    );
}

fn validate_named_fields_array(
    value: Option<&Value>,
    path: &str,
    name_field: &str,
    issues: &mut Vec<ServiceContractIssue>,
) {
    let Some(value) = value else {
        return;
    };
    let Some(array) = value.as_array() else {
        issues.push(ServiceContractIssue::new(path, "field must be an array"));
        return;
    };
    for (index, item) in array.iter().enumerate() {
        if !item.is_object() {
            issues.push(ServiceContractIssue::new(
                format!("{path}[{index}]"),
                "entry must be an object",
            ));
            continue;
        }
        require_non_empty_string(
            item.get(name_field),
            &format!("{path}[{index}].{name_field}"),
            issues,
        );
    }
}

fn validate_local_process(
    value: Option<&Value>,
    path: &str,
    issues: &mut Vec<ServiceContractIssue>,
) {
    let Some(value) = value else {
        return;
    };
    if !value.is_object() {
        issues.push(ServiceContractIssue::new(
            path,
            "localProcess must be an object",
        ));
        return;
    }
    require_non_empty_string(value.get("command"), &format!("{path}.command"), issues);
}

fn validate_install(value: Option<&Value>, issues: &mut Vec<ServiceContractIssue>) {
    let Some(value) = value else {
        return;
    };
    let Some(object) = value.as_object() else {
        issues.push(ServiceContractIssue::new(
            "$.install",
            "install must be an object",
        ));
        return;
    };
    let Some(services) = object.get("services") else {
        return;
    };
    let Some(array) = services.as_array() else {
        issues.push(ServiceContractIssue::new(
            "$.install.services",
            "install services must be an array",
        ));
        return;
    };
    for (index, service) in array.iter().enumerate() {
        if !service.is_object() {
            issues.push(ServiceContractIssue::new(
                format!("$.install.services[{index}]"),
                "service must be an object",
            ));
            continue;
        }
        require_non_empty_string(
            service.get("name"),
            &format!("$.install.services[{index}].name"),
            issues,
        );
        require_non_empty_string(
            service.get("command"),
            &format!("$.install.services[{index}].command"),
            issues,
        );
    }
}

fn validate_modules(value: Option<&Value>, issues: &mut Vec<ServiceContractIssue>) {
    let Some(value) = value else {
        issues.push(ServiceContractIssue::new(
            "$.modules",
            "modules must be an array",
        ));
        return;
    };
    let Some(array) = value.as_array() else {
        issues.push(ServiceContractIssue::new(
            "$.modules",
            "modules must be an array",
        ));
        return;
    };
    if array.is_empty() {
        issues.push(ServiceContractIssue::new(
            "$.modules",
            "modules must not be empty",
        ));
        return;
    }

    let mut names = BTreeSet::new();
    for (index, module) in array.iter().enumerate() {
        let Some(object) = module.as_object() else {
            issues.push(ServiceContractIssue::new(
                format!("$.modules[{index}]"),
                "module must be an object",
            ));
            continue;
        };
        let Some(module_name) = non_empty_string(
            object.get("name"),
            &format!("$.modules[{index}].name"),
            issues,
        ) else {
            continue;
        };
        if !names.insert(module_name.to_owned()) {
            issues.push(ServiceContractIssue::new(
                format!("$.modules[{index}].name"),
                format!("module `{module_name}` is declared more than once"),
            ));
        }
        validate_string_array(
            object.get("capabilities"),
            &format!("$.modules[{index}].capabilities"),
            issues,
        );
        validate_string_array(
            object.get("dependencies"),
            &format!("$.modules[{index}].dependencies"),
            issues,
        );
    }
}

fn validate_service_package_modules(value: Option<&Value>, issues: &mut Vec<ServiceContractIssue>) {
    let Some(value) = value else {
        issues.push(ServiceContractIssue::new(
            "$.modules",
            "modules must be an array",
        ));
        return;
    };
    let Some(array) = value.as_array() else {
        issues.push(ServiceContractIssue::new(
            "$.modules",
            "modules must be an array",
        ));
        return;
    };
    if array.is_empty() {
        issues.push(ServiceContractIssue::new(
            "$.modules",
            "modules must not be empty",
        ));
        return;
    }
    let mut names = BTreeSet::new();
    for (index, module) in array.iter().enumerate() {
        let Some(module_name) =
            non_empty_string(Some(module), &format!("$.modules[{index}]"), issues)
        else {
            continue;
        };
        if !names.insert(module_name.to_owned()) {
            issues.push(ServiceContractIssue::new(
                format!("$.modules[{index}]"),
                format!("module `{module_name}` is declared more than once"),
            ));
        }
    }
}

fn validate_workspace_services(value: Option<&Value>, issues: &mut Vec<ServiceContractIssue>) {
    let Some(value) = value else {
        return;
    };
    let Some(array) = value.as_array() else {
        issues.push(ServiceContractIssue::new(
            "$.services",
            "services must be an array",
        ));
        return;
    };
    let mut names = BTreeSet::new();
    for (index, service) in array.iter().enumerate() {
        let Some(object) = service.as_object() else {
            issues.push(ServiceContractIssue::new(
                format!("$.services[{index}]"),
                "service must be an object",
            ));
            continue;
        };
        let name = non_empty_string(
            object.get("name"),
            &format!("$.services[{index}].name"),
            issues,
        );
        if let Some(name) = name {
            if !names.insert(name.to_owned()) {
                issues.push(ServiceContractIssue::new(
                    format!("$.services[{index}].name"),
                    format!("service `{name}` is declared more than once"),
                ));
            }
        }
        require_non_empty_string(
            object.get("lang"),
            &format!("$.services[{index}].lang"),
            issues,
        );
        require_non_empty_string(
            object.get("cwd"),
            &format!("$.services[{index}].cwd"),
            issues,
        );
        require_non_empty_string(
            object.get("manifest"),
            &format!("$.services[{index}].manifest"),
            issues,
        );
        require_non_empty_string(
            object.get("command"),
            &format!("$.services[{index}].command"),
            issues,
        );
        require_non_empty_string(
            object.get("readyUrl").or_else(|| object.get("ready_url")),
            &format!("$.services[{index}].readyUrl"),
            issues,
        );
        validate_string_array(
            object.get("modules"),
            &format!("$.services[{index}].modules"),
            issues,
        );
    }
}

fn validate_system_services(value: Option<&Value>, issues: &mut Vec<ServiceContractIssue>) {
    let Some(value) = value else {
        return;
    };
    let Some(array) = value.as_array() else {
        issues.push(ServiceContractIssue::new(
            "$.services",
            "services must be an array",
        ));
        return;
    };
    let mut names = BTreeSet::new();
    for (index, service) in array.iter().enumerate() {
        let Some(object) = service.as_object() else {
            issues.push(ServiceContractIssue::new(
                format!("$.services[{index}]"),
                "service must be an object",
            ));
            continue;
        };
        if let Some(name) = non_empty_string(
            object.get("name"),
            &format!("$.services[{index}].name"),
            issues,
        ) && !names.insert(name.to_owned())
        {
            issues.push(ServiceContractIssue::new(
                format!("$.services[{index}].name"),
                format!("service `{name}` is declared more than once"),
            ));
        }
        require_non_empty_string(
            object.get("target"),
            &format!("$.services[{index}].target"),
            issues,
        );
        validate_string_array(
            object.get("modules"),
            &format!("$.services[{index}].modules"),
            issues,
        );
    }
}

fn validate_system_modules(value: Option<&Value>, issues: &mut Vec<ServiceContractIssue>) {
    let Some(value) = value else {
        return;
    };
    let Some(array) = value.as_array() else {
        issues.push(ServiceContractIssue::new(
            "$.modules",
            "modules must be an array",
        ));
        return;
    };
    let mut names = BTreeSet::new();
    for (index, module) in array.iter().enumerate() {
        let Some(object) = module.as_object() else {
            issues.push(ServiceContractIssue::new(
                format!("$.modules[{index}]"),
                "module must be an object",
            ));
            continue;
        };
        if let Some(name) = non_empty_string(
            object.get("name"),
            &format!("$.modules[{index}].name"),
            issues,
        ) && !names.insert(name.to_owned())
        {
            issues.push(ServiceContractIssue::new(
                format!("$.modules[{index}].name"),
                format!("module `{name}` is declared more than once"),
            ));
        }
        if let Some(install_to) = object.get("installTo").or_else(|| object.get("install_to")) {
            require_non_empty_string(
                Some(install_to),
                &format!("$.modules[{index}].installTo"),
                issues,
            );
        }
        validate_string_array(
            object.get("capabilities"),
            &format!("$.modules[{index}].capabilities"),
            issues,
        );
        validate_string_array(
            object.get("dependencies"),
            &format!("$.modules[{index}].dependencies"),
            issues,
        );
    }
}

fn validate_system_dependencies(value: Option<&Value>, issues: &mut Vec<ServiceContractIssue>) {
    let Some(value) = value else {
        return;
    };
    let Some(array) = value.as_array() else {
        issues.push(ServiceContractIssue::new(
            "$.dependencies",
            "dependencies must be an array",
        ));
        return;
    };
    for (index, dependency) in array.iter().enumerate() {
        let Some(object) = dependency.as_object() else {
            issues.push(ServiceContractIssue::new(
                format!("$.dependencies[{index}]"),
                "dependency must be an object",
            ));
            continue;
        };
        require_non_empty_string(
            object.get("from"),
            &format!("$.dependencies[{index}].from"),
            issues,
        );
        require_non_empty_string(
            object.get("capability"),
            &format!("$.dependencies[{index}].capability"),
            issues,
        );
        if let Some(to) = object.get("to") {
            require_non_empty_string(Some(to), &format!("$.dependencies[{index}].to"), issues);
        }
    }
}

fn validate_string_array(
    value: Option<&Value>,
    path: &str,
    issues: &mut Vec<ServiceContractIssue>,
) {
    let Some(value) = value else {
        return;
    };
    let Some(array) = value.as_array() else {
        issues.push(ServiceContractIssue::new(path, "field must be an array"));
        return;
    };
    for (index, item) in array.iter().enumerate() {
        require_non_empty_string(Some(item), &format!("{path}[{index}]"), issues);
    }
}

fn require_non_empty_string(
    value: Option<&Value>,
    path: &str,
    issues: &mut Vec<ServiceContractIssue>,
) {
    let _ = non_empty_string(value, path, issues);
}

fn non_empty_string<'a>(
    value: Option<&'a Value>,
    path: &str,
    issues: &mut Vec<ServiceContractIssue>,
) -> Option<&'a str> {
    match value.and_then(Value::as_str).map(str::trim) {
        Some(value) if !value.is_empty() => Some(value),
        _ => {
            issues.push(ServiceContractIssue::new(
                path,
                "field must be a non-empty string",
            ));
            None
        }
    }
}

fn service_base_url_from_url_suffix(value: &str, suffixes: &[&str]) -> Option<String> {
    let value = value.trim();
    if !(value.starts_with("http://") || value.starts_with("https://")) {
        return None;
    }
    let value = strip_query_fragment(value).trim_end_matches('/');
    suffixes.iter().find_map(|suffix| {
        value
            .strip_suffix(suffix)
            .map(|base_url| base_url.trim_end_matches('/'))
            .map(ToOwned::to_owned)
    })
}

fn strip_query_fragment(value: &str) -> &str {
    let query_index = value.find('?').unwrap_or(value.len());
    let fragment_index = value.find('#').unwrap_or(value.len());
    &value[..query_index.min(fragment_index)]
}

const fn service_release_risk_rank(risk: ServiceReleaseRisk) -> u8 {
    match risk {
        ServiceReleaseRisk::Safe => 0,
        ServiceReleaseRisk::NeedsAttention => 1,
        ServiceReleaseRisk::Breaking => 2,
        ServiceReleaseRisk::Blocked => 3,
    }
}

const fn is_false(value: &bool) -> bool {
    !*value
}

const fn default_service_auto_start() -> bool {
    true
}

const fn default_service_ready_timeout_ms() -> u64 {
    30_000
}

const fn default_workspace_service_ready_timeout_ms() -> u64 {
    10_000
}

fn default_service_manifest() -> String {
    "lenso.service.json".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn service_package_new_uses_v1_protocol() {
        let package = ServicePackage::new(
            "support-suite-provider",
            "0.2.0",
            vec!["support-ticket".to_owned()],
        );
        let value = serde_json::to_value(package).unwrap();

        assert_eq!(value["protocol"], SERVICE_PACKAGE_PROTOCOL);
        assert_eq!(value["serviceManifest"], "lenso.service.json");
        assert_eq!(value["modules"], json!(["support-ticket"]));
    }

    #[test]
    fn service_release_plan_uses_delivery_policy() {
        let diff = ServiceReleaseDiff {
            capabilities: vec![ServiceReleaseModuleChangeSet {
                module: "support-ticket".to_owned(),
                added: Vec::new(),
                removed: vec!["support_ticket.tickets.write".to_owned()],
            }],
            config: ServiceReleaseChangeSet {
                added: vec!["support.mode".to_owned()],
                removed: Vec::new(),
            },
            env: ServiceReleaseChangeSet {
                added: vec!["SUPPORT_API_KEY".to_owned()],
                removed: Vec::new(),
            },
            operations: vec![ServiceReleaseModuleChangeSet {
                module: "support-ticket".to_owned(),
                added: Vec::new(),
                removed: vec!["route:DELETE /tickets/{id}".to_owned()],
            }],
            ..ServiceReleaseDiff::default()
        };
        let current = ServiceReleaseManifestSummary {
            name: "support-suite-provider".to_owned(),
            version: Some("0.1.0".to_owned()),
            manifest_reference: "./support/v1/lenso.service.json".to_owned(),
            package_reference: None,
            input_reference: None,
            modules: vec!["support-ticket".to_owned()],
            compatibility_issue: None,
        };
        let candidate = ServiceReleaseManifestSummary {
            name: "support-suite-provider".to_owned(),
            version: Some("0.2.0".to_owned()),
            manifest_reference: "./support/v2/lenso.service.json".to_owned(),
            package_reference: Some("./support/v2/lenso.service-package.json".to_owned()),
            input_reference: None,
            modules: vec!["support-ticket".to_owned()],
            compatibility_issue: None,
        };

        let plan = ServiceReleasePlan::new("support-suite-provider", current, candidate, diff);
        let value = serde_json::to_value(plan).unwrap();

        assert_eq!(value["protocol"], SERVICE_RELEASE_PLAN_PROTOCOL);
        assert_eq!(value["policy"]["risk"], "breaking");
        assert_eq!(value["restartRequired"], true);
        assert_eq!(
            evaluate_service_release_policy(
                &ServiceReleaseDiff::default(),
                Some("remote protocol is newer"),
            )
            .risk,
            ServiceReleaseRisk::Blocked
        );
    }

    #[test]
    fn service_environment_round_trips_kubernetes_target() {
        let file = ServiceEnvironmentsFile {
            version: 1,
            environments: vec![ServiceEnvironment {
                namespace: Some("lenso-staging".to_owned()),
                kube_context: Some("staging".to_owned()),
                image: Some("ghcr.io/acme/support-suite-provider:0.4.0".to_owned()),
                public_base_url: Some("https://support-staging.example.com".to_owned()),
                release_track: Some("staging".to_owned()),
                config: Some(
                    KubernetesDeploymentConfig::new()
                        .replicas(2)
                        .port(4110)
                        .ingress_host("support-staging.example.com"),
                ),
                ..ServiceEnvironment::kubernetes("staging", "support-suite-provider")
            }],
        };

        let value = serde_json::to_value(&file).unwrap();
        assert_eq!(value["environments"][0]["target"], "kubernetes");
        assert_eq!(
            value["environments"][0]["serviceName"],
            "support-suite-provider"
        );
        assert_eq!(
            value["environments"][0]["config"]["ingressHost"],
            "support-staging.example.com"
        );

        let round_trip: ServiceEnvironmentsFile = serde_json::from_value(value).unwrap();
        assert_eq!(round_trip, file);
    }

    #[test]
    fn service_deployment_observation_uses_stable_state_names() {
        let observation = ServiceDeploymentObservation {
            service_name: "support-suite-provider".to_owned(),
            environment: "staging".to_owned(),
            target: ServiceDeploymentTarget::Kubernetes,
            observed_at_unix_ms: 1_803_744_000_000,
            state: ServiceDeploymentState::Ready,
            drift: ServiceDeploymentDrift::InSync,
            cluster: Some(KubernetesDeploymentObservation {
                namespace: "lenso-staging".to_owned(),
                deployment: "support-suite-provider".to_owned(),
                ready_replicas: Some(2),
                desired_replicas: Some(2),
                available_replicas: Some(2),
                image: Some("ghcr.io/acme/support-suite-provider:0.4.0".to_owned()),
                release_id: Some("rel_staging".to_owned()),
                manifest_reference: Some(
                    "https://support-staging.example.com/lenso/service/v1/manifest".to_owned(),
                ),
                service_endpoint: Some(
                    "support-suite-provider.lenso-staging.svc.cluster.local".to_owned(),
                ),
                ingress_host: Some("support-staging.example.com".to_owned()),
            }),
            host: Some(ServiceDeploymentHostObservation {
                release_id: Some("rel_staging".to_owned()),
                candidate_version: Some("0.4.0".to_owned()),
            }),
            checks: vec![ServiceDeploymentCheck {
                name: "deployment_rollout".to_owned(),
                status: "ok".to_owned(),
                detail: Some("2/2 replicas ready".to_owned()),
            }],
            next_action: Some("monitor rollout and Remote Calls".to_owned()),
        };

        let value = serde_json::to_value(&observation).unwrap();
        assert_eq!(value["state"], "ready");
        assert_eq!(value["drift"], "in_sync");
        assert_eq!(value["cluster"]["readyReplicas"], 2);

        let round_trip: ServiceDeploymentObservation = serde_json::from_value(value).unwrap();
        assert_eq!(round_trip, observation);
    }

    #[test]
    fn valid_service_package_has_no_issues() {
        let issues = validate_service_package_value(&json!({
            "protocol": "lenso.service-package.v1",
            "name": "support-suite-provider",
            "version": "0.2.0",
            "serviceManifest": "lenso.service.json",
            "modules": ["support-ticket", "support-inbox"]
        }));

        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn invalid_service_package_reports_protocol_and_modules() {
        let issues = validate_service_package_value(&json!({
            "protocol": "remote-module",
            "name": "support-suite-provider",
            "version": "0.2.0",
            "serviceManifest": "lenso.service.json",
            "modules": ["support-ticket", "support-ticket", ""]
        }));

        assert_eq!(
            issues
                .iter()
                .map(|issue| issue.path.as_str())
                .collect::<Vec<_>>(),
            vec!["$.protocol", "$.modules[1]", "$.modules[2]"]
        );
    }

    #[test]
    fn valid_service_workspace_has_no_issues() {
        let issues = validate_service_workspace_value(&json!({
            "protocol": "lenso.service-workspace.v1",
            "services": [
                {
                    "name": "support-suite-provider",
                    "lang": "ts",
                    "cwd": "services/support-suite-provider",
                    "manifest": "lenso.service.json",
                    "command": "pnpm start",
                    "readyUrl": "http://127.0.0.1:4110/lenso/service/v1/status",
                    "modules": ["support-ticket"]
                }
            ]
        }));

        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn service_workspace_exports_module_service_start_file() {
        let workspace = ServiceWorkspace::new(vec![ServiceWorkspaceService {
            name: "support-suite-provider".to_owned(),
            lang: "ts".to_owned(),
            cwd: "services/support-suite-provider".to_owned(),
            manifest: "lenso.service.json".to_owned(),
            command: "pnpm start".to_owned(),
            ready_url: "http://127.0.0.1:4110/lenso/service/v1/status".to_owned(),
            auto_start: true,
            ready_timeout_ms: 10_000,
            modules: vec!["support-ticket".to_owned()],
        }]);

        let value = serde_json::to_value(service_workspace_to_module_services(&workspace)).unwrap();

        assert_eq!(value["version"], 1);
        assert_eq!(value["modules"][0]["moduleName"], "support-suite-provider");
        assert_eq!(value["modules"][0]["services"][0]["command"], "pnpm start");
        assert_eq!(
            value["modules"][0]["services"][0]["readyUrl"],
            "http://127.0.0.1:4110/lenso/service/v1/status"
        );
    }

    #[test]
    fn service_workspace_infers_service_base_url() {
        assert_eq!(
            service_base_url_from_ready_url(
                "http://127.0.0.1:4110/lenso/service/v1/status?probe=1"
            )
            .as_deref(),
            Some("http://127.0.0.1:4110/lenso/service/v1")
        );
        assert_eq!(
            service_base_url_from_manifest_url("http://127.0.0.1:4110/lenso/service/v1/manifest")
                .as_deref(),
            Some("http://127.0.0.1:4110/lenso/service/v1")
        );
        assert_eq!(
            service_workspace_base_url(&ServiceWorkspaceService {
                name: "support-suite-provider".to_owned(),
                lang: "ts".to_owned(),
                cwd: "services/support-suite-provider".to_owned(),
                manifest: "lenso.service.json".to_owned(),
                command: "pnpm start".to_owned(),
                ready_url: "http://127.0.0.1:4110/lenso/service/v1/ready".to_owned(),
                auto_start: true,
                ready_timeout_ms: 10_000,
                modules: vec!["support-ticket".to_owned()],
            })
            .as_deref(),
            Some("http://127.0.0.1:4110/lenso/service/v1")
        );
        assert!(service_base_url_from_ready_url("not a url").is_none());
    }

    #[test]
    fn invalid_service_workspace_reports_service_paths() {
        let issues = validate_service_workspace_value(&json!({
            "protocol": "lenso.workspace",
            "services": [
                {
                    "name": "",
                    "modules": ["support-ticket", 42]
                }
            ]
        }));

        assert_eq!(
            issues
                .iter()
                .map(|issue| issue.path.as_str())
                .collect::<Vec<_>>(),
            vec![
                "$.protocol",
                "$.services[0].name",
                "$.services[0].lang",
                "$.services[0].cwd",
                "$.services[0].manifest",
                "$.services[0].command",
                "$.services[0].readyUrl",
                "$.services[0].modules[1]"
            ]
        );
    }

    #[test]
    fn module_contract_new_uses_v1_protocol() {
        let contract = ModuleContract::new("support-ticket", "0.2.0", "linked")
            .capabilities(vec!["support_ticket.tickets.read".to_owned()])
            .dependencies(vec!["auth".to_owned()]);
        let value = serde_json::to_value(contract).unwrap();

        assert_eq!(value["protocol"], MODULE_CONTRACT_PROTOCOL);
        assert_eq!(value["source"], "linked");
        assert_eq!(
            value["capabilities"],
            json!(["support_ticket.tickets.read"])
        );
        assert_eq!(value["dependencies"], json!(["auth"]));
        assert!(validate_module_contract_value(&value).is_empty());
    }

    #[test]
    fn invalid_module_contract_reports_protocol_source_and_arrays() {
        let issues = validate_module_contract_value(&json!({
            "protocol": "lenso.module",
            "name": "",
            "version": "",
            "source": "remote",
            "capabilities": ["support_ticket.read", 42],
            "manifest": []
        }));

        assert_eq!(
            issues
                .iter()
                .map(|issue| issue.path.as_str())
                .collect::<Vec<_>>(),
            vec![
                "$.protocol",
                "$.name",
                "$.version",
                "$.source",
                "$.capabilities[1]",
                "$.manifest"
            ]
        );
    }

    #[test]
    fn module_release_new_uses_v1_protocol() {
        let release = ModuleRelease::new("support-ticket", "0.2.0", "support-suite-provider")
            .capabilities(vec!["support_ticket.tickets.read".to_owned()])
            .dependencies(vec!["auth".to_owned()]);
        let value = serde_json::to_value(release).unwrap();

        assert_eq!(value["protocol"], MODULE_RELEASE_PROTOCOL);
        assert_eq!(value["source"], "service");
        assert_eq!(
            value["provider"]["servicePackage"],
            "lenso.service-package.json"
        );
        assert_eq!(
            value["capabilities"],
            json!(["support_ticket.tickets.read"])
        );
        assert_eq!(value["dependencies"], json!(["auth"]));
    }

    #[test]
    fn valid_module_release_has_no_issues() {
        let issues = validate_module_release_value(&json!({
            "protocol": "lenso.module-release.v1",
            "name": "support-ticket",
            "version": "0.2.0",
            "source": "service",
            "provider": {
                "name": "support-suite-provider",
                "serviceManifest": "https://example.test/lenso/service/v1/manifest"
            },
            "capabilities": ["support_ticket.tickets.read"]
        }));

        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn linked_module_release_does_not_require_provider() {
        let issues = validate_module_release_value(&json!({
            "protocol": "lenso.module-release.v1",
            "name": "auth-password",
            "version": "0.2.0",
            "source": "linked",
            "capabilities": ["auth.password.login"]
        }));

        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn invalid_module_release_reports_protocol_source_provider_and_capabilities() {
        let issues = validate_module_release_value(&json!({
            "protocol": "remote-module",
            "name": "",
            "version": "",
            "source": "remote",
            "provider": { "name": "" },
            "capabilities": ["support_ticket.read", 42]
        }));

        assert_eq!(
            issues
                .iter()
                .map(|issue| issue.path.as_str())
                .collect::<Vec<_>>(),
            vec![
                "$.protocol",
                "$.name",
                "$.version",
                "$.source",
                "$.capabilities[1]"
            ]
        );
    }
}
