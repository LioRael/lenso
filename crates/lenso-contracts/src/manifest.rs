//! A module's pure-data contract: serializable metadata describable without
//! behavior. Owned + serde so every loading source produces the same shape.

use crate::StoryDisplayDescriptor;
use crate::admin::{
    AdminDeclarativeComponent, AdminDeclarativeSurface, AdminEmbeddedEntry, AdminEmbeddedRuntime,
    AdminEmbeddedSurface, AdminPermission, AdminSurface,
};
use crate::admin_schema::AdminSchema;
use crate::console::{
    ConsoleActionInputValue, ConsoleContribution, ConsoleContributionAction, ConsoleSlot,
    ConsoleSurface,
};
use crate::events::{EventHandlerDeclaration, EventSurface};
use crate::http::{ModuleHttpMethod, ModuleHttpRoute, lint_module_http_routes};
use crate::lifecycle::{
    LifecycleActivationJobDeclaration, LifecycleStartupCheckDeclaration, LifecycleStartupCheckKind,
    LifecycleSurface,
};
use crate::runtime::{
    RuntimeFunctionDeclaration, RuntimeSurface, ScheduledFunctionDeclaration,
    WORKFLOW_DEFINITION_PROTOCOL, WorkflowDataContract, WorkflowDefinition,
    WorkflowStepDeclaration,
};
use crate::validate_cron_expression;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use utoipa::ToSchema;

pub const MODULE_MANIFEST_PROTOCOL: &str = "lenso.module-manifest.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleRequirement {
    pub module_id: String,
    pub version_requirement: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub optional: bool,
}

impl ModuleRequirement {
    pub fn new(
        module_id: impl Into<String>,
        version_requirement: impl AsRef<str>,
    ) -> Result<Self, String> {
        let version_requirement = semver::VersionReq::parse(version_requirement.as_ref())
            .map_err(|error| format!("invalid Module version requirement: {error}"))?
            .to_string();
        Ok(Self {
            module_id: module_id.into(),
            version_requirement,
            capabilities: Vec::new(),
            optional: false,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModuleConfigFieldType {
    String,
    Integer,
    Boolean,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModuleConfigScope {
    Module,
    Service,
    Environment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModuleConfigMutability {
    Static,
    Reloadable,
    Runtime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModuleConfigActivation {
    None,
    Build,
    Restart,
    ServiceRestart,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleConfigValidation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleConfigField {
    pub key: String,
    pub field_type: ModuleConfigFieldType,
    pub required: bool,
    pub scope: ModuleConfigScope,
    pub sensitive: bool,
    pub secret_reference: bool,
    pub mutability: ModuleConfigMutability,
    pub activation: ModuleConfigActivation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation: Option<ModuleConfigValidation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleConfigContract {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<ModuleConfigField>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModuleMigrationActivation {
    BeforeActivation,
    AfterActivation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModuleMigrationDeclaration {
    pub migration_id: String,
    pub order: u32,
    pub store: String,
    pub destructive: bool,
    pub reversible: bool,
    pub activation: ModuleMigrationActivation,
}

/// The serializable metadata a module exposes. Runtime config is deliberately
/// NOT here — it stays an internal `&'static` field on [`crate::Module`]
/// because the config registry needs the real (non-serde) `RuntimeConfigType`
/// to validate. Only round-trippable fields belong here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct ModuleManifest {
    pub protocol: String,

    /// Stable fully qualified ModuleId, e.g. `"lenso/identity"`.
    pub module_id: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,

    /// Console story-display metadata.
    #[serde(default)]
    pub story_display: Vec<StoryDisplayDescriptor>,

    /// Admin surface: `Some(AdminSurface::Schema(_))` for schema-driven CRUD,
    /// future custom surfaces for richer module admin UI, or `None` for modules
    /// with no admin surface (e.g. notifications).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admin: Option<AdminSurface>,

    /// Declared module-owned HTTP routes. These are metadata only until a
    /// loading-source-specific mount/proxy protocol exists.
    #[serde(default)]
    pub http_routes: Vec<ModuleHttpRoute>,

    /// Declared runtime behavior. These entries are manifest data only; source
    /// bindings decide how to register executable behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<RuntimeSurface>,

    /// Declared event subscriptions. These entries are manifest data only;
    /// source bindings decide how to register executable behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub events: Option<EventSurface>,

    /// Declared lifecycle work. The host validates and schedules these entries;
    /// modules do not receive arbitrary startup callbacks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<LifecycleSurface>,

    /// Declared Console surfaces provided by trusted frontend packages.
    #[serde(default)]
    pub console: Vec<ConsoleSurface>,

    /// Declared Console extension slots owned by host or module surfaces.
    #[serde(default)]
    pub console_slots: Vec<ConsoleSlot>,

    /// Declared Console slot contributions attached to host or module-owned surfaces.
    #[serde(default)]
    pub console_contributions: Vec<ConsoleContribution>,

    /// RESERVED SEAM — capabilities the module declares (perms/tenancy).
    #[serde(default)]
    pub capabilities: Vec<String>,

    /// Other Modules required by this business capability.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<ModuleRequirement>,

    #[serde(default, skip_serializing_if = "ModuleConfigContract::is_empty")]
    pub config: ModuleConfigContract,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub migrations: Vec<ModuleMigrationDeclaration>,
}

impl ModuleManifest {
    /// Start building a manifest for a fully qualified ModuleId.
    #[must_use]
    pub fn builder(module_id: impl Into<String>) -> ModuleManifestBuilder {
        ModuleManifestBuilder {
            manifest: ModuleManifest {
                protocol: MODULE_MANIFEST_PROTOCOL.to_owned(),
                module_id: module_id.into(),
                summary: None,
                story_display: Vec::new(),
                admin: None,
                http_routes: Vec::new(),
                runtime: None,
                events: None,
                lifecycle: None,
                console: Vec::new(),
                console_slots: Vec::new(),
                console_contributions: Vec::new(),
                capabilities: Vec::new(),
                requires: Vec::new(),
                config: ModuleConfigContract::default(),
                migrations: Vec::new(),
            },
        }
    }
}

impl ModuleConfigContract {
    fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ModuleManifestLintSeverity {
    Ok,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, schemars::JsonSchema)]
pub struct ModuleManifestLint {
    pub severity: ModuleManifestLintSeverity,
    pub subject: String,
    pub message: String,
    pub suggestion: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleCapabilityReference {
    pub capability: String,
    pub subject: String,
}

pub fn lint_module_manifest(manifest: &ModuleManifest) -> Vec<ModuleManifestLint> {
    let mut lints = lint_module_manifest_parts(
        &manifest.module_id,
        manifest.admin.as_ref(),
        &manifest.http_routes,
        manifest.runtime.as_ref(),
        manifest.events.as_ref(),
        manifest.lifecycle.as_ref(),
        &manifest.console,
        &manifest.console_slots,
        &manifest.console_contributions,
        &manifest.capabilities,
        &manifest.requires,
    );
    lint_manifest_contract(manifest, &mut lints);
    lints
}

fn lint_manifest_contract(manifest: &ModuleManifest, lints: &mut Vec<ModuleManifestLint>) {
    if manifest.protocol != MODULE_MANIFEST_PROTOCOL {
        push_contract_error(
            lints,
            "module.protocol",
            format!("Protocol must be {MODULE_MANIFEST_PROTOCOL}."),
            "Use the canonical Module Manifest protocol discriminator.",
        );
    }
    if !sorted_unique(&manifest.capabilities) {
        push_contract_error(
            lints,
            "module.capabilities",
            "Capabilities must be non-empty, sorted, and unique.",
            "Sort and deduplicate ModuleManifest.capabilities.",
        );
    }

    let mut requirement_ids = HashSet::new();
    for requirement in &manifest.requires {
        if !requirement_ids.insert(requirement.module_id.as_str()) {
            push_contract_error(
                lints,
                format!("requirement {}", requirement.module_id),
                "Module requirements must have unique ModuleIds.",
                "Merge duplicate requirements into one canonical declaration.",
            );
        }
        if !matches!(
            semver::VersionReq::parse(&requirement.version_requirement),
            Ok(version) if version.to_string() == requirement.version_requirement
        ) {
            push_contract_error(
                lints,
                format!("requirement {}", requirement.module_id),
                "Module version requirement must be normalized SemVer.",
                "Parse and serialize the version requirement before publishing.",
            );
        }
        if !sorted_unique(&requirement.capabilities) {
            push_contract_error(
                lints,
                format!("requirement {} capabilities", requirement.module_id),
                "Required capabilities must be non-empty, sorted, and unique.",
                "Sort and deduplicate the requirement capabilities.",
            );
        }
    }

    let mut config_keys = HashSet::new();
    for field in &manifest.config.fields {
        if field.key.trim().is_empty() || !config_keys.insert(field.key.as_str()) {
            push_contract_error(
                lints,
                format!("config {}", field.key),
                "Config keys must be non-empty and unique.",
                "Give each config field one stable key.",
            );
        }
        let secret_named = field
            .key
            .split(['.', '-', '_'])
            .any(|part| matches!(part, "secret" | "password" | "token" | "credential"));
        if (field.sensitive || field.secret_reference || secret_named) && field.default.is_some() {
            push_contract_error(
                lints,
                format!("config {} default", field.key),
                "Secret-bearing config fields must not embed default values.",
                "Resolve secret values outside the Module contract.",
            );
        }
        if field.sensitive && !field.secret_reference {
            push_contract_error(
                lints,
                format!("config {} secret_reference", field.key),
                "Sensitive config must be declared as a secret reference.",
                "Set secret_reference and keep the value outside the Manifest.",
            );
        }
    }

    let mut migration_ids = HashSet::new();
    let mut migration_orders = HashSet::new();
    for migration in &manifest.migrations {
        if migration.migration_id.trim().is_empty()
            || migration.store.trim().is_empty()
            || !migration_ids.insert(migration.migration_id.as_str())
            || !migration_orders.insert(migration.order)
        {
            push_contract_error(
                lints,
                format!("migration {}", migration.migration_id),
                "Migrations require non-empty identities and stores with unique order values.",
                "Declare one deterministic order for each migration.",
            );
        }
    }
}

fn sorted_unique(values: &[String]) -> bool {
    values.iter().all(|value| !value.trim().is_empty())
        && values.windows(2).all(|pair| pair[0] < pair[1])
}

fn push_contract_error(
    lints: &mut Vec<ModuleManifestLint>,
    subject: impl Into<String>,
    message: impl Into<String>,
    suggestion: impl Into<String>,
) {
    lints.push(ModuleManifestLint {
        severity: ModuleManifestLintSeverity::Error,
        subject: subject.into(),
        message: message.into(),
        suggestion: suggestion.into(),
    });
}

pub fn lint_module_manifest_parts(
    module_id: &str,
    admin: Option<&AdminSurface>,
    http_routes: &[ModuleHttpRoute],
    runtime: Option<&RuntimeSurface>,
    events: Option<&EventSurface>,
    lifecycle: Option<&LifecycleSurface>,
    console: &[ConsoleSurface],
    console_slots: &[ConsoleSlot],
    console_contributions: &[ConsoleContribution],
    capabilities: &[String],
    requirements: &[ModuleRequirement],
) -> Vec<ModuleManifestLint> {
    let mut lints = Vec::new();
    let module_name = module_id.rsplit('/').next().unwrap_or(module_id);

    if !valid_module_id(module_id) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Error,
            subject: "module.module_id".to_owned(),
            message: "ModuleId must use the fully qualified namespace/name form.".to_owned(),
            suggestion: "Set ModuleManifest.module_id to a stable value such as lenso/auth."
                .to_owned(),
        });
    }

    for capability in capabilities {
        if !valid_capability(capability) {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Warning,
                subject: format!("capability {capability}"),
                message: "Capability name should use dot-separated lowercase identifiers."
                    .to_owned(),
                suggestion: "Use a stable capability name such as module.entity.read.".to_owned(),
            });
        }
    }
    for requirement in requirements {
        if !valid_module_id(&requirement.module_id)
            || semver::VersionReq::parse(&requirement.version_requirement).is_err()
        {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Error,
                subject: format!("requirement {}", requirement.module_id),
                message: "Module requirement identity and version range must be valid.".to_owned(),
                suggestion: "Use a fully qualified ModuleId and normalized SemVer requirement."
                    .to_owned(),
            });
        } else if requirement.module_id == module_id {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Error,
                subject: format!("requirement {}", requirement.module_id),
                message: "Module must not depend on itself.".to_owned(),
                suggestion: "Remove the self requirement from ModuleManifest.requires.".to_owned(),
            });
        }
    }

    for route_lint in lint_module_http_routes(http_routes) {
        lints.push(ModuleManifestLint {
            severity: match route_lint.severity {
                crate::http::ModuleRouteLintSeverity::Ok => ModuleManifestLintSeverity::Ok,
                crate::http::ModuleRouteLintSeverity::Warning => {
                    ModuleManifestLintSeverity::Warning
                }
                crate::http::ModuleRouteLintSeverity::Error => ModuleManifestLintSeverity::Error,
            },
            subject: route_lint.subject,
            message: route_lint.message,
            suggestion: route_lint.suggestion,
        });
    }
    lint_capability_references(
        admin,
        http_routes,
        lifecycle,
        console,
        console_contributions,
        capabilities,
        &mut lints,
    );

    if let Some(admin) = admin {
        lint_admin_surface(admin, &mut lints);
    }
    let mut runtime_lints = Vec::new();
    if let Some(runtime) = runtime {
        lint_runtime_surface(module_name, runtime, &mut runtime_lints);
    }
    if let Some(events) = events {
        lint_event_surface(events, &mut lints);
    }
    if let Some(lifecycle) = lifecycle {
        lint_lifecycle_surface(lifecycle, runtime, capabilities, &mut lints);
    }
    lint_console_surfaces(console, &mut lints);
    lint_console_slots(console_slots, &mut lints);
    lint_console_contributions(console_contributions, &mut lints);
    lints.extend(runtime_lints);

    if lints.is_empty() {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Ok,
            subject: "manifest".to_owned(),
            message: "Module manifest metadata is complete.".to_owned(),
            suggestion: "No action needed.".to_owned(),
        });
    }

