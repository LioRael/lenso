//! Public module-authoring contracts for the Lenso backend framework.
//!
//! This crate is deliberately smaller than the backend workspace. It exposes
//! stable manifest declarations and lint helpers that module authors can use
//! without depending on internal `platform-*` crates.

mod admin;
mod admin_schema;
mod console;
mod cron;
mod events;
mod http;
mod lifecycle;
mod manifest;
mod module_source;
pub mod operation;
mod runtime;
mod story_display;

pub use admin::{
    AdminAction, AdminActionConfirmation, AdminActionDangerLevel, AdminActionInputField,
    AdminActionInputSchema, AdminDeclarativeComponent, AdminDeclarativePage,
    AdminDeclarativeSection, AdminDeclarativeSurface, AdminEmbeddedEntry, AdminEmbeddedRuntime,
    AdminEmbeddedSurface, AdminMetricBinding, AdminPermission, AdminSandboxPolicy, AdminSurface,
};
pub use admin_schema::{AdminSchema, EntitySchema, FieldSchema, FieldType};
pub use console::{
    ConsoleActionInputBinding, ConsoleActionInputValue, ConsoleArea, ConsoleContribution,
    ConsoleContributionAction, ConsoleContributionKind, ConsoleNavigation, ConsoleNavigationGroup,
    ConsolePackage, ConsoleSlot, ConsoleSlotContext, ConsoleSlotContextField,
    ConsoleSlotContextFieldType, ConsoleSurface, ConsoleWorkspaceRef,
};
pub use cron::{CronParseError, CronSchedule, validate_cron_expression};
pub use events::{EventHandlerDeclaration, EventSurface};
pub use http::{
    ModuleHttpMethod, ModuleHttpRoute, ModuleRouteLint, ModuleRouteLintSeverity,
    lint_module_http_routes,
};
pub use lifecycle::{
    LifecycleActivationJobDeclaration, LifecycleActivationRunPolicy,
    LifecycleStartupCheckDeclaration, LifecycleStartupCheckKind, LifecycleSurface,
};
pub use manifest::{
    ModuleCapabilityReference, ModuleManifest, ModuleManifestBuilder, ModuleManifestLint,
    ModuleManifestLintSeverity, lint_module_manifest, lint_module_manifest_parts,
    module_capability_references,
};
pub use module_source::ModuleSource;
pub use operation::{
    ServiceOperationIdempotency, ServiceOperationMetadata, ServiceOperationSafeProbe,
};
pub use runtime::{
    RuntimeFunctionDeclaration, RuntimeRetryPolicyDeclaration, RuntimeSurface,
    ScheduledFunctionDeclaration, WORKFLOW_DEFINITION_PROTOCOL, WorkflowCompensationDeclaration,
    WorkflowDataContract, WorkflowDefinition, WorkflowRetryPolicyDeclaration,
    WorkflowStepDeclaration, workflow_definition_schema,
};
pub use story_display::{StoryDisplayDescriptor, StoryDisplaySource};
