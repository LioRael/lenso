use lenso_service::{
    AUTONOMOUS_SERVICE_V2_FIXTURE_JSON, AutonomousServiceContract, COMMON_CONTEXT_V1_FIXTURE_JSON,
    CommonContextContract, CommonContextRequirement, CompatibilityCategory, EventContext,
    EventContractGenerationError, EventEnvelope, EventEnvelopeIssueCode, SUPPORT_EVENT_SCHEMA_JSON,
    evaluate_generated_event_contract_compatibility, event_envelope_from_cloudevent,
    generate_event_contract, validate_event_envelope_value,
};
use serde_json::json;

#[test]
fn support_event_declaration_builds_a_versioned_event_envelope() {
    let service: AutonomousServiceContract =
        serde_json::from_str(AUTONOMOUS_SERVICE_V2_FIXTURE_JSON).unwrap();
    let payload_schema = serde_json::from_str(SUPPORT_EVENT_SCHEMA_JSON).unwrap();
    let contract = generate_event_contract(&service, &service.event_contracts[0], &payload_schema)
        .expect("support event declaration should generate");
    let context: CommonContextContract =
        serde_json::from_str(COMMON_CONTEXT_V1_FIXTURE_JSON).unwrap();

    let envelope = EventEnvelope::new(
        &contract,
        "event_support_ticket_01",
        "2026-07-14T10:15:30Z",
        context,
        json!({
            "ticketId": "ticket_01",
            "openedAt": "2026-07-14T10:15:00Z"
        }),
    );

    assert_eq!(envelope.protocol, "lenso.event-envelope.v1");
    assert_eq!(envelope.event_id, "event_support_ticket_01");
    assert_eq!(envelope.event_type, "support.ticket-opened.v1");
    assert_eq!(envelope.contract_id, "ticket-opened");
    assert_eq!(envelope.contract_version, "v1");
    assert_eq!(envelope.producer_service_id, "support");
    assert_eq!(envelope.module_id, "support-ticket");
    assert_eq!(envelope.content.content_type, "application/json");
    assert_eq!(
        envelope.content.schema,
        "contracts/events/support/support.ticket-opened.v1.schema.json"
    );
    assert_eq!(
        envelope.context.story.as_ref().unwrap().story_id,
        "story_support_case_01"
    );
    assert_eq!(
        envelope.context.causation.as_ref().unwrap().causation_id,
        "request_01"
    );
    assert_eq!(
        envelope.context.tenant.as_ref().unwrap().tenant_id,
        "tenant_01"
    );
    assert_eq!(
        envelope.context.service_principal.as_ref().unwrap().subject,
        "spiffe://example.com/service/support"
    );
    assert_eq!(
        envelope.context.region.as_ref().unwrap().operating_region,
        "cn-east-1"
    );
}

#[test]
fn event_generation_rejects_a_contract_not_declared_by_the_service() {
    let service: AutonomousServiceContract =
        serde_json::from_str(AUTONOMOUS_SERVICE_V2_FIXTURE_JSON).unwrap();
    let mut undeclared = service.event_contracts[0].clone();
    undeclared
        .context
        .required
        .push(CommonContextRequirement::Deadline);
    let payload_schema = serde_json::from_str(SUPPORT_EVENT_SCHEMA_JSON).unwrap();

    assert_eq!(
        generate_event_contract(&service, &undeclared, &payload_schema),
        Err(EventContractGenerationError::InvalidDeclaration)
    );
    let mut invalid_schema = payload_schema;
    invalid_schema["title"] = json!("support.ticket-opened.invalid");
    assert_eq!(
        generate_event_contract(&service, &service.event_contracts[0], &invalid_schema),
        Err(EventContractGenerationError::InvalidPayloadSchema)
    );
    let nested_schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://contracts.lenso.local/events/support.ticket-opened.v1.schema.json",
        "title": "support.ticket-opened.v1",
        "type": "object",
        "$defs": {
            "ticket": {
                "type": "object",
                "properties": {
                    "tags": {"type": "array", "items": {"type": "string"}}
                },
                "required": ["tags"]
            }
        },
        "properties": {
            "ticket": {"$ref": "#/$defs/ticket"}
        },
        "required": ["ticket"]
    });
    assert!(generate_event_contract(&service, &service.event_contracts[0], &nested_schema).is_ok());
}

#[test]
fn envelope_validation_reports_a_malformed_deserialized_payload_schema() {
    let service: AutonomousServiceContract =
        serde_json::from_str(AUTONOMOUS_SERVICE_V2_FIXTURE_JSON).unwrap();
    let payload_schema = serde_json::from_str(SUPPORT_EVENT_SCHEMA_JSON).unwrap();
    let mut contract =
        generate_event_contract(&service, &service.event_contracts[0], &payload_schema).unwrap();
    let context: CommonContextContract =
        serde_json::from_str(COMMON_CONTEXT_V1_FIXTURE_JSON).unwrap();
    let envelope = EventEnvelope::new(
        &contract,
        "event_support_ticket_01",
        "2026-07-14T10:15:30Z",
        context,
        json!({"ticketId": "ticket_01", "openedAt": "2026-07-14T10:15:00Z"}),
    );
    contract.payload_schema = json!({"type": 42});

    let issues = lenso_service::validate_event_envelope(&contract, &envelope);

    assert_eq!(issues[0].code, EventEnvelopeIssueCode::InvalidContent);
    assert_eq!(issues[0].path, "$.content.data");
    assert!(!issues[0].next_action.is_empty());
}