    lints
}

pub fn module_capability_references(
    admin: Option<&AdminSurface>,
    http_routes: &[ModuleHttpRoute],
    lifecycle: Option<&LifecycleSurface>,
    console: &[ConsoleSurface],
    console_contributions: &[ConsoleContribution],
) -> Vec<ModuleCapabilityReference> {
    let mut references = Vec::new();

    for route in http_routes {
        if let Some(capability) = route.capability.as_deref()
            && present(capability)
        {
            references.push(ModuleCapabilityReference {
                capability: capability.to_owned(),
                subject: format!("http_route.{}", route_identity(route)),
            });
        }
    }

    if let Some(admin) = admin {
        collect_admin_capability_references(admin, &mut references);
    }

    if let Some(lifecycle) = lifecycle {
        for check in &lifecycle.startup_checks {
            if let LifecycleStartupCheckKind::CapabilityDeclared { capability } = &check.check
                && present(capability)
            {
                references.push(ModuleCapabilityReference {
                    capability: capability.to_owned(),
                    subject: format!("lifecycle.startup_check.capability.{capability}"),
                });
            }
        }
    }

    for surface in console {
        let subject = if present(&surface.name) {
            format!("console.surface.{}", surface.name)
        } else {
            "console.surface".to_owned()
        };
        for capability in &surface.required_capabilities {
            if present(capability) {
                references.push(ModuleCapabilityReference {
                    capability: capability.clone(),
                    subject: subject.clone(),
                });
            }
        }
    }

    for contribution in console_contributions {
        let subject = if present(&contribution.target) {
            format!("console.contribution.{}", contribution.target)
        } else {
            "console.contribution".to_owned()
        };
        for capability in &contribution.required_capabilities {
            if present(capability) {
                references.push(ModuleCapabilityReference {
                    capability: capability.clone(),
                    subject: subject.clone(),
                });
            }
        }
    }

    references
}

fn lint_capability_references(
    admin: Option<&AdminSurface>,
    http_routes: &[ModuleHttpRoute],
    lifecycle: Option<&LifecycleSurface>,
    console: &[ConsoleSurface],
    console_contributions: &[ConsoleContribution],
    capabilities: &[String],
    lints: &mut Vec<ModuleManifestLint>,
) {
    let declared = capabilities
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();

    for reference in module_capability_references(
        admin,
        http_routes,
        lifecycle,
        console,
        console_contributions,
    ) {
        // Lifecycle startup checks already produce a lifecycle-specific lint with
        // the check context and required/optional semantics.
        if reference.subject.starts_with("lifecycle.") {
            continue;
        }
        if declared.contains(reference.capability.as_str()) {
            continue;
        }
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Warning,
            subject: format!("capability.reference.{}", reference.subject),
            message: "Capability reference is not declared by the module.".to_owned(),
            suggestion: format!(
                "Add `{}` to ModuleManifest.capabilities or update the reference.",
                reference.capability
            ),
        });
    }
}

fn collect_admin_capability_references(
    admin: &AdminSurface,
    references: &mut Vec<ModuleCapabilityReference>,
) {
    match admin {
        AdminSurface::Schema(schema) => {
            collect_schema_capability_references("admin.schema", schema, references);
        }
        AdminSurface::DeclarativeCustom(surface) => {
            collect_declarative_query_capability_references(surface, references);
            for action in &surface.actions {
                if present(&action.capability) {
                    let action_subject = if present(&action.name) {
                        format!("admin.declarative.action.{}", action.name)
                    } else {
                        "admin.declarative.action".to_owned()
                    };
                    references.push(ModuleCapabilityReference {
                        capability: action.capability.clone(),
                        subject: action_subject,
                    });
                }
            }
            if let Some(schema) = &surface.fallback_schema {
                collect_schema_capability_references(
                    "admin.declarative.fallback_schema",
                    schema,
                    references,
                );
            }
        }
        AdminSurface::EmbeddedCustom(surface) => {
            if let Some(schema) = &surface.fallback_schema {
                collect_schema_capability_references(
                    "admin.embedded.fallback_schema",
                    schema,
                    references,
                );
            }
        }
    }
}

fn collect_schema_capability_references(
    prefix: &str,
    schema: &AdminSchema,
    references: &mut Vec<ModuleCapabilityReference>,
) {
    for entity in &schema.entities {
        if present(&entity.read_capability) {
            references.push(ModuleCapabilityReference {
                capability: entity.read_capability.clone(),
                subject: format!("{prefix}.{}", entity.name),
            });
        }
    }
}

fn lint_runtime_surface(
    module_name: &str,
    runtime: &RuntimeSurface,
    lints: &mut Vec<ModuleManifestLint>,
) {
    if runtime.functions.is_empty() && runtime.schedules.is_empty() && runtime.workflows.is_empty()
    {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Warning,
            subject: "runtime".to_owned(),
            message: "Runtime surface declares no functions, schedules, or workflows.".to_owned(),
            suggestion: "Add at least one runtime declaration or omit the runtime surface."
                .to_owned(),
        });
        return;
    }

    let mut names = HashSet::new();
    for function in &runtime.functions {
        lint_runtime_function(function, &mut names, lints);
    }
    let function_names = runtime_function_names(Some(runtime));
    let mut schedule_names = HashSet::new();
    for schedule in &runtime.schedules {
        lint_scheduled_function(schedule, &function_names, &mut schedule_names, lints);
    }
    let mut workflow_identities = HashSet::new();
    for workflow in &runtime.workflows {
        lint_workflow_definition(module_name, workflow, &mut workflow_identities, lints);
    }
}

fn lint_workflow_definition(
    module_name: &str,
    workflow: &WorkflowDefinition,
    identities: &mut HashSet<(String, String, String)>,
    lints: &mut Vec<ModuleManifestLint>,
) {
    let subject = if present(&workflow.name) && present(&workflow.version) {
        format!("runtime.workflow.{}.{}", workflow.name, workflow.version)
    } else {
        "runtime.workflow".to_owned()
    };

    if workflow.protocol != WORKFLOW_DEFINITION_PROTOCOL {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Error,
            subject: format!("{subject}.protocol"),
            message: "Durable Workflow definition uses an unsupported protocol.".to_owned(),
            suggestion: format!("Set protocol to {WORKFLOW_DEFINITION_PROTOCOL}."),
        });
    }
    if !present(&workflow.owner) || workflow.owner != module_name {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Error,
            subject: format!("{subject}.owner"),
            message: "Durable Workflow owner must match the declaring Module.".to_owned(),
            suggestion: format!("Set owner to the Module name `{module_name}`."),
        });
    }
    if !present(&workflow.name) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Error,
            subject: subject.clone(),
            message: "Durable Workflow definition is missing a stable name.".to_owned(),
            suggestion: "Set a path-safe workflow name such as support_sla.".to_owned(),
        });
    } else if !valid_runtime_function_name(&workflow.name) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Error,
            subject: subject.clone(),
            message: "Durable Workflow name must be path-safe.".to_owned(),
            suggestion: "Use ASCII letters, digits, dot, underscore, or hyphen.".to_owned(),
        });
    }
    if !present(&workflow.version) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Error,
            subject: format!("{subject}.version"),
            message: "Durable Workflow definition is missing a version.".to_owned(),
            suggestion: "Set a stable definition version such as v1.".to_owned(),
        });
    } else if !valid_runtime_function_name(&workflow.version) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Error,
            subject: format!("{subject}.version"),
            message: "Durable Workflow version must be path-safe.".to_owned(),
            suggestion: "Use a stable path-safe version such as v1 or 1.0.0.".to_owned(),
        });
    }
    if present(&workflow.owner)
        && present(&workflow.name)
        && present(&workflow.version)
        && !identities.insert((
            workflow.owner.clone(),
            workflow.name.clone(),
            workflow.version.clone(),
        ))
    {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Error,
            subject: subject.clone(),
            message: "Duplicate Durable Workflow definition identity.".to_owned(),
            suggestion: "Keep one declaration per owner, name, and version.".to_owned(),
        });
    }

    lint_workflow_data_contract(
        &format!("{subject}.input_contract"),
        &workflow.input_contract,
        lints,
    );
    lint_workflow_data_contract(
        &format!("{subject}.result_contract"),
        &workflow.result_contract,
        lints,
    );

    if workflow.steps.is_empty() {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Error,
            subject: format!("{subject}.steps"),
            message: "Durable Workflow definition must declare an ordered first step.".to_owned(),
            suggestion: "Add at least one stable step declaration.".to_owned(),
        });
    }
    let mut step_names = HashSet::new();
    let mut compensation_names = HashSet::new();
    let mut compensation_orders = HashSet::new();
    for step in &workflow.steps {
        let step_subject = if present(&step.name) {
            format!("{subject}.step.{}", step.name)
        } else {
            format!("{subject}.step")
        };
        if !present(&step.name) || !valid_runtime_function_name(&step.name) {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Error,
                subject: step_subject.clone(),
                message: "Durable Workflow step name must be a non-empty path-safe identifier."
                    .to_owned(),
                suggestion: "Use a stable step name such as acknowledge_ticket.".to_owned(),
            });
        } else if !step_names.insert(step.name.clone()) {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Error,
                subject: step_subject.clone(),
                message: "Durable Workflow step name is declared more than once.".to_owned(),
                suggestion: "Keep one ordered declaration per stable step name.".to_owned(),
            });
        }
        lint_workflow_step_recovery(
            step,
            &step_subject,
            &mut compensation_names,
            &mut compensation_orders,
            lints,
        );
    }
}

fn lint_workflow_step_recovery(
    step: &WorkflowStepDeclaration,
    subject: &str,
    compensation_names: &mut HashSet<String>,
    compensation_orders: &mut HashSet<u32>,
    lints: &mut Vec<ModuleManifestLint>,
) {
    if let Some(retry_policy) = &step.retry_policy
        && (retry_policy.max_attempts == 0
            || i32::try_from(retry_policy.max_attempts).is_err()
            || retry_policy.delays_ms.len()
                != usize::try_from(retry_policy.max_attempts.saturating_sub(1))
                    .unwrap_or(usize::MAX)
            || retry_policy
                .delays_ms
                .iter()
                .any(|delay| i64::try_from(*delay).is_err()))
    {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Error,
            subject: format!("{subject}.retry_policy"),
            message: "Durable Workflow retry schedule must use supported attempt and delay values."
                .to_owned(),
            suggestion: "Set maxAttempts to 1..=2147483647 and provide maxAttempts - 1 delaysMs entries within the signed 64-bit range."
                .to_owned(),
        });
    }
    if step
        .timeout_ms
        .is_some_and(|timeout| timeout == 0 || i64::try_from(timeout).is_err())
    {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Error,
            subject: format!("{subject}.timeout_ms"),
            message: "Durable Workflow timeout must use a supported positive value.".to_owned(),
            suggestion: "Set timeoutMs within the positive signed 64-bit range or omit it."
                .to_owned(),
        });
    }
    if let Some(compensation) = &step.compensation {
        let compensation_subject = format!("{subject}.compensation");
        if !present(&compensation.name) || !valid_runtime_function_name(&compensation.name) {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Error,
                subject: format!("{compensation_subject}.name"),
                message:
                    "Durable Workflow compensation name must be a non-empty path-safe identifier."
                        .to_owned(),
                suggestion: "Use a stable compensation name such as release_sla_reservation."
                    .to_owned(),
            });
        } else if !compensation_names.insert(compensation.name.clone()) {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Error,
                subject: format!("{compensation_subject}.name"),
                message: "Durable Workflow compensation name is declared more than once."
                    .to_owned(),
                suggestion: "Keep one stable compensation name per Workflow Definition.".to_owned(),
            });
        }
        if compensation.order == 0 || i32::try_from(compensation.order).is_err() {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Error,
                subject: format!("{compensation_subject}.order"),
                message: "Durable Workflow compensation order must use a supported positive value."
                    .to_owned(),
                suggestion: "Set order to a unique value within 1..=2147483647.".to_owned(),
            });
        } else if !compensation_orders.insert(compensation.order) {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Error,
                subject: format!("{compensation_subject}.order"),
                message: "Durable Workflow compensation order is declared more than once."
                    .to_owned(),
                suggestion: "Assign one deterministic order to each compensation.".to_owned(),
            });
        }
        lint_workflow_data_contract(
            &format!("{compensation_subject}.contract"),
            &compensation.contract,
            lints,
        );
        lint_workflow_data_contract(
            &format!("{compensation_subject}.completion_contract"),
            &compensation.completion_contract,
            lints,
        );
    }
}

fn lint_workflow_data_contract(
    subject: &str,
    contract: &WorkflowDataContract,
    lints: &mut Vec<ModuleManifestLint>,
) {
    if !present(&contract.contract_id) || !present(&contract.version) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Error,
            subject: subject.to_owned(),
            message: "Durable Workflow data contract requires a stable identity and version."
                .to_owned(),
            suggestion: "Set contractId and version to stable contract identifiers.".to_owned(),
        });
    }
}

