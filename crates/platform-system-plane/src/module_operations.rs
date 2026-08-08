//! Managed Service Module inventory, contribution, and descriptor-bound config operations.

use axum::{Extension, Json};
use lenso_contracts::{
    AdminSurface, ConsoleActionInputValue, ConsoleContributionAction, ModuleConfigMutability,
    ModuleDelivery, ModuleHttpMethod, ModuleRelease, digest_json, validate_module_config_value,
};
use lenso_service::system_plane::{
    ActionContributionResolution, ActionContributionResolutionRequest, CapabilityAdvertisement,
    MODULE_OPERATIONS_FEATURE_CONFIG_READ, MODULE_OPERATIONS_FEATURE_CONFIG_WRITE,
    MODULE_OPERATIONS_FEATURE_CONTRIBUTIONS_RESOLVE, MODULE_OPERATIONS_FEATURE_INVENTORY_READ,
    MODULE_OPERATIONS_PATH, MODULE_OPERATIONS_PROTOCOL, ManagedServiceContext,
    ModuleConfigAuditEvidence, ModuleConfigReadRequest, ModuleConfigReadResponse,
    ModuleConfigValue, ModuleConfigWriteRequest, ModuleConfigWriteResponse,
    ModuleInventoryConsoleUi, ModuleInventoryDelivery, ModuleInventoryModule,
    ModuleInventoryRequest, ModuleInventoryRoute, ModuleInventorySnapshot, ModuleRuntimeStatus,
    ResolvedActionContribution, module_operations_schema_digest,
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, RwLock},
};
use utoipa_axum::{router::OpenApiRouter, routes};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleOperationsProviderError {
    pub message: String,
}

impl std::fmt::Display for ModuleOperationsProviderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ModuleOperationsProviderError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleOperationsErrorCode {
    InvalidRequest,
    CapabilityDenied,
    NotFound,
    Conflict,
    StoreUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleOperationsError {
    pub code: ModuleOperationsErrorCode,
    pub message: String,
}

#[derive(Debug, Default)]
struct ModuleOperationsState {
    config: BTreeMap<(String, String), Value>,
    audit: Vec<ModuleConfigAuditEvidence>,
    target_revision: String,
    next_sequence: u64,
}

#[derive(Debug, Clone)]
pub struct ModuleOperationsProvider {
    service_id: String,
    service_principal: String,
    service_revision: String,
    releases: Arc<BTreeMap<String, ModuleRelease>>,
    statuses: Arc<RwLock<BTreeMap<String, ModuleRuntimeStatus>>>,
    state: Arc<RwLock<ModuleOperationsState>>,
}

impl ModuleOperationsProvider {
    /// Builds the Service-owned Module catalog. Every release is validated
    /// before it can be advertised or queried through the System Plane.
    pub fn new(
        service_id: impl Into<String>,
        service_principal: impl Into<String>,
        service_revision: impl Into<String>,
        releases: impl IntoIterator<Item = ModuleRelease>,
    ) -> Result<Self, ModuleOperationsProviderError> {
        let service_id = service_id.into();
        let service_principal = service_principal.into();
        let service_revision = service_revision.into();
        if service_id.trim().is_empty()
            || service_principal.trim().is_empty()
            || service_revision.trim().is_empty()
        {
            return Err(provider_error(
                "Module Operations provider requires Service identity, principal, and revision",
            ));
        }

        let mut catalog = BTreeMap::new();
        for release in releases {
            let issues = release.validate();
            if !issues.is_empty() {
                return Err(provider_error(format!(
                    "Module Release `{}` is invalid: {}",
                    release.module_id,
                    issues
                        .iter()
                        .map(|issue| format!("{}: {}", issue.path, issue.message))
                        .collect::<Vec<_>>()
                        .join("; ")
                )));
            }
            if catalog.insert(release.module_id.clone(), release).is_some() {
                return Err(provider_error(
                    "Module Operations catalog contains a duplicate ModuleId",
                ));
            }
        }

        let target_revision = digest_json(&(&service_id, &service_principal, &service_revision))
            .map_err(|error| {
                provider_error(format!("could not initialize target revision: {error}"))
            })?;
        let mut state = ModuleOperationsState {
            target_revision,
            ..ModuleOperationsState::default()
        };
        state.next_sequence = 1;

        Ok(Self {
            service_id,
            service_principal,
            service_revision,
            releases: Arc::new(catalog),
            statuses: Arc::new(RwLock::new(BTreeMap::new())),
            state: Arc::new(RwLock::new(state)),
        })
    }

    #[must_use]
    pub fn advertisement() -> CapabilityAdvertisement {
        CapabilityAdvertisement {
            contract_id: MODULE_OPERATIONS_PROTOCOL.to_owned(),
            major_version: 1,
            feature_ids: BTreeSet::from([
                MODULE_OPERATIONS_FEATURE_INVENTORY_READ.to_owned(),
                MODULE_OPERATIONS_FEATURE_CONTRIBUTIONS_RESOLVE.to_owned(),
                MODULE_OPERATIONS_FEATURE_CONFIG_READ.to_owned(),
                MODULE_OPERATIONS_FEATURE_CONFIG_WRITE.to_owned(),
            ]),
            schema_digest: module_operations_schema_digest(),
            endpoint: MODULE_OPERATIONS_PATH.to_owned(),
        }
    }

    #[must_use]
    pub fn service_id(&self) -> &str {
        &self.service_id
    }

    #[must_use]
    pub fn service_principal(&self) -> &str {
        &self.service_principal
    }