#[test]
fn cloudevents_structured_representation_round_trips_every_lenso_field() {
    let service: AutonomousServiceContract =
        serde_json::from_str(AUTONOMOUS_SERVICE_V2_FIXTURE_JSON).unwrap();
    let payload_schema = serde_json::from_str(SUPPORT_EVENT_SCHEMA_JSON).unwrap();
    let contract = generate_event_contract(&service, &service.event_contracts[0], &payload_schema)
        .expect("support event declaration should generate");
    let context: CommonContextContract =
        serde_json::from_str(COMMON_CONTEXT_V1_FIXTURE_JSON).unwrap();
    let envelope = EventEnvelope::new(
        &contract,
        "event_support_ticket_01",
        "2026-07-14T10:15:30Z",
        context,
        json!({"ticketId": "ticket_01", "openedAt": "2026-07-14T10:15:00Z"}),
    );

    let cloud_event = envelope.to_cloudevent();
    let serialized = serde_json::to_value(&cloud_event).unwrap();
    assert_eq!(serialized["specversion"], "1.0");
    assert_eq!(serialized["id"], envelope.event_id);
    assert_eq!(serialized["type"], envelope.event_type);
    assert_eq!(serialized["source"], "urn:lenso:service:support");
    assert_eq!(serialized["subject"], "support-ticket/ticket-opened/v1");
    assert_eq!(serialized["data"]["context"], json!(envelope.context));
    assert!(serialized.get("topic").is_none());
    assert!(serialized.get("partition").is_none());
    assert!(serialized.get("offset").is_none());

    let decoded = event_envelope_from_cloudevent(&contract, &cloud_event)
        .expect("CloudEvents representation should decode");
    assert_eq!(decoded, envelope);
}

#[test]
fn envelope_validation_rejects_missing_malformed_untrusted_and_incompatible_context() {
    let service: AutonomousServiceContract =
        serde_json::from_str(AUTONOMOUS_SERVICE_V2_FIXTURE_JSON).unwrap();
    let payload_schema = serde_json::from_str(SUPPORT_EVENT_SCHEMA_JSON).unwrap();
    let contract = generate_event_contract(&service, &service.event_contracts[0], &payload_schema)
        .expect("support event declaration should generate");
    let context: CommonContextContract =
        serde_json::from_str(COMMON_CONTEXT_V1_FIXTURE_JSON).unwrap();
    let envelope = EventEnvelope::new(
        &contract,
        "event_support_ticket_01",
        "2026-07-14T10:15:30Z",
        context,
        json!({"ticketId": "ticket_01", "openedAt": "2026-07-14T10:15:00Z"}),
    );

    let mut missing = serde_json::to_value(&envelope).unwrap();
    missing["context"].as_object_mut().unwrap().remove("story");
    let mut malformed = serde_json::to_value(&envelope).unwrap();
    malformed["context"]["trace"]["traceparent"] = json!("");
    let mut malformed_time = serde_json::to_value(&envelope).unwrap();
    malformed_time["occurredAt"] = json!("yesterday");
    let mut untrusted = serde_json::to_value(&envelope).unwrap();
    untrusted["context"]["trace"]["baggage"]["actor.role"] = json!("admin");
    let mut incompatible = serde_json::to_value(&envelope).unwrap();
    incompatible["contractId"] = json!("ticket-closed");
    let mut incompatible_context = serde_json::to_value(&envelope).unwrap();
    incompatible_context["context"]["protocol"] = json!("lenso.context.v2");
    let mut incompatible_region = serde_json::to_value(&envelope).unwrap();
    incompatible_region["context"]["region"]["operatingRegion"] = json!("eu-west-1");
    let mut invalid_content = serde_json::to_value(&envelope).unwrap();
    invalid_content["content"]["data"] = json!({});

    for (value, expected_code, expected_path) in [
        (
            missing,
            EventEnvelopeIssueCode::MissingRequiredContext,
            "$.context.story.storyId",
        ),
        (
            malformed,
            EventEnvelopeIssueCode::MalformedContext,
            "$.context.trace.traceparent",
        ),
        (
            malformed_time,
            EventEnvelopeIssueCode::InvalidOccurrenceTime,
            "$.occurredAt",
        ),
        (
            untrusted,
            EventEnvelopeIssueCode::UntrustedContext,
            "$.context.trace.baggage.actor.role",
        ),
        (
            incompatible,
            EventEnvelopeIssueCode::IncompatibleContractIdentity,
            "$.contractId",
        ),
        (
            incompatible_context,
            EventEnvelopeIssueCode::IncompatibleContext,
            "$.context.protocol",
        ),
        (
            incompatible_region,
            EventEnvelopeIssueCode::IncompatibleRegion,
            "$.context.region.operatingRegion",
        ),
        (
            invalid_content,
            EventEnvelopeIssueCode::InvalidContent,
            "$.content.data",
        ),
    ] {
        let issues = validate_event_envelope_value(&contract, &value);
        assert_eq!(issues[0].code, expected_code);
        assert_eq!(issues[0].path, expected_path);
        assert!(!issues[0].next_action.is_empty());
    }
}