fn lint_runtime_function(
    function: &RuntimeFunctionDeclaration,
    names: &mut HashSet<String>,
    lints: &mut Vec<ModuleManifestLint>,
) {
    let subject = if present(&function.name) {
        format!("runtime.function.{}", function.name)
    } else {
        "runtime.function".to_owned()
    };

    if !present(&function.name) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Error,
            subject: subject.clone(),
            message: "Runtime function declaration is missing a name.".to_owned(),
            suggestion: "Set a stable versioned function name such as module.action.v1.".to_owned(),
        });
    } else if !valid_runtime_function_name(&function.name) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Warning,
            subject: subject.clone(),
            message: "Runtime function name should be a stable path-safe identifier.".to_owned(),
            suggestion: "Use ASCII letters, digits, dot, underscore, or hyphen.".to_owned(),
        });
    } else if !names.insert(function.name.clone()) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Error,
            subject: subject.clone(),
            message: "Duplicate runtime function declaration.".to_owned(),
            suggestion: "Keep one declaration per runtime function name.".to_owned(),
        });
    }

    if !present(&function.queue) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Warning,
            subject: subject.clone(),
            message: "Runtime function declaration is missing a queue.".to_owned(),
            suggestion: "Set the host queue used to claim this function.".to_owned(),
        });
    }

    if let Some(input_schema) = &function.input_schema
        && input_schema != &function.name
    {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Warning,
            subject: format!("{subject}.input_schema"),
            message: "Runtime function input schema does not match the function name.".to_owned(),
            suggestion: "Use the versioned function name as the input_schema contract identifier."
                .to_owned(),
        });
    }

    if let Some(retry_policy) = &function.retry_policy
        && retry_policy.max_attempts == 0
    {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Warning,
            subject: format!("{subject}.retry_policy"),
            message: "Runtime function retry policy declares zero attempts.".to_owned(),
            suggestion: "Set max_attempts to at least 1 or omit the retry policy.".to_owned(),
        });
    }
}

fn lint_scheduled_function(
    schedule: &ScheduledFunctionDeclaration,
    runtime_functions: &HashSet<String>,
    names: &mut HashSet<String>,
    lints: &mut Vec<ModuleManifestLint>,
) {
    let subject = if present(&schedule.name) {
        format!("runtime.schedule.{}", schedule.name)
    } else {
        "runtime.schedule".to_owned()
    };

    if !present(&schedule.name) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Error,
            subject: subject.clone(),
            message: "Scheduled runtime function is missing a name.".to_owned(),
            suggestion: "Set a stable schedule name such as sync_contacts_hourly.".to_owned(),
        });
    } else if !valid_runtime_function_name(&schedule.name) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Warning,
            subject: subject.clone(),
            message: "Scheduled runtime function name should be path-safe.".to_owned(),
            suggestion: "Use ASCII letters, digits, dot, underscore, or hyphen.".to_owned(),
        });
    } else if !names.insert(schedule.name.clone()) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Error,
            subject: subject.clone(),
            message: "Duplicate scheduled runtime function declaration.".to_owned(),
            suggestion: "Keep one schedule declaration per schedule name.".to_owned(),
        });
    }

    if !present(&schedule.cron) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Error,
            subject: format!("{subject}.cron"),
            message: "Scheduled runtime function is missing a cron expression.".to_owned(),
            suggestion: "Set cron to a standard 5-field UTC cron expression.".to_owned(),
        });
    } else if validate_cron_expression(&schedule.cron).is_err() {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Error,
            subject: format!("{subject}.cron"),
            message: "Scheduled runtime function cron expression is invalid.".to_owned(),
            suggestion: "Use a standard 5-field expression such as */15 * * * *.".to_owned(),
        });
    }

    if !present(&schedule.function_name) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Error,
            subject,
            message: "Scheduled runtime function is missing a function name.".to_owned(),
            suggestion: "Set function_name to a declared runtime function.".to_owned(),
        });
    } else if !runtime_functions.contains(&schedule.function_name) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Error,
            subject,
            message: "Scheduled runtime function references an unknown runtime function."
                .to_owned(),
            suggestion:
                "Declare the function in ModuleManifest.runtime.functions or remove the schedule."
                    .to_owned(),
        });
    }
}

fn lint_event_surface(events: &EventSurface, lints: &mut Vec<ModuleManifestLint>) {
    if events.handlers.is_empty() {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Warning,
            subject: "events.handlers".to_owned(),
            message: "Event surface declares no handlers.".to_owned(),
            suggestion: "Add at least one event handler declaration or omit the events surface."
                .to_owned(),
        });
        return;
    }

    let mut names = HashSet::new();
    for handler in &events.handlers {
        lint_event_handler(handler, &mut names, lints);
    }
}

fn lint_event_handler(
    handler: &EventHandlerDeclaration,
    names: &mut HashSet<String>,
    lints: &mut Vec<ModuleManifestLint>,
) {
    let subject = if present(&handler.name) {
        format!("events.handler.{}", handler.name)
    } else {
        "events.handler".to_owned()
    };

    if !present(&handler.name) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Error,
            subject: subject.clone(),
            message: "Event handler declaration is missing a name.".to_owned(),
            suggestion: "Set a stable handler name such as sync_contact_on_user_registered."
                .to_owned(),
        });
    } else if !valid_runtime_function_name(&handler.name) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Warning,
            subject: subject.clone(),
            message: "Event handler name should be a stable path-safe identifier.".to_owned(),
            suggestion: "Use ASCII letters, digits, dot, underscore, or hyphen.".to_owned(),
        });
    } else if !names.insert(handler.name.clone()) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Error,
            subject: subject.clone(),
            message: "Duplicate event handler declaration.".to_owned(),
            suggestion: "Keep one declaration per event handler name.".to_owned(),
        });
    }

    if !present(&handler.event_name) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Error,
            subject: format!("{subject}.event_name"),
            message: "Event handler declaration is missing an event_name.".to_owned(),
            suggestion: "Set the stable outbox event name this handler consumes.".to_owned(),
        });
    } else if !valid_runtime_function_name(&handler.event_name) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Warning,
            subject: format!("{subject}.event_name"),
            message: "Event name should be a stable path-safe identifier.".to_owned(),
            suggestion: "Use the versioned event name such as identity.user_registered.v1."
                .to_owned(),
        });
    }
}

fn lint_lifecycle_surface(
    lifecycle: &LifecycleSurface,
    runtime: Option<&RuntimeSurface>,
    capabilities: &[String],
    lints: &mut Vec<ModuleManifestLint>,
) {
    if lifecycle.startup_checks.is_empty() && lifecycle.activation_jobs.is_empty() {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Warning,
            subject: "lifecycle".to_owned(),
            message: "Lifecycle surface declares no startup checks or activation jobs.".to_owned(),
            suggestion: "Add lifecycle entries or omit the lifecycle surface.".to_owned(),
        });
        return;
    }

    let runtime_functions = runtime_function_names(runtime);
    let capability_names = capabilities.iter().cloned().collect::<HashSet<_>>();

    for check in &lifecycle.startup_checks {
        lint_lifecycle_startup_check(check, &runtime_functions, &capability_names, lints);
    }

    for job in &lifecycle.activation_jobs {
        lint_lifecycle_activation_job(job, &runtime_functions, lints);
    }
}

fn lint_lifecycle_startup_check(
    check: &LifecycleStartupCheckDeclaration,
    runtime_functions: &HashSet<String>,
    capabilities: &HashSet<String>,
    lints: &mut Vec<ModuleManifestLint>,
) {
    if !present(&check.name) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Warning,
            subject: "lifecycle.startup_check".to_owned(),
            message: "Lifecycle startup check is missing a name.".to_owned(),
            suggestion: "Set a short operator-facing check name.".to_owned(),
        });
    }

    match &check.check {
        LifecycleStartupCheckKind::FunctionRegistered { function_name } => {
            if !runtime_functions.contains(function_name) {
                lints.push(ModuleManifestLint {
                    severity: ModuleManifestLintSeverity::Error,
                    subject: format!(
                        "lifecycle.startup_check.function_registered.{function_name}"
                    ),
                    message: "Lifecycle startup check references an unknown runtime function."
                        .to_owned(),
                    suggestion:
                        "Declare the function in ModuleManifest.runtime.functions or remove the check."
                            .to_owned(),
                });
            }
        }
        LifecycleStartupCheckKind::CapabilityDeclared { capability } => {
            if !capabilities.contains(capability) {
                lints.push(ModuleManifestLint {
                    severity: ModuleManifestLintSeverity::Warning,
                    subject: format!("lifecycle.startup_check.capability.{capability}"),
                    message: "Lifecycle startup check references an undeclared capability."
                        .to_owned(),
                    suggestion:
                        "Add the capability to ModuleManifest.capabilities or update the check."
                            .to_owned(),
                });
            }
        }
    }
}

fn lint_lifecycle_activation_job(
    job: &LifecycleActivationJobDeclaration,
    runtime_functions: &HashSet<String>,
    lints: &mut Vec<ModuleManifestLint>,
) {
    let subject = if present(&job.name) {
        format!("lifecycle.activation_job.{}", job.name)
    } else {
        "lifecycle.activation_job".to_owned()
    };

    if !present(&job.name) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Warning,
            subject: subject.clone(),
            message: "Lifecycle activation job is missing a name.".to_owned(),
            suggestion: "Set a short operator-facing activation job name.".to_owned(),
        });
    }

    if !present(&job.function_name) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Error,
            subject,
            message: "Lifecycle activation job is missing a function name.".to_owned(),
            suggestion: "Set function_name to a declared runtime function.".to_owned(),
        });
    } else if !runtime_functions.contains(&job.function_name) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Error,
            subject,
            message: "Lifecycle activation job references an unknown runtime function.".to_owned(),
            suggestion:
                "Declare the function in ModuleManifest.runtime.functions or remove the activation job."
                    .to_owned(),
        });
    }
}

fn runtime_function_names(runtime: Option<&RuntimeSurface>) -> HashSet<String> {
    runtime
        .into_iter()
        .flat_map(|surface| surface.functions.iter())
        .map(|function| function.name.clone())
        .collect()
}

fn lint_console_surfaces(console: &[ConsoleSurface], lints: &mut Vec<ModuleManifestLint>) {
    let mut names = HashSet::new();
    let mut routes = HashSet::new();

    for surface in console {
        let subject = if present(&surface.name) {
            format!("console.surface.{}", surface.name)
        } else {
            "console.surface".to_owned()
        };

        if !present(&surface.name) {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Error,
                subject: subject.clone(),
                message: "Console surface is missing a name.".to_owned(),
                suggestion: "Set a stable surface name such as stories.".to_owned(),
            });
        } else if !valid_console_surface_name(&surface.name) {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Warning,
                subject: subject.clone(),
                message: "Console surface name should be a path-safe identifier.".to_owned(),
                suggestion: "Use ASCII letters, digits, underscore, or hyphen.".to_owned(),
            });
        } else if !names.insert(surface.name.clone()) {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Error,
                subject: subject.clone(),
                message: "Duplicate console surface declaration.".to_owned(),
                suggestion: "Keep one console surface per surface name.".to_owned(),
            });
        }

        if !present(&surface.label) {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Warning,
                subject: format!("{subject}.label"),
                message: "Console surface is missing an operator-facing label.".to_owned(),
                suggestion: "Set a short navigation label such as Stories.".to_owned(),
            });
        }

        if !surface.route.starts_with('/') || surface.route.contains('*') {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Error,
                subject: format!("{subject}.route"),
                message: "Console surface route must be an absolute static route.".to_owned(),
                suggestion: "Use a Console route such as /runtime/stories.".to_owned(),
            });
        } else if !routes.insert(surface.route.clone()) {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Error,
                subject: format!("{subject}.route"),
                message: "Duplicate console surface route declaration.".to_owned(),
                suggestion: "Keep one console surface per route.".to_owned(),
            });
        }

        match &surface.presentation {
            crate::ConsoleSurfacePresentation::Declarative { schema } => {
                if !schema.is_object() {
                    lints.push(ModuleManifestLint {
                        severity: ModuleManifestLintSeverity::Error,
                        subject: format!("{subject}.presentation.schema"),
                        message: "Declarative Console surface schema must be an object.".to_owned(),
                        suggestion: "Provide a declarative surface schema object.".to_owned(),
                    });
                }
            }
            crate::ConsoleSurfacePresentation::Isolated {
                entry,
                bridge_protocol,
            } => {
                if !present(entry) {
                    lints.push(ModuleManifestLint {
                        severity: ModuleManifestLintSeverity::Error,
                        subject: format!("{subject}.presentation.entry"),
                        message: "Isolated Console surface entry is missing.".to_owned(),
                        suggestion:
                            "Reference an entry from the Module Release Console UI artifact."
                                .to_owned(),
                    });
                }
                if bridge_protocol != crate::CONSOLE_BRIDGE_PROTOCOL {
                    lints.push(ModuleManifestLint {
                        severity: ModuleManifestLintSeverity::Error,
                        subject: format!("{subject}.presentation.bridge_protocol"),
                        message: "Isolated Console surface uses an unsupported bridge protocol."
                            .to_owned(),
                        suggestion: format!(
                            "Use the supported bridge protocol {}.",
                            crate::CONSOLE_BRIDGE_PROTOCOL
                        ),
                    });
                }
            }
        }

        if let Some(navigation) = &surface.navigation {
            lint_console_navigation(&subject, navigation, lints);
        }
    }
}