    #[must_use]
    pub fn service_revision(&self) -> &str {
        &self.service_revision
    }

    /// Seeds one descriptor-declared value for integration tests and Service
    /// bootstrap code. Secret-bearing values are accepted but never returned.
    pub fn set_config_value(
        &self,
        module_id: &str,
        key: &str,
        value: Value,
    ) -> Result<(), ModuleOperationsError> {
        let field = self.config_field(module_id, key)?;
        validate_module_config_value(field, &value).map_err(|message| {
            operation_error(ModuleOperationsErrorCode::InvalidRequest, message)
        })?;
        let mut state = self.state.write().map_err(|_| {
            operation_error(
                ModuleOperationsErrorCode::StoreUnavailable,
                "Module configuration store is unavailable",
            )
        })?;
        state
            .config
            .insert((module_id.to_owned(), key.to_owned()), value);
        Ok(())
    }

    pub fn set_runtime_status(
        &self,
        module_id: &str,
        status: ModuleRuntimeStatus,
    ) -> Result<(), ModuleOperationsError> {
        if !self.releases.contains_key(module_id) {
            return Err(operation_error(
                ModuleOperationsErrorCode::NotFound,
                format!("Module `{module_id}` is not installed"),
            ));
        }
        self.statuses
            .write()
            .map_err(|_| {
                operation_error(
                    ModuleOperationsErrorCode::StoreUnavailable,
                    "Module status store is unavailable",
                )
            })?
            .insert(module_id.to_owned(), status);
        Ok(())
    }

    pub fn inventory(
        &self,
        context: &ManagedServiceContext,
    ) -> Result<ModuleInventorySnapshot, ModuleOperationsError> {
        self.validate_context(context)?;
        let statuses = self.statuses.read().map_err(|_| {
            operation_error(
                ModuleOperationsErrorCode::StoreUnavailable,
                "Module status store is unavailable",
            )
        })?;
        let modules = self
            .releases
            .values()
            .map(|release| inventory_module(release, statuses.get(&release.module_id).copied()))
            .collect::<Result<Vec<_>, _>>()?;
        let snapshot_revision =
            digest_json(&(&self.service_revision, &modules)).map_err(|error| {
                operation_error(
                    ModuleOperationsErrorCode::StoreUnavailable,
                    error.to_string(),
                )
            })?;
        Ok(ModuleInventorySnapshot {
            protocol: MODULE_OPERATIONS_PROTOCOL.to_owned(),
            context: context.clone(),
            service_revision: self.service_revision.clone(),
            snapshot_revision,
            schema_digest: module_operations_schema_digest(),
            modules,
        })
    }

    pub fn resolve_contributions(
        &self,
        request: &ActionContributionResolutionRequest,
    ) -> Result<ActionContributionResolution, ModuleOperationsError> {
        self.validate_context(&request.context)?;
        let slot = self
            .releases
            .values()
            .flat_map(|release| release.manifest.console_slots.iter())
            .find(|slot| slot.id == request.slot && slot.version == request.slot_version)
            .ok_or_else(|| {
                operation_error(
                    ModuleOperationsErrorCode::NotFound,
                    format!(
                        "Console contribution slot `{}` v{} is not installed",
                        request.slot, request.slot_version
                    ),
                )
            })?;
        validate_slot_context(slot, &request.slot_context)?;

        let mut contributions = Vec::new();
        for release in self.releases.values() {
            for contribution in &release.manifest.console_contributions {
                if contribution.target != request.slot
                    || contribution.target_version != request.slot_version
                {
                    continue;
                }
                let action = validate_action_reference(
                    &self.releases,
                    &contribution.action,
                    &request.slot_context,
                    &request.context.capabilities,
                )?;
                let mut required_capabilities = contribution.required_capabilities.clone();
                required_capabilities.sort();
                required_capabilities.dedup();
                require_context_capabilities(
                    &request.context.capabilities,
                    &required_capabilities,
                )?;
                contributions.push(ResolvedActionContribution {
                    contributing_module_id: release.module_id.clone(),
                    target: contribution.target.clone(),
                    target_version: contribution.target_version,
                    label: contribution.label.clone(),
                    action,
                    icon: contribution.icon.clone(),
                    required_capabilities,
                });
            }
        }
        contributions.sort_by(|left, right| {
            (&left.contributing_module_id, &left.label, &left.target).cmp(&(
                &right.contributing_module_id,
                &right.label,
                &right.target,
            ))
        });
        Ok(ActionContributionResolution {
            protocol: MODULE_OPERATIONS_PROTOCOL.to_owned(),
            context: request.context.clone(),
            slot: request.slot.clone(),
            slot_version: request.slot_version,
            contributions,
        })
    }

