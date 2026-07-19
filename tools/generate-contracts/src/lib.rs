use anyhow::Context as _;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

pub fn generate_contracts() -> anyhow::Result<()> {
    write_yaml(
        "contracts/openapi/app-api.v1.yaml",
        &lenso_api::openapi_document(),
    )?;
    write_yaml(
        "contracts/openapi/autonomous-service-runtime.v1.yaml",
        &generated_autonomous_service_runtime_openapi(),
    )?;
    write_json(
        "contracts/workflows/lenso.workflow-definition.v1.schema.json",
        &generated_workflow_definition_schema(),
    )?;
    write_json(
        "contracts/workflows/lenso.workflow-compatibility.v1.json",
        &generated_workflow_compatibility_artifact(),
    )?;
    write_json(
        "contracts/errors/error-response.v1.schema.json",
        &error_response_schema(),
    )?;
    write_json(
        "contracts/services/lenso-service.v2.schema.json",
        &generated_autonomous_service_schema(),
    )?;
    write_json(
        "contracts/services/support-http.v1.bindings.json",
        &generated_direct_http_bindings(),
    )?;
    write_text(
        "contracts/services/support-grpc.v1.proto",
        lenso_service::DIRECT_GRPC_PROTO_V1_FIXTURE,
    )?;
    write_json(
        "contracts/services/support-grpc.v1.bindings.json",
        &generated_direct_grpc_bindings(),
    )?;
    write_json(
        "contracts/services/lenso-system.v2.schema.json",
        &generated_system_v2_schema(),
    )?;
    write_json(
        "contracts/services/lenso-system.v2.fixture.json",
        &generated_system_v2_fixture(),
    )?;
    write_json(
        "contracts/extraction/lenso.extraction-readiness-report.v1.schema.json",
        &generated_extraction_readiness_schema(),
    )?;
    write_json(
        "contracts/extraction/support-ticket.blocked.json",
        &generated_support_ticket_extraction_readiness_blocked(),
    )?;
    write_text(
        "contracts/extraction/support-ticket.blocked.txt",
        &generated_support_ticket_extraction_readiness_blocked_human(),
    )?;
    write_json(
        "contracts/extraction/support-ticket.corrected.json",
        &generated_support_ticket_extraction_readiness_corrected(),
    )?;
    write_text(
        "contracts/extraction/support-ticket.corrected.txt",
        &generated_support_ticket_extraction_readiness_corrected_human(),
    )?;
    write_json(
        "contracts/context/lenso-context.v1.schema.json",
        &generated_common_context_schema(),
    )?;
    write_json(
        "contracts/context/lenso-context.v1.fixture.json",
        &generated_common_context_fixture(),
    )?;
    write_json(
        "contracts/events/lenso/lenso.event-envelope.v1.schema.json",
        &generated_event_envelope_schema(),
    )?;
    write_json(
        "contracts/events/support/support.ticket-opened.v1.schema.json",
        &generated_support_event_schema(),
    )?;
    write_json(
        "contracts/events/support/support.ticket-opened.v1.artifact.json",
        &generated_support_event_contract(),
    )?;
    write_json(
        "contracts/events/support/support.ticket-opened.v1.envelope.json",
        &generated_support_event_envelope(),
    )?;
    write_text(
        "docs/architecture/common-context-contracts.md",
        generated_common_context_glossary(),
    )?;
    write_text(
        "docs/architecture/contract-compatibility.md",
        generated_contract_compatibility(),
    )?;
    write_json(
        "contracts/compatibility/contract-compatibility.v1.json",
        &generated_contract_compatibility_matrix(),
    )?;

    Ok(())
}

pub fn generated_autonomous_service_runtime_openapi() -> utoipa::openapi::OpenApi {
    lenso_autonomous_service::openapi_document()
}

pub fn generated_workflow_definition_schema() -> Value {
    lenso_contracts::workflow_definition_schema()
}

pub fn generated_workflow_compatibility_artifact() -> Value {
    lenso_contracts::workflow_compatibility_artifact()
}

pub fn generated_error_response_schema() -> Value {
    error_response_schema()
}