fn lint_console_slots(console_slots: &[ConsoleSlot], lints: &mut Vec<ModuleManifestLint>) {
    let mut slots = HashSet::new();

    for slot in console_slots {
        let subject = if present(&slot.id) {
            format!("console.slot.{}", slot.id)
        } else {
            "console.slot".to_owned()
        };

        if !present(&slot.id) {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Error,
                subject: subject.clone(),
                message: "Console slot is missing an id.".to_owned(),
                suggestion: "Set a stable dotted slot id such as auth.users.detail.actions."
                    .to_owned(),
            });
        } else if !valid_console_slot_target(&slot.id) {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Warning,
                subject: subject.clone(),
                message: "Console slot id should be a path-safe dotted id.".to_owned(),
                suggestion: "Use ASCII letters, digits, dot, underscore, or hyphen.".to_owned(),
            });
        } else if !slots.insert((slot.id.clone(), slot.version)) {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Error,
                subject: subject.clone(),
                message: "Duplicate console slot declaration.".to_owned(),
                suggestion: "Keep one declaration per console slot id and version.".to_owned(),
            });
        }

        if slot.version == 0 {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Error,
                subject: format!("{subject}.version"),
                message: "Console slot version must be greater than zero.".to_owned(),
                suggestion: "Start slot contracts at version 1.".to_owned(),
            });
        }

        if !present(&slot.label) {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Warning,
                subject: format!("{subject}.label"),
                message: "Console slot is missing an operator-facing label.".to_owned(),
                suggestion: "Set a short label such as User detail actions.".to_owned(),
            });
        }

        if slot.accepts.is_empty() {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Warning,
                subject: format!("{subject}.accepts"),
                message: "Console slot declares no accepted contribution kinds.".to_owned(),
                suggestion: "Declare at least one accepted kind such as admin_action.".to_owned(),
            });
        }

        let mut context_names = HashSet::new();
        for context in &slot.context {
            let context_subject = if present(&context.name) {
                format!("{subject}.context.{}", context.name)
            } else {
                format!("{subject}.context")
            };
            if !present(&context.name) {
                lints.push(ModuleManifestLint {
                    severity: ModuleManifestLintSeverity::Error,
                    subject: context_subject.clone(),
                    message: "Console slot context is missing a name.".to_owned(),
                    suggestion: "Set a stable context name such as selected_user.".to_owned(),
                });
            } else if !valid_slot_context_segment(&context.name) {
                lints.push(ModuleManifestLint {
                    severity: ModuleManifestLintSeverity::Warning,
                    subject: context_subject.clone(),
                    message: "Console slot context name should be path-safe.".to_owned(),
                    suggestion: "Use ASCII letters, digits, underscore, or hyphen.".to_owned(),
                });
            } else if !context_names.insert(context.name.clone()) {
                lints.push(ModuleManifestLint {
                    severity: ModuleManifestLintSeverity::Error,
                    subject: context_subject.clone(),
                    message: "Duplicate console slot context declaration.".to_owned(),
                    suggestion: "Keep one declaration per slot context name.".to_owned(),
                });
            }

            let mut field_names = HashSet::new();
            for field in &context.fields {
                let field_subject = if present(&field.name) {
                    format!("{context_subject}.field.{}", field.name)
                } else {
                    format!("{context_subject}.field")
                };
                if !present(&field.name) {
                    lints.push(ModuleManifestLint {
                        severity: ModuleManifestLintSeverity::Error,
                        subject: field_subject.clone(),
                        message: "Console slot context field is missing a name.".to_owned(),
                        suggestion: "Set a stable field name such as id.".to_owned(),
                    });
                } else if !valid_slot_context_segment(&field.name) {
                    lints.push(ModuleManifestLint {
                        severity: ModuleManifestLintSeverity::Warning,
                        subject: field_subject.clone(),
                        message: "Console slot context field should be path-safe.".to_owned(),
                        suggestion: "Use ASCII letters, digits, underscore, or hyphen.".to_owned(),
                    });
                } else if !field_names.insert(field.name.clone()) {
                    lints.push(ModuleManifestLint {
                        severity: ModuleManifestLintSeverity::Error,
                        subject: field_subject,
                        message: "Duplicate console slot context field declaration.".to_owned(),
                        suggestion: "Keep one declaration per context field name.".to_owned(),
                    });
                }
            }
        }
    }
}

fn lint_console_contributions(
    contributions: &[ConsoleContribution],
    lints: &mut Vec<ModuleManifestLint>,
) {
    for contribution in contributions {
        let subject = if present(&contribution.target) {
            format!("console.contribution.{}", contribution.target)
        } else {
            "console.contribution".to_owned()
        };

        if !present(&contribution.target) {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Error,
                subject: subject.clone(),
                message: "Console contribution is missing a target slot.".to_owned(),
                suggestion: "Set a stable slot target such as auth.users.detail.actions."
                    .to_owned(),
            });
        } else if !valid_console_slot_target(&contribution.target) {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Warning,
                subject: subject.clone(),
                message: "Console contribution target should be a path-safe dotted slot id."
                    .to_owned(),
                suggestion: "Use ASCII letters, digits, dot, underscore, or hyphen.".to_owned(),
            });
        }

        if contribution.target_version == 0 {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Error,
                subject: format!("{subject}.target_version"),
                message: "Console contribution target version must be greater than zero."
                    .to_owned(),
                suggestion: "Set target_version to the slot contract version, usually 1."
                    .to_owned(),
            });
        }

        if !present(&contribution.label) {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Warning,
                subject: format!("{subject}.label"),
                message: "Console contribution is missing an operator-facing label.".to_owned(),
                suggestion: "Set a short action label such as Reset password.".to_owned(),
            });
        }

        match &contribution.action {
            ConsoleContributionAction::AdminAction {
                module,
                name,
                input_bindings,
            } => {
                if !present(module) {
                    lints.push(ModuleManifestLint {
                        severity: ModuleManifestLintSeverity::Error,
                        subject: format!("{subject}.action.module"),
                        message: "Console contribution action is missing a module name.".to_owned(),
                        suggestion: "Set the module that owns the admin action.".to_owned(),
                    });
                }
                if !present(name) {
                    lints.push(ModuleManifestLint {
                        severity: ModuleManifestLintSeverity::Error,
                        subject: format!("{subject}.action.name"),
                        message: "Console contribution action is missing an action name."
                            .to_owned(),
                        suggestion: "Set the admin action name declared by that module.".to_owned(),
                    });
                }
                for binding in input_bindings {
                    if !present(&binding.input) {
                        lints.push(ModuleManifestLint {
                            severity: ModuleManifestLintSeverity::Error,
                            subject: format!("{subject}.action.input_binding"),
                            message:
                                "Console contribution action binding is missing an input name."
                                    .to_owned(),
                            suggestion: "Set the input field that receives the bound value."
                                .to_owned(),
                        });
                    }
                    match &binding.value {
                        ConsoleActionInputValue::SlotContext { path } => {
                            if !present(path) {
                                lints.push(ModuleManifestLint {
                                    severity: ModuleManifestLintSeverity::Error,
                                    subject: format!("{subject}.action.input_binding.path"),
                                    message:
                                        "Console contribution slot-context binding is missing a path."
                                            .to_owned(),
                                    suggestion:
                                        "Set a slot context path such as selected_user.id."
                                            .to_owned(),
                                });
                            } else if !valid_slot_context_path(path) {
                                lints.push(ModuleManifestLint {
                                    severity: ModuleManifestLintSeverity::Warning,
                                    subject: format!("{subject}.action.input_binding.path"),
                                    message:
                                        "Console contribution slot-context path should be path-safe."
                                            .to_owned(),
                                    suggestion:
                                        "Use dot-separated context fields such as selected_user.id."
                                            .to_owned(),
                                });
                            }
                        }
                    }
                }
            }
        }
    }
}

const HOST_SYSTEM_CONSOLE_WORKSPACE_ID: &str = "system";

fn lint_console_navigation(
    subject: &str,
    navigation: &crate::ConsoleNavigation,
    lints: &mut Vec<ModuleManifestLint>,
) {
    let workspace_subject = format!("{subject}.navigation.workspace");
    if !valid_console_navigation_id(&navigation.workspace.id) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Warning,
            subject: format!("{workspace_subject}.id"),
            message: "Console workspace id should be a path-safe identifier.".to_owned(),
            suggestion: "Use ASCII letters, digits, underscore, or hyphen.".to_owned(),
        });
    } else if navigation.workspace.id == HOST_SYSTEM_CONSOLE_WORKSPACE_ID {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Warning,
            subject: format!("{workspace_subject}.id"),
            message: "Console workspace id system is reserved for host-owned surfaces.".to_owned(),
            suggestion:
                "Omit navigation to use the host System workspace, or use a module-owned workspace id."
                    .to_owned(),
        });
    }
    if !present(&navigation.workspace.label) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Warning,
            subject: format!("{workspace_subject}.label"),
            message: "Console workspace is missing an operator-facing label.".to_owned(),
            suggestion: "Set a short workspace label such as CRM.".to_owned(),
        });
    }
    if let Some(group) = &navigation.group {
        let group_subject = format!("{subject}.navigation.group");
        if !valid_console_navigation_id(&group.id) {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Warning,
                subject: format!("{group_subject}.id"),
                message: "Console navigation group id should be a path-safe identifier.".to_owned(),
                suggestion: "Use ASCII letters, digits, underscore, or hyphen.".to_owned(),
            });
        }
        if !present(&group.label) {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Warning,
                subject: format!("{group_subject}.label"),
                message: "Console navigation group is missing an operator-facing label.".to_owned(),
                suggestion: "Set a short group label such as Customers.".to_owned(),
            });
        }
    }
}

fn lint_admin_surface(admin: &AdminSurface, lints: &mut Vec<ModuleManifestLint>) {
    match admin {
        AdminSurface::Schema(schema) => lint_schema_entities("admin.schema", schema, lints),
        AdminSurface::DeclarativeCustom(surface) => {
            if surface.pages.is_empty() && surface.actions.is_empty() {
                lints.push(ModuleManifestLint {
                    severity: ModuleManifestLintSeverity::Warning,
                    subject: "admin.declarative.pages".to_owned(),
                    message: "Declarative admin surface declares no pages or actions.".to_owned(),
                    suggestion:
                        "Add at least one page/action or omit the declarative admin surface."
                            .to_owned(),
                });
            }
            if let Some(schema) = &surface.fallback_schema {
                lint_schema_entities("admin.declarative.fallback_schema", schema, lints);
            }
            let fallback_entities = surface
                .fallback_schema
                .as_ref()
                .map(schema_entity_names)
                .unwrap_or_default();
            for page in &surface.pages {
                for section in &page.sections {
                    match &section.component {
                        AdminDeclarativeComponent::EntityTable { entity }
                        | AdminDeclarativeComponent::EntityDetail { entity } => {
                            if !fallback_entities.contains(entity) {
                                lints.push(ModuleManifestLint {
                                    severity: ModuleManifestLintSeverity::Warning,
                                    subject: format!("admin.declarative.section.{}", section.name),
                                    message: format!(
                                        "Declarative section references unknown fallback entity `{entity}`."
                                    ),
                                    suggestion:
                                        "Declare the entity in fallback_schema or update the section binding."
                                            .to_owned(),
                                });
                            }
                        }
                        AdminDeclarativeComponent::QueryValue {
                            capability,
                            query,
                            value_path,
                        } => lint_query_value(
                            section.name.as_str(),
                            query,
                            capability,
                            value_path,
                            lints,
                        ),
                        AdminDeclarativeComponent::MetricStrip { .. } => {}
                    }
                }
            }
        }
        AdminSurface::EmbeddedCustom(surface) => {
            if surface.runtime != AdminEmbeddedRuntime::Iframe {
                lints.push(ModuleManifestLint {
                    severity: ModuleManifestLintSeverity::Warning,
                    subject: "admin.embedded.runtime".to_owned(),
                    message: "Embedded admin runtime is reserved for a future host policy."
                        .to_owned(),
                    suggestion: "Use iframe for the current embedded admin slice.".to_owned(),
                });
            }
            match &surface.entry {
                AdminEmbeddedEntry::Url {
                    url,
                    allowed_origins,
                } => {
                    if !url.starts_with("https://") && !url.starts_with("http://localhost") {
                        lints.push(ModuleManifestLint {
                            severity: ModuleManifestLintSeverity::Warning,
                            subject: "admin.embedded.entry.url".to_owned(),
                            message:
                                "Embedded admin URL should use HTTPS outside local development."
                                    .to_owned(),
                            suggestion: "Use an HTTPS URL and list its origin in allowed_origins."
                                .to_owned(),
                        });
                    }
                    if allowed_origins.is_empty() {
                        lints.push(ModuleManifestLint {
                            severity: ModuleManifestLintSeverity::Warning,
                            subject: "admin.embedded.entry.allowed_origins".to_owned(),
                            message: "Embedded admin surface declares no allowed origins."
                                .to_owned(),
                            suggestion:
                                "Declare the iframe origin allowlist before enabling the surface."
                                    .to_owned(),
                        });
                    }
                }
            }
            if let Some(schema) = &surface.fallback_schema {
                lint_schema_entities("admin.embedded.fallback_schema", schema, lints);
                let fallback_entities = schema_entity_names(schema);
                for permission in &surface.permissions {
                    if let AdminPermission::ReadEntity { entity } = permission
                        && !fallback_entities.contains(entity)
                    {
                        lints.push(ModuleManifestLint {
                            severity: ModuleManifestLintSeverity::Warning,
                            subject: format!("admin.embedded.permission.{entity}"),
                            message: format!(
                                "Embedded admin permission references unknown fallback entity `{entity}`."
                            ),
                            suggestion:
                                "Declare the entity in fallback_schema or remove the permission."
                                    .to_owned(),
                        });
                    }
                }
            }
        }
    }
}