    pub fn read_config(
        &self,
        request: &ModuleConfigReadRequest,
    ) -> Result<ModuleConfigReadResponse, ModuleOperationsError> {
        self.validate_context(&request.context)?;
        self.require_config_namespace(&request.context, &request.module_id)?;
        let release = self.release(&request.module_id)?;
        let fields = requested_config_fields(&release.manifest.config.fields, &request.keys)?;
        let state = self.state.read().map_err(|_| {
            operation_error(
                ModuleOperationsErrorCode::StoreUnavailable,
                "Module configuration store is unavailable",
            )
        })?;
        let values = fields
            .into_iter()
            .map(|field| {
                if let Some(capability) = field.read_capability.as_deref() {
                    require_context_capabilities(&request.context.capabilities, [capability])?;
                }
                let present = state
                    .config
                    .contains_key(&(request.module_id.clone(), field.key.clone()));
                Ok(ModuleConfigValue {
                    key: field.key.clone(),
                    field_type: field.field_type,
                    scope: field.scope,
                    mutability: field.mutability,
                    activation: field.activation,
                    sensitive: field.sensitive,
                    present,
                    value: (!field.sensitive)
                        .then(|| {
                            state
                                .config
                                .get(&(request.module_id.clone(), field.key.clone()))
                                .cloned()
                        })
                        .flatten(),
                })
            })
            .collect::<Result<Vec<_>, ModuleOperationsError>>()?;
        Ok(ModuleConfigReadResponse {
            protocol: MODULE_OPERATIONS_PROTOCOL.to_owned(),
            context: request.context.clone(),
            module_id: request.module_id.clone(),
            values,
        })
    }

    pub fn write_config(
        &self,
        request: &ModuleConfigWriteRequest,
    ) -> Result<ModuleConfigWriteResponse, ModuleOperationsError> {
        self.validate_context(&request.context)?;
        self.require_config_namespace(&request.context, &request.module_id)?;
        if request.values.is_empty() {
            return Err(operation_error(
                ModuleOperationsErrorCode::InvalidRequest,
                "Module configuration writes require at least one field",
            ));
        }
        let release = self.release(&request.module_id)?;
        let mut fields = BTreeMap::new();
        for field in &release.manifest.config.fields {
            fields.insert(field.key.as_str(), field);
        }
        let mut keys = BTreeSet::new();
        for item in &request.values {
            if !keys.insert(item.key.as_str()) {
                return Err(operation_error(
                    ModuleOperationsErrorCode::InvalidRequest,
                    format!(
                        "Configuration key `{}` was written more than once",
                        item.key
                    ),
                ));
            }
            let field = fields.get(item.key.as_str()).ok_or_else(|| {
                operation_error(
                    ModuleOperationsErrorCode::NotFound,
                    format!(
                        "Configuration field `{}` is not declared by Module `{}`",
                        item.key, request.module_id
                    ),
                )
            })?;
            if field.mutability == ModuleConfigMutability::Static {
                return Err(operation_error(
                    ModuleOperationsErrorCode::Conflict,
                    format!(
                        "Configuration field `{}` is static and cannot be changed at runtime",
                        item.key
                    ),
                ));
            }
            if let Some(capability) = field.write_capability.as_deref() {
                require_context_capabilities(&request.context.capabilities, [capability])?;
            }
            validate_module_config_value(field, &item.value).map_err(|message| {
                operation_error(ModuleOperationsErrorCode::InvalidRequest, message)
            })?;
        }

        let operation_id = digest_json(request).map_err(|error| {
            operation_error(
                ModuleOperationsErrorCode::StoreUnavailable,
                error.to_string(),
            )
        })?;
        let mut state = self.state.write().map_err(|_| {
            operation_error(
                ModuleOperationsErrorCode::StoreUnavailable,
                "Module configuration store is unavailable",
            )
        })?;
        let target_revision_before = state.target_revision.clone();
        let mut evidence = Vec::with_capacity(request.values.len());
        for item in &request.values {
            let old_value_digest = state
                .config
                .get(&(request.module_id.clone(), item.key.clone()))
                .map(digest_value)
                .transpose()
                .map_err(|error| {
                    operation_error(
                        ModuleOperationsErrorCode::StoreUnavailable,
                        error.to_string(),
                    )
                })?;
            state.config.insert(
                (request.module_id.clone(), item.key.clone()),
                item.value.clone(),
            );
            let sequence = state.next_sequence;
            state.next_sequence = state.next_sequence.saturating_add(1);
            let new_value_digest = digest_value(&item.value).map_err(|error| {
                operation_error(
                    ModuleOperationsErrorCode::StoreUnavailable,
                    error.to_string(),
                )
            })?;
            let item_evidence = ModuleConfigAuditEvidence {
                sequence,
                operation_id: operation_id.clone(),
                module_id: request.module_id.clone(),
                key: item.key.clone(),
                sensitive: fields[item.key.as_str()].sensitive,
                old_value_digest,
                new_value_digest,
                recorded_at_unix_ms: now_unix_ms(),
            };
            state.audit.push(item_evidence.clone());
            evidence.push(item_evidence);
        }
        state.target_revision = digest_json(&(
            &target_revision_before,
            &operation_id,
            &request.module_id,
            &evidence
                .iter()
                .map(|item| item.new_value_digest.clone())
                .collect::<Vec<_>>(),
        ))
        .map_err(|error| {
            operation_error(
                ModuleOperationsErrorCode::StoreUnavailable,
                error.to_string(),
            )
        })?;
        Ok(ModuleConfigWriteResponse {
            protocol: MODULE_OPERATIONS_PROTOCOL.to_owned(),
            operation_id,
            context: request.context.clone(),
            module_id: request.module_id.clone(),
            target_revision_before,
            target_revision_after: state.target_revision.clone(),
            authorization_digest: request.context.digest().map_err(|error| {
                operation_error(
                    ModuleOperationsErrorCode::StoreUnavailable,
                    error.to_string(),
                )
            })?,
            evidence,
        })
    }

    fn release(&self, module_id: &str) -> Result<&ModuleRelease, ModuleOperationsError> {
        self.releases.get(module_id).ok_or_else(|| {
            operation_error(
                ModuleOperationsErrorCode::NotFound,
                format!("Module `{module_id}` is not installed"),
            )
        })
    }

