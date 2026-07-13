use crate::{
    AutonomousServiceContract, CausationContext, CommonContextContract, CommonContextIssueCode,
    CommonContextRequirement, CompatibilityCategory, CompatibilityReason,
    ContractCompatibilityKind, ContractCompatibilityResult, ContractContextRequirements,
    DeadlineContext, DelegatedActorContext, EventArtifactFormat, EventArtifactReference,
    EventContractArtifact, IdempotencyKeyContext, RegionContext, ServicePrincipal,
    ServiceTenancyMode, StoryContext, TenantContext, TraceContext, evaluate_event_compatibility,
    validate_common_context_contract_value,
};
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub const EVENT_CONTRACT_ARTIFACT_PROTOCOL: &str = "lenso.event-contract.v1";
pub const EVENT_ENVELOPE_PROTOCOL: &str = "lenso.event-envelope.v1";
const CLOUDEVENTS_SPEC_VERSION: &str = "1.0";
const EVENT_ENVELOPE_SCHEMA: &str =
    "https://lenso.dev/contracts/lenso.event-envelope.v1.schema.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GeneratedEventContract {
    pub protocol: String,
    pub event_type: String,
    pub contract_id: String,
    pub contract_version: String,
    pub producer_service_id: String,
    pub module_id: String,
    pub operating_regions: Vec<String>,
    pub tenancy_mode: ServiceTenancyMode,
    pub context: ContractContextRequirements,
    pub artifact: EventArtifactReference,
    pub payload_schema: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventContractGenerationError {
    EmptyProducerServiceId,
    InvalidDeclaration,
    UnownedModule,
    UnsupportedArtifactFormat,
    InvalidArtifactReference,
    InvalidPayloadSchema,
}

#[must_use]
pub fn generate_event_contract(
    service: &AutonomousServiceContract,
    declaration: &EventContractArtifact,
    payload_schema: &Value,
) -> Result<GeneratedEventContract, EventContractGenerationError> {
    if service.service_id.trim().is_empty() {
        return Err(EventContractGenerationError::EmptyProducerServiceId);
    }
    if !service.modules.contains(&declaration.module_id) {
        return Err(EventContractGenerationError::UnownedModule);
    }
    if !service.event_contracts.contains(declaration)
        || !crate::validate_autonomous_service_contract(service).is_empty()
        || declaration.contract_id.trim().is_empty()
        || declaration.module_id.trim().is_empty()
        || declaration.version.trim().is_empty()
        || declaration.context.protocol != crate::COMMON_CONTEXT_PROTOCOL
        || (declaration.tenancy_mode == ServiceTenancyMode::Required
            && !declaration
                .context
                .required
                .contains(&CommonContextRequirement::Tenant))
    {
        return Err(EventContractGenerationError::InvalidDeclaration);
    }
    if declaration.artifact.format != EventArtifactFormat::JsonSchema {
        return Err(EventContractGenerationError::UnsupportedArtifactFormat);
    }
    let event_type = declaration
        .artifact
        .path
        .rsplit('/')
        .next()
        .and_then(|name| name.strip_suffix(".schema.json"))
        .filter(|name| !name.is_empty())
        .ok_or(EventContractGenerationError::InvalidArtifactReference)?;
    if !event_type.ends_with(&format!(
        "{}.{}",
        declaration.contract_id, declaration.version
    )) {
        return Err(EventContractGenerationError::InvalidArtifactReference);
    }
    if !valid_payload_schema_definition(payload_schema, event_type) {
        return Err(EventContractGenerationError::InvalidPayloadSchema);
    }

    Ok(GeneratedEventContract {
        protocol: EVENT_CONTRACT_ARTIFACT_PROTOCOL.to_owned(),
        event_type: event_type.to_owned(),
        contract_id: declaration.contract_id.clone(),
        contract_version: declaration.version.clone(),
        producer_service_id: service.service_id.clone(),
        module_id: declaration.module_id.clone(),
        operating_regions: service.operating_regions.clone(),
        tenancy_mode: declaration.tenancy_mode.clone(),
        context: declaration.context.clone(),
        artifact: declaration.artifact.clone(),
        payload_schema: payload_schema.clone(),
    })
}