fn lint_schema_entities(prefix: &str, schema: &AdminSchema, lints: &mut Vec<ModuleManifestLint>) {
    if schema.entities.is_empty() {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Warning,
            subject: prefix.to_owned(),
            message: "Admin schema declares no entities.".to_owned(),
            suggestion: "Add at least one entity or omit the admin schema surface.".to_owned(),
        });
    }
    for entity in &schema.entities {
        if !present(&entity.read_capability) {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Warning,
                subject: format!("{prefix}.{}", entity.name),
                message: "Admin entity is missing read capability.".to_owned(),
                suggestion: "Declare the capability required to read this entity.".to_owned(),
            });
        }
    }
}

fn collect_declarative_query_capability_references(
    surface: &AdminDeclarativeSurface,
    references: &mut Vec<ModuleCapabilityReference>,
) {
    for page in &surface.pages {
        for section in &page.sections {
            let AdminDeclarativeComponent::QueryValue {
                capability, query, ..
            } = &section.component
            else {
                continue;
            };
            if present(capability) {
                let subject = if present(query) {
                    format!("admin.declarative.query.{query}")
                } else {
                    format!("admin.declarative.section.{}", section.name)
                };
                references.push(ModuleCapabilityReference {
                    capability: capability.clone(),
                    subject,
                });
            }
        }
    }
}

fn lint_query_value(
    section_name: &str,
    query: &str,
    capability: &str,
    value_path: &str,
    lints: &mut Vec<ModuleManifestLint>,
) {
    let subject = if present(query) {
        format!("admin.declarative.query.{query}")
    } else {
        format!("admin.declarative.section.{section_name}")
    };
    if !valid_runtime_function_name(query) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Warning,
            subject: subject.clone(),
            message: "Declarative query name should be a stable path-safe identifier.".to_owned(),
            suggestion: "Use ASCII letters, digits, dot, underscore, or hyphen.".to_owned(),
        });
    }
    if !present(value_path) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Warning,
            subject: subject.clone(),
            message: "Declarative query value is missing a value path.".to_owned(),
            suggestion: "Set value_path to the JSON field rendered by this section.".to_owned(),
        });
    }
    if !present(capability) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Warning,
            subject,
            message: "Declarative query is missing a read capability.".to_owned(),
            suggestion: "Declare the capability required to read this query.".to_owned(),
        });
    }
}

fn schema_entity_names(schema: &AdminSchema) -> HashSet<String> {
    schema
        .entities
        .iter()
        .map(|entity| entity.name.clone())
        .collect()
}

fn present(value: &str) -> bool {
    !value.trim().is_empty()
}

fn valid_capability(value: &str) -> bool {
    let mut parts = value.split('.');
    let Some(first) = parts.next() else {
        return false;
    };
    present(first)
        && value.contains('.')
        && std::iter::once(first).chain(parts).all(|part| {
            present(part)
                && part.chars().all(|character| {
                    character.is_ascii_lowercase() || character == '_' || character.is_ascii_digit()
                })
        })
}

fn valid_runtime_function_name(value: &str) -> bool {
    present(value)
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || character == '.'
                || character == '_'
                || character == '-'
        })
}

fn valid_console_surface_name(value: &str) -> bool {
    present(value)
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        })
}

fn valid_console_slot_target(value: &str) -> bool {
    present(value)
        && value.contains('.')
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || character == '.'
                || character == '_'
                || character == '-'
        })
}

fn valid_slot_context_path(value: &str) -> bool {
    present(value) && value.split('.').all(valid_slot_context_segment)
}

fn valid_slot_context_segment(value: &str) -> bool {
    present(value)
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        })
}

fn valid_console_navigation_id(value: &str) -> bool {
    valid_console_surface_name(value)
}

fn valid_module_id(value: &str) -> bool {
    let Some((namespace, name)) = value.split_once('/') else {
        return false;
    };
    !namespace.is_empty()
        && !name.is_empty()
        && !name.contains('/')
        && [namespace, name].into_iter().all(|segment| {
            segment.starts_with(|character: char| character.is_ascii_lowercase())
                && segment.chars().all(|character| {
                    character.is_ascii_lowercase()
                        || character.is_ascii_digit()
                        || character == '-'
                        || character == '_'
                })
        })
}

fn route_identity(route: &ModuleHttpRoute) -> String {
    format!("{} {}", method_label(route.method), route.path)
}

fn method_label(method: ModuleHttpMethod) -> &'static str {
    match method {
        ModuleHttpMethod::Get => "GET",
        ModuleHttpMethod::Post => "POST",
        ModuleHttpMethod::Put => "PUT",
        ModuleHttpMethod::Patch => "PATCH",
        ModuleHttpMethod::Delete => "DELETE",
    }
}

/// Fluent builder for [`ModuleManifest`]. Reusable by every loading source.
#[derive(Debug)]
pub struct ModuleManifestBuilder {
    manifest: ModuleManifest,
}

impl ModuleManifestBuilder {
    #[must_use]
    pub fn summary(mut self, summary: impl Into<String>) -> Self {
        self.manifest.summary = Some(summary.into());
        self
    }

    /// Attach console story-display metadata.
    #[must_use]
    pub fn story_display(mut self, story_display: Vec<StoryDisplayDescriptor>) -> Self {
        self.manifest.story_display = story_display;
        self
    }

    /// Attach declared capabilities.
    #[must_use]
    pub fn capabilities(mut self, mut capabilities: Vec<String>) -> Self {
        capabilities.sort();
        self.manifest.capabilities = capabilities;
        self
    }

    /// Attach required Modules.
    #[must_use]
    pub fn requires(mut self, mut requirements: Vec<ModuleRequirement>) -> Self {
        for requirement in &mut requirements {
            requirement.capabilities.sort();
        }
        requirements.sort_by(|left, right| left.module_id.cmp(&right.module_id));
        self.manifest.requires = requirements;
        self
    }

    /// Compatibility for currently published first-party authoring crates.
    /// New code must use [`Self::requires`]. This method does not preserve a
    /// legacy wire field; it produces canonical Module requirements.
    #[doc(hidden)]
    #[must_use]
    pub fn dependencies(mut self, dependencies: Vec<String>) -> Self {
        let mut requirements = dependencies
            .into_iter()
            .map(|dependency| ModuleRequirement {
                module_id: if dependency.contains('/') {
                    dependency
                } else {
                    format!("lenso/{dependency}")
                },
                version_requirement: "*".to_owned(),
                capabilities: Vec::new(),
                optional: false,
            })
            .collect::<Vec<_>>();
        requirements.sort_by(|left, right| left.module_id.cmp(&right.module_id));
        self.manifest.requires = requirements;
        self
    }

    #[must_use]
    pub fn config(mut self, mut config: ModuleConfigContract) -> Self {
        config
            .fields
            .sort_by(|left, right| left.key.cmp(&right.key));
        self.manifest.config = config;
        self
    }

    #[must_use]
    pub fn migrations(mut self, mut migrations: Vec<ModuleMigrationDeclaration>) -> Self {
        migrations.sort_by_key(|migration| migration.order);
        self.manifest.migrations = migrations;
        self
    }

    /// Attach declared module-owned HTTP routes.
    #[must_use]
    pub fn http_routes(mut self, routes: Vec<ModuleHttpRoute>) -> Self {
        self.manifest.http_routes = routes;
        self
    }

    /// Attach runtime declarations.
    #[must_use]
    pub fn runtime(mut self, runtime: RuntimeSurface) -> Self {
        self.manifest.runtime = Some(runtime);
        self
    }

    /// Attach event handler declarations.
    #[must_use]
    pub fn events(mut self, events: EventSurface) -> Self {
        self.manifest.events = Some(events);
        self
    }

    /// Attach a schema-driven admin surface.
    #[must_use]
    pub fn admin(mut self, schema: AdminSchema) -> Self {
        self.manifest.admin = Some(AdminSurface::Schema(schema));
        self
    }

    /// Attach a host-rendered custom admin surface declaration.
    #[must_use]
    pub fn declarative_admin(mut self, surface: AdminDeclarativeSurface) -> Self {
        self.manifest.admin = Some(AdminSurface::DeclarativeCustom(surface));
        self
    }

    /// Attach a sandboxed module-owned admin surface declaration.
    #[must_use]
    pub fn embedded_admin(mut self, surface: AdminEmbeddedSurface) -> Self {
        self.manifest.admin = Some(AdminSurface::EmbeddedCustom(surface));
        self
    }

    /// Attach lifecycle declarations.
    #[must_use]
    pub fn lifecycle(mut self, lifecycle: LifecycleSurface) -> Self {
        self.manifest.lifecycle = Some(lifecycle);
        self
    }

    /// Attach trusted Console frontend surface declarations.
    #[must_use]
    pub fn console(mut self, console: Vec<ConsoleSurface>) -> Self {
        self.manifest.console = console;
        self
    }

    /// Attach Console extension slot declarations.
    #[must_use]
    pub fn console_slots(mut self, console_slots: Vec<ConsoleSlot>) -> Self {
        self.manifest.console_slots = console_slots;
        self
    }

    /// Attach trusted Console slot contribution declarations.
    #[must_use]
    pub fn console_contributions(
        mut self,
        console_contributions: Vec<ConsoleContribution>,
    ) -> Self {
        self.manifest.console_contributions = console_contributions;
        self
    }