    fn config_field(
        &self,
        module_id: &str,
        key: &str,
    ) -> Result<&lenso_contracts::ModuleConfigField, ModuleOperationsError> {
        self.release(module_id)?
            .manifest
            .config
            .fields
            .iter()
            .find(|field| field.key == key)
            .ok_or_else(|| {
                operation_error(
                    ModuleOperationsErrorCode::NotFound,
                    format!("Configuration field `{key}` is not declared by Module `{module_id}`"),
                )
            })
    }

    fn validate_context(
        &self,
        context: &ManagedServiceContext,
    ) -> Result<(), ModuleOperationsError> {
        if context.service_id != self.service_id
            || context.target_service_principal != self.service_principal
            || context.system_id.trim().is_empty()
            || context.environment_id.trim().is_empty()
            || context.caller_module_id.trim().is_empty()
            || context.delegated_actor_subject.trim().is_empty()
            || !canonical_digest(&context.delegated_authority_digest)
        {
            return Err(operation_error(
                ModuleOperationsErrorCode::InvalidRequest,
                "Managed Service context does not identify this Service or its delegated authority",
            ));
        }
        Ok(())
    }

    fn require_config_namespace(
        &self,
        context: &ManagedServiceContext,
        module_id: &str,
    ) -> Result<(), ModuleOperationsError> {
        if context.caller_module_id != module_id {
            return Err(operation_error(
                ModuleOperationsErrorCode::CapabilityDenied,
                format!(
                    "Module configuration is restricted to the calling Module namespace `{}`",
                    context.caller_module_id
                ),
            ));
        }
        Ok(())
    }
}

#[must_use]
pub fn module_operations_router<S>(
    provider: Option<Arc<ModuleOperationsProvider>>,
) -> OpenApiRouter<S>
where
    S: Clone + Send + Sync + 'static,
{
    OpenApiRouter::new()
        .routes(routes!(module_inventory))
        .routes(routes!(resolve_action_contributions))
        .routes(routes!(read_module_config))
        .routes(routes!(write_module_config))
        .layer(Extension(provider))
}

#[utoipa::path(
    post,
    path = "/system-plane/v1/modules",
    request_body = ModuleInventoryRequest,
    responses(
        (status = 200, body = ModuleInventorySnapshot),
        (status = 400, body = crate::SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 401, body = crate::SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 403, body = crate::SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 503, body = crate::SystemPlaneErrorBody, content_type = "application/problem+json")
    ),
    security(("bearer_auth" = [])),
    tag = "system-plane-module-operations"
)]
async fn module_inventory(
    caller: crate::AuthorizedSystemPlaneCaller,
    Extension(provider): Extension<Option<Arc<ModuleOperationsProvider>>>,
    Json(request): Json<ModuleInventoryRequest>,
) -> Result<Json<ModuleInventorySnapshot>, crate::SystemPlaneRejection> {
    let provider = require_provider(provider)?;
    authorize_context(&caller, &request.context)?;
    caller.require_capability(
        MODULE_OPERATIONS_PROTOCOL,
        &module_operations_schema_digest(),
        [MODULE_OPERATIONS_FEATURE_INVENTORY_READ],
    )?;
    provider
        .inventory(&request.context)
        .map(Json)
        .map_err(rejection)
}

#[utoipa::path(
    post,
    path = "/system-plane/v1/modules/action-contributions/resolve",
    request_body = ActionContributionResolutionRequest,
    responses(
        (status = 200, body = ActionContributionResolution),
        (status = 400, body = crate::SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 401, body = crate::SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 403, body = crate::SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 404, body = crate::SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 503, body = crate::SystemPlaneErrorBody, content_type = "application/problem+json")
    ),
    security(("bearer_auth" = [])),
    tag = "system-plane-module-operations"
)]
async fn resolve_action_contributions(
    caller: crate::AuthorizedSystemPlaneCaller,
    Extension(provider): Extension<Option<Arc<ModuleOperationsProvider>>>,
    Json(request): Json<ActionContributionResolutionRequest>,
) -> Result<Json<ActionContributionResolution>, crate::SystemPlaneRejection> {
    let provider = require_provider(provider)?;
    authorize_context(&caller, &request.context)?;
    caller.require_capability(
        MODULE_OPERATIONS_PROTOCOL,
        &module_operations_schema_digest(),
        [MODULE_OPERATIONS_FEATURE_CONTRIBUTIONS_RESOLVE],
    )?;
    provider
        .resolve_contributions(&request)
        .map(Json)
        .map_err(rejection)
}

#[utoipa::path(
    post,
    path = "/system-plane/v1/modules/config/read",
    request_body = ModuleConfigReadRequest,
    responses(
        (status = 200, body = ModuleConfigReadResponse),
        (status = 400, body = crate::SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 401, body = crate::SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 403, body = crate::SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 404, body = crate::SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 503, body = crate::SystemPlaneErrorBody, content_type = "application/problem+json")
    ),
    security(("bearer_auth" = [])),
    tag = "system-plane-module-operations"
)]
async fn read_module_config(
    caller: crate::AuthorizedSystemPlaneCaller,
    Extension(provider): Extension<Option<Arc<ModuleOperationsProvider>>>,
    Json(request): Json<ModuleConfigReadRequest>,
) -> Result<Json<ModuleConfigReadResponse>, crate::SystemPlaneRejection> {
    let provider = require_provider(provider)?;
    authorize_context(&caller, &request.context)?;
    caller.require_capability(
        MODULE_OPERATIONS_PROTOCOL,
        &module_operations_schema_digest(),
        [MODULE_OPERATIONS_FEATURE_CONFIG_READ],
    )?;
    provider.read_config(&request).map(Json).map_err(rejection)
}

