//! A module's pure-data contract: serializable metadata describable without
//! behavior. Owned + serde so every loading source produces the same shape.

use crate::admin::{
    AdminDeclarativeComponent, AdminDeclarativeSurface, AdminEmbeddedEntry, AdminEmbeddedRuntime,
    AdminEmbeddedSurface, AdminPermission, AdminSurface,
};
use crate::admin_schema::AdminSchema;
use crate::console::ConsoleSurface;
use crate::events::{EventHandlerDeclaration, EventSurface};
use crate::http::{ModuleHttpMethod, ModuleHttpRoute, lint_module_http_routes};
use crate::lifecycle::{
    LifecycleActivationJobDeclaration, LifecycleStartupCheckDeclaration, LifecycleStartupCheckKind,
    LifecycleSurface,
};
use crate::module::ModuleSource;
use crate::runtime::{RuntimeFunctionDeclaration, RuntimeSurface};
use platform_core::StoryDisplayDescriptor;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use utoipa::ToSchema;

/// The serializable metadata a module exposes. Runtime config is deliberately
/// NOT here — it stays an internal `&'static` field on [`crate::Module`]
/// because the config registry needs the real (non-serde) `RuntimeConfigType`
/// to validate. Only round-trippable fields belong here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ModuleManifest {
    /// Stable module name, e.g. `"identity"`.
    pub name: String,

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

    /// Declared Runtime Console surfaces provided by trusted frontend packages.
    #[serde(default)]
    pub console: Vec<ConsoleSurface>,

    /// RESERVED SEAM — capabilities the module declares (perms/tenancy).
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl ModuleManifest {
    /// Start building a manifest for `name`.
    #[must_use]
    pub fn builder(name: impl Into<String>) -> ModuleManifestBuilder {
        ModuleManifestBuilder {
            manifest: ModuleManifest {
                name: name.into(),
                story_display: Vec::new(),
                admin: None,
                http_routes: Vec::new(),
                runtime: None,
                events: None,
                lifecycle: None,
                console: Vec::new(),
                capabilities: Vec::new(),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModuleManifestLintSeverity {
    Ok,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
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

pub fn lint_module_manifest(
    source: ModuleSource,
    manifest: &ModuleManifest,
) -> Vec<ModuleManifestLint> {
    lint_module_manifest_parts(
        source,
        &manifest.name,
        manifest.admin.as_ref(),
        &manifest.http_routes,
        manifest.runtime.as_ref(),
        manifest.events.as_ref(),
        manifest.lifecycle.as_ref(),
        &manifest.console,
        &manifest.capabilities,
    )
}

pub fn lint_module_manifest_parts(
    source: ModuleSource,
    name: &str,
    admin: Option<&AdminSurface>,
    http_routes: &[ModuleHttpRoute],
    runtime: Option<&RuntimeSurface>,
    events: Option<&EventSurface>,
    lifecycle: Option<&LifecycleSurface>,
    console: &[ConsoleSurface],
    capabilities: &[String],
) -> Vec<ModuleManifestLint> {
    let mut lints = Vec::new();

    if !present(name) {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Error,
            subject: "module.name".to_owned(),
            message: "Missing module manifest name.".to_owned(),
            suggestion: "Set ModuleManifest.name to the stable module identifier.".to_owned(),
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

    for route_lint in lint_module_http_routes(source, http_routes) {
        lints.push(ModuleManifestLint {
            severity: match route_lint.severity {
                crate::ModuleRouteLintSeverity::Ok => ModuleManifestLintSeverity::Ok,
                crate::ModuleRouteLintSeverity::Warning => ModuleManifestLintSeverity::Warning,
                crate::ModuleRouteLintSeverity::Error => ModuleManifestLintSeverity::Error,
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
        capabilities,
        &mut lints,
    );

    if let Some(admin) = admin {
        lint_admin_surface(admin, &mut lints);
    }
    let mut runtime_lints = Vec::new();
    if let Some(runtime) = runtime {
        lint_runtime_surface(runtime, &mut runtime_lints);
    }
    if let Some(events) = events {
        lint_event_surface(events, &mut lints);
    }
    if let Some(lifecycle) = lifecycle {
        lint_lifecycle_surface(lifecycle, runtime, capabilities, &mut lints);
    }
    lint_console_surfaces(console, &mut lints);
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

    references
}

fn lint_capability_references(
    admin: Option<&AdminSurface>,
    http_routes: &[ModuleHttpRoute],
    lifecycle: Option<&LifecycleSurface>,
    console: &[ConsoleSurface],
    capabilities: &[String],
    lints: &mut Vec<ModuleManifestLint>,
) {
    let declared = capabilities
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();

    for reference in module_capability_references(admin, http_routes, lifecycle, console) {
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

fn lint_runtime_surface(runtime: &RuntimeSurface, lints: &mut Vec<ModuleManifestLint>) {
    if runtime.functions.is_empty() {
        lints.push(ModuleManifestLint {
            severity: ModuleManifestLintSeverity::Warning,
            subject: "runtime.functions".to_owned(),
            message: "Runtime surface declares no functions.".to_owned(),
            suggestion: "Add at least one function declaration or omit the runtime surface."
                .to_owned(),
        });
        return;
    }

    let mut names = HashSet::new();
    for function in &runtime.functions {
        lint_runtime_function(function, &mut names, lints);
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

        if !valid_console_package_name(&surface.package.name) {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Warning,
                subject: format!("{subject}.package"),
                message: "Console surface package should be an npm package name.".to_owned(),
                suggestion: "Use a build-time package name such as @lenso/story-console."
                    .to_owned(),
            });
        }

        if !present(&surface.package.export) {
            lints.push(ModuleManifestLint {
                severity: ModuleManifestLintSeverity::Warning,
                subject: format!("{subject}.package.export"),
                message: "Console surface package export is missing.".to_owned(),
                suggestion: "Set the named export registered by the Runtime Console build."
                    .to_owned(),
            });
        }

        if let Some(navigation) = &surface.navigation {
            lint_console_navigation(&subject, navigation, lints);
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
            if surface.pages.is_empty() {
                lints.push(ModuleManifestLint {
                    severity: ModuleManifestLintSeverity::Warning,
                    subject: "admin.declarative.pages".to_owned(),
                    message: "Declarative admin surface declares no pages.".to_owned(),
                    suggestion: "Add at least one page or omit the declarative admin surface."
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

fn valid_console_navigation_id(value: &str) -> bool {
    valid_console_surface_name(value)
}

fn valid_console_package_name(value: &str) -> bool {
    present(value)
        && !value.contains(' ')
        && (value.starts_with('@') || value.chars().any(|character| character == '-'))
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
    /// Attach console story-display metadata.
    #[must_use]
    pub fn story_display(mut self, story_display: Vec<StoryDisplayDescriptor>) -> Self {
        self.manifest.story_display = story_display;
        self
    }

    /// Attach declared capabilities.
    #[must_use]
    pub fn capabilities(mut self, capabilities: Vec<String>) -> Self {
        self.manifest.capabilities = capabilities;
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

    /// Attach trusted Runtime Console frontend surface declarations.
    #[must_use]
    pub fn console(mut self, console: Vec<ConsoleSurface>) -> Self {
        self.manifest.console = console;
        self
    }

    /// Finish building.
    #[must_use]
    pub fn build(self) -> ModuleManifest {
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
        ConsoleArea, ConsolePackage, ConsoleSurface, EventHandlerDeclaration, EventSurface,
    };
    use crate::{
        LifecycleActivationJobDeclaration, LifecycleActivationRunPolicy,
        LifecycleStartupCheckDeclaration, LifecycleStartupCheckKind, LifecycleSurface,
    };
    use crate::{ModuleHttpMethod, ModuleHttpRoute};
    use crate::{RuntimeFunctionDeclaration, RuntimeRetryPolicyDeclaration, RuntimeSurface};
    use platform_core::{StoryDisplayDescriptor, StoryDisplaySource};

    #[test]
    fn manifest_round_trips_through_json() {
        let manifest = ModuleManifest::builder("identity")
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
        let manifest = ModuleManifest::builder("platform-story")
            .console(vec![ConsoleSurface {
                name: "stories".to_owned(),
                label: "Stories".to_owned(),
                area: ConsoleArea::Runtime,
                route: "/runtime/stories".to_owned(),
                package: ConsolePackage {
                    name: "@lenso/story-console".to_owned(),
                    export: "storyConsoleModule".to_owned(),
                },
                icon: Some("workflow".to_owned()),
                required_capabilities: vec!["runtime.stories.read".to_owned()],
                navigation: None,
            }])
            .capabilities(vec!["runtime.stories.read".to_owned()])
            .build();

        let json = serde_json::to_string(&manifest).expect("serialize");
        assert!(json.contains(r#""console""#), "got {json}");
        assert!(json.contains(r#""area":"runtime""#), "got {json}");

        let back: ModuleManifest = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(manifest, back);
    }

    #[test]
    fn console_surface_navigation_round_trips() {
        let surface = ConsoleSurface {
            name: "contacts".to_owned(),
            label: "Contacts".to_owned(),
            area: ConsoleArea::Data,
            route: "/crm/contacts".to_owned(),
            package: crate::ConsolePackage {
                name: "@lenso/crm-console".to_owned(),
                export: "crmConsoleModule".to_owned(),
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
        let manifest = ModuleManifest::builder("crm")
            .capabilities(vec!["crm.contacts.read".to_owned()])
            .console(vec![ConsoleSurface {
                name: "contacts".to_owned(),
                label: "Contacts".to_owned(),
                area: ConsoleArea::Data,
                route: "/crm/contacts".to_owned(),
                package: crate::ConsolePackage {
                    name: "@lenso/crm-console".to_owned(),
                    export: "crmConsoleModule".to_owned(),
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

        let subjects: Vec<_> = lint_module_manifest(ModuleSource::Linked, &manifest)
            .into_iter()
            .map(|lint| lint.subject)
            .collect();

        assert!(
            subjects.contains(&"console.surface.contacts.navigation.workspace.label".to_owned())
        );
    }

    #[test]
    fn console_navigation_lints_reserved_system_workspace() {
        let manifest = ModuleManifest::builder("crm")
            .capabilities(vec!["crm.contacts.read".to_owned()])
            .console(vec![ConsoleSurface {
                name: "contacts".to_owned(),
                label: "Contacts".to_owned(),
                area: ConsoleArea::Data,
                route: "/crm/contacts".to_owned(),
                package: crate::ConsolePackage {
                    name: "@lenso/crm-console".to_owned(),
                    export: "crmConsoleModule".to_owned(),
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

        let lints = lint_module_manifest(ModuleSource::Remote, &manifest);

        assert!(lints.iter().any(|lint| {
            lint.subject == "console.surface.contacts.navigation.workspace.id"
                && lint.severity == ModuleManifestLintSeverity::Warning
                && lint.message
                    == "Console workspace id system is reserved for host-owned surfaces."
        }));
    }

    #[test]
    fn lints_invalid_console_surface_declarations() {
        let manifest = ModuleManifest::builder("platform-story")
            .console(vec![
                ConsoleSurface {
                    name: "stories".to_owned(),
                    label: "Stories".to_owned(),
                    area: ConsoleArea::Runtime,
                    route: "runtime/stories".to_owned(),
                    package: ConsolePackage {
                        name: "story console".to_owned(),
                        export: String::new(),
                    },
                    icon: None,
                    required_capabilities: vec!["runtime.stories.read".to_owned()],
                    navigation: None,
                },
                ConsoleSurface {
                    name: "stories".to_owned(),
                    label: "Stories duplicate".to_owned(),
                    area: ConsoleArea::Runtime,
                    route: "/runtime/stories".to_owned(),
                    package: ConsolePackage {
                        name: "@lenso/story-console".to_owned(),
                        export: "storyConsoleModule".to_owned(),
                    },
                    icon: None,
                    required_capabilities: vec![],
                    navigation: None,
                },
            ])
            .build();

        let lints = lint_module_manifest(ModuleSource::Linked, &manifest);
        let subjects = lints
            .iter()
            .map(|lint| lint.subject.as_str())
            .collect::<Vec<_>>();

        assert!(subjects.contains(&"console.surface.stories.route"));
        assert!(subjects.contains(&"console.surface.stories.package"));
        assert!(subjects.contains(&"console.surface.stories.package.export"));
        assert!(subjects.contains(&"capability.reference.console.surface.stories"));
        assert!(lints.iter().any(|lint| {
            lint.subject == "console.surface.stories"
                && lint.message == "Duplicate console surface declaration."
        }));
    }

    #[test]
    fn empty_admin_is_skipped_in_json() {
        let manifest = ModuleManifest::builder("notifications").build();
        let json = serde_json::to_string(&manifest).expect("serialize");
        assert!(
            !json.contains("admin"),
            "admin: None must be skipped, got {json}"
        );
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
        let manifest = ModuleManifest::builder("identity").admin(schema).build();
        let json = serde_json::to_string(&manifest).expect("serialize");
        assert!(json.contains(r#""kind":"schema""#), "got {json}");
    }

    #[test]
    fn manifest_with_declarative_admin_serializes_kind() {
        use crate::admin::AdminDeclarativeSurface;

        let manifest = ModuleManifest::builder("remote-crm")
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

        let manifest = ModuleManifest::builder("remote-crm")
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
        let manifest = ModuleManifest::builder("remote-crm")
            .http_routes(vec![
                ModuleHttpRoute {
                    method: ModuleHttpMethod::Get,
                    path: "/contacts".to_owned(),
                    capability: Some("remote_crm.contacts.read".to_owned()),
                    display_name: Some("List Contacts".to_owned()),
                    story_title: Some("List Contacts".to_owned()),
                },
                ModuleHttpRoute {
                    method: ModuleHttpMethod::Post,
                    path: "/contacts".to_owned(),
                    capability: Some("remote_crm.contacts.write".to_owned()),
                    display_name: None,
                    story_title: None,
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
        let manifest = ModuleManifest::builder("remote-crm")
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
                }],
            })
            .build();

        let json = serde_json::to_string(&manifest).expect("serialize");

        assert!(json.contains(r#""runtime""#), "got {json}");
        assert!(
            json.contains(r#""name":"remote_crm.sync_contact.v1""#),
            "got {json}"
        );
        assert!(json.contains(r#""queue":"remote-crm""#), "got {json}");
        let back: ModuleManifest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(manifest, back);
    }

    #[test]
    fn manifest_with_event_handlers_round_trips_through_json() {
        let manifest = ModuleManifest::builder("remote-crm")
            .events(EventSurface {
                handlers: vec![EventHandlerDeclaration {
                    name: "sync_contact_on_user_registered".to_owned(),
                    event_name: "identity.user_registered.v1".to_owned(),
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
        let manifest = ModuleManifest::builder("remote-crm")
            .capabilities(vec!["RemoteCRM Contacts Read".to_owned()])
            .build();

        assert!(
            lint_module_manifest(ModuleSource::Remote, &manifest)
                .iter()
                .any(|lint| lint.subject == "capability RemoteCRM Contacts Read"
                    && lint.severity == ModuleManifestLintSeverity::Warning)
        );
    }

    #[test]
    fn manifest_lint_warns_for_unknown_declarative_fallback_entities() {
        let manifest = ModuleManifest::builder("remote-crm")
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
            lint_module_manifest(ModuleSource::Remote, &manifest)
                .iter()
                .any(|lint| lint.subject == "admin.declarative.section.missing"
                    && lint.severity == ModuleManifestLintSeverity::Warning)
        );
    }

    #[test]
    fn manifest_lint_warns_for_embedded_origin_policy() {
        let manifest = ModuleManifest::builder("remote-crm")
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

        let lints = lint_module_manifest(ModuleSource::Remote, &manifest);

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
        let manifest = ModuleManifest::builder("remote-crm")
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
                    },
                    RuntimeFunctionDeclaration {
                        name: "remote_crm.sync_contact.v1".to_owned(),
                        version: 1,
                        queue: "remote-crm".to_owned(),
                        input_schema: Some("remote_crm.sync_contact.input.v1".to_owned()),
                        retry_policy: None,
                    },
                    RuntimeFunctionDeclaration {
                        name: "remote_crm.sync_contact.v1".to_owned(),
                        version: 1,
                        queue: "remote-crm".to_owned(),
                        input_schema: Some("remote_crm.sync_contact.v1".to_owned()),
                        retry_policy: None,
                    },
                ],
            })
            .build();

        let lints = lint_module_manifest(ModuleSource::Remote, &manifest);

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
        let manifest = ModuleManifest::builder("remote-crm")
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
                }],
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
        let manifest = ModuleManifest::builder("remote-crm")
            .runtime(RuntimeSurface { functions: vec![] })
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

        let lints = lint_module_manifest(ModuleSource::Remote, &manifest);

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
        let manifest = ModuleManifest::builder("remote-crm")
            .lifecycle(LifecycleSurface {
                startup_checks: vec![],
                activation_jobs: vec![],
            })
            .build();

        let lints = lint_module_manifest(ModuleSource::Remote, &manifest);

        assert!(lints.iter().any(|lint| {
            lint.subject == "lifecycle"
                && lint.severity == ModuleManifestLintSeverity::Warning
                && lint.message
                    == "Lifecycle surface declares no startup checks or activation jobs."
        }));
    }

    #[test]
    fn manifest_lint_warns_for_activation_job_missing_name() {
        let manifest = ModuleManifest::builder("remote-crm")
            .runtime(RuntimeSurface {
                functions: vec![RuntimeFunctionDeclaration {
                    name: "remote_crm.warm_contact_cache.v1".to_owned(),
                    version: 1,
                    queue: "remote-crm".to_owned(),
                    input_schema: Some("remote_crm.warm_contact_cache.v1".to_owned()),
                    retry_policy: None,
                }],
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

        let lints = lint_module_manifest(ModuleSource::Remote, &manifest);

        assert!(lints.iter().any(|lint| {
            lint.subject == "lifecycle.activation_job"
                && lint.severity == ModuleManifestLintSeverity::Warning
                && lint.message == "Lifecycle activation job is missing a name."
        }));
    }

    #[test]
    fn manifest_lint_errors_for_activation_job_missing_function_name() {
        let manifest = ModuleManifest::builder("remote-crm")
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

        let lints = lint_module_manifest(ModuleSource::Remote, &manifest);

        assert!(lints.iter().any(|lint| {
            lint.subject == "lifecycle.activation_job"
                && lint.severity == ModuleManifestLintSeverity::Error
                && lint.message == "Lifecycle activation job is missing a function name."
        }));
    }

    #[test]
    fn manifest_lint_warns_for_undeclared_capability_references() {
        use crate::admin::{AdminAction, AdminActionDangerLevel};

        let manifest = ModuleManifest::builder("remote-crm")
            .capabilities(vec!["remote_crm.contacts.write".to_owned()])
            .http_routes(vec![ModuleHttpRoute {
                method: ModuleHttpMethod::Get,
                path: "/contacts/{id}".to_owned(),
                capability: Some("remote_crm.contacts.read".to_owned()),
                display_name: Some("Fetch Contact".to_owned()),
                story_title: Some("Fetch Contact".to_owned()),
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

        let lints = lint_module_manifest(ModuleSource::Remote, &manifest);

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
                },
                ModuleHttpRoute {
                    method: ModuleHttpMethod::Get,
                    path: "/contacts/{id}".to_owned(),
                    capability: None,
                    display_name: None,
                    story_title: None,
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
                }],
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
                area: ConsoleArea::Data,
                route: "/remote-crm/contacts".to_owned(),
                package: ConsolePackage {
                    name: "@lenso/remote-crm-console".to_owned(),
                    export: "remoteCrmConsoleModule".to_owned(),
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

        let catalog: Vec<_> = lint_module_manifest(ModuleSource::Remote, &manifest)
            .into_iter()
            .map(|lint| (lint.severity, lint.subject))
            .collect();

        assert_eq!(
            catalog,
            vec![
                (ModuleManifestLintSeverity::Error, "module.name".to_owned()),
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
            ],
        );
    }
}
