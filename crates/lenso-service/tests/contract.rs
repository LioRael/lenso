use lenso_service::{
    AUTONOMOUS_SERVICE_V2_FIXTURE_JSON, AutonomousServiceContract, AutonomousServiceIssueCode,
    AutonomousServiceStore, AutonomousServiceWorkload, COMMON_CONTEXT_V1_FIXTURE_JSON,
    COMMON_CONTEXT_V1_SCHEMA_JSON, CommonContextContract, CommonContextIssueCode,
    CommonContextRequirement, ConfigActivation, ConfigContract, ConfigFieldContract,
    ConfigMutability, ConfigScope, ContractArtifactCheckErrorCode, ContractArtifactKind,
    ContractContextRequirements, ContractOwner, ContractSemanticKind, EventArtifactFormat,
    EventArtifactReference, EventContractArtifact, LEGACY_CONTRACT_FIXTURES,
    MIXED_SYSTEM_V2_FIXTURE_JSON, MODULE_CONTRACT_SCHEMA_JSON, MODULE_RELEASE_SCHEMA_JSON,
    ModuleContract, ModuleManifest, ReliabilityContract, SERVICE_CONTRACT_SCHEMA_JSON,
    SERVICE_SYSTEM_SCHEMA_JSON, SERVICE_V2_CONTRACT_SCHEMA_JSON, SERVICE_WORKSPACE_SCHEMA_JSON,
    SYSTEM_V2_CONTRACT_SCHEMA_JSON, SchemaArtifactReference, ServiceArtifactFormat,
    ServiceArtifactReference, ServiceCompatibility, ServiceContract, ServiceContractArtifact,
    ServiceHealth, ServiceLocalProcess, ServiceProvider, ServiceSystem, ServiceSystemDependency,
    ServiceSystemModule, ServiceSystemService, ServiceTenancyMode, ServiceWorkspace,
    ServiceWorkspaceService, WorkloadRole, check_contract_artifact_value,
    check_contract_artifact_value_with_artifacts, service_system_graph, system_v2_graph,
    validate_autonomous_service_artifact_references, validate_module_contract_value,
    validate_service_contract_value, validate_service_system_value,
    validate_service_workspace_value,
};
use serde_json::json;

#[test]
fn raw_openapi_document_canonicalizes_for_the_authoritative_evaluator() {
    let document = json!({
        "openapi": "3.1.0",
        "info": { "title": "Support", "version": "v1" },
        "paths": { "/tickets": { "post": {
            "operationId": "createTicket",
            "requestBody": { "content": { "application/json": { "schema": { "$ref": "#/components/schemas/CreateTicket" } } } },
            "responses": { "200": { "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Ticket" } } } } }
        } } },
        "components": { "schemas": {
            "CreateTicket": { "type": "object", "required": ["subject"], "properties": { "subject": { "type": "string" } } },
            "Ticket": { "type": "object", "required": ["id"], "properties": { "id": { "type": "string" } } }
        } }
    });
    let canonical = lenso_service::canonicalize_openapi_request_response(&document).unwrap();
    assert_eq!(canonical["format"], "openapi");
    assert_eq!(canonical["version"], "v1");
    assert_eq!(
        canonical["operations"]["createTicket"]["request"]["required"][0],
        "subject"
    );
}

#[test]
fn raw_canonicalizers_block_unverifiable_or_empty_contracts() {
    let unresolved = json!({
        "openapi": "3.1.0",
        "info": { "title": "Support", "version": "v1" },
        "paths": { "/tickets": { "get": {
            "operationId": "getTicket",
            "responses": { "200": { "content": { "application/json": { "schema": { "$ref": "external.yaml#/Ticket" } } } } }
        } } }
    });
    let errors = lenso_service::canonicalize_openapi_request_response(&unresolved).unwrap_err();
    assert_eq!(errors[0].code, "openapi_reference_unverifiable");

    let empty = json!({
        "openapi": "3.1.0",
        "info": { "title": "Support", "version": "v1" },
        "paths": {}
    });
    let errors = lenso_service::canonicalize_openapi_request_response(&empty).unwrap_err();
    assert_eq!(errors[0].code, "openapi_operations_missing");

    use prost::Message;
    let descriptor = prost_types::FileDescriptorSet::default().encode_to_vec();
    let errors =
        lenso_service::canonicalize_protobuf_request_response("v1", &descriptor).unwrap_err();
    assert_eq!(errors[0].code, "protobuf_operations_missing");
}

