use crate::{
    AutonomousServiceContract, AutonomousServiceStore, AutonomousServiceWorkload,
    CommonContextRequirement, ContractContextRequirements, DirectGrpcBindings, DirectHttpBindings,
    EventArtifactFormat, EventArtifactReference, EventContractArtifact,
    ExtractionContractArtifactFormat, ExtractionContractDirection, ExtractionContractKind,
    ExtractionInputPinKind, ExtractionPlan, ExtractionPlanInputs, ExtractionWorkloadRole,
    ModuleManifest, ServiceArtifactFormat, ServiceArtifactReference, ServiceContractArtifact,
    ServiceTenancyMode, WorkloadRole, ensure_extraction_plan_fresh, extraction_input_digest,
    extraction_plan_integrity_is_valid, generate_direct_grpc_bindings,
    generate_direct_http_bindings, validate_autonomous_service_artifact_references,
    validate_autonomous_service_contract,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};

pub const EXTRACTION_SCAFFOLD_PROTOCOL: &str = "lenso.extraction-scaffold.v1";
pub const EXTRACTION_SCAFFOLD_GENERATOR_VERSION: &str = "lenso.extraction-scaffold-generator.v1";
const EXTRACTION_SCAFFOLD_SCHEMA_ID: &str =
    "https://contracts.lenso.local/extraction/lenso.extraction-scaffold.v1.schema.json";
