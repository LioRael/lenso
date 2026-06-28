use axum::{Json, Router, routing::get};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub use lenso_contracts::ModuleManifest;

pub const SERVICE_CONTRACT_PROTOCOL: &str = "lenso.service.v1";
pub const SERVICE_PACKAGE_PROTOCOL: &str = "lenso.service-package.v1";
pub const MODULE_RELEASE_PROTOCOL: &str = "lenso.module-release.v1";
pub const SERVICE_CONTRACT_SCHEMA_JSON: &str =
    include_str!("../schemas/lenso-service.v1.schema.json");
pub const SERVICE_PACKAGE_SCHEMA_JSON: &str =
    include_str!("../schemas/lenso-service-package.v1.schema.json");
pub const MODULE_RELEASE_SCHEMA_JSON: &str =
    include_str!("../schemas/lenso-module-release.v1.schema.json");

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
pub struct ServiceContract {
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
    pub provider: ModuleReleaseProvider,
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
            provider: ModuleReleaseProvider {
                name: provider_name.into(),
                service_package: Some("lenso.service-package.json".to_owned()),
                service_manifest: None,
            },
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
        self.provider.service_package = None;
        self.provider.service_manifest = Some(service_manifest.into());
        self
    }
}

impl ServiceContract {
    #[must_use]
    pub fn new(name: impl Into<String>, modules: Vec<ModuleManifest>) -> Self {
        Self {
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

#[must_use]
pub fn validate_service_contract_value(value: &Value) -> Vec<ServiceContractIssue> {
    let Some(object) = value.as_object() else {
        return vec![ServiceContractIssue::new(
            "$",
            "service contract must be an object",
        )];
    };

    let mut issues = Vec::new();
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
    match object.get("source").and_then(Value::as_str) {
        Some("service") => {}
        _ => issues.push(ServiceContractIssue::new(
            "$.source",
            "source must be `service`",
        )),
    }
    validate_module_release_provider(object.get("provider"), &mut issues);
    validate_string_array(object.get("capabilities"), "$.capabilities", &mut issues);
    validate_string_array(object.get("dependencies"), "$.dependencies", &mut issues);
    issues
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

const fn default_service_auto_start() -> bool {
    true
}

const fn default_service_ready_timeout_ms() -> u64 {
    30_000
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
                "$.provider.name",
                "$.provider.servicePackage",
                "$.capabilities[1]"
            ]
        );
    }
}