#[utoipa::path(
    post,
    path = "/system-plane/v1/modules/config/write",
    request_body = ModuleConfigWriteRequest,
    responses(
        (status = 200, body = ModuleConfigWriteResponse),
        (status = 400, body = crate::SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 401, body = crate::SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 403, body = crate::SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 404, body = crate::SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 409, body = crate::SystemPlaneErrorBody, content_type = "application/problem+json"),
        (status = 503, body = crate::SystemPlaneErrorBody, content_type = "application/problem+json")
    ),
    security(("bearer_auth" = [])),
    tag = "system-plane-module-operations"
)]
async fn write_module_config(
    caller: crate::AuthorizedSystemPlaneCaller,
    Extension(provider): Extension<Option<Arc<ModuleOperationsProvider>>>,
    Json(request): Json<ModuleConfigWriteRequest>,
) -> Result<Json<ModuleConfigWriteResponse>, crate::SystemPlaneRejection> {
    let provider = require_provider(provider)?;
    authorize_context(&caller, &request.context)?;
    caller.require_capability(
        MODULE_OPERATIONS_PROTOCOL,
        &module_operations_schema_digest(),
        [MODULE_OPERATIONS_FEATURE_CONFIG_WRITE],
    )?;
    provider.write_config(&request).map(Json).map_err(rejection)
}

fn require_provider(
    provider: Option<Arc<ModuleOperationsProvider>>,
) -> Result<Arc<ModuleOperationsProvider>, crate::SystemPlaneRejection> {
    provider.ok_or_else(|| {
        crate::SystemPlaneRejection::unavailable(
            "module_operations_unavailable",
            "Module Operations capability is not configured for this Service",
            "configure_module_operations",
        )
    })
}

fn authorize_context(
    caller: &crate::AuthorizedSystemPlaneCaller,
    context: &ManagedServiceContext,
) -> Result<(), crate::SystemPlaneRejection> {
    let core = caller.runtime.registry.document();
    if context.system_id != caller.enrollment.system_id
        || context.service_id != core.service_id
        || context.target_service_principal != core.service_principal
    {
        return Err(crate::SystemPlaneRejection::new(
            axum::http::StatusCode::FORBIDDEN,
            "system_plane_target_context_mismatch",
            "Managed Service context does not match the authenticated enrollment target",
            "use_the_enrolled_service_context",
        ));
    }
    if caller.enrollment.system_id != "system-sandbox" {
        let granted = caller
            .enrollment
            .capabilities
            .iter()
            .flat_map(|capability| capability.feature_ids.iter())
            .collect::<BTreeSet<_>>();
        if context
            .capabilities
            .iter()
            .any(|capability| !granted.contains(capability))
        {
            return Err(crate::SystemPlaneRejection::new(
                axum::http::StatusCode::FORBIDDEN,
                "system_plane_context_capability_not_granted",
                "Managed Service context contains a capability outside the active enrollment grant",
                "request_the_required_module_capability",
            ));
        }
    }
    Ok(())
}

fn rejection(error: ModuleOperationsError) -> crate::SystemPlaneRejection {
    let (status, code, next_action) = match error.code {
        ModuleOperationsErrorCode::InvalidRequest => (
            axum::http::StatusCode::BAD_REQUEST,
            "module_operations_invalid_request",
            "send_a_descriptor_bound_module_request",
        ),
        ModuleOperationsErrorCode::CapabilityDenied => (
            axum::http::StatusCode::FORBIDDEN,
            "module_operations_capability_denied",
            "request_the_required_module_capability",
        ),
        ModuleOperationsErrorCode::NotFound => (
            axum::http::StatusCode::NOT_FOUND,
            "module_operations_not_found",
            "refresh_module_inventory",
        ),
        ModuleOperationsErrorCode::Conflict => (
            axum::http::StatusCode::CONFLICT,
            "module_operations_conflict",
            "refresh_module_inventory_and_retry",
        ),
        ModuleOperationsErrorCode::StoreUnavailable => (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "module_operations_unavailable",
            "restore_module_operations_store",
        ),
    };
    crate::SystemPlaneRejection::new(status, code, error.message, next_action)
}

fn operation_error(
    code: ModuleOperationsErrorCode,
    message: impl Into<String>,
) -> ModuleOperationsError {
    ModuleOperationsError {
        code,
        message: message.into(),
    }
}

fn provider_error(message: impl Into<String>) -> ModuleOperationsProviderError {
    ModuleOperationsProviderError {
        message: message.into(),
    }
}

