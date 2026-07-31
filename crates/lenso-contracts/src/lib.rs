//! Public module-authoring contracts for the Lenso backend framework.
//!
//! This crate is deliberately smaller than the backend workspace. It exposes
//! stable manifest declarations and lint helpers that module authors can use
//! without depending on internal `platform-*` crates.

mod admin;
mod admin_schema;
mod catalog;
mod console;
mod cron;
mod events;
mod http;
mod lifecycle;
mod manifest;
mod module_release;
pub mod operation;
mod runtime;
mod schema;
mod story_display;

pub use admin::{
    AdminAction, AdminActionConfirmation, AdminActionDangerLevel, AdminActionInputField,
    AdminActionInputSchema, AdminDeclarativeComponent, AdminDeclarativePage,
    AdminDeclarativeSection, AdminDeclarativeSurface, AdminEmbeddedEntry, AdminEmbeddedRuntime,
    AdminEmbeddedSurface, AdminMetricBinding, AdminPermission, AdminSandboxPolicy, AdminSurface,
};
pub use admin_schema::{AdminSchema, EntitySchema, FieldSchema, FieldType};
pub use catalog::*;
pub use console::{
    CONSOLE_BRIDGE_PROTOCOL, ConsoleActionInputBinding, ConsoleActionInputValue,
    ConsoleContribution, ConsoleContributionAction, ConsoleContributionKind, ConsoleNavigation,
    ConsoleNavigationGroup, ConsolePermissionGrant, ConsolePermissionRequest, ConsoleSlot,
    ConsoleSlotContext, ConsoleSlotContextField, ConsoleSlotContextFieldType, ConsoleSurface,
    ConsoleSurfacePresentation, ConsoleWorkspaceRef,
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
    MODULE_MANIFEST_PROTOCOL, ModuleCapabilityReference, ModuleConfigActivation,
    ModuleConfigContract, ModuleConfigField, ModuleConfigFieldType, ModuleConfigMutability,
    ModuleConfigScope, ModuleConfigValidation, ModuleManifest, ModuleManifestBuilder,
    ModuleManifestLint, ModuleManifestLintSeverity, ModuleMigrationActivation,
    ModuleMigrationDeclaration, ModuleRequirement, lint_module_manifest,
    lint_module_manifest_parts, module_capability_references,
};
pub use module_release::{
    ArtifactReference, AttestationReference, ConsoleUiArtifact, ConsoleUiArtifactEntry,
    ConsoleUiArtifactFormat, LinkedModuleDelivery, MODULE_RELEASE_PROTOCOL,
    ModuleCompatibilityDeclaration, ModuleContractIssue, ModuleDelivery, ModuleRelease,
    ServiceModuleDelivery, ServiceResponsibilityProfile, canonical_json, digest_json,
};
pub use operation::{
    ServiceOperationIdempotency, ServiceOperationMetadata, ServiceOperationSafeProbe,
};
pub use runtime::{
    RuntimeFunctionDeclaration, RuntimeRetryPolicyDeclaration, RuntimeSurface,
    ScheduledFunctionDeclaration, WORKFLOW_COMPATIBILITY_PROTOCOL, WORKFLOW_DEFINITION_PROTOCOL,
    WorkflowCompatibilityCategory, WorkflowCompatibilityReason, WorkflowCompatibilityResult,
    WorkflowCompensationDeclaration, WorkflowDataContract, WorkflowDefinition,
    WorkflowDefinitionReference, WorkflowRetryPolicyDeclaration, WorkflowStepDeclaration,
    evaluate_workflow_compatibility, workflow_compatibility_artifact, workflow_definition_schema,
};
pub use schema::{module_manifest_schema, module_release_schema};
pub use story_display::{StoryDisplayDescriptor, StoryDisplaySource};
