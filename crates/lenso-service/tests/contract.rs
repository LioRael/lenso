use lenso_service::{
    MODULE_CONTRACT_SCHEMA_JSON, MODULE_RELEASE_SCHEMA_JSON, ModuleContract, ModuleManifest,
    SERVICE_CONTRACT_SCHEMA_JSON, SERVICE_WORKSPACE_SCHEMA_JSON, ServiceCompatibility,
    ServiceContract, ServiceHealth, ServiceLocalProcess, ServiceProvider, ServiceWorkspace,
    ServiceWorkspaceService, validate_module_contract_value, validate_service_contract_value,
    validate_service_workspace_value,
};
use serde_json::json;

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