fn inventory_module(
    release: &ModuleRelease,
    status: Option<ModuleRuntimeStatus>,
) -> Result<ModuleInventoryModule, ModuleOperationsError> {
    let release_digest = digest_json(release).map_err(|error| {
        operation_error(
            ModuleOperationsErrorCode::StoreUnavailable,
            error.to_string(),
        )
    })?;
    let delivery = match release.delivery {
        ModuleDelivery::Linked(_) => ModuleInventoryDelivery::Linked,
        ModuleDelivery::Service(_) => ModuleInventoryDelivery::Service,
    };
    let mut routes = release
        .manifest
        .http_routes
        .iter()
        .map(|route| ModuleInventoryRoute {
            method: match route.method {
                ModuleHttpMethod::Get => "GET".to_owned(),
                ModuleHttpMethod::Post => "POST".to_owned(),
                ModuleHttpMethod::Put => "PUT".to_owned(),
                ModuleHttpMethod::Patch => "PATCH".to_owned(),
                ModuleHttpMethod::Delete => "DELETE".to_owned(),
                _ => "UNKNOWN".to_owned(),
            },
            path: route.path.clone(),
            capability: route.capability.clone(),
        })
        .collect::<Vec<_>>();
    routes.sort_by(|left, right| (&left.method, &left.path).cmp(&(&right.method, &right.path)));
    let mut dependency_module_ids = release
        .manifest
        .requires
        .iter()
        .map(|requirement| requirement.module_id.clone())
        .collect::<Vec<_>>();
    dependency_module_ids.sort();
    let runtime_functions = release
        .manifest
        .runtime
        .as_ref()
        .map(|runtime| {
            let mut functions = runtime
                .functions
                .iter()
                .map(|function| function.name.clone())
                .collect::<Vec<_>>();
            functions.sort();
            functions
        })
        .unwrap_or_default();
    let console_ui =
        release
            .console_ui_artifact
            .as_ref()
            .map(|artifact| ModuleInventoryConsoleUi {
                format: lenso_contracts::CONSOLE_UI_ESM_FORMAT.to_owned(),
                protocol_major: artifact.protocol_major,
                artifact_digest: artifact.artifact.digest.clone(),
                entry: artifact.entry.clone(),
                style_assets: artifact
                    .style_assets
                    .iter()
                    .map(|asset| asset.path.clone())
                    .collect(),
            });
    Ok(ModuleInventoryModule {
        module_id: release.module_id.clone(),
        version: release.version.clone(),
        release_digest,
        manifest_digest: release.manifest_digest.clone(),
        delivery,
        dependency_module_ids,
        routes,
        runtime_functions,
        runtime_status: status.unwrap_or(ModuleRuntimeStatus::Active),
        console_ui,
    })
}

fn requested_config_fields<'a>(
    fields: &'a [lenso_contracts::ModuleConfigField],
    requested: &[String],
) -> Result<Vec<&'a lenso_contracts::ModuleConfigField>, ModuleOperationsError> {
    let keys = if requested.is_empty() {
        fields
            .iter()
            .map(|field| field.key.as_str())
            .collect::<Vec<_>>()
    } else {
        let mut keys = requested.iter().map(String::as_str).collect::<Vec<_>>();
        keys.sort_unstable();
        if keys.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(operation_error(
                ModuleOperationsErrorCode::InvalidRequest,
                "Configuration read keys must be unique",
            ));
        }
        keys
    };
    keys.into_iter()
        .map(|key| {
            fields.iter().find(|field| field.key == key).ok_or_else(|| {
                operation_error(
                    ModuleOperationsErrorCode::NotFound,
                    format!("Configuration field `{key}` is not declared by the Module"),
                )
            })
        })
        .collect()
}

fn validate_action_reference(
    releases: &BTreeMap<String, ModuleRelease>,
    action: &ConsoleContributionAction,
    slot_context: &BTreeMap<String, Value>,
    context_capabilities: &BTreeSet<String>,
) -> Result<ConsoleContributionAction, ModuleOperationsError> {
    match action {
        ConsoleContributionAction::AdminAction {
            module,
            name,
            input_bindings,
        } => {
            let release = releases.get(module).ok_or_else(|| {
                operation_error(
                    ModuleOperationsErrorCode::NotFound,
                    format!("Contributed Admin Module `{module}` is not installed"),
                )
            })?;
            let Some(AdminSurface::DeclarativeCustom(surface)) = release.manifest.admin.as_ref()
            else {
                return Err(operation_error(
                    ModuleOperationsErrorCode::InvalidRequest,
                    format!(
                        "Contributed Admin Action `{module}:{name}` is not declaratively declared"
                    ),
                ));
            };
            let admin_action = surface
                .actions
                .iter()
                .find(|candidate| candidate.name == *name)
                .ok_or_else(|| {
                    operation_error(
                        ModuleOperationsErrorCode::NotFound,
                        format!("Contributed Admin Action `{module}:{name}` is not declared"),
                    )
                })?;
            require_context_capabilities(context_capabilities, [&admin_action.capability])?;
            for binding in input_bindings {
                let ConsoleActionInputValue::SlotContext { path } = &binding.value else {
                    return Err(operation_error(
                        ModuleOperationsErrorCode::InvalidRequest,
                        "Only explicit slot context action bindings are supported",
                    ));
                };
                let value = slot_context.get(path).or_else(|| {
                    path.split('.')
                        .next()
                        .and_then(|head| slot_context.get(head))
                });
                if value.is_none() {
                    return Err(operation_error(
                        ModuleOperationsErrorCode::InvalidRequest,
                        format!(
                            "Action input binding `{path}` is not present in the explicit slot context"
                        ),
                    ));
                }
            }
            Ok(action.clone())
        }
        _ => Err(operation_error(
            ModuleOperationsErrorCode::InvalidRequest,
            "Unsupported Console contribution action kind",
        )),
    }
}