#[must_use]
pub fn evaluate_generated_event_contract_compatibility(
    before: &GeneratedEventContract,
    after: &GeneratedEventContract,
) -> ContractCompatibilityResult {
    let mut result = ContractCompatibilityResult {
        category: CompatibilityCategory::Safe,
        contract_kind: ContractCompatibilityKind::EventContract,
        contract_id: after.contract_id.clone(),
        changed_version: after.contract_version.clone(),
        affected_references: vec![
            format!("autonomous_service:{}", after.producer_service_id),
            format!("module:{}", after.module_id),
        ],
        reasons: Vec::new(),
    };
    if !valid_generated_event_contract(before) || !valid_generated_event_contract(after) {
        compatibility_issue(
            &mut result,
            CompatibilityCategory::Blocked,
            "event_artifact_unverifiable",
            "$",
            "Generated Event Contract artifacts must use supported protocols and internally consistent identities.",
            "Regenerate both Event Contract artifacts from valid Autonomous Service declarations and payload schemas.",
        );
    }
    if before.contract_version == after.contract_version {
        compatibility_issue(
            &mut result,
            CompatibilityCategory::Blocked,
            "event_version_unverifiable",
            "$.contractVersion",
            "Event Contract evolution must identify a changed Contract Version.",
            "Generate the candidate artifact with a distinct version.",
        );
    }
    for (field, old, new) in [
        ("contractId", &before.contract_id, &after.contract_id),
        (
            "producerServiceId",
            &before.producer_service_id,
            &after.producer_service_id,
        ),
        ("moduleId", &before.module_id, &after.module_id),
    ] {
        if old != new {
            compatibility_issue(
                &mut result,
                CompatibilityCategory::Breaking,
                &format!("event_{}_changed", super::camel_to_snake(field)),
                &format!("$.{field}"),
                "A stable Event Contract identity changed.",
                "Keep the existing identity or publish a separately coordinated Event Contract.",
            );
        }
    }
    for (code, path, old, new) in [
        (
            "event_type_identity_changed",
            "$.eventType",
            event_type_family(before),
            event_type_family(after),
        ),
        (
            "event_artifact_identity_changed",
            "$.artifact.path",
            artifact_family(before),
            artifact_family(after),
        ),
    ] {
        if old != new {
            compatibility_issue(
                &mut result,
                CompatibilityCategory::Breaking,
                code,
                path,
                "A stable Event Contract artifact identity changed.",
                "Keep the existing Event Type and artifact family or publish a separately coordinated Event Contract.",
            );
        }
    }
    if before.tenancy_mode != after.tenancy_mode {
        let category = if after.tenancy_mode == ServiceTenancyMode::Required {
            CompatibilityCategory::Breaking
        } else {
            CompatibilityCategory::NeedsAttention
        };
        compatibility_issue(
            &mut result,
            category,
            "event_tenancy_changed",
            "$.tenancyMode",
            "The Event Contract Tenancy Mode changed.",
            "Review Producer and Consumer tenant scoping before publishing.",
        );
    }
    let old_context = before
        .context
        .required
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let new_context = after
        .context
        .required
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    for requirement in new_context.difference(&old_context) {
        compatibility_issue(
            &mut result,
            CompatibilityCategory::Breaking,
            "event_required_context_added",
            "$.context.required",
            &format!("A new required context field was added: {requirement:?}."),
            "Keep the context optional or coordinate the requirement with every affected Producer and Consumer.",
        );
    }
    for requirement in old_context.difference(&new_context) {
        compatibility_issue(
            &mut result,
            CompatibilityCategory::NeedsAttention,
            "event_required_context_removed",
            "$.context.required",
            &format!("A required context field was removed: {requirement:?}."),
            "Review identity, tenancy, causation, and evidence semantics with affected owners.",
        );
    }
    let old_regions = before.operating_regions.iter().collect::<BTreeSet<_>>();
    let new_regions = after.operating_regions.iter().collect::<BTreeSet<_>>();
    if old_regions != new_regions {
        compatibility_issue(
            &mut result,
            CompatibilityCategory::NeedsAttention,
            "event_operating_regions_changed",
            "$.operatingRegions",
            "The producing Service Operating Regions changed.",
            "Review regional publication and consumption expectations before publishing.",
        );
    }
    let payload_result = evaluate_event_compatibility(&json!({
        "contractId": after.contract_id,
        "changedVersion": after.contract_version,
        "affectedReferences": result.affected_references,
        "before": {
            "format": "json_schema",
            "version": before.contract_version,
            "schema": before.payload_schema
        },
        "after": {
            "format": "json_schema",
            "version": after.contract_version,
            "schema": after.payload_schema
        }
    }));
    result.category = result.category.max(payload_result.category);
    result.reasons.extend(payload_result.reasons);
    if result.category != CompatibilityCategory::Safe {
        result
            .reasons
            .retain(|reason| reason.code != "event_backward_compatible");
    }
    result.affected_references.sort();
    result.affected_references.dedup();
    result.reasons.sort();
    result.reasons.dedup();
    result
}