#[test]
fn protobuf_descriptor_set_canonicalizes_for_the_authoritative_evaluator() {
    use prost::Message;
    use prost_types::{
        DescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet,
        MethodDescriptorProto, ServiceDescriptorProto, field_descriptor_proto,
    };

    let message = |name: &str, field_name: &str| DescriptorProto {
        name: Some(name.to_owned()),
        field: vec![FieldDescriptorProto {
            name: Some(field_name.to_owned()),
            number: Some(1),
            label: Some(field_descriptor_proto::Label::Optional as i32),
            r#type: Some(field_descriptor_proto::Type::String as i32),
            ..Default::default()
        }],
        ..Default::default()
    };
    let descriptor = FileDescriptorSet {
        file: vec![FileDescriptorProto {
            package: Some("support.v1".to_owned()),
            message_type: vec![
                message("GetTicketRequest", "id"),
                message("Ticket", "status"),
            ],
            service: vec![ServiceDescriptorProto {
                name: Some("Support".to_owned()),
                method: vec![MethodDescriptorProto {
                    name: Some("GetTicket".to_owned()),
                    input_type: Some(".support.v1.GetTicketRequest".to_owned()),
                    output_type: Some(".support.v1.Ticket".to_owned()),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        }],
    };
    let canonical =
        lenso_service::canonicalize_protobuf_request_response("v1", &descriptor.encode_to_vec())
            .unwrap();
    assert_eq!(canonical["format"], "protobuf");
    assert_eq!(canonical["version"], "v1");
    assert_eq!(
        canonical["operations"]["support.v1.Support.GetTicket"]["response"]["fields"][0]["name"],
        "status"
    );
}

#[test]
fn request_response_compatibility_golden_pairs_cover_every_public_category() {
    use lenso_service::{
        REQUEST_RESPONSE_COMPATIBILITY_FIXTURES, RequestResponseCompatibilityCategory,
        evaluate_request_response_compatibility_in_system,
    };

    let expected = [
        RequestResponseCompatibilityCategory::Safe,
        RequestResponseCompatibilityCategory::NeedsAttention,
        RequestResponseCompatibilityCategory::Breaking,
        RequestResponseCompatibilityCategory::Blocked,
    ];
    assert_eq!(
        REQUEST_RESPONSE_COMPATIBILITY_FIXTURES.len(),
        expected.len()
    );
    for (fixture, expected_category) in REQUEST_RESPONSE_COMPATIBILITY_FIXTURES.iter().zip(expected)
    {
        let input: serde_json::Value = serde_json::from_str(fixture.json).unwrap();
        let mut system: serde_json::Value =
            serde_json::from_str(MIXED_SYSTEM_V2_FIXTURE_JSON).unwrap();
        if input["before"]["format"] == "protobuf" {
            system["contracts"][0]["artifact"]["format"] = json!("protobuf");
        }
        let result = evaluate_request_response_compatibility_in_system(&system, &input);
        assert_eq!(result.category, expected_category, "{}", fixture.name);
        assert!(!result.reasons.is_empty());
        assert!(
            result
                .reasons
                .iter()
                .all(|reason| !reason.next_action.is_empty())
        );
        assert!(!result.changed_version.is_empty());
    }
}

#[test]
fn request_response_compatibility_reports_kind_version_and_affected_relationships() {
    use lenso_service::{
        RequestResponseCompatibilityCategory, RequestResponseContractKind,
        evaluate_request_response_compatibility_in_system,
    };

    let input =
        serde_json::from_str(lenso_service::REQUEST_RESPONSE_COMPATIBILITY_SAFE_FIXTURE_JSON)
            .unwrap();
    let system = serde_json::from_str(MIXED_SYSTEM_V2_FIXTURE_JSON).unwrap();
    let result = evaluate_request_response_compatibility_in_system(&system, &input);
    assert_eq!(result.category, RequestResponseCompatibilityCategory::Safe);
    assert_eq!(
        result.contract_kind,
        RequestResponseContractKind::ServiceContract
    );
    assert_eq!(result.changed_version, "v2");
    assert_eq!(result.producers, ["autonomous_service:support"]);
    assert_eq!(result.consumers, ["host:support-host"]);
}

#[test]
fn request_response_compatibility_is_canonical_and_never_guesses_safe() {
    use lenso_service::{
        RequestResponseCompatibilityCategory, evaluate_request_response_compatibility,
        evaluate_request_response_compatibility_in_system,
    };

    let input: serde_json::Value =
        serde_json::from_str(lenso_service::REQUEST_RESPONSE_COMPATIBILITY_SAFE_FIXTURE_JSON)
            .unwrap();
    let mut reordered = input.clone();
    reordered["before"]["operations"]
        .as_object_mut()
        .unwrap()
        .insert(
            "z-unused".to_owned(),
            json!({"request": {}, "response": {}}),
        );
    reordered["after"]["operations"]
        .as_object_mut()
        .unwrap()
        .insert(
            "z-unused".to_owned(),
            json!({"request": {}, "response": {}}),
        );
    let mut original = input;
    original["before"]["operations"]
        .as_object_mut()
        .unwrap()
        .insert(
            "z-unused".to_owned(),
            json!({"response": {}, "request": {}}),
        );
    original["after"]["operations"]
        .as_object_mut()
        .unwrap()
        .insert(
            "z-unused".to_owned(),
            json!({"response": {}, "request": {}}),
        );
    let system: serde_json::Value = serde_json::from_str(MIXED_SYSTEM_V2_FIXTURE_JSON).unwrap();
    assert_eq!(
        serde_json::to_vec(&evaluate_request_response_compatibility_in_system(
            &system, &original
        ))
        .unwrap(),
        serde_json::to_vec(&evaluate_request_response_compatibility_in_system(
            &system, &reordered
        ))
        .unwrap()
    );

    let mut unverifiable: serde_json::Value =
        serde_json::from_str(lenso_service::REQUEST_RESPONSE_COMPATIBILITY_SAFE_FIXTURE_JSON)
            .unwrap();
    assert_eq!(
        evaluate_request_response_compatibility(&unverifiable).category,
        RequestResponseCompatibilityCategory::Blocked
    );
    unverifiable["contractId"] = json!("invented-contract");
    assert_eq!(
        evaluate_request_response_compatibility_in_system(&system, &unverifiable).category,
        RequestResponseCompatibilityCategory::Blocked
    );
}

#[test]
fn provider_protocol_and_autonomous_service_contract_results_are_distinct() {
    use lenso_service::{
        RequestResponseContractKind, evaluate_request_response_compatibility_in_system,
    };

    let mut input: serde_json::Value =
        serde_json::from_str(lenso_service::REQUEST_RESPONSE_COMPATIBILITY_SAFE_FIXTURE_JSON)
            .unwrap();
    input["contractKind"] = json!("provider_protocol");
    let system = serde_json::from_str(MIXED_SYSTEM_V2_FIXTURE_JSON).unwrap();
    let result = evaluate_request_response_compatibility_in_system(&system, &input);
    assert_eq!(
        result.contract_kind,
        RequestResponseContractKind::ProviderProtocol
    );
    assert!(
        result
            .reasons
            .iter()
            .all(|reason| reason.code.starts_with("provider_protocol_")
                || reason.code == "relationship_unverifiable")
    );
}

#[test]
fn mixed_system_v2_fixture_builds_a_deterministic_explicit_graph() {
    let source: serde_json::Value = serde_json::from_str(MIXED_SYSTEM_V2_FIXTURE_JSON).unwrap();
    let check = check_contract_artifact_value(&source).unwrap();
    assert_eq!(check.detected_protocol, "lenso.system.v2");
    assert_eq!(check.semantic_kind, ContractSemanticKind::MixedSystem);

    let graph = system_v2_graph(&source).unwrap();
    assert_eq!(graph.artifact_protocol, "lenso.system.v2");
    assert_eq!(graph.semantic_kind, ContractSemanticKind::MixedSystem);
    for kind in [
        "host",
        "provider",
        "autonomous_service",
        "module",
        "workload",
        "producer",
        "consumer",
    ] {
        assert!(
            graph.nodes.iter().any(|node| node.kind == kind),
            "missing {kind}"
        );
    }
    assert!(graph.issues.is_empty());

    let mut reordered = source.clone();
    for field in ["providers", "autonomousServices", "contracts", "consumers"] {
        reordered[field].as_array_mut().unwrap().reverse();
    }
    reordered["host"]["modules"]
        .as_array_mut()
        .unwrap()
        .reverse();
    for service in reordered["autonomousServices"].as_array_mut().unwrap() {
        service["modules"].as_array_mut().unwrap().reverse();
        service["workloads"].as_array_mut().unwrap().reverse();
    }
    assert_eq!(
        serde_json::to_string(&graph).unwrap(),
        serde_json::to_string(&system_v2_graph(&reordered).unwrap()).unwrap()
    );
}

#[test]
fn system_v2_validation_has_stable_actionable_boundary_codes() {
    let source: serde_json::Value = serde_json::from_str(MIXED_SYSTEM_V2_FIXTURE_JSON).unwrap();
    let cases = [
        ("missing_ownership", json!(null), "missing_ownership"),
        (
            "unresolved_reference",
            json!("missing-contract"),
            "unresolved_reference",
        ),
        ("ambiguous_kind", json!("service"), "ambiguous_kind"),
        (
            "incompatible_tenancy",
            json!("none"),
            "incompatible_tenancy",
        ),
    ];

    for (case, replacement, expected_code) in cases {
        let mut invalid = source.clone();
        match case {
            "missing_ownership" => invalid["autonomousServices"][0]["modules"] = replacement,
            "unresolved_reference" => invalid["consumers"][0]["contractId"] = replacement,
            "ambiguous_kind" => invalid["consumers"][0]["ownerKind"] = replacement,
            "incompatible_tenancy" => invalid["consumers"][0]["tenancyMode"] = replacement,
            _ => unreachable!(),
        }
        let error = system_v2_graph(&invalid).unwrap_err();
        assert_eq!(error[0].code, expected_code);
        assert!(!error[0].next_action.is_empty());
    }
}

#[test]
fn system_v2_validation_is_deterministic_and_requires_a_mixed_topology() {
    let source: serde_json::Value = serde_json::from_str(MIXED_SYSTEM_V2_FIXTURE_JSON).unwrap();
    let mut invalid = source.clone();
    invalid["consumers"][0]["contractId"] = json!("missing-z");
    invalid["consumers"][1]["contractId"] = json!("missing-a");
    let mut reordered = invalid.clone();
    reordered["consumers"].as_array_mut().unwrap().reverse();
    assert_eq!(
        system_v2_graph(&invalid).unwrap_err(),
        system_v2_graph(&reordered).unwrap_err()
    );

    for field in ["providers", "autonomousServices", "contracts", "consumers"] {
        let mut missing = source.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(system_v2_graph(&missing).unwrap_err().iter().any(|issue| {
            issue.code == "missing_ownership" && issue.path == format!("$.{field}")
        }));
    }

    let mut colliding = source;
    colliding["providers"][0]["providerId"] = json!("support-host");
    assert!(
        system_v2_graph(&colliding)
            .unwrap_err()
            .iter()
            .any(|issue| issue.code == "ambiguous_kind")
    );
}

#[test]
fn system_v2_schema_is_packaged_with_the_sdk() {
    let schema: serde_json::Value = serde_json::from_str(SYSTEM_V2_CONTRACT_SCHEMA_JSON).unwrap();
    assert_eq!(schema["properties"]["protocol"]["const"], "lenso.system.v2");
}

#[test]
fn common_context_v1_fixture_round_trips_through_the_public_contract() {
    let source: serde_json::Value = serde_json::from_str(COMMON_CONTEXT_V1_FIXTURE_JSON).unwrap();
    let contract: CommonContextContract = serde_json::from_value(source.clone()).unwrap();

    assert_eq!(contract.story.story_id, "story_support_case_01");
    assert_eq!(
        contract.trace.traceparent,
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
    );
    assert_eq!(
        contract.service_principal.subject,
        "spiffe://example.com/service/support"
    );
    assert_eq!(contract.delegated_actor.subject, "user_01");
    assert_eq!(contract.tenant.tenant_id, "tenant_01");
    assert_eq!(contract.idempotency_key.value, "create-case-01");
    assert_eq!(contract.causation.causation_id, "request_01");
    assert_eq!(contract.region.operating_region, "cn-east-1");
    assert_eq!(serde_json::to_value(&contract).unwrap(), source);
    assert!(lenso_service::validate_common_context_contract(&contract).is_empty());
}

#[test]
fn common_context_v1_rejects_authorization_claims_from_baggage() {
    let mut source: serde_json::Value =
        serde_json::from_str(COMMON_CONTEXT_V1_FIXTURE_JSON).unwrap();
    source["trace"]["baggage"] = json!({
        "auth.actor": "user_from_baggage",
        "actorSubject": "user_from_camel_case_baggage",
        "tenantId": "tenant_from_camel_case_baggage",
        "x-tenant": "tenant_from_baggage"
    });

    let issues = lenso_service::validate_common_context_contract_value(&source);
    assert_eq!(issues.len(), 4);
    assert_eq!(issues[0].code, CommonContextIssueCode::UntrustedActorClaim);
    assert_eq!(issues[0].path, "$.trace.baggage.actorSubject");
    assert_eq!(issues[1].code, CommonContextIssueCode::UntrustedActorClaim);
    assert_eq!(issues[1].path, "$.trace.baggage.auth.actor");
    assert_eq!(issues[2].code, CommonContextIssueCode::UntrustedTenantClaim);
    assert_eq!(issues[2].path, "$.trace.baggage.tenantId");
    assert_eq!(issues[3].code, CommonContextIssueCode::UntrustedTenantClaim);
    assert_eq!(issues[3].path, "$.trace.baggage.x-tenant");
    assert!(issues.iter().all(|issue| !issue.next_action.is_empty()));
}

#[test]
fn common_context_v1_allows_non_authorization_baggage() {
    let mut source: serde_json::Value =
        serde_json::from_str(COMMON_CONTEXT_V1_FIXTURE_JSON).unwrap();
    source["trace"]["baggage"] = json!({
        "workflow.id": "workflow_01",
        "vendor.routing": "canary",
        "experiment": "context-v1"
    });

    assert!(lenso_service::validate_common_context_contract_value(&source).is_empty());
}

#[test]
fn common_context_v1_validates_the_receiving_audience() {
    let contract: CommonContextContract =
        serde_json::from_str(COMMON_CONTEXT_V1_FIXTURE_JSON).unwrap();

    assert!(
        lenso_service::validate_common_context_contract_for_audience(&contract, "support-api")
            .is_empty()
    );
    let issues =
        lenso_service::validate_common_context_contract_for_audience(&contract, "billing-api");
    assert_eq!(issues.len(), 3);
    assert!(
        issues
            .iter()
            .all(|issue| issue.code == CommonContextIssueCode::AudienceMismatch)
    );
}

#[test]
fn common_context_v1_requires_bounded_delegated_permissions() {
    let mut source: serde_json::Value =
        serde_json::from_str(COMMON_CONTEXT_V1_FIXTURE_JSON).unwrap();
    source["delegatedActor"]["permissions"] = json!([]);

    let issues = lenso_service::validate_common_context_contract_value(&source);
    assert_eq!(issues.len(), 1);
    assert_eq!(
        issues[0].code,
        CommonContextIssueCode::InvalidDelegatedActorContext
    );
    assert_eq!(issues[0].path, "$.delegatedActor.permissions");
    assert_eq!(
        issues[0].next_action,
        "Declare at least one permission narrowed for this delegation."
    );
}

#[test]
fn common_context_v1_keeps_identifiers_and_identity_boundaries_distinct() {
    let source: serde_json::Value = serde_json::from_str(COMMON_CONTEXT_V1_FIXTURE_JSON).unwrap();

    assert_ne!(source["story"]["storyId"], source["trace"]["traceparent"]);
    assert_ne!(
        source["idempotencyKey"]["value"],
        source["causation"]["causationId"]
    );
    assert_ne!(
        source["region"]["operatingRegion"],
        source["servicePrincipal"]["subject"]
    );
}

#[test]
fn common_context_v1_schema_covers_every_context_contract() {
    let schema: serde_json::Value = serde_json::from_str(COMMON_CONTEXT_V1_SCHEMA_JSON).unwrap();

    assert_eq!(schema["title"], "LensoCommonContextContract");
    assert_eq!(
        schema["properties"]["protocol"]["const"],
        "lenso.context.v1"
    );
    for field in [
        "story",
        "trace",
        "servicePrincipal",
        "delegatedActor",
        "tenant",
        "deadline",
        "idempotencyKey",
        "causation",
        "region",
    ] {
        assert!(schema["properties"].get(field).is_some(), "missing {field}");
    }
}

#[test]
fn autonomous_service_v2_fixture_round_trips_through_the_public_contract() {
    let source: serde_json::Value =
        serde_json::from_str(AUTONOMOUS_SERVICE_V2_FIXTURE_JSON).unwrap();
    let contract: AutonomousServiceContract = serde_json::from_value(source.clone()).unwrap();

    assert_eq!(contract.service_id, "support");
    assert_eq!(contract.workloads.len(), 4);
    assert_eq!(contract.workloads[0].role, WorkloadRole::API);
    assert_eq!(contract.workloads[1].role, WorkloadRole::WORKER);
    assert_eq!(contract.workloads[2].role, WorkloadRole::MIGRATION);
    assert_eq!(contract.workloads[3].role.as_str(), "indexer");
    assert_eq!(contract.service_contracts.len(), 2);
    assert_eq!(
        contract.service_contracts[0].artifact.format,
        ServiceArtifactFormat::Openapi
    );
    assert_eq!(
        contract.service_contracts[1].artifact.format,
        ServiceArtifactFormat::Protobuf
    );
    assert_eq!(contract.event_contracts.len(), 1);
    assert_eq!(contract.config_contract.as_ref().unwrap().fields.len(), 2);
    assert_eq!(
        contract
            .reliability_contract
            .as_ref()
            .unwrap()
            .availability_target,
        "99.9%"
    );
    assert_eq!(serde_json::to_value(&contract).unwrap(), source);
    assert!(lenso_service::validate_autonomous_service_contract(&contract).is_empty());
}

#[test]
fn autonomous_service_v2_public_types_declare_owned_contract_artifacts() {
    let mut service = AutonomousServiceContract::new(
        "support",
        vec![AutonomousServiceWorkload::new(
            "support-api",
            "support",
            WorkloadRole::API,
        )],
        ServiceTenancyMode::Required,
        vec!["cn-east-1".to_owned()],
    );
    service.modules = vec!["support-ticket".to_owned()];
    service.service_contracts = vec![ServiceContractArtifact::new(
        "support-api",
        "support-ticket",
        "v1",
        ServiceTenancyMode::Required,
        ServiceArtifactReference::new(
            ServiceArtifactFormat::Openapi,
            "contracts/openapi/support.v1.yaml",
        ),
    )];
    service.service_contracts[0].context =
        ContractContextRequirements::new(vec![CommonContextRequirement::Tenant]);
    service.event_contracts = vec![EventContractArtifact::new(
        "ticket-opened",
        "support-ticket",
        "v1",
        ServiceTenancyMode::Required,
        EventArtifactReference::new(
            EventArtifactFormat::JsonSchema,
            "contracts/events/support/support.ticket-opened.v1.schema.json",
        ),
    )];
    service.event_contracts[0].context =
        ContractContextRequirements::new(vec![CommonContextRequirement::Story]);
    service.config_contract = Some(ConfigContract::new(
        "support-config",
        "v1",
        SchemaArtifactReference::new("contracts/config/support.v1.schema.json"),
        vec![ConfigFieldContract {
            path: "notifications.webhook".to_owned(),
            shape: "uri".to_owned(),
            sensitive: true,
            scope: ConfigScope::Service,
            mutability: ConfigMutability::Mutable,
            activation: ConfigActivation::Hot,
        }],
    ));
    let mut reliability = ReliabilityContract::new(
        "support-reliability",
        "v1",
        SchemaArtifactReference::new("contracts/reliability/support.v1.schema.json"),
        "99.9%",
        "43m per 30d",
    );
    reliability.health_semantics = vec!["ready means serving traffic".to_owned()];
    reliability.degraded_modes = vec!["queue notifications".to_owned()];
    reliability.rollout_safety = vec!["rollback on elevated errors".to_owned()];
    service.reliability_contract = Some(reliability);

    assert!(lenso_service::validate_autonomous_service_contract(&service).is_empty());
    let value = serde_json::to_value(service).unwrap();
    assert_eq!(
        value["serviceContracts"][0]["artifact"]["format"],
        "openapi"
    );
    assert_eq!(
        value["eventContracts"][0]["artifact"]["format"],
        "json_schema"
    );
    assert_eq!(value["configContract"]["fields"][0]["activation"], "hot");
    assert_eq!(value["reliabilityContract"]["availabilityTarget"], "99.9%");
}

#[test]
fn autonomous_service_v2_contract_artifact_failures_are_deterministic() {
    let mut source: serde_json::Value =
        serde_json::from_str(AUTONOMOUS_SERVICE_V2_FIXTURE_JSON).unwrap();
    source["serviceContracts"] = json!([
        {
            "contractId": "support-api",
            "moduleId": "missing-module",
            "version": "v1",
            "tenancyMode": "required",
            "artifact": {"format": "asyncapi", "path": "contracts/api.yaml"},
            "context": {"protocol": "lenso.context.v1", "required": []}
        },
        {
            "contractId": "support-api",
            "moduleId": "support-ticket",
            "version": "v2",
            "tenancyMode": "required",
            "artifact": {"format": "openapi", "path": ""},
            "context": {"protocol": "lenso.context.v1", "required": []}
        }
    ]);

    let issues = lenso_service::validate_autonomous_service_contract_value(&source);
    assert_eq!(
        issues
            .iter()
            .map(|issue| (issue.code, issue.path.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (
                AutonomousServiceIssueCode::UnsupportedArtifactFormat,
                "$.serviceContracts[0].artifact.format"
            ),
            (
                AutonomousServiceIssueCode::UnresolvedModuleReference,
                "$.serviceContracts[0].moduleId"
            ),
            (
                AutonomousServiceIssueCode::DuplicateContractIdentity,
                "$.serviceContracts[1].contractId"
            ),
            (
                AutonomousServiceIssueCode::InvalidArtifactReference,
                "$.serviceContracts[1].artifact.path"
            ),
        ]
    );
    assert!(issues.iter().all(|issue| !issue.next_action.is_empty()));
}

#[test]
fn autonomous_service_v2_reports_unresolved_packaged_artifacts() {
    let source: serde_json::Value =
        serde_json::from_str(AUTONOMOUS_SERVICE_V2_FIXTURE_JSON).unwrap();
    let available = [
        "contracts/openapi/support.v1.yaml",
        "contracts/protobuf/support.v1.proto",
        "contracts/events/support/support.ticket-opened.v1.schema.json",
        "contracts/config/support.v1.schema.json",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();

    let issues = validate_autonomous_service_artifact_references(&source, &available);
    assert_eq!(issues.len(), 1);
    assert_eq!(
        issues[0].code,
        AutonomousServiceIssueCode::UnresolvedArtifactReference
    );
    assert_eq!(issues[0].path, "$.reliabilityContract.artifact.path");
    assert!(!issues[0].next_action.is_empty());
    let error = check_contract_artifact_value_with_artifacts(&source, &available).unwrap_err();
    assert_eq!(
        error.code,
        ContractArtifactCheckErrorCode::UnresolvedArtifactReference
    );
}

#[test]
fn autonomous_service_v2_fixture_references_packaged_contract_files() {
    let source: serde_json::Value =
        serde_json::from_str(AUTONOMOUS_SERVICE_V2_FIXTURE_JSON).unwrap();
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let paths = [
        source["serviceContracts"][0]["artifact"]["path"]
            .as_str()
            .unwrap(),
        source["serviceContracts"][1]["artifact"]["path"]
            .as_str()
            .unwrap(),
        source["eventContracts"][0]["artifact"]["path"]
            .as_str()
            .unwrap(),
        source["configContract"]["artifact"]["path"]
            .as_str()
            .unwrap(),
        source["reliabilityContract"]["artifact"]["path"]
            .as_str()
            .unwrap(),
    ];
    assert!(paths.iter().all(|path| repository.join(path).is_file()));
    let available = paths.into_iter().map(str::to_owned).collect();
    assert!(check_contract_artifact_value_with_artifacts(&source, &available).is_ok());
}

#[test]
fn autonomous_service_v2_rejects_malformed_config_and_reliability_contracts() {
    let mut source: serde_json::Value =
        serde_json::from_str(AUTONOMOUS_SERVICE_V2_FIXTURE_JSON).unwrap();
    source["configContract"]["fields"][0]["scope"] = json!("global");
    source["configContract"]["fields"][1]["path"] = json!("sla.defaultHours");
    source["reliabilityContract"]["availabilityTarget"] = json!("");

    let issues = lenso_service::validate_autonomous_service_contract_value(&source);
    assert_eq!(
        issues.iter().map(|issue| issue.code).collect::<Vec<_>>(),
        [
            AutonomousServiceIssueCode::InvalidConfigContract,
            AutonomousServiceIssueCode::DuplicateConfigField,
            AutonomousServiceIssueCode::InvalidReliabilityContract,
        ]
    );
    assert!(issues.iter().all(|issue| !issue.next_action.is_empty()));
}

#[test]
fn module_identity_survives_provider_to_autonomous_service_declarations() {
    let module_id = "support-ticket";
    let linked = ModuleManifest::builder(module_id).build();
    let provider = ServiceContract::new("support-provider", vec![linked.clone()]);
    let autonomous: AutonomousServiceContract =
        serde_json::from_str(AUTONOMOUS_SERVICE_V2_FIXTURE_JSON).unwrap();

    assert_eq!(linked.name, module_id);
    assert_eq!(provider.modules[0].name, linked.name);
    assert!(autonomous.modules.iter().any(|module| module == module_id));
    let contract = autonomous
        .service_contracts
        .iter()
        .find(|contract| contract.contract_id == "support-http")
        .unwrap();
    assert_eq!(contract.module_id, linked.name);
}

#[test]
fn autonomous_service_v2_identity_is_independent_of_runtime_topology() {
    let service = AutonomousServiceContract::new(
        "support",
        vec![AutonomousServiceWorkload::new(
            "support-api",
            "support",
            WorkloadRole::API,
        )],
        ServiceTenancyMode::Required,
        vec!["cn-east-1".to_owned()],
    );
    let identity = service.service_id.clone();
    let mut changed_topology = service.clone();
    changed_topology
        .workloads
        .push(AutonomousServiceWorkload::new(
            "support-worker",
            "support",
            WorkloadRole::WORKER,
        ));
    changed_topology.operating_regions = vec!["cn-east-1".to_owned(), "cn-north-1".to_owned()];

    assert_eq!(changed_topology.service_id, identity);
}

#[test]
fn autonomous_service_v2_validation_has_stable_codes_and_next_actions() {
    let mut service = AutonomousServiceContract::new(
        "support",
        vec![
            AutonomousServiceWorkload::new("api", "billing", WorkloadRole::API),
            AutonomousServiceWorkload::new("api", "support", WorkloadRole::WORKER),
        ],
        ServiceTenancyMode::Optional,
        vec![
            "cn-east-1".to_owned(),
            "cn-east-1".to_owned(),
            "".to_owned(),
        ],
    );
    service.stores = vec![
        AutonomousServiceStore::new("primary", "support"),
        AutonomousServiceStore::new("primary", "billing"),
    ];

    let issues = lenso_service::validate_autonomous_service_contract(&service);
    let codes = issues.iter().map(|issue| issue.code).collect::<Vec<_>>();

    assert_eq!(
        codes,
        vec![
            AutonomousServiceIssueCode::WorkloadOwnerMismatch,
            AutonomousServiceIssueCode::DuplicateWorkloadIdentity,
            AutonomousServiceIssueCode::StoreOwnerMismatch,
            AutonomousServiceIssueCode::DuplicateStoreIdentity,
            AutonomousServiceIssueCode::DuplicateOperatingRegion,
            AutonomousServiceIssueCode::InvalidOperatingRegion,
        ]
    );
    assert!(issues.iter().all(|issue| !issue.next_action.is_empty()));
}

#[test]
fn autonomous_service_v2_schema_and_artifact_check_agree() {
    let schema: serde_json::Value = serde_json::from_str(SERVICE_V2_CONTRACT_SCHEMA_JSON).unwrap();
    let source: serde_json::Value =
        serde_json::from_str(AUTONOMOUS_SERVICE_V2_FIXTURE_JSON).unwrap();
    let check = check_contract_artifact_value(&source).unwrap();

    assert_eq!(schema["title"], "LensoAutonomousServiceContract");
    assert_eq!(
        schema["properties"]["protocol"]["const"],
        "lenso.service.v2"
    );
    assert_eq!(check.semantic_kind, ContractSemanticKind::AutonomousService);
    assert_eq!(check.detected_protocol, "lenso.service.v2");
    let summary = check.autonomous_service.unwrap();
    assert_eq!(summary.modules, ["support-sla", "support-ticket"]);
    assert_eq!(summary.service_contracts, ["support-grpc", "support-http"]);
    assert_eq!(summary.event_contracts, ["ticket-opened"]);
    assert!(summary.has_config_contract);
    assert!(summary.has_reliability_contract);
    assert!(check.provider_semantics.is_none());
}

#[test]
fn autonomous_service_v2_raw_validation_rejects_invalid_tenancy_deterministically() {
    let mut source: serde_json::Value =
        serde_json::from_str(AUTONOMOUS_SERVICE_V2_FIXTURE_JSON).unwrap();
    source["tenancyMode"] = json!("sometimes");

    let issues = lenso_service::validate_autonomous_service_contract_value(&source);
    assert_eq!(issues.len(), 1);
    assert_eq!(
        issues[0].code,
        AutonomousServiceIssueCode::InvalidTenancyMode
    );
    assert_eq!(issues[0].path, "$.tenancyMode");
    assert_eq!(
        serde_json::to_value(&issues).unwrap()[0],
        json!({
            "code": "invalid_tenancy_mode",
            "path": "$.tenancyMode",
            "message": "tenancyMode must be `none`, `optional`, or `required`",
            "nextAction": "Choose one supported Tenancy Mode."
        })
    );
}

#[test]
fn autonomous_service_v2_check_rejects_schema_unknown_topology_fields() {
    let mut source: serde_json::Value =
        serde_json::from_str(AUTONOMOUS_SERVICE_V2_FIXTURE_JSON).unwrap();
    source["endpoints"] = json!(["https://support.example"]);
    source["workloads"][0]["instances"] = json!(3);
    source["stores"][0]["deploymentTarget"] = json!("kubernetes");

    let issues = lenso_service::validate_autonomous_service_contract_value(&source);
    assert_eq!(issues.len(), 3);
    assert!(issues.iter().all(|issue| {
        issue.code == AutonomousServiceIssueCode::UnknownField && !issue.next_action.is_empty()
    }));

    let error = check_contract_artifact_value(&source).unwrap_err();
    assert_eq!(
        serde_json::to_value(error).unwrap()["code"],
        "unknown_field"
    );
}

#[test]
fn autonomous_service_v2_check_uses_specific_validation_code_and_sorted_output() {
    let mut source: serde_json::Value =
        serde_json::from_str(AUTONOMOUS_SERVICE_V2_FIXTURE_JSON).unwrap();
    source["workloads"].as_array_mut().unwrap().reverse();
    let check = check_contract_artifact_value(&source).unwrap();
    assert_eq!(
        check.autonomous_service.unwrap().workloads,
        [
            "support-api",
            "support-indexer",
            "support-migrate",
            "support-worker"
        ]
    );

    source["tenancyMode"] = json!("sometimes");
    let error = check_contract_artifact_value(&source).unwrap_err();
    let output = serde_json::to_value(error).unwrap();
    assert_eq!(output["code"], "invalid_tenancy_mode");
    assert_eq!(output["path"], "$.tenancyMode");
    assert_eq!(output["nextAction"], "Choose one supported Tenancy Mode.");
}

#[test]
fn autonomous_service_v2_check_rejects_empty_optional_version() {
    let mut source: serde_json::Value =
        serde_json::from_str(AUTONOMOUS_SERVICE_V2_FIXTURE_JSON).unwrap();
    source["version"] = json!("");

    let error = check_contract_artifact_value(&source).unwrap_err();
    assert_eq!(
        serde_json::to_value(error).unwrap(),
        json!({
            "code": "invalid_version",
            "path": "$.version",
            "message": "version must be a non-empty string when present",
            "nextAction": "Set a non-empty Service version or remove the optional field."
        })
    );
}

#[test]
fn service_contract_serializes_provider_and_modules() {
    let contract = ServiceContract::new(
        "support-suite-provider",
        vec![
            ModuleManifest::builder("support-ticket")
                .capabilities(vec!["support_ticket.tickets.read".to_owned()])
                .build(),
        ],
    )
    .version("0.2.0")
    .provider(ServiceProvider {
        name: "support-suite-provider".to_owned(),
        vendor: Some("Lenso".to_owned()),
        summary: Some("Support workflow provider".to_owned()),
        homepage: None,
    })
    .compatibility(ServiceCompatibility {
        remote_protocol_version: Some("1".to_owned()),
        required_host_features: vec!["service.status".to_owned()],
        sdk_language: Some("rust".to_owned()),
        sdk_version: Some("0.1.0".to_owned()),
    })
    .health(ServiceHealth {
        ready_url: Some("http://127.0.0.1:4110/lenso/service/v1/ready".to_owned()),
        status_url: Some("http://127.0.0.1:4110/lenso/service/v1/status".to_owned()),
        ..ServiceHealth::default()
    })
    .local_process(ServiceLocalProcess {
        command: "cargo run".to_owned(),
        cwd: None,
        env: Default::default(),
        auto_start: true,
        ready_timeout_ms: 30_000,
    });

    let value = serde_json::to_value(contract).unwrap();

    assert_eq!(value["protocol"], "lenso.service.v1");
    assert_eq!(value["name"], "support-suite-provider");
    assert_eq!(value["version"], "0.2.0");
    assert_eq!(value["provider"]["vendor"], "Lenso");
    assert_eq!(value["compatibility"]["remoteProtocolVersion"], "1");
    assert_eq!(
        value["health"]["readyUrl"],
        "http://127.0.0.1:4110/lenso/service/v1/ready"
    );
    assert_eq!(
        value["health"]["statusUrl"],
        "http://127.0.0.1:4110/lenso/service/v1/status"
    );
    assert_eq!(value["modules"][0]["name"], "support-ticket");

    let provider = value["provider"].as_object().unwrap();
    let health = value["health"].as_object().unwrap();
    assert!(!provider.contains_key("homepage"));
    assert!(!health.contains_key("manifestUrl"));
    assert!(!health.contains_key("livenessUrl"));
    assert!(validate_service_contract_value(&value).is_empty());
}

#[test]
fn service_contract_schema_is_packaged_with_the_sdk() {
    let schema: serde_json::Value = serde_json::from_str(SERVICE_CONTRACT_SCHEMA_JSON).unwrap();

    assert_eq!(schema["title"], "LensoServiceContract");
    assert_eq!(schema["required"], json!(["name", "modules"]));
}

#[test]
fn protocol_less_legacy_service_contract_remains_compatible() {
    let mut source: serde_json::Value =
        serde_json::from_str(LEGACY_CONTRACT_FIXTURES[0].json).unwrap();
    source.as_object_mut().unwrap().remove("protocol");
    let original = source.clone();

    assert!(validate_service_contract_value(&source).is_empty());
    let contract: ServiceContract = serde_json::from_value(source.clone()).unwrap();

    assert_eq!(contract.protocol, "lenso.service.v1");
    assert_eq!(source, original);
}

#[test]
fn module_release_schema_is_packaged_with_the_sdk() {
    let schema: serde_json::Value = serde_json::from_str(MODULE_RELEASE_SCHEMA_JSON).unwrap();

    assert_eq!(schema["title"], "LensoModuleRelease");
    assert_eq!(
        schema["required"],
        json!(["protocol", "name", "version", "source"])
    );
}

#[test]
fn module_contract_schema_is_packaged_with_the_sdk() {
    let schema: serde_json::Value = serde_json::from_str(MODULE_CONTRACT_SCHEMA_JSON).unwrap();

    assert_eq!(schema["title"], "LensoModuleContract");
    assert_eq!(
        schema["required"],
        json!(["protocol", "name", "version", "source"])
    );
}

#[test]
fn service_workspace_schema_is_packaged_with_the_sdk() {
    let schema: serde_json::Value = serde_json::from_str(SERVICE_WORKSPACE_SCHEMA_JSON).unwrap();

    assert_eq!(schema["title"], "LensoServiceWorkspace");
    assert_eq!(schema["required"], json!(["protocol"]));
}

#[test]
fn service_system_schema_is_packaged_with_the_sdk() {
    let schema: serde_json::Value = serde_json::from_str(SERVICE_SYSTEM_SCHEMA_JSON).unwrap();

    assert_eq!(schema["title"], "LensoServiceSystem");
    assert_eq!(schema["required"], json!(["protocol", "name"]));
}

#[test]
fn service_workspace_serializes_local_services() {
    let workspace = ServiceWorkspace::new(vec![ServiceWorkspaceService {
        auto_start: true,
        command: "pnpm start".to_owned(),
        cwd: "services/support-suite-provider".to_owned(),
        lang: "ts".to_owned(),
        manifest: "lenso.service.json".to_owned(),
        modules: vec!["support-ticket".to_owned()],
        name: "support-suite-provider".to_owned(),
        ready_timeout_ms: 10_000,
        ready_url: "http://127.0.0.1:4110/lenso/service/v1/status".to_owned(),
    }]);
    let value = serde_json::to_value(workspace).unwrap();

    assert_eq!(value["protocol"], "lenso.service-workspace.v1");
    assert_eq!(value["services"][0]["name"], "support-suite-provider");
    assert_eq!(
        value["services"][0]["readyUrl"],
        "http://127.0.0.1:4110/lenso/service/v1/status"
    );
    assert!(validate_service_workspace_value(&value).is_empty());
}

#[test]
fn service_system_serializes_services_modules_and_dependencies() {
    let mut system = ServiceSystem::new("support-platform");
    system.environments = vec!["local".to_owned(), "staging".to_owned(), "prod".to_owned()];
    system.services = vec![
        ServiceSystemService {
            cwd: Some("services/support".to_owned()),
            manifest: Some("lenso.service.json".to_owned()),
            modules: vec!["support-ticket".to_owned()],
            name: "support".to_owned(),
            target: "local".to_owned(),
        },
        ServiceSystemService {
            cwd: None,
            manifest: None,
            modules: vec!["invoice".to_owned()],
            name: "billing".to_owned(),
            target: "kubernetes".to_owned(),
        },
    ];
    system.modules = vec![
        ServiceSystemModule {
            capabilities: vec!["support.ticket.read".to_owned()],
            dependencies: vec!["billing.invoice.read".to_owned()],
            install_to: Some("service:support".to_owned()),
            name: "support-ticket".to_owned(),
        },
        ServiceSystemModule {
            capabilities: vec!["billing.invoice.read".to_owned()],
            dependencies: Vec::new(),
            install_to: Some("service:billing".to_owned()),
            name: "invoice".to_owned(),
        },
    ];
    system.dependencies = vec![ServiceSystemDependency {
        capability: "billing.invoice.read".to_owned(),
        from: "support".to_owned(),
        to: Some("billing".to_owned()),
    }];

    let value = serde_json::to_value(&system).unwrap();
    assert_eq!(value["protocol"], "lenso.system.v1");
    assert_eq!(value["services"][0]["modules"][0], "support-ticket");
    assert!(validate_service_system_value(&value).is_empty());

    let graph = service_system_graph(&system);
    assert_eq!(graph.name, "support-platform");
    assert_eq!(graph.modules[0].owner, "support");
    assert_eq!(graph.dependencies[0].state, "resolved");
    assert!(graph.issues.is_empty());
}

#[test]
fn service_system_graph_reports_unresolved_dependencies() {
    let mut system = ServiceSystem::new("support-platform");
    system.services = vec![ServiceSystemService {
        cwd: None,
        manifest: None,
        modules: vec!["support-ticket".to_owned()],
        name: "support".to_owned(),
        target: "local".to_owned(),
    }];
    system.modules = vec![ServiceSystemModule {
        capabilities: Vec::new(),
        dependencies: vec!["billing.invoice.read".to_owned()],
        install_to: Some("service:support".to_owned()),
        name: "support-ticket".to_owned(),
    }];

    let graph = service_system_graph(&system);

    assert_eq!(graph.dependencies[0].state, "unresolved");
    assert_eq!(graph.issues[0].code, "dependency_unresolved");
}

#[test]
fn service_system_graph_checks_explicit_target_capabilities() {
    let mut system = ServiceSystem::new("support-platform");
    system.services = vec![ServiceSystemService {
        cwd: None,
        manifest: None,
        modules: vec!["billing".to_owned()],
        name: "billing-service".to_owned(),
        target: "external".to_owned(),
    }];
    system.modules = vec![ServiceSystemModule {
        capabilities: vec!["billing.invoice.read".to_owned()],
        dependencies: Vec::new(),
        install_to: Some("service:billing-service".to_owned()),
        name: "billing".to_owned(),
    }];
    system.dependencies = vec![ServiceSystemDependency {
        capability: "billing.invoice.write".to_owned(),
        from: "support-service".to_owned(),
        to: Some("billing-service".to_owned()),
    }];

    let graph = service_system_graph(&system);

    assert_eq!(graph.dependencies[0].state, "missing_capability");
    assert_eq!(graph.issues[0].code, "dependency_missing_capability");
}

#[test]
fn module_contract_serializes_standalone_module_shape() {
    let contract = ModuleContract::new("support-ticket", "0.2.0", "linked").manifest(
        ModuleManifest::builder("support-ticket")
            .capabilities(vec!["support_ticket.tickets.read".to_owned()])
            .build(),
    );
    let value = serde_json::to_value(contract).unwrap();

    assert_eq!(value["protocol"], "lenso.module.v1");
    assert_eq!(value["source"], "linked");
    assert_eq!(value["manifest"]["name"], "support-ticket");
    assert!(validate_module_contract_value(&value).is_empty());
}

#[test]
fn service_contract_validation_reports_paths() {
    let issues = validate_service_contract_value(&json!({
        "name": "",
        "install": {
            "services": [
                { "name": "support-suite-provider" }
            ]
        },
        "modules": [
            {
                "name": "support-ticket",
                "capabilities": ["support_ticket.read", 42]
            },
            {
                "name": "support-ticket"
            }
        ]
    }));

    let paths = issues
        .iter()
        .map(|issue| issue.path.as_str())
        .collect::<Vec<_>>();
    assert!(paths.contains(&"$.name"));
    assert!(paths.contains(&"$.install.services[0].command"));
    assert!(paths.contains(&"$.modules[0].capabilities[1]"));
    assert!(paths.contains(&"$.modules[1].name"));
}

#[test]
fn service_system_validation_reports_paths() {
    let issues = validate_service_system_value(&json!({
        "protocol": "lenso.system.v1",
        "name": "",
        "services": [
            { "name": "support", "target": "local", "modules": ["support-ticket"] },
            { "name": "support", "target": "" }
        ],
        "modules": [
            { "name": "support-ticket", "installTo": "service:support", "dependencies": [42] }
        ],
        "dependencies": [
            { "from": "support" }
        ]
    }));
    let paths = issues
        .iter()
        .map(|issue| issue.path.as_str())
        .collect::<Vec<_>>();

    assert!(paths.contains(&"$.name"));
    assert!(paths.contains(&"$.services[1].name"));
    assert!(paths.contains(&"$.services[1].target"));
    assert!(paths.contains(&"$.modules[0].dependencies[0]"));
    assert!(paths.contains(&"$.dependencies[0].capability"));
}

#[test]
fn legacy_contract_fixture_matrix_normalizes_to_provider_semantics() {
    assert_eq!(LEGACY_CONTRACT_FIXTURES.len(), 2);

    for fixture in LEGACY_CONTRACT_FIXTURES {
        let source: serde_json::Value = serde_json::from_str(fixture.json).unwrap();
        let original = source.clone();
        let check = check_contract_artifact_value(&source).unwrap();
        let provider_semantics = check.provider_semantics.as_ref().unwrap();

        assert_eq!(check.detected_protocol, fixture.protocol);
        assert_eq!(check.semantic_kind, fixture.semantic_kind);
        assert_eq!(provider_semantics.auth_owner, ContractOwner::Host);
        assert_eq!(provider_semantics.proxy_policy_owner, ContractOwner::Host);
        assert_eq!(provider_semantics.retry_owner, ContractOwner::Host);
        assert_eq!(provider_semantics.runtime_queue_owner, ContractOwner::Host);
        assert_eq!(provider_semantics.outbox_owner, ContractOwner::Host);
        assert_eq!(provider_semantics.story_owner, ContractOwner::Host);
        assert_eq!(
            source, original,
            "normalization must not rewrite the source"
        );
    }

    let service = check_contract_artifact_value(
        &serde_json::from_str(LEGACY_CONTRACT_FIXTURES[0].json).unwrap(),
    )
    .unwrap();
    assert_eq!(service.artifact_kind, ContractArtifactKind::Service);
    assert_eq!(service.semantic_kind, ContractSemanticKind::Provider);
    assert_eq!(
        service.provider_semantics.unwrap().providers,
        ["support-suite-provider"]
    );

    let system = check_contract_artifact_value(
        &serde_json::from_str(LEGACY_CONTRACT_FIXTURES[1].json).unwrap(),
    )
    .unwrap();
    assert_eq!(system.artifact_kind, ContractArtifactKind::System);
    assert_eq!(system.semantic_kind, ContractSemanticKind::ProviderSystem);
    assert_eq!(
        system.provider_semantics.unwrap().providers,
        ["support-suite-provider"]
    );
}

#[test]
fn contract_artifact_check_is_machine_readable() {
    let source: serde_json::Value = serde_json::from_str(LEGACY_CONTRACT_FIXTURES[0].json).unwrap();
    let value = serde_json::to_value(check_contract_artifact_value(&source).unwrap()).unwrap();

    assert_eq!(value["detectedProtocol"], "lenso.service.v1");
    assert_eq!(value["artifactKind"], "service");
    assert_eq!(value["semanticKind"], "provider");
    assert_eq!(value["providerSemantics"]["authOwner"], "host");
    assert_eq!(value["providerSemantics"]["proxyPolicyOwner"], "host");
    assert_eq!(value["providerSemantics"]["retryOwner"], "host");
    assert_eq!(value["providerSemantics"]["runtimeQueueOwner"], "host");
    assert_eq!(value["providerSemantics"]["outboxOwner"], "host");
    assert_eq!(value["providerSemantics"]["storyOwner"], "host");
}

#[test]
fn contract_artifact_check_rejects_ambiguous_protocols_with_next_action() {
    let error = check_contract_artifact_value(&json!({
        "name": "support-suite-provider",
        "modules": []
    }))
    .unwrap_err();

    assert_eq!(
        error.code,
        ContractArtifactCheckErrorCode::AmbiguousProtocol
    );
    assert_eq!(error.path, "$.protocol");
    assert_eq!(
        error.next_action,
        "Set `protocol` to a supported Provider-era protocol or `lenso.service.v2`."
    );
    assert_eq!(
        serde_json::to_value(&error).unwrap()["code"],
        "ambiguous_protocol"
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&error.to_string()).unwrap()["code"],
        "ambiguous_protocol"
    );
}

#[test]
fn contract_artifact_check_rejects_unsupported_protocols_with_next_action() {
    let error = check_contract_artifact_value(&json!({
        "protocol": "lenso.service.v99",
        "name": "future-service",
        "modules": []
    }))
    .unwrap_err();

    assert_eq!(
        error.code,
        ContractArtifactCheckErrorCode::UnsupportedProtocol
    );
    assert_eq!(error.path, "$.protocol");
    assert_eq!(
        error.next_action,
        "Use a supported protocol or upgrade Lenso for this artifact version."
    );
    assert_eq!(
        serde_json::to_value(&error).unwrap()["code"],
        "unsupported_protocol"
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&error.to_string()).unwrap()["code"],
        "unsupported_protocol"
    );
}