#[test]
fn subset_context_round_trips_without_undeclared_context_fields() {
    let mut service: AutonomousServiceContract =
        serde_json::from_str(AUTONOMOUS_SERVICE_V2_FIXTURE_JSON).unwrap();
    service.event_contracts[0].tenancy_mode = lenso_service::ServiceTenancyMode::Optional;
    service.event_contracts[0].context.required = vec![CommonContextRequirement::Story];
    let payload_schema = serde_json::from_str(SUPPORT_EVENT_SCHEMA_JSON).unwrap();
    let contract = generate_event_contract(&service, &service.event_contracts[0], &payload_schema)
        .expect("subset context declaration should generate");
    let full: CommonContextContract = serde_json::from_str(COMMON_CONTEXT_V1_FIXTURE_JSON).unwrap();
    let context = EventContext {
        protocol: full.protocol,
        story: Some(full.story),
        trace: None,
        service_principal: None,
        delegated_actor: None,
        tenant: None,
        deadline: None,
        idempotency_key: None,
        causation: None,
        region: None,
    };
    let envelope = EventEnvelope::new(
        &contract,
        "event_support_ticket_02",
        "2026-07-14T10:15:30Z",
        context,
        json!({"ticketId": "ticket_02", "openedAt": "2026-07-14T10:15:00Z"}),
    );

    assert!(lenso_service::validate_event_envelope(&contract, &envelope).is_empty());
    assert_eq!(
        event_envelope_from_cloudevent(&contract, &envelope.to_cloudevent()).unwrap(),
        envelope
    );
}

#[test]
fn generated_event_artifacts_classify_every_compatibility_category() {
    let service: AutonomousServiceContract =
        serde_json::from_str(AUTONOMOUS_SERVICE_V2_FIXTURE_JSON).unwrap();
    let payload_schema = serde_json::from_str(SUPPORT_EVENT_SCHEMA_JSON).unwrap();
    let before = generate_event_contract(&service, &service.event_contracts[0], &payload_schema)
        .expect("support event declaration should generate");
    let mut safe = before.clone();
    safe.contract_version = "v2".to_owned();
    safe.event_type = "support.ticket-opened.v2".to_owned();
    safe.artifact.path = "contracts/events/support/support.ticket-opened.v2.schema.json".to_owned();
    safe.payload_schema["title"] = json!("support.ticket-opened.v2");
    safe.payload_schema["properties"]["priority"] = json!({"type": "string"});
    let mut needs_attention = safe.clone();
    needs_attention
        .operating_regions
        .push("eu-west-1".to_owned());
    let mut breaking = safe.clone();
    breaking
        .context
        .required
        .push(CommonContextRequirement::Deadline);
    let mut blocked = safe.clone();
    blocked.protocol = "lenso.event-contract.v2".to_owned();

    for (after, expected) in [
        (safe, CompatibilityCategory::Safe),
        (needs_attention, CompatibilityCategory::NeedsAttention),
        (breaking, CompatibilityCategory::Breaking),
        (blocked, CompatibilityCategory::Blocked),
    ] {
        let result = evaluate_generated_event_contract_compatibility(&before, &after);
        assert_eq!(result.category, expected);
        assert!(!result.reasons.is_empty());
        assert!(result.reasons.iter().all(|reason| {
            !reason.code.is_empty() && !reason.path.is_empty() && !reason.next_action.is_empty()
        }));
        if expected != CompatibilityCategory::Safe {
            assert!(
                result
                    .reasons
                    .iter()
                    .all(|reason| reason.code != "event_backward_compatible")
            );
        }
    }
    let mut renamed = before.clone();
    renamed.contract_version = "v2".to_owned();
    renamed.event_type = "renamed.ticket-opened.v2".to_owned();
    renamed.artifact.path =
        "contracts/events/support/renamed.ticket-opened.v2.schema.json".to_owned();
    renamed.payload_schema["title"] = json!("renamed.ticket-opened.v2");
    assert_eq!(
        evaluate_generated_event_contract_compatibility(&before, &renamed).category,
        CompatibilityCategory::Breaking
    );
    let mut redirected = before.clone();
    redirected.contract_version = "v2".to_owned();
    redirected.event_type = "support.ticket-opened.v2".to_owned();
    redirected.artifact.path =
        "contracts/events/redirected/support.ticket-opened.v2.schema.json".to_owned();
    redirected.payload_schema["title"] = json!("support.ticket-opened.v2");
    assert_eq!(
        evaluate_generated_event_contract_compatibility(&before, &redirected).category,
        CompatibilityCategory::Breaking
    );
}
