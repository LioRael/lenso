use lenso_service::{
    AUTONOMOUS_SERVICE_V2_FIXTURE_JSON, AutonomousServiceContract, AutonomousServiceIssueCode,
    AutonomousServiceStore, AutonomousServiceWorkload, ContractArtifactCheckErrorCode,
    ContractArtifactKind, ContractOwner, ContractSemanticKind, LEGACY_CONTRACT_FIXTURES,
    MODULE_CONTRACT_SCHEMA_JSON, MODULE_RELEASE_SCHEMA_JSON, ModuleContract, ModuleManifest,
    SERVICE_CONTRACT_SCHEMA_JSON, SERVICE_SYSTEM_SCHEMA_JSON, SERVICE_V2_CONTRACT_SCHEMA_JSON,
    SERVICE_WORKSPACE_SCHEMA_JSON, ServiceCompatibility, ServiceContract, ServiceHealth,
    ServiceLocalProcess, ServiceProvider, ServiceSystem, ServiceSystemDependency,
    ServiceSystemModule, ServiceSystemService, ServiceTenancyMode, ServiceWorkspace,
    ServiceWorkspaceService, WorkloadRole, check_contract_artifact_value, service_system_graph,
    validate_module_contract_value, validate_service_contract_value, validate_service_system_value,
    validate_service_workspace_value,
};
use serde_json::json;

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
    assert_eq!(serde_json::to_value(&contract).unwrap(), source);
    assert!(lenso_service::validate_autonomous_service_contract(&contract).is_empty());
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
    assert!(check.autonomous_service.is_some());
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
        "Set `protocol` to a supported Provider-era artifact protocol."
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