    /// Finish building.
    #[must_use]
    pub fn build(mut self) -> ModuleManifest {
        // Published first-party authoring crates from the pre-reset workspace
        // still pass their local names to the builder. Canonicalize that
        // authoring input without accepting the removed legacy wire shape.
        if !self.manifest.module_id.contains('/') {
            self.manifest.module_id = format!("lenso/{}", self.manifest.module_id);
        }
        self.manifest
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::{
        AdminDeclarativeComponent, AdminDeclarativePage, AdminDeclarativeSection,
        AdminDeclarativeSurface,
    };
    use crate::{
        AdminEmbeddedEntry, AdminEmbeddedRuntime, AdminEmbeddedSurface, AdminSandboxPolicy,
        CONSOLE_BRIDGE_PROTOCOL, ConsoleActionInputBinding, ConsoleActionInputValue,
        ConsoleContribution, ConsoleContributionAction, ConsoleContributionKind, ConsoleSlot,
        ConsoleSlotContext, ConsoleSlotContextField, ConsoleSlotContextFieldType, ConsoleSurface,
        ConsoleSurfacePresentation, EventHandlerDeclaration, EventSurface,
    };
    use crate::{
        LifecycleActivationJobDeclaration, LifecycleActivationRunPolicy,
        LifecycleStartupCheckDeclaration, LifecycleStartupCheckKind, LifecycleSurface,
    };
    use crate::{ModuleHttpMethod, ModuleHttpRoute};
    use crate::{
        RuntimeFunctionDeclaration, RuntimeRetryPolicyDeclaration, RuntimeSurface,
        WorkflowCompensationDeclaration, WorkflowDataContract, WorkflowDefinition,
        WorkflowRetryPolicyDeclaration, WorkflowStepDeclaration,
    };
    use crate::{StoryDisplayDescriptor, StoryDisplaySource};

    #[test]
    fn manifest_round_trips_through_json() {
        let manifest = ModuleManifest::builder("lenso/identity")
            .story_display(vec![StoryDisplayDescriptor {
                source: StoryDisplaySource::ExecutionName {
                    name: "identity.create_user".to_owned(),
                },
                display_name: "Create User".to_owned(),
                story_title: Some("User Registration".to_owned()),
            }])
            .build();

        let json = serde_json::to_string(&manifest).expect("serialize");
        let back: ModuleManifest = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(manifest, back);
    }

    #[test]
    fn manifest_with_console_surface_round_trips_through_json() {
        let manifest = ModuleManifest::builder("lenso/platform-story")
            .console(vec![ConsoleSurface {
                name: "stories".to_owned(),
                label: "Stories".to_owned(),
                route: "/runtime/stories".to_owned(),
                presentation: ConsoleSurfacePresentation::Isolated {
                    entry: "storyConsoleModule".to_owned(),

                    bridge_protocol: CONSOLE_BRIDGE_PROTOCOL.to_owned(),
                },
                icon: Some("workflow".to_owned()),
                required_capabilities: vec!["runtime.stories.read".to_owned()],
                navigation: None,
            }])
            .capabilities(vec!["runtime.stories.read".to_owned()])
            .build();

        let json = serde_json::to_string(&manifest).expect("serialize");
        assert!(json.contains(r#""console""#), "got {json}");
        assert!(json.contains(r#""kind":"isolated""#), "got {json}");

        let back: ModuleManifest = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(manifest, back);
    }

    #[test]
    fn manifest_with_console_contribution_round_trips_through_json() {
        let contribution = ConsoleContribution {
            target: "auth.users.detail.actions".to_owned(),
            target_version: 1,
            label: "Reset password".to_owned(),
            action: ConsoleContributionAction::AdminAction {
                module: "auth-password".to_owned(),
                name: "reset_password".to_owned(),
                input_bindings: vec![ConsoleActionInputBinding {
                    input: "user_id".to_owned(),
                    value: ConsoleActionInputValue::SlotContext {
                        path: "selected_user.id".to_owned(),
                    },
                }],
            },
            icon: Some("key-round".to_owned()),
            required_capabilities: vec!["auth_password.credentials.write".to_owned()],
        };
        let manifest = ModuleManifest::builder("lenso/auth-password")
            .capabilities(vec!["auth_password.credentials.write".to_owned()])
            .console_contributions(vec![contribution.clone()])
            .build();

        let json = serde_json::to_string(&manifest).expect("serialize");
        assert!(json.contains(r#""console_contributions""#), "got {json}");
        assert!(
            json.contains(r#""target":"auth.users.detail.actions""#),
            "got {json}"
        );
        assert!(json.contains(r#""target_version":1"#), "got {json}");
        assert!(json.contains(r#""kind":"admin_action""#), "got {json}");
        assert!(json.contains(r#""kind":"slot_context""#), "got {json}");

        let back: ModuleManifest = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(back.console_contributions, vec![contribution]);
    }

    #[test]
    fn manifest_with_console_slot_round_trips_through_json() {
        let slot = ConsoleSlot {
            id: "auth.users.detail.actions".to_owned(),
            version: 1,
            label: "User detail actions".to_owned(),
            accepts: vec![ConsoleContributionKind::AdminAction],
            context: vec![ConsoleSlotContext {
                name: "selected_user".to_owned(),
                fields: vec![ConsoleSlotContextField {
                    name: "id".to_owned(),
                    field_type: ConsoleSlotContextFieldType::String,
                    required: true,
                }],
            }],
        };
        let manifest = ModuleManifest::builder("lenso/auth")
            .console_slots(vec![slot.clone()])
            .build();

        let json = serde_json::to_string(&manifest).expect("serialize");
        assert!(json.contains(r#""console_slots""#), "got {json}");
        assert!(
            json.contains(r#""id":"auth.users.detail.actions""#),
            "got {json}"
        );
        assert!(json.contains(r#""accepts":["admin_action"]"#), "got {json}");

        let back: ModuleManifest = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(back.console_slots, vec![slot]);
    }

    #[test]
    fn console_contribution_capability_references_are_linted() {
        let manifest = ModuleManifest::builder("lenso/auth-password")
            .console_contributions(vec![ConsoleContribution {
                target: "auth.users.detail.actions".to_owned(),
                target_version: 1,
                label: "Reset password".to_owned(),
                action: ConsoleContributionAction::AdminAction {
                    module: "auth-password".to_owned(),
                    name: "reset_password".to_owned(),
                    input_bindings: vec![ConsoleActionInputBinding {
                        input: "user_id".to_owned(),
                        value: ConsoleActionInputValue::SlotContext {
                            path: "selected_user.id".to_owned(),
                        },
                    }],
                },
                icon: None,
                required_capabilities: vec!["auth_password.credentials.write".to_owned()],
            }])
            .build();

        let lints = lint_module_manifest(&manifest);

        assert!(lints.iter().any(|lint| {
            lint.subject == "capability.reference.console.contribution.auth.users.detail.actions"
                && lint.message == "Capability reference is not declared by the module."
        }));
    }

    #[test]
    fn console_surface_navigation_round_trips() {
        let surface = ConsoleSurface {
            name: "contacts".to_owned(),
            label: "Contacts".to_owned(),
            route: "/crm/contacts".to_owned(),
            presentation: ConsoleSurfacePresentation::Isolated {
                entry: "crmConsoleModule".to_owned(),

                bridge_protocol: CONSOLE_BRIDGE_PROTOCOL.to_owned(),
            },
            icon: Some("users".to_owned()),
            required_capabilities: vec!["crm.contacts.read".to_owned()],
            navigation: Some(crate::ConsoleNavigation {
                workspace: crate::ConsoleWorkspaceRef {
                    id: "crm".to_owned(),
                    label: "CRM".to_owned(),
                    icon: Some("briefcase".to_owned()),
                },
                group: Some(crate::ConsoleNavigationGroup {
                    id: "customers".to_owned(),
                    label: "Customers".to_owned(),
                    icon: None,
                    order: Some(20),
                }),
                order: Some(10),
            }),
        };

        let json = serde_json::to_string(&surface).expect("serialize");
        let back: ConsoleSurface = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(back, surface);
    }

    #[test]
    fn console_navigation_lints_empty_workspace_label() {
        let manifest = ModuleManifest::builder("acme/crm")
            .capabilities(vec!["crm.contacts.read".to_owned()])
            .console(vec![ConsoleSurface {
                name: "contacts".to_owned(),
                label: "Contacts".to_owned(),
                route: "/crm/contacts".to_owned(),
                presentation: ConsoleSurfacePresentation::Isolated {
                    entry: "crmConsoleModule".to_owned(),

                    bridge_protocol: CONSOLE_BRIDGE_PROTOCOL.to_owned(),
                },
                icon: None,
                required_capabilities: vec!["crm.contacts.read".to_owned()],
                navigation: Some(crate::ConsoleNavigation {
                    workspace: crate::ConsoleWorkspaceRef {
                        id: "crm".to_owned(),
                        label: "".to_owned(),
                        icon: None,
                    },
                    group: None,
                    order: None,
                }),
            }])
            .build();

        let subjects: Vec<_> = lint_module_manifest(&manifest)
            .into_iter()
            .map(|lint| lint.subject)
            .collect();

        assert!(
            subjects.contains(&"console.surface.contacts.navigation.workspace.label".to_owned())
        );
    }

    #[test]
    fn console_navigation_lints_reserved_system_workspace() {
        let manifest = ModuleManifest::builder("acme/crm")
            .capabilities(vec!["crm.contacts.read".to_owned()])
            .console(vec![ConsoleSurface {
                name: "contacts".to_owned(),
                label: "Contacts".to_owned(),
                route: "/crm/contacts".to_owned(),
                presentation: ConsoleSurfacePresentation::Isolated {
                    entry: "crmConsoleModule".to_owned(),

                    bridge_protocol: CONSOLE_BRIDGE_PROTOCOL.to_owned(),
                },
                icon: None,
                required_capabilities: vec!["crm.contacts.read".to_owned()],
                navigation: Some(crate::ConsoleNavigation {
                    workspace: crate::ConsoleWorkspaceRef {
                        id: "system".to_owned(),
                        label: "System".to_owned(),
                        icon: Some("settings".to_owned()),
                    },
                    group: None,
                    order: Some(10),
                }),
            }])
            .build();

        let lints = lint_module_manifest(&manifest);

        assert!(lints.iter().any(|lint| {
            lint.subject == "console.surface.contacts.navigation.workspace.id"
                && lint.severity == ModuleManifestLintSeverity::Warning
                && lint.message
                    == "Console workspace id system is reserved for host-owned surfaces."
        }));
    }

    #[test]
    fn lints_invalid_console_surface_declarations() {
        let manifest = ModuleManifest::builder("lenso/platform-story")
            .console(vec![
                ConsoleSurface {
                    name: "stories".to_owned(),
                    label: "Stories".to_owned(),
                    route: "runtime/stories".to_owned(),
                    presentation: ConsoleSurfacePresentation::Isolated {
                        entry: String::new(),
                        bridge_protocol: "unsupported".to_owned(),
                    },
                    icon: None,
                    required_capabilities: vec!["runtime.stories.read".to_owned()],
                    navigation: None,
                },
                ConsoleSurface {
                    name: "stories".to_owned(),
                    label: "Stories duplicate".to_owned(),
                    route: "/runtime/stories".to_owned(),
                    presentation: ConsoleSurfacePresentation::Isolated {
                        entry: "storyConsoleModule".to_owned(),

                        bridge_protocol: CONSOLE_BRIDGE_PROTOCOL.to_owned(),
                    },
                    icon: None,
                    required_capabilities: vec![],
                    navigation: None,
                },
            ])
            .build();

        let lints = lint_module_manifest(&manifest);
        let subjects = lints
            .iter()
            .map(|lint| lint.subject.as_str())
            .collect::<Vec<_>>();

        assert!(subjects.contains(&"console.surface.stories.route"));
        assert!(subjects.contains(&"console.surface.stories.presentation.entry"));
        assert!(subjects.contains(&"console.surface.stories.presentation.bridge_protocol"));
        assert!(subjects.contains(&"capability.reference.console.surface.stories"));
        assert!(lints.iter().any(|lint| {
            lint.subject == "console.surface.stories"
                && lint.message == "Duplicate console surface declaration."
        }));
    }

    #[test]
    fn empty_admin_is_skipped_in_json() {
        let manifest = ModuleManifest::builder("lenso/notifications").build();
        let json = serde_json::to_string(&manifest).expect("serialize");
        assert!(
            !json.contains("admin"),
            "admin: None must be skipped, got {json}"
        );
    }

    #[test]
    fn manifest_lints_self_dependency() {
        let manifest = ModuleManifest::builder("lenso/auth")
            .requires(vec![
                ModuleRequirement::new("lenso/auth", "*").expect("valid requirement"),
            ])
            .build();

        let lints = lint_module_manifest(&manifest);

        assert!(lints.iter().any(|lint| {
            lint.severity == ModuleManifestLintSeverity::Error
                && lint.subject == "requirement lenso/auth"
                && lint.message == "Module must not depend on itself."
        }));
    }

    #[test]
    fn manifest_with_admin_serializes_schema_kind() {
        use crate::admin_schema::{AdminSchema, EntitySchema, FieldSchema, FieldType};
        let schema = AdminSchema {
            entities: vec![EntitySchema {
                name: "users".to_owned(),
                label: "Users".to_owned(),
                read_capability: "identity.users.read".to_owned(),
                fields: vec![FieldSchema {
                    name: "email".into(),
                    label: "Email".into(),
                    field_type: FieldType::String,
                    nullable: false,
                }],
            }],
        };
        let manifest = ModuleManifest::builder("lenso/identity")
            .admin(schema)
            .build();
        let json = serde_json::to_string(&manifest).expect("serialize");
        assert!(json.contains(r#""kind":"schema""#), "got {json}");
    }

    #[test]
    fn manifest_with_declarative_admin_serializes_kind() {
        use crate::admin::AdminDeclarativeSurface;

        let manifest = ModuleManifest::builder("acme/remote-crm")
            .declarative_admin(AdminDeclarativeSurface {
                pages: vec![],
                actions: vec![],
                fallback_schema: None,
            })
            .build();
        let json = serde_json::to_string(&manifest).expect("serialize");
        assert!(
            json.contains(r#""kind":"declarative_custom""#),
            "got {json}"
        );
    }

    #[test]
    fn manifest_with_embedded_admin_serializes_kind() {
        use crate::admin::{
            AdminEmbeddedEntry, AdminEmbeddedRuntime, AdminEmbeddedSurface, AdminSandboxPolicy,
        };

        let manifest = ModuleManifest::builder("acme/remote-crm")
            .embedded_admin(AdminEmbeddedSurface {
                runtime: AdminEmbeddedRuntime::Iframe,
                entry: AdminEmbeddedEntry::Url {
                    url: "https://crm.example.test/admin".to_owned(),
                    allowed_origins: vec!["https://crm.example.test".to_owned()],
                },
                sandbox: AdminSandboxPolicy {
                    allow_scripts: true,
                    allow_forms: false,
                    allow_popups: false,
                    allow_same_origin: false,
                },
                permissions: vec![],
                fallback_schema: None,
            })
            .build();
        let json = serde_json::to_string(&manifest).expect("serialize");
        assert!(json.contains(r#""kind":"embedded_custom""#), "got {json}");
    }

    #[test]
    fn manifest_with_http_routes_round_trips_through_json() {
        let manifest = ModuleManifest::builder("acme/remote-crm")
            .http_routes(vec![
                ModuleHttpRoute {
                    method: ModuleHttpMethod::Get,
                    path: "/contacts".to_owned(),
                    capability: Some("remote_crm.contacts.read".to_owned()),
                    display_name: Some("List Contacts".to_owned()),
                    story_title: Some("List Contacts".to_owned()),
                    operation: None,
                },
                ModuleHttpRoute {
                    method: ModuleHttpMethod::Post,
                    path: "/contacts".to_owned(),
                    capability: Some("remote_crm.contacts.write".to_owned()),
                    display_name: None,
                    story_title: None,
                    operation: None,
                },
            ])
            .build();

        let json = serde_json::to_string(&manifest).expect("serialize");
        assert!(json.contains(r#""http_routes""#), "got {json}");
        assert!(json.contains(r#""method":"GET""#), "got {json}");
        assert!(
            json.contains(r#""display_name":"List Contacts""#),
            "got {json}"
        );
        let back: ModuleManifest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(manifest, back);
    }

    #[test]
    fn manifest_with_runtime_functions_round_trips_through_json() {
        let manifest = ModuleManifest::builder("acme/remote-crm")
            .runtime(RuntimeSurface {
                functions: vec![RuntimeFunctionDeclaration {
                    name: "remote_crm.sync_contact.v1".to_owned(),
                    version: 1,
                    queue: "remote-crm".to_owned(),
                    input_schema: Some("remote_crm.sync_contact.v1".to_owned()),
                    retry_policy: Some(RuntimeRetryPolicyDeclaration {
                        max_attempts: 3,
                        initial_delay_ms: 1000,
                    }),
                    operation: None,
                }],
                schedules: vec![ScheduledFunctionDeclaration {
                    name: "sync_contacts_hourly".to_owned(),
                    function_name: "remote_crm.sync_contact.v1".to_owned(),
                    cron: "0 * * * *".to_owned(),
                    input: serde_json::json!({ "reason": "schedule" }),
                }],
                workflows: vec![],
            })
            .build();

        let json = serde_json::to_string(&manifest).expect("serialize");

        assert!(json.contains(r#""runtime""#), "got {json}");
        assert!(
            json.contains(r#""name":"remote_crm.sync_contact.v1""#),
            "got {json}"
        );
        assert!(json.contains(r#""queue":"remote-crm""#), "got {json}");
        assert!(json.contains(r#""schedules""#), "got {json}");
        assert!(
            !json.contains(r#""workflows""#),
            "empty workflow declarations must not change existing Runtime Function manifests: {json}"
        );
        let back: ModuleManifest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(manifest, back);
    }

    #[test]
    fn manifest_with_versioned_workflow_round_trips_and_lints_cleanly() {
        let manifest = ModuleManifest::builder("acme/support-sla")
            .runtime(RuntimeSurface {
                functions: vec![],
                schedules: vec![],
                workflows: vec![WorkflowDefinition::new(
                    "support-sla",
                    "ticket_sla",
                    "v1",
                    WorkflowDataContract::new("support.sla.start", "v1"),
                    WorkflowDataContract::new("support.sla.result", "v1"),
                    vec![
                        WorkflowStepDeclaration::new("acknowledge_ticket")
                            .with_display_name("Acknowledge ticket")
                            .with_retry_policy(WorkflowRetryPolicyDeclaration::new(
                                3,
                                vec![1_000, 5_000],
                            ))
                            .with_timeout_ms(30_000)
                            .with_compensation(
                                WorkflowCompensationDeclaration::new(
                                    "withdraw_sla_acknowledgement",
                                    1,
                                    WorkflowDataContract::new("sla-compensation-requested", "v1"),
                                )
                                .with_completion_contract(
                                    WorkflowDataContract::new("sla-compensated", "v1"),
                                ),
                            ),
                        WorkflowStepDeclaration::new("await_resolution"),
                    ],
                )],
            })
            .build();

        let json = serde_json::to_string(&manifest).expect("serialize");
        let back: ModuleManifest = serde_json::from_str(&json).expect("deserialize");
        let lints = lint_module_manifest(&back);
        let workflow_schema = crate::workflow_definition_schema();
        let compensation_schema = &workflow_schema["$defs"]["compensation"];

        assert_eq!(manifest, back);
        assert!(json.contains(r#""protocol":"lenso.workflow-definition.v1""#));
        assert!(json.contains(r#""inputContract""#));
        assert!(json.contains(r#""maxAttempts":3"#));
        assert!(json.contains(r#""delaysMs":[1000,5000]"#));
        assert!(json.contains(r#""timeoutMs":30000"#));
        assert!(json.contains(r#""name":"withdraw_sla_acknowledgement""#));
        assert!(json.contains(r#""order":1"#));
        assert!(json.contains(r#""contract":{"contractId":"sla-compensation-requested""#));
        assert!(json.contains(r#""completionContract":{"contractId":"sla-compensated""#));
        assert_eq!(
            compensation_schema["properties"]["completionContract"]["$ref"],
            "#/$defs/dataContract"
        );
        assert!(
            compensation_schema["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "completionContract")
        );
        assert!(lints.iter().all(|lint| {
            lint.severity != ModuleManifestLintSeverity::Error
                && !lint.subject.starts_with("runtime.workflow")
        }));
    }

    #[test]
    fn manifest_lint_rejects_unowned_or_ambiguous_workflows() {
        let invalid = WorkflowDefinition::new(
            "another-module",
            "ticket_sla",
            "v1",
            WorkflowDataContract::new("", "v1"),
            WorkflowDataContract::new("support.sla.result", "v1"),
            vec![
                WorkflowStepDeclaration::new("acknowledge_ticket")
                    .with_retry_policy(WorkflowRetryPolicyDeclaration::new(3, vec![1_000]))
                    .with_timeout_ms(0)
                    .with_compensation(WorkflowCompensationDeclaration::new(
                        "invalid compensation",
                        0,
                        WorkflowDataContract::new("", ""),
                    )),
                WorkflowStepDeclaration::new("acknowledge_ticket").with_compensation(
                    WorkflowCompensationDeclaration::new(
                        "invalid compensation",
                        0,
                        WorkflowDataContract::new("", ""),
                    ),
                ),
            ],
        );
        let manifest = ModuleManifest::builder("acme/support-sla")
            .runtime(RuntimeSurface {
                functions: vec![],
                schedules: vec![],
                workflows: vec![invalid.clone(), invalid],
            })
            .build();

        let lints = lint_module_manifest(&manifest);

        assert!(
            lints
                .iter()
                .any(|lint| lint.subject == "runtime.workflow.ticket_sla.v1.owner")
        );
        assert!(lints.iter().any(|lint| {
            lint.subject == "runtime.workflow.ticket_sla.v1"
                && lint.message == "Duplicate Durable Workflow definition identity."
        }));
        assert!(lints.iter().any(|lint| {
            lint.subject == "runtime.workflow.ticket_sla.v1.step.acknowledge_ticket"
                && lint.message == "Durable Workflow step name is declared more than once."
        }));
        assert!(lints.iter().any(|lint| {
            lint.subject == "runtime.workflow.ticket_sla.v1.step.acknowledge_ticket.retry_policy"
        }));
        assert!(lints.iter().any(|lint| {
            lint.subject == "runtime.workflow.ticket_sla.v1.step.acknowledge_ticket.timeout_ms"
        }));
        assert!(lints.iter().any(|lint| {
            lint.subject
                == "runtime.workflow.ticket_sla.v1.step.acknowledge_ticket.compensation.name"
        }));
        assert!(lints.iter().any(|lint| {
            lint.subject
                == "runtime.workflow.ticket_sla.v1.step.acknowledge_ticket.compensation.order"
        }));
        assert!(lints.iter().any(|lint| {
            lint.subject
                == "runtime.workflow.ticket_sla.v1.step.acknowledge_ticket.compensation.contract"
        }));
    }

    #[test]
    fn manifest_with_event_handlers_round_trips_through_json() {
        let manifest = ModuleManifest::builder("acme/remote-crm")
            .events(EventSurface {
                handlers: vec![EventHandlerDeclaration {
                    name: "sync_contact_on_user_registered".to_owned(),
                    event_name: "identity.user_registered.v1".to_owned(),
                    operation: None,
                }],
            })
            .build();

        let json = serde_json::to_string(&manifest).expect("serialize");

        assert!(json.contains(r#""events""#), "got {json}");
        assert!(
            json.contains(r#""name":"sync_contact_on_user_registered""#),
            "got {json}"
        );
        assert!(
            json.contains(r#""event_name":"identity.user_registered.v1""#),
            "got {json}"
        );
        let back: ModuleManifest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(manifest, back);
    }

    #[test]
    fn manifest_lint_warns_for_invalid_capability_names() {
        let manifest = ModuleManifest::builder("acme/remote-crm")
            .capabilities(vec!["RemoteCRM Contacts Read".to_owned()])
            .build();

        assert!(
            lint_module_manifest(&manifest)
                .iter()
                .any(|lint| lint.subject == "capability RemoteCRM Contacts Read"
                    && lint.severity == ModuleManifestLintSeverity::Warning)
        );
    }

    #[test]
    fn manifest_lint_warns_for_unknown_declarative_fallback_entities() {
        let manifest = ModuleManifest::builder("acme/remote-crm")
            .declarative_admin(AdminDeclarativeSurface {
                pages: vec![AdminDeclarativePage {
                    name: "dashboard".to_owned(),
                    label: "Dashboard".to_owned(),
                    sections: vec![AdminDeclarativeSection {
                        name: "missing".to_owned(),
                        label: "Missing".to_owned(),
                        component: AdminDeclarativeComponent::EntityTable {
                            entity: "contacts".to_owned(),
                        },
                    }],
                }],
                actions: vec![],
                fallback_schema: None,
            })
            .build();

        assert!(
            lint_module_manifest(&manifest)
                .iter()
                .any(|lint| lint.subject == "admin.declarative.section.missing"
                    && lint.severity == ModuleManifestLintSeverity::Warning)
        );
    }

    #[test]
    fn manifest_lint_warns_for_embedded_origin_policy() {
        let manifest = ModuleManifest::builder("acme/remote-crm")
            .embedded_admin(AdminEmbeddedSurface {
                runtime: AdminEmbeddedRuntime::Iframe,
                entry: AdminEmbeddedEntry::Url {
                    url: "http://crm.example.test/admin".to_owned(),
                    allowed_origins: vec![],
                },
                sandbox: AdminSandboxPolicy {
                    allow_scripts: true,
                    allow_forms: false,
                    allow_popups: false,
                    allow_same_origin: false,
                },
                permissions: vec![],
                fallback_schema: None,
            })
            .build();

        let lints = lint_module_manifest(&manifest);

        assert!(
            lints
                .iter()
                .any(|lint| lint.subject == "admin.embedded.entry.url")
        );
        assert!(
            lints
                .iter()
                .any(|lint| lint.subject == "admin.embedded.entry.allowed_origins")
        );
    }

    #[test]
    fn manifest_lint_warns_for_runtime_function_declarations() {
        let manifest = ModuleManifest::builder("acme/remote-crm")
            .runtime(RuntimeSurface {
                functions: vec![
                    RuntimeFunctionDeclaration {
                        name: "remote_crm/sync_contact.v1".to_owned(),
                        version: 1,
                        queue: "".to_owned(),
                        input_schema: Some("remote_crm.sync_contact.v1".to_owned()),
                        retry_policy: Some(RuntimeRetryPolicyDeclaration {
                            max_attempts: 0,
                            initial_delay_ms: 1000,
                        }),
                        operation: None,
                    },
                    RuntimeFunctionDeclaration {
                        name: "remote_crm.sync_contact.v1".to_owned(),
                        version: 1,
                        queue: "remote-crm".to_owned(),
                        input_schema: Some("remote_crm.sync_contact.input.v1".to_owned()),
                        retry_policy: None,
                        operation: None,
                    },
                    RuntimeFunctionDeclaration {
                        name: "remote_crm.sync_contact.v1".to_owned(),
                        version: 1,
                        queue: "remote-crm".to_owned(),
                        input_schema: Some("remote_crm.sync_contact.v1".to_owned()),
                        retry_policy: None,
                        operation: None,
                    },
                ],
                schedules: vec![],
                workflows: vec![],
            })
            .build();

        let lints = lint_module_manifest(&manifest);

        assert!(lints.iter().any(|lint| {
            lint.subject == "runtime.function.remote_crm/sync_contact.v1"
                && lint.severity == ModuleManifestLintSeverity::Warning
        }));
        assert!(lints.iter().any(|lint| {
            lint.subject == "runtime.function.remote_crm/sync_contact.v1.retry_policy"
                && lint.severity == ModuleManifestLintSeverity::Warning
        }));
        assert!(lints.iter().any(|lint| {
            lint.subject == "runtime.function.remote_crm.sync_contact.v1.input_schema"
                && lint.severity == ModuleManifestLintSeverity::Warning
        }));
        assert!(lints.iter().any(|lint| {
            lint.subject == "runtime.function.remote_crm.sync_contact.v1"
                && lint.severity == ModuleManifestLintSeverity::Error
        }));
    }

    #[test]
    fn manifest_with_lifecycle_round_trips_through_json() {
        let manifest = ModuleManifest::builder("acme/remote-crm")
            .runtime(RuntimeSurface {
                functions: vec![RuntimeFunctionDeclaration {
                    name: "remote_crm.warm_contact_cache.v1".to_owned(),
                    version: 1,
                    queue: "remote-crm".to_owned(),
                    input_schema: Some("remote_crm.warm_contact_cache.v1".to_owned()),
                    retry_policy: Some(RuntimeRetryPolicyDeclaration {
                        max_attempts: 2,
                        initial_delay_ms: 500,
                    }),
                    operation: None,
                }],
                schedules: vec![],
                workflows: vec![],
            })
            .lifecycle(LifecycleSurface {
                startup_checks: vec![LifecycleStartupCheckDeclaration {
                    name: "warm cache function is registered".to_owned(),
                    required: true,
                    check: LifecycleStartupCheckKind::FunctionRegistered {
                        function_name: "remote_crm.warm_contact_cache.v1".to_owned(),
                    },
                }],
                activation_jobs: vec![LifecycleActivationJobDeclaration {
                    name: "warm contact cache".to_owned(),
                    function_name: "remote_crm.warm_contact_cache.v1".to_owned(),
                    run_policy: LifecycleActivationRunPolicy::EveryStartup,
                    input: serde_json::json!({ "reason": "worker_startup" }),
                    required: true,
                }],
            })
            .build();

        let json = serde_json::to_string(&manifest).expect("serialize");

        assert!(json.contains(r#""lifecycle""#), "got {json}");
        assert!(
            json.contains(r#""kind":"function_registered""#),
            "got {json}"
        );
        assert!(
            json.contains(r#""run_policy":"every_startup""#),
            "got {json}"
        );
        let back: ModuleManifest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(manifest, back);
    }

    #[test]
    fn manifest_lint_flags_lifecycle_declarations_that_cannot_run() {
        let manifest = ModuleManifest::builder("acme/remote-crm")
            .runtime(RuntimeSurface {
                functions: vec![],
                schedules: vec![],
                workflows: vec![],
            })
            .lifecycle(LifecycleSurface {
                startup_checks: vec![
                    LifecycleStartupCheckDeclaration {
                        name: "".to_owned(),
                        required: true,
                        check: LifecycleStartupCheckKind::FunctionRegistered {
                            function_name: "remote_crm.missing.v1".to_owned(),
                        },
                    },
                    LifecycleStartupCheckDeclaration {
                        name: "missing capability".to_owned(),
                        required: true,
                        check: LifecycleStartupCheckKind::CapabilityDeclared {
                            capability: "remote_crm.contacts.read".to_owned(),
                        },
                    },
                ],
                activation_jobs: vec![LifecycleActivationJobDeclaration {
                    name: "warm contact cache".to_owned(),
                    function_name: "remote_crm.warm_contact_cache.v1".to_owned(),
                    run_policy: LifecycleActivationRunPolicy::EveryStartup,
                    input: serde_json::json!({}),
                    required: true,
                }],
            })
            .build();

        let lints = lint_module_manifest(&manifest);

        assert!(lints.iter().any(|lint| {
            lint.subject == "lifecycle.startup_check"
                && lint.severity == ModuleManifestLintSeverity::Warning
                && lint.message == "Lifecycle startup check is missing a name."
        }));
        assert!(lints.iter().any(|lint| {
            lint.subject == "lifecycle.startup_check.function_registered.remote_crm.missing.v1"
                && lint.severity == ModuleManifestLintSeverity::Error
        }));
        assert!(lints.iter().any(|lint| {
            lint.subject == "lifecycle.startup_check.capability.remote_crm.contacts.read"
                && lint.severity == ModuleManifestLintSeverity::Warning
        }));
        assert!(lints.iter().any(|lint| {
            lint.subject == "lifecycle.activation_job.warm contact cache"
                && lint.severity == ModuleManifestLintSeverity::Error
        }));
    }

    #[test]
    fn manifest_lint_warns_for_empty_lifecycle_surface() {
        let manifest = ModuleManifest::builder("acme/remote-crm")
            .lifecycle(LifecycleSurface {
                startup_checks: vec![],
                activation_jobs: vec![],
            })
            .build();

        let lints = lint_module_manifest(&manifest);

        assert!(lints.iter().any(|lint| {
            lint.subject == "lifecycle"
                && lint.severity == ModuleManifestLintSeverity::Warning
                && lint.message
                    == "Lifecycle surface declares no startup checks or activation jobs."
        }));
    }

    #[test]
    fn manifest_lint_warns_for_activation_job_missing_name() {
        let manifest = ModuleManifest::builder("acme/remote-crm")
            .runtime(RuntimeSurface {
                functions: vec![RuntimeFunctionDeclaration {
                    name: "remote_crm.warm_contact_cache.v1".to_owned(),
                    version: 1,
                    queue: "remote-crm".to_owned(),
                    input_schema: Some("remote_crm.warm_contact_cache.v1".to_owned()),
                    retry_policy: None,
                    operation: None,
                }],
                schedules: vec![],
                workflows: vec![],
            })
            .lifecycle(LifecycleSurface {
                startup_checks: vec![],
                activation_jobs: vec![LifecycleActivationJobDeclaration {
                    name: "".to_owned(),
                    function_name: "remote_crm.warm_contact_cache.v1".to_owned(),
                    run_policy: LifecycleActivationRunPolicy::EveryStartup,
                    input: serde_json::json!({}),
                    required: true,
                }],
            })
            .build();

        let lints = lint_module_manifest(&manifest);

        assert!(lints.iter().any(|lint| {
            lint.subject == "lifecycle.activation_job"
                && lint.severity == ModuleManifestLintSeverity::Warning
                && lint.message == "Lifecycle activation job is missing a name."
        }));
    }

    #[test]
    fn manifest_lint_errors_for_activation_job_missing_function_name() {
        let manifest = ModuleManifest::builder("acme/remote-crm")
            .lifecycle(LifecycleSurface {
                startup_checks: vec![],
                activation_jobs: vec![LifecycleActivationJobDeclaration {
                    name: "".to_owned(),
                    function_name: "".to_owned(),
                    run_policy: LifecycleActivationRunPolicy::EveryStartup,
                    input: serde_json::json!({}),
                    required: true,
                }],
            })
            .build();

        let lints = lint_module_manifest(&manifest);

        assert!(lints.iter().any(|lint| {
            lint.subject == "lifecycle.activation_job"
                && lint.severity == ModuleManifestLintSeverity::Error
                && lint.message == "Lifecycle activation job is missing a function name."
        }));
    }

    #[test]
    fn manifest_lint_warns_for_undeclared_capability_references() {
        use crate::admin::{AdminAction, AdminActionDangerLevel};

        let manifest = ModuleManifest::builder("acme/remote-crm")
            .capabilities(vec!["remote_crm.contacts.write".to_owned()])
            .http_routes(vec![ModuleHttpRoute {
                method: ModuleHttpMethod::Get,
                path: "/contacts/{id}".to_owned(),
                capability: Some("remote_crm.contacts.read".to_owned()),
                display_name: Some("Fetch Contact".to_owned()),
                story_title: Some("Fetch Contact".to_owned()),
                operation: None,
            }])
            .declarative_admin(AdminDeclarativeSurface {
                pages: vec![AdminDeclarativePage {
                    name: "contacts".to_owned(),
                    label: "Contacts".to_owned(),
                    sections: vec![AdminDeclarativeSection {
                        name: "contacts".to_owned(),
                        label: "Contacts".to_owned(),
                        component: AdminDeclarativeComponent::EntityTable {
                            entity: "contacts".to_owned(),
                        },
                    }],
                }],
                actions: vec![AdminAction {
                    name: "sync_contacts".to_owned(),
                    label: "Sync Contacts".to_owned(),
                    capability: "remote_crm.contacts.sync".to_owned(),
                    input_schema: None,
                    confirmation: None,
                    danger_level: AdminActionDangerLevel::Low,
                    operation: None,
                }],
                fallback_schema: Some(AdminSchema {
                    entities: vec![crate::EntitySchema {
                        name: "contacts".to_owned(),
                        label: "Contacts".to_owned(),
                        fields: vec![],
                        read_capability: "remote_crm.contacts.read".to_owned(),
                    }],
                }),
            })
            .build();

        let lints = lint_module_manifest(&manifest);

        assert!(lints.iter().any(|lint| {
            lint.severity == ModuleManifestLintSeverity::Warning
                && lint.subject == "capability.reference.http_route.GET /contacts/{id}"
                && lint.message == "Capability reference is not declared by the module."
        }));
        assert!(lints.iter().any(|lint| {
            lint.severity == ModuleManifestLintSeverity::Warning
                && lint.subject == "capability.reference.admin.declarative.action.sync_contacts"
                && lint.message == "Capability reference is not declared by the module."
        }));
        assert!(lints.iter().any(|lint| {
            lint.severity == ModuleManifestLintSeverity::Warning
                && lint.subject == "capability.reference.admin.declarative.fallback_schema.contacts"
                && lint.message == "Capability reference is not declared by the module."
        }));
    }

    #[test]
    fn manifest_lint_catalog_covers_current_subjects() {
        let schema = AdminSchema {
            entities: vec![crate::EntitySchema {
                name: "contacts".to_owned(),
                label: "Contacts".to_owned(),
                fields: vec![],
                read_capability: "".to_owned(),
            }],
        };
        let manifest = ModuleManifest::builder("")
            .capabilities(vec!["RemoteCRM Contacts Read".to_owned()])
            .http_routes(vec![
                ModuleHttpRoute {
                    method: ModuleHttpMethod::Get,
                    path: "/contacts/{id}".to_owned(),
                    capability: None,
                    display_name: None,
                    story_title: None,
                    operation: None,
                },
                ModuleHttpRoute {
                    method: ModuleHttpMethod::Get,
                    path: "/contacts/{id}".to_owned(),
                    capability: None,
                    display_name: None,
                    story_title: None,
                    operation: None,
                },
            ])
            .embedded_admin(AdminEmbeddedSurface {
                runtime: AdminEmbeddedRuntime::Wasm,
                entry: AdminEmbeddedEntry::Url {
                    url: "http://crm.example.test/admin".to_owned(),
                    allowed_origins: vec![],
                },
                sandbox: AdminSandboxPolicy {
                    allow_scripts: true,
                    allow_forms: false,
                    allow_popups: false,
                    allow_same_origin: false,
                },
                permissions: vec![AdminPermission::ReadEntity {
                    entity: "missing".to_owned(),
                }],
                fallback_schema: Some(schema),
            })
            .runtime(RuntimeSurface {
                functions: vec![RuntimeFunctionDeclaration {
                    name: "remote_crm.sync_contact.v1".to_owned(),
                    version: 1,
                    queue: "".to_owned(),
                    input_schema: Some("remote_crm.sync_contact.input.v1".to_owned()),
                    retry_policy: Some(RuntimeRetryPolicyDeclaration {
                        max_attempts: 0,
                        initial_delay_ms: 1000,
                    }),
                    operation: None,
                }],
                schedules: vec![ScheduledFunctionDeclaration {
                    name: "sync_contacts_hourly".to_owned(),
                    function_name: "remote_crm.missing.v1".to_owned(),
                    cron: "bad cron".to_owned(),
                    input: serde_json::json!({}),
                }],
                workflows: vec![],
            })
            .lifecycle(LifecycleSurface {
                startup_checks: vec![LifecycleStartupCheckDeclaration {
                    name: "missing function".to_owned(),
                    required: true,
                    check: LifecycleStartupCheckKind::FunctionRegistered {
                        function_name: "remote_crm.missing.v1".to_owned(),
                    },
                }],
                activation_jobs: vec![LifecycleActivationJobDeclaration {
                    name: "missing activation".to_owned(),
                    function_name: "remote_crm.missing.v1".to_owned(),
                    run_policy: LifecycleActivationRunPolicy::EveryStartup,
                    input: serde_json::json!({}),
                    required: true,
                }],
            })
            .console(vec![ConsoleSurface {
                name: "contacts".to_owned(),
                label: "Contacts".to_owned(),
                route: "/remote-crm/contacts".to_owned(),
                presentation: ConsoleSurfacePresentation::Isolated {
                    entry: "remoteCrmConsoleModule".to_owned(),

                    bridge_protocol: CONSOLE_BRIDGE_PROTOCOL.to_owned(),
                },
                icon: None,
                required_capabilities: Vec::new(),
                navigation: Some(crate::ConsoleNavigation {
                    workspace: crate::ConsoleWorkspaceRef {
                        id: "system".to_owned(),
                        label: "System".to_owned(),
                        icon: None,
                    },
                    group: None,
                    order: None,
                }),
            }])
            .build();

        let catalog: Vec<_> = lint_module_manifest(&manifest)
            .into_iter()
            .map(|lint| (lint.severity, lint.subject))
            .collect();

        assert_eq!(
            catalog,
            vec![
                (
                    ModuleManifestLintSeverity::Error,
                    "module.module_id".to_owned(),
                ),
                (
                    ModuleManifestLintSeverity::Warning,
                    "capability RemoteCRM Contacts Read".to_owned(),
                ),
                (
                    ModuleManifestLintSeverity::Error,
                    "GET /contacts/{id}".to_owned(),
                ),
                (
                    ModuleManifestLintSeverity::Warning,
                    "GET /contacts/{id}".to_owned(),
                ),
                (
                    ModuleManifestLintSeverity::Warning,
                    "GET /contacts/{id}".to_owned(),
                ),
                (
                    ModuleManifestLintSeverity::Warning,
                    "GET /contacts/{id}".to_owned(),
                ),
                (
                    ModuleManifestLintSeverity::Warning,
                    "GET /contacts/{id}".to_owned(),
                ),
                (
                    ModuleManifestLintSeverity::Warning,
                    "admin.embedded.runtime".to_owned(),
                ),
                (
                    ModuleManifestLintSeverity::Warning,
                    "admin.embedded.entry.url".to_owned(),
                ),
                (
                    ModuleManifestLintSeverity::Warning,
                    "admin.embedded.entry.allowed_origins".to_owned(),
                ),
                (
                    ModuleManifestLintSeverity::Warning,
                    "admin.embedded.fallback_schema.contacts".to_owned(),
                ),
                (
                    ModuleManifestLintSeverity::Warning,
                    "admin.embedded.permission.missing".to_owned(),
                ),
                (
                    ModuleManifestLintSeverity::Error,
                    "lifecycle.startup_check.function_registered.remote_crm.missing.v1".to_owned(),
                ),
                (
                    ModuleManifestLintSeverity::Error,
                    "lifecycle.activation_job.missing activation".to_owned(),
                ),
                (
                    ModuleManifestLintSeverity::Warning,
                    "console.surface.contacts.navigation.workspace.id".to_owned(),
                ),
                (
                    ModuleManifestLintSeverity::Warning,
                    "runtime.function.remote_crm.sync_contact.v1".to_owned(),
                ),
                (
                    ModuleManifestLintSeverity::Warning,
                    "runtime.function.remote_crm.sync_contact.v1.input_schema".to_owned(),
                ),
                (
                    ModuleManifestLintSeverity::Warning,
                    "runtime.function.remote_crm.sync_contact.v1.retry_policy".to_owned(),
                ),
                (
                    ModuleManifestLintSeverity::Error,
                    "runtime.schedule.sync_contacts_hourly.cron".to_owned(),
                ),
                (
                    ModuleManifestLintSeverity::Error,
                    "runtime.schedule.sync_contacts_hourly".to_owned(),
                ),
            ],
        );
    }
}