const EXTRACTION_SCAFFOLD_APPLY_PROTOCOL: &str = "lenso.extraction-scaffold-apply.v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractionScaffoldArtifact {
    pub contract_id: String,
    pub version: String,
    pub contents: String,
    pub protobuf_descriptor: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct ExtractionScaffoldInputs {
    pub plan: ExtractionPlan,
    pub module: ModuleManifest,
    pub artifacts: Vec<ExtractionScaffoldArtifact>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionScaffoldFileKind {
    CargoManifest,
    WorkloadEntrypoint,
    ModuleManifest,
    ServiceManifest,
    ContractArtifact,
    GeneratedBinding,
    ServiceClient,
    MigrationGuide,
    OwnershipReceipt,
    Readme,
    RustLibrary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionScaffoldFile {
    pub path: String,
    pub kind: ExtractionScaffoldFileKind,
    pub digest: String,
    pub contents: String,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionGeneratedBindingKind {
    Http,
    Grpc,
    Event,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionScaffoldBindingRole {
    Server,
    Client,
    Publisher,
    Handler,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionGeneratedBinding {
    pub contract_id: String,
    pub version: String,
    pub kind: ExtractionGeneratedBindingKind,
    pub role: ExtractionScaffoldBindingRole,
    pub artifact_path: String,
    pub artifact_digest: String,
    pub binding_path: String,
    pub binding_digest: String,
    pub tenancy_mode: ServiceTenancyMode,
    #[serde(default)]
    pub required_context: Vec<CommonContextRequirement>,
    #[serde(default)]
    pub operation_ids: Vec<String>,
    #[serde(default)]
    pub event_types: Vec<String>,
    #[serde(default)]
    pub generated_client_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionPreservedIdentity {
    pub module_name: String,
    pub module_manifest_digest: String,
    pub module_manifest: Value,
    pub capabilities: Vec<String>,
    pub operation_ids: Vec<String>,
    pub event_types: Vec<String>,
    pub runtime_function_names: Vec<String>,
    pub schedule_names: Vec<String>,
    pub workflow_identities: Vec<String>,
    pub story_titles: Vec<String>,
    pub admin_identity: Value,
    pub console_identity: Value,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionScaffoldEffects {
    pub writes_repository_files: bool,
    pub starts_workloads: bool,
    pub copies_data: bool,
    pub changes_authority: bool,
    pub changes_provider_path: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionScaffold {
    pub protocol: String,
    pub generator_version: String,
    pub scaffold_id: String,
    pub scaffold_digest: String,
    pub plan_id: String,
    pub plan_digest: String,
    pub target_module: String,
    pub candidate_service_id: String,
    pub destination_root: String,
    pub linked_authority_remains_authoritative: bool,
    pub provider_compatibility_preserved: bool,
    pub preserved_identity: ExtractionPreservedIdentity,
    pub candidate_service: Value,
    pub bindings: Vec<ExtractionGeneratedBinding>,
    pub local_behavior_ids: Vec<String>,
    pub boundary_replacements: Vec<String>,
    pub files: Vec<ExtractionScaffoldFile>,
    pub patch: String,
    pub effects: ExtractionScaffoldEffects,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExtractionScaffoldContent<'a> {
    protocol: &'a str,
    generator_version: &'a str,
    plan_id: &'a str,
    plan_digest: &'a str,
    target_module: &'a str,
    candidate_service_id: &'a str,
    destination_root: &'a str,
    linked_authority_remains_authoritative: bool,
    provider_compatibility_preserved: bool,
    preserved_identity: &'a ExtractionPreservedIdentity,
    candidate_service: &'a Value,
    bindings: &'a [ExtractionGeneratedBinding],
    local_behavior_ids: &'a [String],
    boundary_replacements: &'a [String],
    files: &'a [ExtractionScaffoldFile],
    patch: &'a str,
    effects: ExtractionScaffoldEffects,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionScaffoldGenerationIssueCode {
    PlanInvalid,
    ModuleIdentityMismatch,
    ArtifactMissing,
    ArtifactUnrecognized,
    ArtifactDigestMismatch,
    ArtifactInvalid,
    BindingGenerationFailed,
    OperationIdentityMismatch,
    EventIdentityMismatch,
    CandidateInvalid,
    InvalidPath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionScaffoldGenerationError {
    pub code: ExtractionScaffoldGenerationIssueCode,
    pub message: String,
    pub next_actions: Vec<String>,
}

impl fmt::Display for ExtractionScaffoldGenerationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ExtractionScaffoldGenerationError {}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionScaffoldIssueCode {
    IntegrityInvalid,
    FileDigestInvalid,
    FilePathInvalid,
    FileOrderInvalid,
    PatchInvalid,
    ModuleIdentityChanged,
    CandidateServiceInvalid,
    WorkloadEntrypointMissing,
    BindingInvalid,
    AuthorityChanged,
    ProviderPathChanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionScaffoldIssue {
    pub code: ExtractionScaffoldIssueCode,
    pub path: String,
    pub message: String,
    pub next_action: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionScaffoldApplyErrorCode {
    PlanStale,
    ScaffoldInvalid,
    ScaffoldConflict,
    RepositoryInvalid,
    Io,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionScaffoldApplyError {
    pub code: ExtractionScaffoldApplyErrorCode,
    pub message: String,
    pub conflicting_paths: Vec<String>,
    pub next_actions: Vec<String>,
    pub effects: ExtractionScaffoldEffects,
}

impl fmt::Display for ExtractionScaffoldApplyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ExtractionScaffoldApplyError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionScaffoldApplyResult {
    pub protocol: String,
    pub scaffold_id: String,
    pub plan_id: String,
    pub created_files: Vec<String>,
    pub unchanged_files: Vec<String>,
    pub linked_authority_remains_authoritative: bool,
    pub effects: ExtractionScaffoldEffects,
}

#[derive(Debug)]
struct GeneratedBindingOutput {
    binding: ExtractionGeneratedBinding,
    contents: String,
}

pub fn generate_extraction_scaffold(
    inputs: &ExtractionScaffoldInputs,
) -> Result<ExtractionScaffold, ExtractionScaffoldGenerationError> {
    validate_scaffold_inputs(inputs)?;
    let plan = &inputs.plan;
    let destination_root = format!("services/{}", plan.proposed_service.service_id);
    validate_relative_path(&destination_root).map_err(|message| {
        generation_error(
            ExtractionScaffoldGenerationIssueCode::InvalidPath,
            message,
            "Use a stable Service identity that produces a repository-relative destination.",
        )
    })?;

    let artifacts = inputs
        .artifacts
        .iter()
        .map(|artifact| {
            (
                (artifact.contract_id.as_str(), artifact.version.as_str()),
                artifact,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut files = Vec::new();
    let mut generated_bindings = Vec::new();
    for contract in &plan.proposed_service.contract_versions {
        let artifact = artifacts
            .get(&(contract.contract_id.as_str(), contract.version.as_str()))
            .expect("validated artifacts cover every planned Contract Version");
        files.push(scaffold_file(
            &destination_root,
            &contract.artifact_reference,
            ExtractionScaffoldFileKind::ContractArtifact,
            artifact.contents.clone(),
        )?);
        let output = generated_binding(plan, contract, artifact, &destination_root)?;
        files.push(scaffold_file(
            "",
            &output.binding.binding_path,
            ExtractionScaffoldFileKind::GeneratedBinding,
            output.contents,
        )?);
        generated_bindings.push(output.binding);
    }
    generated_bindings.sort();
    validate_manifest_contract_identities(&inputs.module, &generated_bindings)?;

    let candidate_service = candidate_service(plan)?;
    let candidate_service_value = serde_json::to_value(&candidate_service).map_err(|error| {
        generation_error(
            ExtractionScaffoldGenerationIssueCode::CandidateInvalid,
            format!("Candidate Service could not serialize: {error}"),
            "Correct the approved plan and regenerate the scaffold.",
        )
    })?;
    let available_paths = plan
        .proposed_service
        .contract_versions
        .iter()
        .map(|contract| contract.artifact_reference.clone())
        .collect::<BTreeSet<_>>();
    let mut candidate_issues = validate_autonomous_service_contract(&candidate_service);
    candidate_issues.extend(validate_autonomous_service_artifact_references(
        &candidate_service_value,
        &available_paths,
    ));
    if let Some(issue) = candidate_issues.first() {
        return Err(generation_error(
            ExtractionScaffoldGenerationIssueCode::CandidateInvalid,
            format!(
                "Candidate Service is invalid at {}: {}",
                issue.path, issue.message
            ),
            issue.next_action.clone(),
        ));
    }

    let identity = preserved_identity(&inputs.module)?;
    let crate_name = rust_identifier(&format!("{}_candidate", plan.proposed_service.service_id));
    let module_json = pretty_json(&identity.module_manifest)?;
    let service_json = pretty_json(&candidate_service_value)?;
    files.extend([
        scaffold_file(
            &destination_root,
            "Cargo.toml",
            ExtractionScaffoldFileKind::CargoManifest,
            cargo_manifest(&crate_name, &plan.proposed_service.workloads),
        )?,
        scaffold_file(
            &destination_root,
            "lenso.module.json",
            ExtractionScaffoldFileKind::ModuleManifest,
            module_json,
        )?,
        scaffold_file(
            &destination_root,
            "lenso.service.json",
            ExtractionScaffoldFileKind::ServiceManifest,
            service_json,
        )?,
        scaffold_file(
            &destination_root,
            "src/lib.rs",
            ExtractionScaffoldFileKind::RustLibrary,
            rust_library(&identity, &plan.proposed_service.service_id),
        )?,
        scaffold_file(
            &destination_root,
            "README.md",
            ExtractionScaffoldFileKind::Readme,
            candidate_readme(plan),
        )?,
        scaffold_file(
            &destination_root,
            "migrations/README.md",
            ExtractionScaffoldFileKind::MigrationGuide,
            migration_readme(plan),
        )?,
    ]);
    for workload in &plan.proposed_service.workloads {
        let role = workload_role_label(workload.role);
        files.push(scaffold_file(
            &destination_root,
            &format!("src/bin/{role}.rs"),
            ExtractionScaffoldFileKind::WorkloadEntrypoint,
            workload_entrypoint(
                &plan.proposed_service.service_id,
                &inputs.module.name,
                &workload.workload_id,
                role,
            ),
        )?);
    }

    let mut boundary_replacements = Vec::new();
    for client in &plan.proposed_service.generated_clients {
        let binding = generated_bindings
            .iter()
            .find(|binding| {
                binding.contract_id == client.contract_id && binding.version == client.version
            })
            .ok_or_else(|| {
                generation_error(
                    ExtractionScaffoldGenerationIssueCode::BindingGenerationFailed,
                    format!(
                        "Generated client `{}` has no authoritative binding.",
                        client.client_id
                    ),
                    "Pin and supply the authoritative HTTP or gRPC Contract artifact.",
                )
            })?;
        let path = format!("generated/clients/{}.json", stable_slug(&client.client_id));
        let value = json!({
            "protocol": "lenso.generated-service-client.v1",
            "clientId": client.client_id,
            "ownerId": client.owner_id,
            "contractId": client.contract_id,
            "version": client.version,
            "transport": binding_kind_label(binding.kind),
            "artifact": client.artifact_reference,
            "binding": strip_destination(&destination_root, &binding.binding_path),
            "operationIds": binding.operation_ids,
            "tenancyMode": binding.tenancy_mode,
            "requiredContext": binding.required_context,
        });
        files.push(scaffold_file(
            &destination_root,
            &path,
            ExtractionScaffoldFileKind::ServiceClient,
            pretty_json(&value)?,
        )?);
        boundary_replacements.push(client.client_id.clone());
    }
    boundary_replacements.sort();
    boundary_replacements.dedup();

    files.sort_by(|left, right| left.path.cmp(&right.path));
    ensure_unique_file_paths(&files)?;
    let managed_files = files
        .iter()
        .map(|file| json!({ "path": file.path, "digest": file.digest }))
        .collect::<Vec<_>>();
    let receipt = json!({
        "protocol": "lenso.extraction-scaffold-ownership.v1",
        "generatorVersion": EXTRACTION_SCAFFOLD_GENERATOR_VERSION,
        "planId": plan.plan_id,
        "planDigest": plan.plan_digest,
        "linkedAuthorityRemainsAuthoritative": true,
        "managedFiles": managed_files,
    });
    files.push(scaffold_file(
        &destination_root,
        ".lenso/extraction-scaffold.json",
        ExtractionScaffoldFileKind::OwnershipReceipt,
        pretty_json(&receipt)?,
    )?);
    files.sort_by(|left, right| left.path.cmp(&right.path));

    let local_behavior_ids = local_behavior_ids(&inputs.module);
    let patch = render_patch(&files);
    let effects = ExtractionScaffoldEffects::default();
    let content = ExtractionScaffoldContent {
        protocol: EXTRACTION_SCAFFOLD_PROTOCOL,
        generator_version: EXTRACTION_SCAFFOLD_GENERATOR_VERSION,
        plan_id: &plan.plan_id,
        plan_digest: &plan.plan_digest,
        target_module: &inputs.module.name,
        candidate_service_id: &plan.proposed_service.service_id,
        destination_root: &destination_root,
        linked_authority_remains_authoritative: true,
        provider_compatibility_preserved: true,
        preserved_identity: &identity,
        candidate_service: &candidate_service_value,
        bindings: &generated_bindings,
        local_behavior_ids: &local_behavior_ids,
        boundary_replacements: &boundary_replacements,
        files: &files,
        patch: &patch,
        effects,
    };
    let scaffold_digest = digest_serializable(&content)?;
    let scaffold = ExtractionScaffold {
        protocol: EXTRACTION_SCAFFOLD_PROTOCOL.to_owned(),
        generator_version: EXTRACTION_SCAFFOLD_GENERATOR_VERSION.to_owned(),
        scaffold_id: format!("extraction-scaffold:{scaffold_digest}"),
        scaffold_digest,
        plan_id: plan.plan_id.clone(),
        plan_digest: plan.plan_digest.clone(),
        target_module: inputs.module.name.clone(),
        candidate_service_id: plan.proposed_service.service_id.clone(),
        destination_root,
        linked_authority_remains_authoritative: true,
        provider_compatibility_preserved: true,
        preserved_identity: identity,
        candidate_service: candidate_service_value,
        bindings: generated_bindings,
        local_behavior_ids,
        boundary_replacements,
        files,
        patch,
        effects,
    };
    debug_assert!(extraction_scaffold_integrity_is_valid(&scaffold));
    Ok(scaffold)
}

pub fn dry_run_extraction_scaffold(
    inputs: &ExtractionScaffoldInputs,
) -> Result<ExtractionScaffold, ExtractionScaffoldGenerationError> {
    generate_extraction_scaffold(inputs)
}

fn validate_scaffold_inputs(
    inputs: &ExtractionScaffoldInputs,
) -> Result<(), ExtractionScaffoldGenerationError> {
    if !extraction_plan_integrity_is_valid(&inputs.plan) {
        return Err(generation_error(
            ExtractionScaffoldGenerationIssueCode::PlanInvalid,
            "The approved Extraction Plan failed content-address validation.",
            "Discard the modified plan and generate a fresh Extraction Plan.",
        ));
    }
    if inputs.plan.target_module != inputs.module.name
        || inputs.plan.proposed_service.module_id != inputs.module.name
    {
        return Err(generation_error(
            ExtractionScaffoldGenerationIssueCode::ModuleIdentityMismatch,
            "The linked Module identity does not match the approved Extraction Plan.",
            "Load the exact Module declaration pinned by the plan.",
        ));
    }
    let module_digest = digest_serializable(&inputs.module)?;
    let module_pin = inputs.plan.pinned_inputs.iter().find(|pin| {
        pin.kind == ExtractionInputPinKind::ModuleDeclaration && pin.subject == inputs.module.name
    });
    if module_pin.is_none_or(|pin| pin.digest != module_digest) {
        return Err(generation_error(
            ExtractionScaffoldGenerationIssueCode::ModuleIdentityMismatch,
            "The current Module declaration differs from the plan-pinned declaration.",
            "Regenerate the Extraction Plan from the current Module declaration.",
        ));
    }
    let planned = inputs
        .plan
        .proposed_service
        .contract_versions
        .iter()
        .map(|contract| (contract.contract_id.as_str(), contract.version.as_str()))
        .collect::<BTreeSet<_>>();
    let supplied = inputs
        .artifacts
        .iter()
        .map(|artifact| (artifact.contract_id.as_str(), artifact.version.as_str()))
        .collect::<BTreeSet<_>>();
    if supplied.len() != inputs.artifacts.len() {
        return Err(generation_error(
            ExtractionScaffoldGenerationIssueCode::ArtifactUnrecognized,
            "The scaffold input contains duplicate Contract artifacts.",
            "Supply exactly one authoritative artifact for every planned Contract Version.",
        ));
    }
    if let Some((contract_id, version)) = planned.difference(&supplied).next() {
        return Err(generation_error(
            ExtractionScaffoldGenerationIssueCode::ArtifactMissing,
            format!("Contract artifact `{contract_id}@{version}` is missing."),
            "Supply the exact artifact pinned by the approved Extraction Plan.",
        ));
    }
    if let Some((contract_id, version)) = supplied.difference(&planned).next() {
        return Err(generation_error(
            ExtractionScaffoldGenerationIssueCode::ArtifactUnrecognized,
            format!("Contract artifact `{contract_id}@{version}` is not in the plan."),
            "Remove unplanned artifacts and regenerate the deterministic scaffold.",
        ));
    }
    for contract in &inputs.plan.proposed_service.contract_versions {
        let artifact = inputs
            .artifacts
            .iter()
            .find(|artifact| {
                artifact.contract_id == contract.contract_id && artifact.version == contract.version
            })
            .expect("planned and supplied identity sets match");
        if extraction_input_digest(artifact.contents.as_bytes()) != contract.artifact_digest {
            return Err(generation_error(
                ExtractionScaffoldGenerationIssueCode::ArtifactDigestMismatch,
                format!(
                    "Contract artifact `{}@{}` does not match its pinned digest.",
                    contract.contract_id, contract.version
                ),
                "Load the exact authoritative artifact or regenerate the Extraction Plan.",
            ));
        }
        validate_relative_path(&contract.artifact_reference).map_err(|message| {
            generation_error(
                ExtractionScaffoldGenerationIssueCode::InvalidPath,
                message,
                "Use a repository-relative authoritative Contract artifact path.",
            )
        })?;
    }
    Ok(())
}

fn generated_binding(
    plan: &ExtractionPlan,
    contract: &crate::ExtractionPlanContractVersion,
    artifact: &ExtractionScaffoldArtifact,
    destination_root: &str,
) -> Result<GeneratedBindingOutput, ExtractionScaffoldGenerationError> {
    let role = match (contract.kind, contract.direction) {
        (ExtractionContractKind::Service, ExtractionContractDirection::Provides) => {
            ExtractionScaffoldBindingRole::Server
        }
        (ExtractionContractKind::Service, ExtractionContractDirection::Consumes) => {
            ExtractionScaffoldBindingRole::Client
        }
        (ExtractionContractKind::Event, ExtractionContractDirection::Provides) => {
            ExtractionScaffoldBindingRole::Publisher
        }
        (ExtractionContractKind::Event, ExtractionContractDirection::Consumes) => {
            ExtractionScaffoldBindingRole::Handler
        }
    };
    let client_ids = plan
        .proposed_service
        .generated_clients
        .iter()
        .filter(|client| {
            client.contract_id == contract.contract_id && client.version == contract.version
        })
        .map(|client| client.client_id.clone())
        .collect::<Vec<_>>();
    let binding_name = format!(
        "{}-{}-{}",
        stable_slug(&contract.contract_id),
        stable_slug(&contract.version),
        binding_role_label(role)
    );
    let binding_relative = format!("generated/bindings/{binding_name}.json");
    let binding_path = format!("{destination_root}/{binding_relative}");
    let (kind, contents, operation_ids, event_types) = match contract.artifact_format {
        ExtractionContractArtifactFormat::Openapi => {
            let document = serde_yaml::from_str::<Value>(&artifact.contents).map_err(|error| {
                generation_error(
                    ExtractionScaffoldGenerationIssueCode::ArtifactInvalid,
                    format!(
                        "OpenAPI artifact `{}@{}` is invalid: {error}",
                        contract.contract_id, contract.version
                    ),
                    "Correct the authoritative OpenAPI artifact and regenerate the plan.",
                )
            })?;
            let bindings =
                generate_direct_http_bindings(&contract.contract_id, &contract.version, &document)
                    .map_err(|error| {
                        generation_error(
                            ExtractionScaffoldGenerationIssueCode::BindingGenerationFailed,
                            error.to_string(),
                            "Correct the authoritative OpenAPI operation and policy declarations.",
                        )
                    })?;
            let operation_ids = bindings
                .operations
                .iter()
                .map(|operation| operation.operation_id.clone())
                .collect();
            (
                ExtractionGeneratedBindingKind::Http,
                pretty_json(&serde_json::to_value(bindings).expect("bindings serialize"))?,
                operation_ids,
                Vec::new(),
            )
        }
        ExtractionContractArtifactFormat::Protobuf
            if contract.kind == ExtractionContractKind::Service =>
        {
            let descriptor = artifact.protobuf_descriptor.as_deref().ok_or_else(|| {
                generation_error(
                    ExtractionScaffoldGenerationIssueCode::ArtifactInvalid,
                    format!(
                        "Protobuf artifact `{}@{}` is missing its generated descriptor.",
                        contract.contract_id, contract.version
                    ),
                    "Generate the descriptor from the exact pinned Protobuf source.",
                )
            })?;
            let bindings = generate_direct_grpc_bindings(
                &contract.contract_id,
                &contract.version,
                &artifact.contents,
                descriptor,
            )
            .map_err(|error| {
                generation_error(
                    ExtractionScaffoldGenerationIssueCode::BindingGenerationFailed,
                    error,
                    "Correct the authoritative Protobuf operations and call-policy annotations.",
                )
            })?;
            let operation_ids = bindings
                .operations
                .iter()
                .map(|operation| operation.operation_id.clone())
                .collect();
            (
                ExtractionGeneratedBindingKind::Grpc,
                pretty_json(&serde_json::to_value(bindings).expect("bindings serialize"))?,
                operation_ids,
                Vec::new(),
            )
        }
        ExtractionContractArtifactFormat::JsonSchema => {
            let schema = serde_json::from_str::<Value>(&artifact.contents).map_err(|error| {
                generation_error(
                    ExtractionScaffoldGenerationIssueCode::ArtifactInvalid,
                    format!(
                        "Event JSON Schema `{}@{}` is invalid: {error}",
                        contract.contract_id, contract.version
                    ),
                    "Correct the authoritative Event Contract schema.",
                )
            })?;
            jsonschema::validator_for(&schema).map_err(|error| {
                generation_error(
                    ExtractionScaffoldGenerationIssueCode::ArtifactInvalid,
                    format!(
                        "Event JSON Schema `{}@{}` cannot compile: {error}",
                        contract.contract_id, contract.version
                    ),
                    "Correct the authoritative Event Contract schema.",
                )
            })?;
            let event_type = event_type_from_schema(&schema, &contract.artifact_reference)?;
            let binding = json!({
                "protocol": "lenso.generated-event-binding.v1",
                "contractId": contract.contract_id,
                "version": contract.version,
                "eventType": event_type,
                "direction": contract.direction,
                "artifact": contract.artifact_reference,
                "artifactDigest": contract.artifact_digest,
                "tenancyMode": contract.tenancy_mode,
                "requiredContext": contract.required_context,
            });
            (
                ExtractionGeneratedBindingKind::Event,
                pretty_json(&binding)?,
                Vec::new(),
                vec![event_type],
            )
        }
        ExtractionContractArtifactFormat::Protobuf => {
            return Err(generation_error(
                ExtractionScaffoldGenerationIssueCode::BindingGenerationFailed,
                format!(
                    "Event Protobuf scaffold generation is not available for `{}@{}`.",
                    contract.contract_id, contract.version
                ),
                "Use the authoritative JSON Schema Event Contract for this beta scaffold.",
            ));
        }
    };
    Ok(GeneratedBindingOutput {
        binding: ExtractionGeneratedBinding {
            contract_id: contract.contract_id.clone(),
            version: contract.version.clone(),
            kind,
            role,
            artifact_path: format!("{destination_root}/{}", contract.artifact_reference),
            artifact_digest: contract.artifact_digest.clone(),
            binding_path,
            binding_digest: extraction_input_digest(contents.as_bytes()),
            tenancy_mode: contract.tenancy_mode.clone(),
            required_context: contract.required_context.clone(),
            operation_ids,
            event_types,
            generated_client_ids: client_ids,
        },
        contents,
    })
}

fn validate_manifest_contract_identities(
    module: &ModuleManifest,
    bindings: &[ExtractionGeneratedBinding],
) -> Result<(), ExtractionScaffoldGenerationError> {
    let http_bindings = bindings
        .iter()
        .filter(|binding| {
            binding.kind == ExtractionGeneratedBindingKind::Http
                && binding.role == ExtractionScaffoldBindingRole::Server
        })
        .collect::<Vec<_>>();
    for route in &module.http_routes {
        let Some(operation_id) = route
            .operation
            .as_ref()
            .and_then(|operation| operation.operation_id.as_deref())
        else {
            continue;
        };
        if !http_bindings.iter().any(|binding| {
            binding
                .operation_ids
                .iter()
                .any(|item| item == operation_id)
        }) {
            return Err(generation_error(
                ExtractionScaffoldGenerationIssueCode::OperationIdentityMismatch,
                format!(
                    "Module operation `{operation_id}` is absent from the authoritative provided HTTP binding."
                ),
                "Keep the Module operation identifier and authoritative OpenAPI operationId identical.",
            ));
        }
    }
    let consumed_event_types = bindings
        .iter()
        .filter(|binding| binding.role == ExtractionScaffoldBindingRole::Handler)
        .flat_map(|binding| binding.event_types.iter())
        .collect::<BTreeSet<_>>();
    for handler in module
        .events
        .iter()
        .flat_map(|events| events.handlers.iter())
    {
        if !consumed_event_types.contains(&handler.event_name) {
            return Err(generation_error(
                ExtractionScaffoldGenerationIssueCode::EventIdentityMismatch,
                format!(
                    "Module Event type `{}` is absent from the authoritative consumed Event bindings.",
                    handler.event_name
                ),
                "Keep the Module Event type and authoritative Event Contract title identical.",
            ));
        }
    }
    Ok(())
}

fn candidate_service(
    plan: &ExtractionPlan,
) -> Result<AutonomousServiceContract, ExtractionScaffoldGenerationError> {
    let service_id = &plan.proposed_service.service_id;
    let workloads = plan
        .proposed_service
        .workloads
        .iter()
        .map(|workload| {
            AutonomousServiceWorkload::new(
                &workload.workload_id,
                service_id,
                match workload.role {
                    ExtractionWorkloadRole::Api => WorkloadRole::API,
                    ExtractionWorkloadRole::Worker => WorkloadRole::WORKER,
                    ExtractionWorkloadRole::Migration => WorkloadRole::MIGRATION,
                },
            )
        })
        .collect();
    let tenancy_mode = plan
        .proposed_service
        .contract_versions
        .iter()
        .map(|contract| contract.tenancy_mode.clone())
        .max()
        .unwrap_or(ServiceTenancyMode::None);
    let mut service = AutonomousServiceContract::new(
        service_id,
        workloads,
        tenancy_mode,
        vec!["local-sandbox".to_owned()],
    );
    service.version = Some("0.0.0-extraction-candidate".to_owned());
    service.modules = vec![plan.target_module.clone()];
    service.stores = vec![AutonomousServiceStore::new(
        &plan.proposed_service.store.store_id,
        service_id,
    )];
    for contract in &plan.proposed_service.contract_versions {
        if contract.direction != ExtractionContractDirection::Provides {
            continue;
        }
        match contract.kind {
            ExtractionContractKind::Service => {
                let format = match contract.artifact_format {
                    ExtractionContractArtifactFormat::Openapi => ServiceArtifactFormat::Openapi,
                    ExtractionContractArtifactFormat::Protobuf => ServiceArtifactFormat::Protobuf,
                    ExtractionContractArtifactFormat::JsonSchema => {
                        return Err(generation_error(
                            ExtractionScaffoldGenerationIssueCode::CandidateInvalid,
                            "A provided Service Contract cannot use JSON Schema as its transport artifact.",
                            "Correct the approved Contract Version.",
                        ));
                    }
                };
                let mut declaration = ServiceContractArtifact::new(
                    &contract.contract_id,
                    &plan.target_module,
                    &contract.version,
                    contract.tenancy_mode.clone(),
                    ServiceArtifactReference::new(format, &contract.artifact_reference),
                );
                declaration.context =
                    ContractContextRequirements::new(contract.required_context.clone());
                service.service_contracts.push(declaration);
            }
            ExtractionContractKind::Event => {
                let format = match contract.artifact_format {
                    ExtractionContractArtifactFormat::JsonSchema => EventArtifactFormat::JsonSchema,
                    ExtractionContractArtifactFormat::Protobuf => EventArtifactFormat::Protobuf,
                    ExtractionContractArtifactFormat::Openapi => {
                        return Err(generation_error(
                            ExtractionScaffoldGenerationIssueCode::CandidateInvalid,
                            "A provided Event Contract cannot use OpenAPI as its artifact.",
                            "Correct the approved Contract Version.",
                        ));
                    }
                };
                let mut declaration = EventContractArtifact::new(
                    &contract.contract_id,
                    &plan.target_module,
                    &contract.version,
                    contract.tenancy_mode.clone(),
                    EventArtifactReference::new(format, &contract.artifact_reference),
                );
                declaration.context =
                    ContractContextRequirements::new(contract.required_context.clone());
                service.event_contracts.push(declaration);
            }
        }
    }
    service.service_contracts.sort_by(|left, right| {
        (&left.contract_id, &left.version).cmp(&(&right.contract_id, &right.version))
    });
    service.event_contracts.sort_by(|left, right| {
        (&left.contract_id, &left.version).cmp(&(&right.contract_id, &right.version))
    });
    Ok(service)
}

fn preserved_identity(
    module: &ModuleManifest,
) -> Result<ExtractionPreservedIdentity, ExtractionScaffoldGenerationError> {
    let module_manifest = serde_json::to_value(module).map_err(|error| {
        generation_error(
            ExtractionScaffoldGenerationIssueCode::ModuleIdentityMismatch,
            format!("Module declaration could not serialize: {error}"),
            "Correct the linked Module declaration before extraction.",
        )
    })?;
    let runtime = module.runtime.as_ref();
    let mut operation_ids = module
        .http_routes
        .iter()
        .filter_map(|route| route.operation.as_ref()?.operation_id.clone())
        .chain(runtime.into_iter().flat_map(|runtime| {
            runtime
                .functions
                .iter()
                .filter_map(|function| function.operation.as_ref()?.operation_id.clone())
        }))
        .collect::<Vec<_>>();
    normalize_strings(&mut operation_ids);
    let mut event_types = module
        .events
        .iter()
        .flat_map(|events| {
            events
                .handlers
                .iter()
                .map(|handler| handler.event_name.clone())
        })
        .collect::<Vec<_>>();
    normalize_strings(&mut event_types);
    let mut runtime_function_names = runtime
        .into_iter()
        .flat_map(|runtime| {
            runtime
                .functions
                .iter()
                .map(|function| function.name.clone())
        })
        .collect::<Vec<_>>();
    normalize_strings(&mut runtime_function_names);
    let mut schedule_names = runtime
        .into_iter()
        .flat_map(|runtime| {
            runtime
                .schedules
                .iter()
                .map(|schedule| schedule.name.clone())
        })
        .collect::<Vec<_>>();
    normalize_strings(&mut schedule_names);
    let mut workflow_identities = runtime
        .into_iter()
        .flat_map(|runtime| {
            runtime.workflows.iter().map(|workflow| {
                format!("{}/{}@{}", workflow.owner, workflow.name, workflow.version)
            })
        })
        .collect::<Vec<_>>();
    normalize_strings(&mut workflow_identities);
    let mut story_titles = module
        .http_routes
        .iter()
        .filter_map(|route| route.story_title.clone())
        .chain(
            module
                .story_display
                .iter()
                .filter_map(|story| story.story_title.clone()),
        )
        .collect::<Vec<_>>();
    normalize_strings(&mut story_titles);
    Ok(ExtractionPreservedIdentity {
        module_name: module.name.clone(),
        module_manifest_digest: digest_serializable(module)?,
        module_manifest,
        capabilities: module.capabilities.clone(),
        operation_ids,
        event_types,
        runtime_function_names,
        schedule_names,
        workflow_identities,
        story_titles,
        admin_identity: serde_json::to_value(&module.admin).expect("admin identity serializes"),
        console_identity: serde_json::to_value(&module.console)
            .expect("console identity serializes"),
    })
}

fn local_behavior_ids(module: &ModuleManifest) -> Vec<String> {
    let mut behaviors = module
        .http_routes
        .iter()
        .map(|route| {
            route
                .operation
                .as_ref()
                .and_then(|operation| operation.operation_id.clone())
                .unwrap_or_else(|| format!("{} {}", http_method_label(route.method), route.path))
        })
        .chain(module.events.iter().flat_map(|events| {
            events
                .handlers
                .iter()
                .map(|handler| format!("event-handler:{}", handler.name))
        }))
        .chain(module.runtime.iter().flat_map(|runtime| {
            runtime
                .functions
                .iter()
                .map(|function| format!("runtime-function:{}", function.name))
        }))
        .chain(module.runtime.iter().flat_map(|runtime| {
            runtime
                .schedules
                .iter()
                .map(|schedule| format!("schedule:{}", schedule.name))
        }))
        .chain(module.runtime.iter().flat_map(|runtime| {
            runtime.workflows.iter().map(|workflow| {
                format!(
                    "workflow:{}/{}@{}",
                    workflow.owner, workflow.name, workflow.version
                )
            })
        }))
        .collect::<Vec<_>>();
    normalize_strings(&mut behaviors);
    behaviors
}

fn scaffold_file(
    root: &str,
    relative: &str,
    kind: ExtractionScaffoldFileKind,
    contents: String,
) -> Result<ExtractionScaffoldFile, ExtractionScaffoldGenerationError> {
    let path = if root.is_empty() {
        relative.to_owned()
    } else {
        format!("{root}/{relative}")
    };
    validate_relative_path(&path).map_err(|message| {
        generation_error(
            ExtractionScaffoldGenerationIssueCode::InvalidPath,
            message,
            "Use repository-relative generated file paths without traversal or platform prefixes.",
        )
    })?;
    Ok(ExtractionScaffoldFile {
        path,
        kind,
        digest: extraction_input_digest(contents.as_bytes()),
        contents,
    })
}

fn ensure_unique_file_paths(
    files: &[ExtractionScaffoldFile],
) -> Result<(), ExtractionScaffoldGenerationError> {
    let mut paths = BTreeSet::new();
    if let Some(file) = files.iter().find(|file| !paths.insert(&file.path)) {
        return Err(generation_error(
            ExtractionScaffoldGenerationIssueCode::InvalidPath,
            format!("Generated path `{}` is duplicated.", file.path),
            "Correct the Contract identities so every generated output path is unique.",
        ));
    }
    Ok(())
}

fn event_type_from_schema(
    schema: &Value,
    artifact_reference: &str,
) -> Result<String, ExtractionScaffoldGenerationError> {
    let title = schema
        .get("title")
        .and_then(Value::as_str)
        .filter(|title| !title.trim().is_empty())
        .ok_or_else(|| {
            generation_error(
                ExtractionScaffoldGenerationIssueCode::EventIdentityMismatch,
                "An Event Contract schema must expose its stable Event type as `title`.",
                "Set the JSON Schema title to the authoritative Event type.",
            )
        })?;
    let file_identity = artifact_reference
        .rsplit('/')
        .next()
        .and_then(|name| name.strip_suffix(".schema.json"));
    if file_identity != Some(title) {
        return Err(generation_error(
            ExtractionScaffoldGenerationIssueCode::EventIdentityMismatch,
            format!("Event type `{title}` does not match artifact `{artifact_reference}`."),
            "Keep the Event type and authoritative schema filename identical.",
        ));
    }
    Ok(title.to_owned())
}

fn cargo_manifest(crate_name: &str, workloads: &[crate::ExtractionWorkloadPlan]) -> String {
    let mut output = format!(
        "[package]\nname = \"{crate_name}\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n[lib]\npath = \"src/lib.rs\"\n"
    );
    for workload in workloads {
        let role = workload_role_label(workload.role);
        output.push_str(&format!(
            "\n[[bin]]\nname = \"{}\"\npath = \"src/bin/{role}.rs\"\n",
            workload.workload_id
        ));
    }
    output
}

fn rust_library(identity: &ExtractionPreservedIdentity, service_id: &str) -> String {
    format!(
        "//! Generated extraction candidate. The linked Module remains authoritative.\n\npub const SERVICE_ID: &str = {};\npub const MODULE_ID: &str = {};\npub const LINKED_AUTHORITY_REMAINS_AUTHORITATIVE: bool = true;\npub const MODULE_MANIFEST_JSON: &str = include_str!(\"../lenso.module.json\");\npub const SERVICE_MANIFEST_JSON: &str = include_str!(\"../lenso.service.json\");\n\n#[must_use]\npub fn validate_public_entrypoints() -> bool {{\n    !SERVICE_ID.is_empty()\n        && !MODULE_ID.is_empty()\n        && LINKED_AUTHORITY_REMAINS_AUTHORITATIVE\n        && !MODULE_MANIFEST_JSON.is_empty()\n        && !SERVICE_MANIFEST_JSON.is_empty()\n}}\n",
        rust_string(service_id),
        rust_string(&identity.module_name),
    )
}

fn workload_entrypoint(service_id: &str, module_id: &str, workload_id: &str, role: &str) -> String {
    format!(
        "//! Generated {role} Workload validation entrypoint.\n\npub const SERVICE_ID: &str = {};\npub const MODULE_ID: &str = {};\npub const WORKLOAD_ID: &str = {};\npub const WORKLOAD_ROLE: &str = {};\n\npub fn run() {{\n    println!(\"{{{{\\\"serviceId\\\":\\\"{{}}\\\",\\\"moduleId\\\":\\\"{{}}\\\",\\\"workloadId\\\":\\\"{{}}\\\",\\\"role\\\":\\\"{{}}\\\",\\\"authority\\\":\\\"linked_host\\\"}}}}\", SERVICE_ID, MODULE_ID, WORKLOAD_ID, WORKLOAD_ROLE);\n}}\n\nfn main() {{\n    run();\n}}\n",
        rust_string(service_id),
        rust_string(module_id),
        rust_string(workload_id),
        rust_string(role),
    )
}

fn candidate_readme(plan: &ExtractionPlan) -> String {
    format!(
        "# {} extraction candidate\n\nThis scaffold preserves Module `{}` and is bound to `{}`.\n\nThe linked Host remains authoritative. These API, Worker, and Migration entrypoints validate the candidate shape only; Store expansion and Workload startup belong to the next approved Extraction Plan phase.\n\nGenerated Contract bindings live under `generated/bindings/`; only planned cross-Service boundaries receive generated clients under `generated/clients/`. Existing Provider v1 files and behavior are not changed.\n",
        plan.proposed_service.service_id, plan.target_module, plan.plan_id
    )
}

fn migration_readme(plan: &ExtractionPlan) -> String {
    let migrations = plan
        .data_mapping
        .migrations
        .iter()
        .map(|migration| format!("- `{}`", migration.source_migration))
        .collect::<Vec<_>>();
    format!(
        "# Candidate migrations\n\nThe Migration Workload is scaffolded, but this phase does not apply schema or data changes. The expand-first phase must copy the plan-owned migrations from authoritative Module sources and bind receipts to `{}`.\n\nPlanned Module migrations:\n{}\n",
        plan.plan_id,
        if migrations.is_empty() {
            "- none declared".to_owned()
        } else {
            migrations.join("\n")
        }
    )
}

#[must_use]
pub fn render_extraction_scaffold_patch(scaffold: &ExtractionScaffold) -> String {
    render_patch(&scaffold.files)
}

fn render_patch(files: &[ExtractionScaffoldFile]) -> String {
    let mut output = String::new();
    for file in files {
        let line_count = file.contents.lines().count();
        output.push_str(&format!(
            "diff --git a/{0} b/{0}\nnew file mode 100644\n--- /dev/null\n+++ b/{0}\n@@ -0,0 +1,{line_count} @@\n",
            file.path
        ));
        for line in file.contents.split_inclusive('\n') {
            output.push('+');
            output.push_str(line);
            if !line.ends_with('\n') {
                output.push('\n');
                output.push_str("\\ No newline at end of file\n");
            }
        }
    }
    output
}

#[must_use]
pub fn extraction_scaffold_integrity_is_valid(scaffold: &ExtractionScaffold) -> bool {
    if scaffold.protocol != EXTRACTION_SCAFFOLD_PROTOCOL
        || scaffold.generator_version != EXTRACTION_SCAFFOLD_GENERATOR_VERSION
        || scaffold.scaffold_id != format!("extraction-scaffold:{}", scaffold.scaffold_digest)
    {
        return false;
    }
    let content = scaffold_content(scaffold);
    digest_serializable(&content).is_ok_and(|digest| digest == scaffold.scaffold_digest)
}

fn scaffold_content(scaffold: &ExtractionScaffold) -> ExtractionScaffoldContent<'_> {
    ExtractionScaffoldContent {
        protocol: &scaffold.protocol,
        generator_version: &scaffold.generator_version,
        plan_id: &scaffold.plan_id,
        plan_digest: &scaffold.plan_digest,
        target_module: &scaffold.target_module,
        candidate_service_id: &scaffold.candidate_service_id,
        destination_root: &scaffold.destination_root,
        linked_authority_remains_authoritative: scaffold.linked_authority_remains_authoritative,
        provider_compatibility_preserved: scaffold.provider_compatibility_preserved,
        preserved_identity: &scaffold.preserved_identity,
        candidate_service: &scaffold.candidate_service,
        bindings: &scaffold.bindings,
        local_behavior_ids: &scaffold.local_behavior_ids,
        boundary_replacements: &scaffold.boundary_replacements,
        files: &scaffold.files,
        patch: &scaffold.patch,
        effects: scaffold.effects,
    }
}

#[must_use]
pub fn validate_extraction_scaffold(scaffold: &ExtractionScaffold) -> Vec<ExtractionScaffoldIssue> {
    let mut issues = Vec::new();
    if !extraction_scaffold_integrity_is_valid(scaffold) {
        push_issue(
            &mut issues,
            ExtractionScaffoldIssueCode::IntegrityInvalid,
            "$.scaffoldDigest",
            "The scaffold content address is invalid.",
            "Discard the modified scaffold and regenerate it from the approved plan.",
        );
    }
    if !scaffold.linked_authority_remains_authoritative || scaffold.effects.changes_authority {
        push_issue(
            &mut issues,
            ExtractionScaffoldIssueCode::AuthorityChanged,
            "$.linkedAuthorityRemainsAuthoritative",
            "The scaffold phase must not change Module authority.",
            "Keep the linked Host authoritative until protected Cutover.",
        );
    }
    if !scaffold.provider_compatibility_preserved || scaffold.effects.changes_provider_path {
        push_issue(
            &mut issues,
            ExtractionScaffoldIssueCode::ProviderPathChanged,
            "$.providerCompatibilityPreserved",
            "The scaffold must not reinterpret or modify Provider v1 behavior.",
            "Generate the candidate under the Autonomous Service v2 destination only.",
        );
    }
    if scaffold.effects.starts_workloads
        || scaffold.effects.copies_data
        || scaffold.effects.writes_repository_files
    {
        push_issue(
            &mut issues,
            ExtractionScaffoldIssueCode::AuthorityChanged,
            "$.effects",
            "A generated or dry-run scaffold must have zero effects.",
            "Use apply only after reviewing the deterministic patch.",
        );
    }
    let mut previous = None;
    let mut paths = BTreeSet::new();
    for (index, file) in scaffold.files.iter().enumerate() {
        let path = format!("$.files[{index}]");
        if validate_relative_path(&file.path).is_err()
            || !file
                .path
                .starts_with(&format!("{}/", scaffold.destination_root))
        {
            push_issue(
                &mut issues,
                ExtractionScaffoldIssueCode::FilePathInvalid,
                &format!("{path}.path"),
                "A generated file path escapes the candidate Service destination.",
                "Regenerate the scaffold with repository-relative candidate paths.",
            );
        }
        if !paths.insert(&file.path)
            || previous.is_some_and(|item: &str| item >= file.path.as_str())
        {
            push_issue(
                &mut issues,
                ExtractionScaffoldIssueCode::FileOrderInvalid,
                &format!("{path}.path"),
                "Generated file paths must be unique and deterministically ordered.",
                "Regenerate the scaffold instead of reordering files manually.",
            );
        }
        previous = Some(file.path.as_str());
        if extraction_input_digest(file.contents.as_bytes()) != file.digest {
            push_issue(
                &mut issues,
                ExtractionScaffoldIssueCode::FileDigestInvalid,
                &format!("{path}.digest"),
                "A generated file no longer matches its digest.",
                "Discard the changed scaffold and regenerate it.",
            );
        }
    }
    if scaffold.patch != render_patch(&scaffold.files) {
        push_issue(
            &mut issues,
            ExtractionScaffoldIssueCode::PatchInvalid,
            "$.patch",
            "The review patch does not match the generated files.",
            "Regenerate the scaffold and review the exact patch.",
        );
    }
    validate_scaffold_manifests(scaffold, &mut issues);
    validate_scaffold_bindings(scaffold, &mut issues);
    for role in ["api", "worker", "migration"] {
        let expected = format!("{}/src/bin/{role}.rs", scaffold.destination_root);
        if !paths.contains(&expected) {
            push_issue(
                &mut issues,
                ExtractionScaffoldIssueCode::WorkloadEntrypointMissing,
                "$.files",
                &format!("The {role} Workload entrypoint is missing."),
                "Regenerate all planned Workload entrypoints.",
            );
        }
    }
    issues.sort_by(|left, right| (&left.path, &left.code).cmp(&(&right.path, &right.code)));
    issues
}

fn validate_scaffold_manifests(
    scaffold: &ExtractionScaffold,
    issues: &mut Vec<ExtractionScaffoldIssue>,
) {
    let module_path = format!("{}/lenso.module.json", scaffold.destination_root);
    let module = scaffold.files.iter().find(|file| file.path == module_path);
    let module_matches = module
        .and_then(|file| serde_json::from_str::<Value>(&file.contents).ok())
        .is_some_and(|value| value == scaffold.preserved_identity.module_manifest);
    if !module_matches {
        push_issue(
            issues,
            ExtractionScaffoldIssueCode::ModuleIdentityChanged,
            "$.preservedIdentity.moduleManifest",
            "The generated Module declaration does not preserve the linked Module identity.",
            "Regenerate the candidate from the exact plan-pinned ModuleManifest.",
        );
    }
    let candidate_issues = validate_autonomous_service_contract_value_for_scaffold(
        &scaffold.candidate_service,
        scaffold,
    );
    if let Some(message) = candidate_issues.first() {
        push_issue(
            issues,
            ExtractionScaffoldIssueCode::CandidateServiceInvalid,
            "$.candidateService",
            message,
            "Regenerate the candidate Service from the approved plan.",
        );
    }
}

fn validate_autonomous_service_contract_value_for_scaffold(
    value: &Value,
    scaffold: &ExtractionScaffold,
) -> Vec<String> {
    let contract = serde_json::from_value::<AutonomousServiceContract>(value.clone());
    let Ok(contract) = contract else {
        return vec!["Candidate Service JSON cannot be decoded.".to_owned()];
    };
    let mut messages = validate_autonomous_service_contract(&contract)
        .into_iter()
        .map(|issue| issue.message)
        .collect::<Vec<_>>();
    if contract.service_id != scaffold.candidate_service_id
        || contract.modules != [scaffold.target_module.clone()]
    {
        messages.push("Candidate Service or Module identity changed.".to_owned());
    }
    messages
}

fn validate_scaffold_bindings(
    scaffold: &ExtractionScaffold,
    issues: &mut Vec<ExtractionScaffoldIssue>,
) {
    for (index, binding) in scaffold.bindings.iter().enumerate() {
        let file = scaffold
            .files
            .iter()
            .find(|file| file.path == binding.binding_path);
        let valid_digest = file.is_some_and(|file| file.digest == binding.binding_digest);
        let valid_identity = file.is_some_and(|file| match binding.kind {
            ExtractionGeneratedBindingKind::Http => {
                serde_json::from_str::<DirectHttpBindings>(&file.contents).is_ok_and(|value| {
                    value.contract_id == binding.contract_id
                        && value.version == binding.version
                        && value
                            .operations
                            .iter()
                            .map(|operation| &operation.operation_id)
                            .eq(binding.operation_ids.iter())
                })
            }
            ExtractionGeneratedBindingKind::Grpc => {
                serde_json::from_str::<DirectGrpcBindings>(&file.contents).is_ok_and(|value| {
                    value.contract_id == binding.contract_id
                        && value.version == binding.version
                        && value
                            .operations
                            .iter()
                            .map(|operation| &operation.operation_id)
                            .eq(binding.operation_ids.iter())
                })
            }
            ExtractionGeneratedBindingKind::Event => serde_json::from_str::<Value>(&file.contents)
                .is_ok_and(|value| {
                    value.get("contractId").and_then(Value::as_str)
                        == Some(binding.contract_id.as_str())
                        && value.get("version").and_then(Value::as_str)
                            == Some(binding.version.as_str())
                        && value.get("eventType").and_then(Value::as_str)
                            == binding.event_types.first().map(String::as_str)
                }),
        });
        if !valid_digest || !valid_identity {
            push_issue(
                issues,
                ExtractionScaffoldIssueCode::BindingInvalid,
                &format!("$.bindings[{index}]"),
                "A generated binding does not match its authoritative Contract identity.",
                "Regenerate bindings from the exact pinned Contract artifact.",
            );
        }
    }
}

pub fn apply_extraction_scaffold(
    repository_root: &Path,
    scaffold: &ExtractionScaffold,
    plan: &ExtractionPlan,
    current_inputs: &ExtractionPlanInputs,
) -> Result<ExtractionScaffoldApplyResult, ExtractionScaffoldApplyError> {
    if !repository_root.is_dir() {
        return Err(apply_error(
            ExtractionScaffoldApplyErrorCode::RepositoryInvalid,
            "The scaffold repository root is not an existing directory.",
            Vec::new(),
            "Select the intended repository root before applying the scaffold.",
        ));
    }
    let issues = validate_extraction_scaffold(scaffold);
    if !issues.is_empty()
        || plan.plan_id != scaffold.plan_id
        || plan.plan_digest != scaffold.plan_digest
        || !extraction_plan_integrity_is_valid(plan)
    {
        return Err(apply_error(
            ExtractionScaffoldApplyErrorCode::ScaffoldInvalid,
            "The scaffold or its approved Extraction Plan failed integrity validation.",
            Vec::new(),
            "Discard the modified artifacts and regenerate the exact scaffold patch.",
        ));
    }
    ensure_extraction_plan_fresh(plan, current_inputs).map_err(|rejection| {
        ExtractionScaffoldApplyError {
            code: ExtractionScaffoldApplyErrorCode::PlanStale,
            message: rejection.message,
            conflicting_paths: Vec::new(),
            next_actions: rejection.next_actions,
            effects: ExtractionScaffoldEffects::default(),
        }
    })?;

    let mut created = Vec::new();
    let mut unchanged = Vec::new();
    let mut conflicts = Vec::new();
    for file in &scaffold.files {
        let path = repository_root.join(&file.path);
        ensure_no_symlink_ancestors(repository_root, &path).map_err(|message| {
            apply_error(
                ExtractionScaffoldApplyErrorCode::ScaffoldConflict,
                message,
                vec![file.path.clone()],
                "Remove the symlinked target or choose a clean extraction destination.",
            )
        })?;
        match fs::read(&path) {
            Ok(contents) if contents == file.contents.as_bytes() => {
                unchanged.push(file.path.clone());
            }
            Ok(_) => conflicts.push(file.path.clone()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if path.exists() {
                    conflicts.push(file.path.clone());
                } else {
                    created.push(file.path.clone());
                }
            }
            Err(error) => {
                return Err(apply_error(
                    ExtractionScaffoldApplyErrorCode::Io,
                    format!("Could not inspect `{}`: {error}", file.path),
                    vec![file.path.clone()],
                    "Resolve repository permissions and retry the unchanged scaffold.",
                ));
            }
        }
    }
    if !conflicts.is_empty() {
        conflicts.sort();
        return Err(apply_error(
            ExtractionScaffoldApplyErrorCode::ScaffoldConflict,
            "Scaffold apply refused to overwrite changed or unrecognized user files.",
            conflicts,
            "Review the conflicting files, move the candidate destination, or regenerate from the current repository state.",
        ));
    }

    for file in scaffold
        .files
        .iter()
        .filter(|file| created.binary_search(&file.path).is_ok())
    {
        let path = repository_root.join(&file.path);
        let parent = path.parent().expect("generated files have a parent");
        fs::create_dir_all(parent).map_err(|error| {
            apply_error(
                ExtractionScaffoldApplyErrorCode::Io,
                format!("Could not create `{}`: {error}", parent.display()),
                vec![file.path.clone()],
                "Resolve repository permissions and retry the same content-addressed scaffold.",
            )
        })?;
        ensure_no_symlink_ancestors(repository_root, &path).map_err(|message| {
            apply_error(
                ExtractionScaffoldApplyErrorCode::ScaffoldConflict,
                message,
                vec![file.path.clone()],
                "Remove the symlinked target or choose a clean extraction destination.",
            )
        })?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| {
                apply_error(
                    ExtractionScaffoldApplyErrorCode::ScaffoldConflict,
                    format!("Refused to create `{}`: {error}", file.path),
                    vec![file.path.clone()],
                    "Inspect the target created after preflight and retry only after it is resolved.",
                )
            })?;
        output
            .write_all(file.contents.as_bytes())
            .map_err(|error| {
                apply_error(
                    ExtractionScaffoldApplyErrorCode::Io,
                    format!("Could not write `{}`: {error}", file.path),
                    vec![file.path.clone()],
                    "Resolve the filesystem error before retrying the content-addressed scaffold.",
                )
            })?;
    }
    created.sort();
    unchanged.sort();
    Ok(ExtractionScaffoldApplyResult {
        protocol: EXTRACTION_SCAFFOLD_APPLY_PROTOCOL.to_owned(),
        scaffold_id: scaffold.scaffold_id.clone(),
        plan_id: scaffold.plan_id.clone(),
        effects: ExtractionScaffoldEffects {
            writes_repository_files: !created.is_empty(),
            ..ExtractionScaffoldEffects::default()
        },
        created_files: created,
        unchanged_files: unchanged,
        linked_authority_remains_authoritative: true,
    })
}

fn ensure_no_symlink_ancestors(root: &Path, target: &Path) -> Result<(), String> {
    let relative = target
        .strip_prefix(root)
        .map_err(|_| "Generated target escaped the repository root.".to_owned())?;
    let mut current = PathBuf::from(root);
    for component in relative.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "Scaffold target `{}` traverses a symbolic link.",
                    current.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => {
                return Err(format!(
                    "Could not inspect scaffold target `{}`: {error}",
                    current.display()
                ));
            }
        }
    }
    Ok(())
}

pub fn extraction_scaffold_json(
    scaffold: &ExtractionScaffold,
) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(scaffold).map(|value| format!("{value}\n"))
}

#[must_use]
pub fn extraction_scaffold_schema() -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(ExtractionScaffold))
        .expect("Extraction Scaffold schema must serialize");
    let object = schema
        .as_object_mut()
        .expect("Extraction Scaffold schema must be an object");
    object.insert(
        "$id".to_owned(),
        Value::String(EXTRACTION_SCAFFOLD_SCHEMA_ID.to_owned()),
    );
    object.insert(
        "title".to_owned(),
        Value::String("Lenso Extraction Scaffold v1".to_owned()),
    );
    schema["properties"]["protocol"] = json!({
        "type": "string",
        "const": EXTRACTION_SCAFFOLD_PROTOCOL
    });
    schema["properties"]["generatorVersion"] = json!({
        "type": "string",
        "const": EXTRACTION_SCAFFOLD_GENERATOR_VERSION
    });
    schema["properties"]["scaffoldId"] = json!({
        "type": "string",
        "pattern": "^extraction-scaffold:sha256:[0-9a-f]{64}$"
    });
    schema["properties"]["scaffoldDigest"] = json!({
        "type": "string",
        "pattern": "^sha256:[0-9a-f]{64}$"
    });
    for field in [
        "writesRepositoryFiles",
        "startsWorkloads",
        "copiesData",
        "changesAuthority",
        "changesProviderPath",
    ] {
        schema["$defs"]["ExtractionScaffoldEffects"]["properties"][field] = json!({
            "type": "boolean",
            "const": false
        });
    }
    schema
}

fn validate_relative_path(path: &str) -> Result<(), String> {
    if path.trim().is_empty() || path.contains('\\') {
        return Err(format!(
            "Generated path `{path}` is not repository-relative."
        ));
    }
    let path = Path::new(path);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
        || path
            .components()
            .any(|component| component == Component::CurDir)
    {
        return Err(format!(
            "Generated path `{}` contains unsafe traversal.",
            path.display()
        ));
    }
    Ok(())
}

fn digest_serializable<T: Serialize + ?Sized>(
    value: &T,
) -> Result<String, ExtractionScaffoldGenerationError> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        generation_error(
            ExtractionScaffoldGenerationIssueCode::CandidateInvalid,
            format!("Extraction scaffold content could not serialize: {error}"),
            "Correct the structured scaffold input and regenerate.",
        )
    })?;
    Ok(extraction_input_digest(&bytes))
}

fn pretty_json(value: &Value) -> Result<String, ExtractionScaffoldGenerationError> {
    serde_json::to_string_pretty(value)
        .map(|value| format!("{value}\n"))
        .map_err(|error| {
            generation_error(
                ExtractionScaffoldGenerationIssueCode::CandidateInvalid,
                format!("Generated JSON could not serialize: {error}"),
                "Correct the structured scaffold input and regenerate.",
            )
        })
}

fn generation_error(
    code: ExtractionScaffoldGenerationIssueCode,
    message: impl Into<String>,
    next_action: impl Into<String>,
) -> ExtractionScaffoldGenerationError {
    ExtractionScaffoldGenerationError {
        code,
        message: message.into(),
        next_actions: vec![next_action.into()],
    }
}

fn apply_error(
    code: ExtractionScaffoldApplyErrorCode,
    message: impl Into<String>,
    conflicting_paths: Vec<String>,
    next_action: impl Into<String>,
) -> ExtractionScaffoldApplyError {
    ExtractionScaffoldApplyError {
        code,
        message: message.into(),
        conflicting_paths,
        next_actions: vec![next_action.into()],
        effects: ExtractionScaffoldEffects::default(),
    }
}

fn push_issue(
    issues: &mut Vec<ExtractionScaffoldIssue>,
    code: ExtractionScaffoldIssueCode,
    path: impl Into<String>,
    message: impl Into<String>,
    next_action: impl Into<String>,
) {
    issues.push(ExtractionScaffoldIssue {
        code,
        path: path.into(),
        message: message.into(),
        next_action: next_action.into(),
    });
}

fn stable_slug(value: &str) -> String {
    let mut slug = String::new();
    let mut separator = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            if separator && !slug.is_empty() {
                slug.push('-');
            }
            slug.push(character.to_ascii_lowercase());
            separator = false;
        } else {
            separator = true;
        }
    }
    slug
}

fn rust_identifier(value: &str) -> String {
    let mut identifier = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    while identifier.contains("__") {
        identifier = identifier.replace("__", "_");
    }
    identifier.trim_matches('_').to_owned()
}

fn rust_string(value: &str) -> String {
    serde_json::to_string(value).expect("JSON strings are valid Rust string literals here")
}

fn normalize_strings(values: &mut Vec<String>) {
    values.retain(|value| !value.trim().is_empty());
    values.sort();
    values.dedup();
}

fn strip_destination<'a>(destination: &str, path: &'a str) -> &'a str {
    path.strip_prefix(destination)
        .and_then(|path| path.strip_prefix('/'))
        .unwrap_or(path)
}

const fn binding_kind_label(kind: ExtractionGeneratedBindingKind) -> &'static str {
    match kind {
        ExtractionGeneratedBindingKind::Http => "http",
        ExtractionGeneratedBindingKind::Grpc => "grpc",
        ExtractionGeneratedBindingKind::Event => "event",
    }
}

const fn binding_role_label(role: ExtractionScaffoldBindingRole) -> &'static str {
    match role {
        ExtractionScaffoldBindingRole::Server => "server",
        ExtractionScaffoldBindingRole::Client => "client",
        ExtractionScaffoldBindingRole::Publisher => "publisher",
        ExtractionScaffoldBindingRole::Handler => "handler",
    }
}