pub fn generated_autonomous_service_schema() -> Value {
    serde_json::from_str(lenso_service::SERVICE_V2_CONTRACT_SCHEMA_JSON)
        .expect("packaged Autonomous Service schema must be valid JSON")
}

pub fn generated_direct_http_bindings() -> Value {
    let openapi: Value = serde_yaml::from_str(lenso_service::DIRECT_HTTP_OPENAPI_V1_FIXTURE_YAML)
        .expect("packaged direct HTTP OpenAPI fixture must be valid YAML");
    serde_json::to_value(
        lenso_service::generate_direct_http_bindings("support-http", "v1", &openapi)
            .expect("packaged direct HTTP OpenAPI fixture must generate bindings"),
    )
    .expect("direct HTTP bindings must serialize")
}

pub fn generated_direct_grpc_bindings() -> Value {
    serde_json::to_value(
        lenso_service::generate_direct_grpc_bindings(
            "support-grpc",
            "v1",
            lenso_service::DIRECT_GRPC_PROTO_V1_FIXTURE,
            lenso_service::DIRECT_GRPC_DESCRIPTOR_V1,
        )
        .expect("packaged direct gRPC Protobuf fixture must generate bindings"),
    )
    .expect("direct gRPC bindings must serialize")
}

pub fn generated_direct_grpc_proto() -> &'static str {
    lenso_service::DIRECT_GRPC_PROTO_V1_FIXTURE
}

pub fn generated_system_v2_schema() -> Value {
    serde_json::from_str(lenso_service::SYSTEM_V2_CONTRACT_SCHEMA_JSON)
        .expect("packaged System v2 schema must be valid JSON")
}

pub fn generated_system_v2_fixture() -> Value {
    serde_json::from_str(lenso_service::MIXED_SYSTEM_V2_FIXTURE_JSON)
        .expect("packaged mixed System v2 fixture must be valid JSON")
}

pub fn generated_extraction_readiness_schema() -> Value {
    lenso_service::extraction_readiness_report_schema()
}

pub fn generated_support_ticket_extraction_readiness_blocked() -> Value {
    serde_json::to_value(support_ticket_extraction_readiness_report(true))
        .expect("blocked support-ticket Extraction Readiness Report must serialize")
}

pub fn generated_support_ticket_extraction_readiness_blocked_human() -> String {
    lenso_service::render_extraction_readiness_report(&support_ticket_extraction_readiness_report(
        true,
    ))
}

pub fn generated_support_ticket_extraction_readiness_corrected() -> Value {
    serde_json::to_value(support_ticket_extraction_readiness_report(false))
        .expect("corrected support-ticket Extraction Readiness Report must serialize")
}

pub fn generated_support_ticket_extraction_readiness_corrected_human() -> String {
    lenso_service::render_extraction_readiness_report(&support_ticket_extraction_readiness_report(
        false,
    ))
}

fn support_ticket_extraction_readiness_report(
    blocked: bool,
) -> lenso_service::ExtractionReadinessReport {
    use lenso_contracts::{
        AdminSchema, ConsoleArea, ConsolePackage, ConsoleSurface, EntitySchema,
        EventHandlerDeclaration, EventSurface, FieldSchema, FieldType, ModuleHttpMethod,
        ModuleHttpRoute, ModuleManifest, RuntimeFunctionDeclaration, RuntimeSurface,
        ScheduledFunctionDeclaration, StoryDisplayDescriptor, StoryDisplaySource,
        WorkflowDataContract, WorkflowDefinition, WorkflowStepDeclaration,
    };
    use lenso_service::{
        CompatibilityCategory, ExtractionBoundaryEvidence, ExtractionBoundaryReference,
        ExtractionBoundaryReferenceKind, ExtractionConsumerCompatibilityEvidence,
        ExtractionContractDirection, ExtractionContractEvidence, ExtractionContractKind,
        ExtractionCursorEvidence, ExtractionDataAccessEvidence, ExtractionDataAccessKind,
        ExtractionDataEvidenceSource, ExtractionDataTableEvidence, ExtractionDataVolumeEvidence,
        ExtractionEvidenceStatus, ExtractionMigrationEvidence, ExtractionReadinessEvidence,
        ExtractionServiceDataEvidence, ExtractionTransactionEvidence,
    };

    let module = ModuleManifest::builder("support-ticket")
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
        .build();
    let system = json!({
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
    });
    let mut evidence = ExtractionReadinessEvidence {
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
                evidence_references: vec!["contracts/openapi/support-ticket.v1.yaml".to_owned()],
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
        service_data: Some(ExtractionServiceDataEvidence {
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
        }),
    };
    if blocked {
        evidence
            .boundary
            .as_mut()
            .expect("boundary fixture")
            .references = vec![
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
        ];
        let contracts = evidence.contracts.as_mut().expect("contract fixture");
        contracts[0].status = ExtractionEvidenceStatus::Missing;
        contracts[0].contract_id = None;
        contracts[2].status = ExtractionEvidenceStatus::Missing;
        contracts[2].contract_id = None;
        let consumer = &mut evidence
            .active_consumers
            .as_mut()
            .expect("consumer fixture")[0];
        consumer.classification = CompatibilityCategory::Breaking;
        consumer.next_action = "Migrate the Consumer to support.sla-updated.v1.".to_owned();
        let service_data = evidence
            .service_data
            .as_mut()
            .expect("service data fixture");
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
    }
    lenso_service::evaluate_extraction_readiness(&module, &system, &evidence)
}