fn valid_generated_event_contract(contract: &GeneratedEventContract) -> bool {
    contract.protocol == EVENT_CONTRACT_ARTIFACT_PROTOCOL
        && contract.context.protocol == crate::COMMON_CONTEXT_PROTOCOL
        && contract.artifact.format == EventArtifactFormat::JsonSchema
        && artifact_event_type(&contract.artifact.path) == Some(contract.event_type.as_str())
        && contract.event_type.ends_with(&format!(
            "{}.{}",
            contract.contract_id, contract.contract_version
        ))
        && valid_payload_schema_definition(&contract.payload_schema, &contract.event_type)
}

fn artifact_event_type(path: &str) -> Option<&str> {
    path.rsplit('/')
        .next()
        .and_then(|name| name.strip_suffix(".schema.json"))
}

fn event_type_family(contract: &GeneratedEventContract) -> Option<&str> {
    contract
        .event_type
        .strip_suffix(&format!(".{}", contract.contract_version))
}

fn artifact_family(contract: &GeneratedEventContract) -> Option<&str> {
    contract
        .artifact
        .path
        .strip_suffix(&format!(".{}.schema.json", contract.contract_version))
}

fn compatibility_issue(
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventContext {
    pub protocol: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub story: Option<StoryContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace: Option<TraceContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_principal: Option<ServicePrincipal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegated_actor: Option<DelegatedActorContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<TenantContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<DeadlineContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<IdempotencyKeyContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causation: Option<CausationContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<RegionContext>,
}

impl From<CommonContextContract> for EventContext {
    fn from(context: CommonContextContract) -> Self {
        Self {
            protocol: context.protocol,
            story: Some(context.story),
            trace: Some(context.trace),
            service_principal: Some(context.service_principal),
            delegated_actor: Some(context.delegated_actor),
            tenant: Some(context.tenant),
            deadline: Some(context.deadline),
            idempotency_key: Some(context.idempotency_key),
            causation: Some(context.causation),
            region: Some(context.region),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventContent {
    pub content_type: String,
    pub schema: String,
    pub data: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventEnvelope {
    pub protocol: String,
    pub event_id: String,
    pub event_type: String,
    pub contract_id: String,
    pub contract_version: String,
    pub producer_service_id: String,
    pub module_id: String,
    pub occurred_at: String,
    pub tenancy_mode: ServiceTenancyMode,
    pub context: EventContext,
    pub content: EventContent,
}

impl EventEnvelope {
    #[must_use]
    pub fn new<C>(
        contract: &GeneratedEventContract,
        event_id: impl Into<String>,
        occurred_at: impl Into<String>,
        context: C,
        data: Value,
    ) -> Self
    where
        C: Into<EventContext>,
    {
        Self {
            protocol: EVENT_ENVELOPE_PROTOCOL.to_owned(),
            event_id: event_id.into(),
            event_type: contract.event_type.clone(),
            contract_id: contract.contract_id.clone(),
            contract_version: contract.contract_version.clone(),
            producer_service_id: contract.producer_service_id.clone(),
            module_id: contract.module_id.clone(),
            occurred_at: occurred_at.into(),
            tenancy_mode: contract.tenancy_mode.clone(),
            context: context.into(),
            content: EventContent {
                content_type: "application/json".to_owned(),
                schema: contract.artifact.path.clone(),
                data,
            },
        }
    }

    #[must_use]
    pub fn to_cloudevent(&self) -> CloudEvent {
        CloudEvent {
            specversion: CLOUDEVENTS_SPEC_VERSION.to_owned(),
            id: self.event_id.clone(),
            source: format!("urn:lenso:service:{}", self.producer_service_id),
            event_type: self.event_type.clone(),
            subject: format!(
                "{}/{}/{}",
                self.module_id, self.contract_id, self.contract_version
            ),
            time: self.occurred_at.clone(),
            datacontenttype: "application/json".to_owned(),
            dataschema: EVENT_ENVELOPE_SCHEMA.to_owned(),
            data: serde_json::to_value(self).expect("EventEnvelope must serialize"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CloudEvent {
    pub specversion: String,
    pub id: String,
    pub source: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub subject: String,
    pub time: String,
    pub datacontenttype: String,
    pub dataschema: String,
    pub data: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventEnvelopeIssueCode {
    InvalidProtocol,
    InvalidEventIdentity,
    IncompatibleContractIdentity,
    IncompatibleContext,
    IncompatibleProducerIdentity,
    IncompatibleModuleIdentity,
    IncompatibleRegion,
    IncompatibleTenancy,
    InvalidOccurrenceTime,
    InvalidContentMetadata,
    InvalidContent,
    InvalidCloudEventsRepresentation,
    MissingRequiredContext,
    MalformedContext,
    UntrustedContext,
}

pub fn event_envelope_from_cloudevent(
    contract: &GeneratedEventContract,
    cloud_event: &CloudEvent,
) -> Result<EventEnvelope, Vec<EventEnvelopeIssue>> {
    let mut issues = Vec::new();
    for (path, actual, expected) in [
        (
            "$.specversion",
            cloud_event.specversion.as_str(),
            CLOUDEVENTS_SPEC_VERSION,
        ),
        (
            "$.id",
            cloud_event.id.as_str(),
            string_at(&cloud_event.data, "eventId"),
        ),
        (
            "$.source",
            cloud_event.source.as_str(),
            &format!("urn:lenso:service:{}", contract.producer_service_id),
        ),
        (
            "$.type",
            cloud_event.event_type.as_str(),
            contract.event_type.as_str(),
        ),
        (
            "$.subject",
            cloud_event.subject.as_str(),
            &format!(
                "{}/{}/{}",
                contract.module_id, contract.contract_id, contract.contract_version
            ),
        ),
        (
            "$.time",
            cloud_event.time.as_str(),
            string_at(&cloud_event.data, "occurredAt"),
        ),
        (
            "$.datacontenttype",
            cloud_event.datacontenttype.as_str(),
            "application/json",
        ),
        (
            "$.dataschema",
            cloud_event.dataschema.as_str(),
            EVENT_ENVELOPE_SCHEMA,
        ),
    ] {
        if actual != expected {
            push_issue(
                &mut issues,
                EventEnvelopeIssueCode::InvalidCloudEventsRepresentation,
                path,
                format!("CloudEvents attribute must match `{expected}`"),
                "Restore the authoritative Lenso Event Envelope attribute before decoding.",
            );
        }
    }
    issues.extend(validate_event_envelope_value(contract, &cloud_event.data));
    if !issues.is_empty() {
        return Err(issues);
    }
    serde_json::from_value(cloud_event.data.clone()).map_err(|error| {
        vec![EventEnvelopeIssue {
            code: EventEnvelopeIssueCode::InvalidCloudEventsRepresentation,
            path: "$.data".to_owned(),
            message: format!("CloudEvents data is not a Lenso Event Envelope: {error}"),
            next_action:
                "Encode the complete validated Lenso Event Envelope as structured CloudEvents data."
                    .to_owned(),
        }]
    })
}

fn string_at<'a>(value: &'a Value, field: &str) -> &'a str {
    value.get(field).and_then(Value::as_str).unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventEnvelopeIssue {
    pub code: EventEnvelopeIssueCode,
    pub path: String,
    pub message: String,
    pub next_action: String,
}

#[must_use]
pub fn validate_event_envelope(
    contract: &GeneratedEventContract,
    envelope: &EventEnvelope,
) -> Vec<EventEnvelopeIssue> {
    validate_event_envelope_value(
        contract,
        &serde_json::to_value(envelope).expect("EventEnvelope must serialize"),
    )
}

#[must_use]
pub fn validate_event_envelope_value(
    contract: &GeneratedEventContract,
    value: &Value,
) -> Vec<EventEnvelopeIssue> {
    let mut issues = Vec::new();
    validate_exact_string(
        value,
        "protocol",
        EVENT_ENVELOPE_PROTOCOL,
        EventEnvelopeIssueCode::InvalidProtocol,
        "Use the supported Event Envelope protocol.",
        &mut issues,
    );
    validate_non_empty_string(
        value,
        "eventId",
        EventEnvelopeIssueCode::InvalidEventIdentity,
        "Assign a stable event identity before publication.",
        &mut issues,
    );
    if value
        .get("occurredAt")
        .and_then(Value::as_str)
        .is_some_and(|time| !time.is_empty() && DateTime::parse_from_rfc3339(time).is_err())
    {
        push_issue(
            &mut issues,
            EventEnvelopeIssueCode::InvalidOccurrenceTime,
            "$.occurredAt",
            "occurredAt must be an RFC 3339 timestamp",
            "Set the authoritative event occurrence time in RFC 3339 form.",
        );
    }
    for (field, expected, code, action) in [
        (
            "eventType",
            contract.event_type.as_str(),
            EventEnvelopeIssueCode::IncompatibleContractIdentity,
            "Regenerate the envelope from the authoritative Event Contract.",
        ),
        (
            "contractId",
            contract.contract_id.as_str(),
            EventEnvelopeIssueCode::IncompatibleContractIdentity,
            "Use the declared Event Contract identity.",
        ),
        (
            "contractVersion",
            contract.contract_version.as_str(),
            EventEnvelopeIssueCode::IncompatibleContractIdentity,
            "Use the declared Event Contract version.",
        ),
        (
            "producerServiceId",
            contract.producer_service_id.as_str(),
            EventEnvelopeIssueCode::IncompatibleProducerIdentity,
            "Use the Service identity that generated the Event Contract artifact.",
        ),
        (
            "moduleId",
            contract.module_id.as_str(),
            EventEnvelopeIssueCode::IncompatibleModuleIdentity,
            "Use the Module identity declared by the Event Contract.",
        ),
        (
            "content/schema",
            contract.artifact.path.as_str(),
            EventEnvelopeIssueCode::InvalidContentMetadata,
            "Use the authoritative payload artifact reference.",
        ),
        (
            "content/contentType",
            "application/json",
            EventEnvelopeIssueCode::InvalidContentMetadata,
            "Use `application/json` for the declared JSON Schema payload.",
        ),
    ] {
        validate_exact_string(value, field, expected, code, action, &mut issues);
    }
    validate_non_empty_string(
        value,
        "occurredAt",
        EventEnvelopeIssueCode::InvalidOccurrenceTime,
        "Set an RFC 3339 occurrence time.",
        &mut issues,
    );

    let expected_tenancy =
        serde_json::to_value(&contract.tenancy_mode).expect("ServiceTenancyMode must serialize");
    if value.get("tenancyMode") != Some(&expected_tenancy) {
        push_issue(
            &mut issues,
            EventEnvelopeIssueCode::IncompatibleTenancy,
            "$.tenancyMode",
            "tenancyMode does not match the Event Contract",
            "Use the Tenancy Mode declared by the Event Contract.",
        );
    }

    validate_payload_value(
        &contract.payload_schema,
        value.pointer("/content/data").unwrap_or(&Value::Null),
        "$.content.data",
        &mut issues,
    );

    let context = value.get("context").unwrap_or(&Value::Null);
    for issue in validate_common_context_contract_value(context) {
        let requirement = requirement_for_common_issue(issue.code);
        let required =
            requirement.is_some_and(|required| contract.context.required.contains(&required));
        let present = requirement
            .and_then(context_field_for_requirement)
            .is_some_and(|field| context.get(field).is_some());
        let untrusted = matches!(
            issue.code,
            CommonContextIssueCode::UntrustedActorClaim
                | CommonContextIssueCode::UntrustedTenantClaim
        );
        let incompatible = matches!(
            issue.code,
            CommonContextIssueCode::InvalidProtocol | CommonContextIssueCode::AudienceMismatch
        );
        if !required && !present && !untrusted && !incompatible {
            continue;
        }
        let path = format!(
            "$.context{}",
            issue.path.strip_prefix('$').unwrap_or(&issue.path)
        );
        let missing = common_issue_pointer(&issue.path)
            .is_none_or(|pointer| context.pointer(&pointer).is_none());
        let code = if incompatible {
            EventEnvelopeIssueCode::IncompatibleContext
        } else if untrusted {
            EventEnvelopeIssueCode::UntrustedContext
        } else if missing {
            EventEnvelopeIssueCode::MissingRequiredContext
        } else {
            EventEnvelopeIssueCode::MalformedContext
        };
        push_issue(&mut issues, code, path, issue.message, issue.next_action);
    }
    if contract
        .context
        .required
        .contains(&CommonContextRequirement::Region)
        && context
            .pointer("/region/operatingRegion")
            .and_then(Value::as_str)
            .is_some_and(|region| {
                !contract
                    .operating_regions
                    .iter()
                    .any(|known| known == region)
            })
    {
        push_issue(
            &mut issues,
            EventEnvelopeIssueCode::IncompatibleRegion,
            "$.context.region.operatingRegion",
            "Operating Region is not declared by the producing Service",
            "Use an Operating Region declared by the authoritative Autonomous Service contract.",
        );
    }
    issues
}

fn context_field_for_requirement(requirement: CommonContextRequirement) -> Option<&'static str> {
    match requirement {
        CommonContextRequirement::Story => Some("story"),
        CommonContextRequirement::Trace => Some("trace"),
        CommonContextRequirement::ServicePrincipal => Some("servicePrincipal"),
        CommonContextRequirement::DelegatedActor => Some("delegatedActor"),
        CommonContextRequirement::Tenant => Some("tenant"),
        CommonContextRequirement::Deadline => Some("deadline"),
        CommonContextRequirement::IdempotencyKey => Some("idempotencyKey"),
        CommonContextRequirement::Causation => Some("causation"),
        CommonContextRequirement::Region => Some("region"),
    }
}

fn valid_payload_schema_definition(schema: &Value, event_type: &str) -> bool {
    schema.get("title").and_then(Value::as_str) == Some(event_type)
        && jsonschema::draft202012::meta::validate(schema).is_ok()
        && jsonschema::draft202012::options()
            .should_validate_formats(true)
            .build(schema)
            .is_ok()
}

fn validate_payload_value(
    schema: &Value,
    data: &Value,
    path: &str,
    issues: &mut Vec<EventEnvelopeIssue>,
) {
    let Ok(validator) = jsonschema::draft202012::options()
        .should_validate_formats(true)
        .build(schema)
    else {
        push_issue(
            issues,
            EventEnvelopeIssueCode::InvalidContent,
            path,
            "authoritative Event Contract payload schema is invalid",
            "Regenerate or replace the Event Contract artifact before validating event content.",
        );
        return;
    };
    let mut errors = validator
        .iter_errors(data)
        .map(|error| {
            let suffix = error
                .instance_path()
                .to_string()
                .split('/')
                .filter(|segment| !segment.is_empty())
                .map(|segment| format!(".{segment}"))
                .collect::<String>();
            (format!("{path}{suffix}"), error.to_string())
        })
        .collect::<Vec<_>>();
    errors.sort();
    errors.dedup();
    for (error_path, message) in errors {
        push_issue(
            issues,
            EventEnvelopeIssueCode::InvalidContent,
            error_path,
            message,
            "Provide content that satisfies the authoritative generated payload schema.",
        );
    }
}

fn requirement_for_common_issue(code: CommonContextIssueCode) -> Option<CommonContextRequirement> {
    match code {
        CommonContextIssueCode::InvalidStoryContext => Some(CommonContextRequirement::Story),
        CommonContextIssueCode::InvalidTraceContext => Some(CommonContextRequirement::Trace),
        CommonContextIssueCode::InvalidServicePrincipal => {
            Some(CommonContextRequirement::ServicePrincipal)
        }
        CommonContextIssueCode::InvalidDelegatedActorContext => {
            Some(CommonContextRequirement::DelegatedActor)
        }
        CommonContextIssueCode::InvalidTenantContext => Some(CommonContextRequirement::Tenant),
        CommonContextIssueCode::InvalidDeadline => Some(CommonContextRequirement::Deadline),
        CommonContextIssueCode::InvalidIdempotencyKey => {
            Some(CommonContextRequirement::IdempotencyKey)
        }
        CommonContextIssueCode::InvalidCausation => Some(CommonContextRequirement::Causation),
        CommonContextIssueCode::InvalidRegion => Some(CommonContextRequirement::Region),
        CommonContextIssueCode::InvalidProtocol
        | CommonContextIssueCode::UntrustedActorClaim
        | CommonContextIssueCode::UntrustedTenantClaim
        | CommonContextIssueCode::AudienceMismatch => None,
    }
}

fn common_issue_pointer(path: &str) -> Option<String> {
    path.strip_prefix("$.")
        .map(|suffix| format!("/{}", suffix.replace('.', "/")))
}

fn validate_exact_string(
    value: &Value,
    field: &str,
    expected: &str,
    code: EventEnvelopeIssueCode,
    next_action: &str,
    issues: &mut Vec<EventEnvelopeIssue>,
) {
    let pointer = format!("/{}", field.replace('/', "/"));
    if value.pointer(&pointer).and_then(Value::as_str) != Some(expected) {
        push_issue(
            issues,
            code,
            format!("$.{}", field.replace('/', ".")),
            format!("value must match `{expected}`"),
            next_action,
        );
    }
}

fn validate_non_empty_string(
    value: &Value,
    field: &str,
    code: EventEnvelopeIssueCode,
    next_action: &str,
    issues: &mut Vec<EventEnvelopeIssue>,
) {
    if value
        .get(field)
        .and_then(Value::as_str)
        .is_none_or(|text| text.trim().is_empty())
    {
        push_issue(
            issues,
            code,
            format!("$.{field}"),
            format!("{field} must be a non-empty string"),
            next_action,
        );
    }
}

fn push_issue(
    issues: &mut Vec<EventEnvelopeIssue>,
    code: EventEnvelopeIssueCode,
    path: impl Into<String>,
    message: impl Into<String>,
    next_action: impl Into<String>,
) {
    issues.push(EventEnvelopeIssue {
        code,
        path: path.into(),
        message: message.into(),
        next_action: next_action.into(),
    });
}