const fn workload_role_label(role: ExtractionWorkloadRole) -> &'static str {
    match role {
        ExtractionWorkloadRole::Api => "api",
        ExtractionWorkloadRole::Worker => "worker",
        ExtractionWorkloadRole::Migration => "migration",
    }
}

fn http_method_label(method: lenso_contracts::ModuleHttpMethod) -> &'static str {
    match method {
        lenso_contracts::ModuleHttpMethod::Get => "GET",
        lenso_contracts::ModuleHttpMethod::Post => "POST",
        lenso_contracts::ModuleHttpMethod::Put => "PUT",
        lenso_contracts::ModuleHttpMethod::Patch => "PATCH",
        lenso_contracts::ModuleHttpMethod::Delete => "DELETE",
        _ => "UNKNOWN",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CompatibilityCategory, EXTRACTION_READINESS_ANALYZER_VERSION,
        EXTRACTION_READINESS_REPORT_PROTOCOL, ExtractionAuthorityKind, ExtractionContractEvidence,
        ExtractionDataEvidenceSource, ExtractionDataTableEvidence, ExtractionEvidenceDigest,
        ExtractionEvidenceStatus, ExtractionExpectedAuthority, ExtractionPlanContractVersion,
        ExtractionPlanInputs, ExtractionReadinessEffects, ExtractionReadinessReport,
        ExtractionReadinessSurfaceSummary, ExtractionServiceDataEvidence, generate_extraction_plan,
    };
    use lenso_contracts::{
        EventHandlerDeclaration, EventSurface, ModuleHttpMethod, ModuleHttpRoute,
        ServiceOperationMetadata,
    };
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    fn module() -> ModuleManifest {
        ModuleManifest::builder("support-ticket")
            .capabilities(vec!["support.tickets.read".to_owned()])
            .http_routes(vec![ModuleHttpRoute {
                method: ModuleHttpMethod::Get,
                path: "/v1/tickets/{ticket_id}".to_owned(),
                capability: Some("support.tickets.read".to_owned()),
                display_name: Some("Get ticket".to_owned()),
                story_title: Some("Support ticket opened".to_owned()),
                operation: Some(ServiceOperationMetadata {
                    operation_id: Some("getTicket".to_owned()),
                    ..ServiceOperationMetadata::default()
                }),
            }])
            .events(EventSurface {
                handlers: vec![EventHandlerDeclaration {
                    name: "apply_sla_update".to_owned(),
                    event_name: "support.sla-updated.v1".to_owned(),
                    operation: None,
                }],
            })
            .build()
    }

    fn event_schema() -> String {
        serde_json::to_string_pretty(&json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "https://contracts.lenso.local/events/support.sla-updated.v1.schema.json",
            "title": "support.sla-updated.v1",
            "type": "object",
            "properties": { "ticketId": { "type": "string" } },
            "additionalProperties": false
        }))
        .map(|value| format!("{value}\n"))
        .unwrap()
    }

    fn current_inputs() -> ExtractionPlanInputs {
        let module = module();
        let http = crate::DIRECT_HTTP_OPENAPI_V1_FIXTURE_YAML;
        let event = event_schema();
        ExtractionPlanInputs {
            readiness_report: ExtractionReadinessReport {
                protocol: EXTRACTION_READINESS_REPORT_PROTOCOL.to_owned(),
                analyzer_version: EXTRACTION_READINESS_ANALYZER_VERSION.to_owned(),
                target_module: module.name.clone(),
                system_id: Some("support-system".to_owned()),
                target_owner: Some("support-host".to_owned()),
                classification: CompatibilityCategory::Safe,
                ready: true,
                issue_codes: Vec::new(),
                contract_evidence: vec![
                    ExtractionContractEvidence {
                        subject: "http:GET /v1/tickets/{ticket_id}".to_owned(),
                        kind: ExtractionContractKind::Service,
                        direction: ExtractionContractDirection::Provides,
                        status: ExtractionEvidenceStatus::Present,
                        contract_id: Some("support-ticket-http.v1".to_owned()),
                        evidence_references: vec!["contracts/openapi/support.v1.yaml".to_owned()],
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
                ],
                active_consumers: Vec::new(),
                surfaces: ExtractionReadinessSurfaceSummary::default(),
                service_data: ExtractionServiceDataEvidence {
                    complete: true,
                    tables: vec![ExtractionDataTableEvidence {
                        table: "support.tickets".to_owned(),
                        owner_module: Some("support-ticket".to_owned()),
                        source: ExtractionDataEvidenceSource::StaticDeclaration,
                        volume: None,
                        cursor: None,
                        evidence_references: Vec::new(),
                    }],
                    ..ExtractionServiceDataEvidence::default()
                },
                findings: Vec::new(),
                effects: ExtractionReadinessEffects::default(),
            },
            module,
            system: json!({
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
            }),
            contract_versions: vec![
                ExtractionPlanContractVersion {
                    contract_id: "support-ticket-http.v1".to_owned(),
                    version: "v1".to_owned(),
                    kind: ExtractionContractKind::Service,
                    direction: ExtractionContractDirection::Provides,
                    artifact_reference: "contracts/openapi/support.v1.yaml".to_owned(),
                    artifact_digest: extraction_input_digest(http.as_bytes()),
                    artifact_format: ExtractionContractArtifactFormat::Openapi,
                    tenancy_mode: ServiceTenancyMode::Required,
                    required_context: vec![CommonContextRequirement::Tenant],
                    producer_id: None,
                    consumer_ids: Vec::new(),
                },
                ExtractionPlanContractVersion {
                    contract_id: "support.sla-updated.v1".to_owned(),
                    version: "v1".to_owned(),
                    kind: ExtractionContractKind::Event,
                    direction: ExtractionContractDirection::Consumes,
                    artifact_reference: "contracts/events/support.sla-updated.v1.schema.json"
                        .to_owned(),
                    artifact_digest: extraction_input_digest(event.as_bytes()),
                    artifact_format: ExtractionContractArtifactFormat::JsonSchema,
                    tenancy_mode: ServiceTenancyMode::Required,
                    required_context: vec![CommonContextRequirement::Tenant],
                    producer_id: Some("support-sla-service".to_owned()),
                    consumer_ids: Vec::new(),
                },
            ],
            expected_authority: ExtractionExpectedAuthority {
                kind: ExtractionAuthorityKind::LinkedHost,
                owner_id: "support-host".to_owned(),
                revision: "support-r1".to_owned(),
            },
            evidence_digests: vec![ExtractionEvidenceDigest {
                reference: "analyzer:support".to_owned(),
                digest: extraction_input_digest(b"support-evidence"),
            }],
        }
    }

    fn scaffold_inputs() -> (ExtractionScaffoldInputs, ExtractionPlanInputs) {
        let current = current_inputs();
        let plan = generate_extraction_plan(&current).expect("plan");
        (
            ExtractionScaffoldInputs {
                plan,
                module: current.module.clone(),
                artifacts: vec![
                    ExtractionScaffoldArtifact {
                        contract_id: "support-ticket-http.v1".to_owned(),
                        version: "v1".to_owned(),
                        contents: crate::DIRECT_HTTP_OPENAPI_V1_FIXTURE_YAML.to_owned(),
                        protobuf_descriptor: None,
                    },
                    ExtractionScaffoldArtifact {
                        contract_id: "support.sla-updated.v1".to_owned(),
                        version: "v1".to_owned(),
                        contents: event_schema(),
                        protobuf_descriptor: None,
                    },
                ],
            },
            current,
        )
    }

    #[test]
    fn dry_run_is_deterministic_identity_preserving_and_zero_effect() {
        let (inputs, _) = scaffold_inputs();
        let left = generate_extraction_scaffold(&inputs).expect("scaffold");
        let right = dry_run_extraction_scaffold(&inputs).expect("dry run");
        assert_eq!(left, right);
        assert!(extraction_scaffold_integrity_is_valid(&left));
        assert!(validate_extraction_scaffold(&left).is_empty());
        assert_eq!(left.effects, ExtractionScaffoldEffects::default());
        assert!(left.linked_authority_remains_authoritative);
        assert!(left.provider_compatibility_preserved);
        assert_eq!(
            left.preserved_identity.module_manifest,
            serde_json::to_value(module()).unwrap()
        );
        assert_eq!(left.preserved_identity.operation_ids, ["getTicket"]);
        assert_eq!(
            left.preserved_identity.event_types,
            ["support.sla-updated.v1"]
        );
        assert!(left.patch.contains("src/bin/api.rs"));
        assert!(left.patch.contains("src/bin/worker.rs"));
        assert!(left.patch.contains("src/bin/migration.rs"));
    }

    #[test]
    fn authoritative_contract_changes_are_rejected() {
        let (mut inputs, _) = scaffold_inputs();
        inputs.artifacts[0].contents.push_str("# changed\n");
        let error = generate_extraction_scaffold(&inputs).expect_err("digest drift must fail");
        assert_eq!(
            error.code,
            ExtractionScaffoldGenerationIssueCode::ArtifactDigestMismatch
        );
    }

    #[test]
    fn apply_is_idempotent_preserves_provider_and_refuses_user_changes() {
        let (inputs, current) = scaffold_inputs();
        let scaffold = generate_extraction_scaffold(&inputs).expect("scaffold");
        let root = temp_root();
        fs::create_dir_all(&root).unwrap();
        let provider = root.join("lenso.service.json");
        fs::write(&provider, "{\"protocol\":\"lenso.service.v1\"}\n").unwrap();

        let applied = apply_extraction_scaffold(&root, &scaffold, &inputs.plan, &current)
            .expect("first apply");
        assert!(!applied.created_files.is_empty());
        assert!(applied.effects.writes_repository_files);
        assert_eq!(
            fs::read_to_string(&provider).unwrap(),
            "{\"protocol\":\"lenso.service.v1\"}\n"
        );
        let repeated = apply_extraction_scaffold(&root, &scaffold, &inputs.plan, &current)
            .expect("repeated apply");
        assert!(repeated.created_files.is_empty());
        assert_eq!(repeated.unchanged_files.len(), scaffold.files.len());
        assert!(!repeated.effects.writes_repository_files);

        let changed = root.join(&scaffold.files[0].path);
        fs::write(&changed, "user change\n").unwrap();
        let error = apply_extraction_scaffold(&root, &scaffold, &inputs.plan, &current)
            .expect_err("changed generated file must fail");
        assert_eq!(
            error.code,
            ExtractionScaffoldApplyErrorCode::ScaffoldConflict
        );
        assert_eq!(error.conflicting_paths, [scaffold.files[0].path.clone()]);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn stale_plan_is_rejected_before_repository_writes() {
        let (inputs, mut current) = scaffold_inputs();
        let scaffold = generate_extraction_scaffold(&inputs).expect("scaffold");
        current.expected_authority.revision = "support-r2".to_owned();
        let root = temp_root();
        fs::create_dir_all(&root).unwrap();
        let error = apply_extraction_scaffold(&root, &scaffold, &inputs.plan, &current)
            .expect_err("stale plan must fail");
        assert_eq!(error.code, ExtractionScaffoldApplyErrorCode::PlanStale);
        assert!(!root.join(&scaffold.destination_root).exists());
        fs::remove_dir_all(&root).unwrap();
    }

    fn temp_root() -> PathBuf {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "lenso-extraction-scaffold-{}-{sequence}",
            std::process::id()
        ))
    }
}