pub fn generated_common_context_schema() -> Value {
    serde_json::from_str(lenso_service::COMMON_CONTEXT_V1_SCHEMA_JSON)
        .expect("packaged common context schema must be valid JSON")
}

pub fn generated_common_context_fixture() -> Value {
    serde_json::from_str(lenso_service::COMMON_CONTEXT_V1_FIXTURE_JSON)
        .expect("packaged common context fixture must be valid JSON")
}

pub fn generated_event_envelope_schema() -> Value {
    serde_json::from_str(lenso_service::EVENT_ENVELOPE_V1_SCHEMA_JSON)
        .expect("packaged Event Envelope schema must be valid JSON")
}

pub fn generated_support_event_contract() -> Value {
    let service: lenso_service::AutonomousServiceContract =
        serde_json::from_str(lenso_service::AUTONOMOUS_SERVICE_V2_FIXTURE_JSON)
            .expect("packaged Autonomous Service fixture must be valid");
    let payload_schema = generated_support_event_schema();
    serde_json::to_value(
        lenso_service::generate_event_contract(
            &service,
            &service.event_contracts[0],
            &payload_schema,
        )
        .expect("support Event Contract must generate"),
    )
    .expect("generated support Event Contract must serialize")
}

pub fn generated_support_event_schema() -> Value {
    serde_json::from_str(lenso_service::SUPPORT_EVENT_SCHEMA_JSON)
        .expect("packaged support Event schema must be valid JSON")
}

pub fn generated_support_event_envelope() -> Value {
    let contract: lenso_service::GeneratedEventContract =
        serde_json::from_value(generated_support_event_contract())
            .expect("generated support Event Contract must deserialize");
    let context: lenso_service::CommonContextContract =
        serde_json::from_str(lenso_service::COMMON_CONTEXT_V1_FIXTURE_JSON)
            .expect("packaged common context fixture must be valid");
    let envelope = lenso_service::EventEnvelope::new(
        &contract,
        "event_support_ticket_01",
        "2026-07-14T10:15:30Z",
        context,
        json!({
            "ticketId": "ticket_01",
            "openedAt": "2026-07-14T10:15:00Z"
        }),
    );
    assert!(lenso_service::validate_event_envelope(&contract, &envelope).is_empty());
    serde_json::to_value(envelope).expect("generated support Event Envelope must serialize")
}

pub fn generated_common_context_glossary() -> &'static str {
    lenso_service::COMMON_CONTEXT_GLOSSARY_MARKDOWN
}

pub fn generated_contract_compatibility() -> &'static str {
    lenso_service::CONTRACT_COMPATIBILITY_MARKDOWN
}