fn validate_slot_context(
    slot: &lenso_contracts::ConsoleSlot,
    values: &BTreeMap<String, Value>,
) -> Result<(), ModuleOperationsError> {
    for context in &slot.context {
        for field in &context.fields {
            let key = format!("{}.{}", context.name, field.name);
            let value = values.get(&key).or_else(|| values.get(&field.name));
            if field.required && value.is_none() {
                return Err(operation_error(
                    ModuleOperationsErrorCode::InvalidRequest,
                    format!("Required slot context field `{key}` is missing"),
                ));
            }
            if let Some(value) = value {
                let valid = match field.field_type {
                    lenso_contracts::ConsoleSlotContextFieldType::String => value.is_string(),
                    lenso_contracts::ConsoleSlotContextFieldType::Boolean => value.is_boolean(),
                    lenso_contracts::ConsoleSlotContextFieldType::Number => value.is_number(),
                    lenso_contracts::ConsoleSlotContextFieldType::Timestamp => value.is_string(),
                    _ => false,
                };
                if !valid {
                    return Err(operation_error(
                        ModuleOperationsErrorCode::InvalidRequest,
                        format!("Slot context field `{key}` has the wrong type"),
                    ));
                }
            }
        }
    }
    Ok(())
}

fn require_context_capabilities(
    context: &BTreeSet<String>,
    required: impl IntoIterator<Item = impl AsRef<str>>,
) -> Result<(), ModuleOperationsError> {
    for capability in required {
        if !context.contains(capability.as_ref()) {
            return Err(operation_error(
                ModuleOperationsErrorCode::CapabilityDenied,
                format!(
                    "Required Module capability `{}` was not delegated",
                    capability.as_ref()
                ),
            ));
        }
    }
    Ok(())
}

fn digest_value(value: &Value) -> serde_json::Result<String> {
    digest_json(value)
}