pub fn generated_contract_compatibility_matrix() -> Value {
    let mut rows = Vec::new();
    for (kind, fixtures, evaluate) in [
        (
            "event_contract",
            lenso_service::EVENT_COMPATIBILITY_FIXTURES,
            lenso_service::evaluate_event_compatibility
                as fn(&Value) -> lenso_service::ContractCompatibilityResult,
        ),
        (
            "config_contract",
            lenso_service::CONFIG_COMPATIBILITY_FIXTURES,
            lenso_service::evaluate_config_compatibility
                as fn(&Value) -> lenso_service::ContractCompatibilityResult,
        ),
        (
            "reliability_contract",
            lenso_service::RELIABILITY_COMPATIBILITY_FIXTURES,
            lenso_service::evaluate_reliability_compatibility
                as fn(&Value) -> lenso_service::ContractCompatibilityResult,
        ),
    ] {
        for fixture in fixtures {
            let input: Value = serde_json::from_str(fixture.json)
                .expect("compatibility fixture must be valid JSON");
            rows.push(
                json!({"contractKind": kind, "name": fixture.name, "result": evaluate(&input)}),
            );
        }
    }
    let before: lenso_service::GeneratedEventContract =
        serde_json::from_value(generated_support_event_contract())
            .expect("generated support Event Contract must deserialize");
    for (name, candidate) in generated_support_event_compatibility_candidates(&before) {
        rows.push(json!({
            "contractKind": "generated_event_contract",
            "name": name,
            "result": lenso_service::evaluate_generated_event_contract_compatibility(
                &before,
                &candidate,
            )
        }));
    }
    Value::Array(rows)
}

fn generated_support_event_compatibility_candidates(
    before: &lenso_service::GeneratedEventContract,
) -> Vec<(&'static str, lenso_service::GeneratedEventContract)> {
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
        .push(lenso_service::CommonContextRequirement::Deadline);
    let mut blocked = safe.clone();
    blocked.protocol = "lenso.event-contract.v2".to_owned();
    vec![
        ("safe", safe),
        ("needs_attention", needs_attention),
        ("breaking", breaking),
        ("blocked", blocked),
    ]
}

fn write_yaml(path: impl AsRef<Path>, value: &impl serde::Serialize) -> anyhow::Result<()> {
    let path = path.as_ref();
    ensure_parent(path)?;
    let rendered = serde_yaml::to_string(value).context("contract should serialize as yaml")?;
    fs::write(path, rendered).with_context(|| format!("failed to write {}", path.display()))
}

fn write_json(path: impl AsRef<Path>, value: &Value) -> anyhow::Result<()> {
    let path = path.as_ref();
    ensure_parent(path)?;
    let rendered = format!(
        "{}\n",
        serde_json::to_string_pretty(value).context("contract should serialize as json")?
    );
    fs::write(path, rendered).with_context(|| format!("failed to write {}", path.display()))
}

fn write_text(path: impl AsRef<Path>, value: &str) -> anyhow::Result<()> {
    let path = path.as_ref();
    ensure_parent(path)?;
    fs::write(path, value).with_context(|| format!("failed to write {}", path.display()))
}

fn ensure_parent(path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    Ok(())
}

fn error_response_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://contracts.lenso.local/errors/error-response.v1.schema.json",
        "title": "ErrorResponse",
        "type": "object",
        "required": [
            "type",
            "title",
            "status",
            "detail",
            "code",
            "request_id",
            "correlation_id",
            "errors"
        ],
        "properties": {
            "type": {
                "type": "string",
                "format": "uri-reference"
            },
            "title": { "type": "string" },
            "status": {
                "type": "integer",
                "minimum": 100,
                "maximum": 599
            },
            "detail": { "type": "string" },
            "code": { "type": "string" },
            "request_id": { "type": ["string", "null"] },
            "correlation_id": { "type": ["string", "null"] },
            "errors": {
                "type": "array",
                "items": { "$ref": "#/$defs/ProblemErrorDetail" }
            },
            "next_actions": {
                "type": ["array", "null"],
                "items": { "type": "string" }
            }
        },
        "$defs": {
            "ProblemErrorDetail": {
                "type": "object",
                "required": ["field", "reason"],
                "properties": {
                    "field": { "type": ["string", "null"] },
                    "reason": { "type": "string" }
                },
                "additionalProperties": false
            }
        },
        "additionalProperties": false
    })
}