fn canonical_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lenso_contracts::{
        AdminAction, AdminActionDangerLevel, AdminDeclarativeSurface, ConsoleActionInputBinding,
        ConsoleActionInputValue, ConsoleContribution, ConsoleContributionAction,
        ConsoleContributionKind, ConsoleSlot, ConsoleSlotContext, ConsoleSlotContextField,
        ConsoleSlotContextFieldType, ModuleConfigActivation, ModuleConfigField,
        ModuleConfigFieldType, ModuleConfigMutability, ModuleConfigScope, ModuleRelease,
        digest_json,
    };

    fn release() -> ModuleRelease {
        serde_json::from_value(
            lenso_contracts::console_contract_vectors()["positive"]["release"].clone(),
        )
        .expect("positive Console vector should produce a Module Release")
    }

    fn context(capabilities: impl IntoIterator<Item = &'static str>) -> ManagedServiceContext {
        ManagedServiceContext::new(
            "system-1",
            "service-1",
            "production",
            "spiffe://lenso/service-1",
            "acme/support-console",
            "operator-1",
            format!("sha256:{}", "f".repeat(64)),
            capabilities,
        )
    }

    fn provider() -> ModuleOperationsProvider {
        ModuleOperationsProvider::new(
            "service-1",
            "spiffe://lenso/service-1",
            "revision-1",
            [release()],
        )
        .expect("positive Module Release should be accepted")
    }

    #[test]
    fn inventory_is_authoritative_and_reports_esm_delivery() {
        let provider = provider();
        let snapshot = provider
            .inventory(&context([MODULE_OPERATIONS_FEATURE_INVENTORY_READ]))
            .unwrap();
        assert_eq!(snapshot.protocol, MODULE_OPERATIONS_PROTOCOL);
        assert_eq!(snapshot.modules.len(), 1);
        assert_eq!(snapshot.modules[0].module_id, "acme/support-console");
        assert_eq!(
            snapshot.modules[0].console_ui.as_ref().unwrap().format,
            "console_ui_esm"
        );
        assert_eq!(
            snapshot.modules[0].runtime_status,
            ModuleRuntimeStatus::Active
        );
    }

    #[test]
    fn configuration_is_typed_capability_checked_namespace_bound_and_audited() {
        let provider = provider();
        provider
            .set_config_value(
                "acme/support-console",
                "endpoint",
                Value::String("https://old.example".to_owned()),
            )
            .unwrap();
        let request = ModuleConfigReadRequest {
            context: context(["support.endpoint.read"]),
            module_id: "acme/support-console".to_owned(),
            keys: vec!["endpoint".to_owned()],
        };
        assert_eq!(
            provider.read_config(&request).unwrap().values[0].value,
            Some(Value::String("https://old.example".to_owned()))
        );

        let response = provider
            .write_config(&ModuleConfigWriteRequest {
                context: context(["support.endpoint.write"]),
                module_id: "acme/support-console".to_owned(),
                values: vec![lenso_service::system_plane::ModuleConfigWriteValue {
                    key: "endpoint".to_owned(),
                    value: Value::String("https://new.example".to_owned()),
                }],
            })
            .unwrap();
        assert_eq!(response.evidence.len(), 1);
        assert_eq!(response.evidence[0].key, "endpoint");
        assert!(!response.evidence[0].new_value_digest.is_empty());

        let wrong_type = provider.write_config(&ModuleConfigWriteRequest {
            context: context(["support.endpoint.write"]),
            module_id: "acme/support-console".to_owned(),
            values: vec![lenso_service::system_plane::ModuleConfigWriteValue {
                key: "endpoint".to_owned(),
                value: Value::Bool(true),
            }],
        });
        assert_eq!(
            wrong_type.unwrap_err().code,
            ModuleOperationsErrorCode::InvalidRequest
        );

        let wrong_namespace = provider.read_config(&ModuleConfigReadRequest {
            context: ManagedServiceContext {
                caller_module_id: "other/module".to_owned(),
                ..context(["support.endpoint.read"])
            },
            module_id: "acme/support-console".to_owned(),
            keys: Vec::new(),
        });
        assert_eq!(
            wrong_namespace.unwrap_err().code,
            ModuleOperationsErrorCode::CapabilityDenied
        );
    }

    #[test]
    fn contributions_are_data_only_and_require_current_action_capability() {
        let mut release = release();
        release
            .manifest
            .capabilities
            .push("support.action.execute".to_owned());
        release.manifest.capabilities.sort();
        release.manifest.console_slots = vec![ConsoleSlot {
            id: "support.detail.actions".to_owned(),
            version: 1,
            label: "Ticket actions".to_owned(),
            accepts: vec![ConsoleContributionKind::AdminAction],
            context: vec![ConsoleSlotContext {
                name: "ticket".to_owned(),
                fields: vec![ConsoleSlotContextField {
                    name: "id".to_owned(),
                    field_type: ConsoleSlotContextFieldType::String,
                    required: true,
                }],
            }],
        }];
        release.manifest.admin = Some(AdminSurface::DeclarativeCustom(AdminDeclarativeSurface {
            pages: Vec::new(),
            actions: vec![AdminAction {
                name: "reopen".to_owned(),
                label: "Reopen".to_owned(),
                capability: "support.action.execute".to_owned(),
                input_schema: None,
                confirmation: None,
                danger_level: AdminActionDangerLevel::Low,
                operation: None,
            }],
            fallback_schema: None,
        }));
        release.manifest.console_contributions = vec![ConsoleContribution {
            target: "support.detail.actions".to_owned(),
            target_version: 1,
            label: "Reopen".to_owned(),
            action: ConsoleContributionAction::AdminAction {
                module: release.module_id.clone(),
                name: "reopen".to_owned(),
                input_bindings: vec![ConsoleActionInputBinding {
                    input: "ticket_id".to_owned(),
                    value: ConsoleActionInputValue::SlotContext {
                        path: "ticket.id".to_owned(),
                    },
                }],
            },
            icon: None,
            required_capabilities: Vec::new(),
        }];
        release.manifest_digest = digest_json(&release.manifest).unwrap();
        assert!(
            release.validate().is_empty(),
            "fixture release should remain valid"
        );
        let provider = ModuleOperationsProvider::new(
            "service-1",
            "spiffe://lenso/service-1",
            "revision-1",
            [release],
        )
        .unwrap();

        let request = ActionContributionResolutionRequest {
            context: context(["support.action.execute"]),
            slot: "support.detail.actions".to_owned(),
            slot_version: 1,
            slot_context: BTreeMap::from([(
                "ticket.id".to_owned(),
                Value::String("t-1".to_owned()),
            )]),
        };
        let result = provider.resolve_contributions(&request).unwrap();
        assert_eq!(result.contributions.len(), 1);
        assert_eq!(
            result.contributions[0].contributing_module_id,
            "acme/support-console"
        );

        let denied = provider.resolve_contributions(&ActionContributionResolutionRequest {
            context: context([]),
            ..request
        });
        assert_eq!(
            denied.unwrap_err().code,
            ModuleOperationsErrorCode::CapabilityDenied
        );
    }

    #[test]
    fn sensitive_configuration_is_write_only_and_audit_evidence_contains_only_digests() {
        let mut release = release();
        release
            .manifest
            .capabilities
            .push("support.token.write".to_owned());
        release.manifest.capabilities.sort();
        release.manifest.config.fields.push(ModuleConfigField {
            key: "api_token".to_owned(),
            field_type: ModuleConfigFieldType::String,
            required: false,
            scope: ModuleConfigScope::Module,
            sensitive: true,
            secret_reference: true,
            mutability: ModuleConfigMutability::Runtime,
            activation: ModuleConfigActivation::None,
            read_capability: None,
            write_capability: Some("support.token.write".to_owned()),
            default: None,
            validation: None,
        });
        release.manifest_digest = digest_json(&release.manifest).unwrap();
        assert!(
            release.validate().is_empty(),
            "fixture release should remain valid"
        );
        let provider = ModuleOperationsProvider::new(
            "service-1",
            "spiffe://lenso/service-1",
            "revision-1",
            [release],
        )
        .unwrap();
        provider
            .set_config_value(
                "acme/support-console",
                "api_token",
                Value::String("super-secret".to_owned()),
            )
            .unwrap();

        let read = provider
            .read_config(&ModuleConfigReadRequest {
                context: context([]),
                module_id: "acme/support-console".to_owned(),
                keys: vec!["api_token".to_owned()],
            })
            .unwrap();
        assert!(read.values[0].present);
        assert!(read.values[0].sensitive);
        assert_eq!(read.values[0].value, None);

        let write = provider
            .write_config(&ModuleConfigWriteRequest {
                context: context(["support.token.write"]),
                module_id: "acme/support-console".to_owned(),
                values: vec![lenso_service::system_plane::ModuleConfigWriteValue {
                    key: "api_token".to_owned(),
                    value: Value::String("rotated-secret".to_owned()),
                }],
            })
            .unwrap();
        assert!(write.evidence[0].sensitive);
        assert!(write.evidence[0].old_value_digest.is_some());
        assert!(!write.evidence[0].new_value_digest.is_empty());
        let evidence = serde_json::to_string(&write.evidence).unwrap();
        assert!(!evidence.contains("rotated-secret"));
        assert!(!evidence.contains("super-secret"));
    }
}
