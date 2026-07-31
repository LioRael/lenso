use std::collections::BTreeMap;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::{Extension, Json};
use platform_core::runtime_config::store::{load_all_values, upsert_value};
use platform_core::{AppContext, AppError, ErrorCode, RequestContext};
use platform_http::responses::json;
use platform_http::{
    ApiErrorResponse, ApiOpenApiRouter, ErrorResponse, HttpRequestContext, JsonBody, OpenApiRouter,
    UserActor, routes,
};
use platform_module::{
    AdminActionSource, AdminDataSource, AdminListQuery, AdminSurface, ConsoleContributionAction,
    Module, ModuleManifest,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json as json_value};
use utoipa::ToSchema;

const CONSOLE_ADMIN_SCOPE: &str = "console.admin";
const MODULE_METADATA_READ: &str = "auth.providers.read";
const AUTH_MODULE: &str = "auth";
const CONSOLE_ADMIN_SCOPES_KEY: &str = "auth.console_admin_user_scopes";
const SHARED_CONFIG_SERVICE: &str = "*";

#[derive(Clone, Default)]
pub(crate) struct ConsoleBridgeRegistry {
    modules: Arc<BTreeMap<String, ConsoleBridgeModule>>,
}

impl std::fmt::Debug for ConsoleBridgeRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ConsoleBridgeRegistry")
            .field("modules", &self.modules.keys().collect::<Vec<_>>())
            .finish()
    }
}

#[derive(Clone)]
struct ConsoleBridgeModule {
    manifest: ModuleManifest,
    admin_data: Option<Arc<dyn AdminDataSource>>,
    admin_actions: Option<Arc<dyn AdminActionSource>>,
}

impl ConsoleBridgeRegistry {
    pub(crate) fn from_modules(modules: Vec<Module>) -> Self {
        Self {
            modules: Arc::new(
                modules
                    .into_iter()
                    .map(|module| {
                        (
                            module.manifest.module_id.clone(),
                            ConsoleBridgeModule {
                                manifest: module.manifest,
                                admin_data: module.admin_data,
                                admin_actions: module.admin_actions,
                            },
                        )
                    })
                    .collect(),
            ),
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConsoleBridgeRequest {
    module_release_digest: String,
    ui_artifact_digest: String,
    permission: String,
    payload: ConsoleBridgeOperation,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
enum ConsoleBridgeOperation {
    AdminDataList {
        entity: String,
        #[serde(default = "default_limit")]
        limit: i64,
        #[serde(default)]
        cursor: Option<String>,
    },
    AdminActionInvoke {
        action: String,
        #[serde(default)]
        input: Value,
    },
    ConfigValues,
    ConfigWrite {
        service: String,
        key: String,
        value: Value,
    },
    ContributionsResolve {
        target: String,
        #[serde(default)]
        context: Value,
    },
    ModulesMetadata,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ConsoleBridgeResponse {
    data: Value,
}

pub(crate) fn router() -> ApiOpenApiRouter {
    OpenApiRouter::new().routes(routes!(invoke_console_bridge))
}

#[utoipa::path(
    post,
    path = "/modules/{module}/http/console-bridge/{permission}",
    operation_id = "invoke_module_console_bridge",
    tag = "module-console-bridge",
    params(
        ("module" = String, Path, description = "Module identity"),
        ("permission" = String, Path, description = "Exact composition grant")
    ),
    request_body(content = ConsoleBridgeRequest, content_type = "application/json"),
    responses(
        (status = 200, body = ConsoleBridgeResponse, content_type = "application/json"),
        (status = 400, body = ErrorResponse, content_type = "application/problem+json"),
        (status = 401, body = ErrorResponse, content_type = "application/problem+json"),
        (status = 403, body = ErrorResponse, content_type = "application/problem+json"),
        (status = 404, body = ErrorResponse, content_type = "application/problem+json")
    )
)]
async fn invoke_console_bridge(
    State(ctx): State<AppContext>,
    Extension(registry): Extension<ConsoleBridgeRegistry>,
    actor: UserActor,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Path((module_name, permission)): Path<(String, String)>,
    JsonBody(request): JsonBody<ConsoleBridgeRequest>,
) -> Result<Json<ConsoleBridgeResponse>, ApiErrorResponse> {
    validate_request(&module_name, &permission, &request, &actor, &request_ctx)?;
    let module = registry.modules.get(&module_name).ok_or_else(|| {
        api_error(
            ErrorCode::NotFound,
            "Console Bridge Module was not found",
            &request_ctx,
        )
    })?;
    validate_permission(module, &permission, &request.payload, &request_ctx)?;

    let data = match request.payload {
        ConsoleBridgeOperation::AdminDataList {
            entity,
            limit,
            cursor,
        } => {
            let source = module.admin_data.as_ref().ok_or_else(|| {
                api_error(
                    ErrorCode::NotFound,
                    "Module does not provide Console admin data",
                    &request_ctx,
                )
            })?;
            let page = source
                .list(&entity, &AdminListQuery::new(limit.clamp(1, 200), cursor))
                .await
                .map_err(|error| ApiErrorResponse::with_context(error, &request_ctx))?;
            json_value!({ "data": page.records, "nextCursor": page.next_cursor })
        }
        ConsoleBridgeOperation::AdminActionInvoke { action, input } => {
            let source = module.admin_actions.as_ref().ok_or_else(|| {
                api_error(
                    ErrorCode::NotFound,
                    "Module does not provide Console admin actions",
                    &request_ctx,
                )
            })?;
            source
                .invoke(&action, input)
                .await
                .map_err(|error| ApiErrorResponse::with_context(error, &request_ctx))?
        }
        ConsoleBridgeOperation::ConfigValues => {
            let values = load_all_values(&ctx.db)
                .await
                .map_err(|error| ApiErrorResponse::with_context(error, &request_ctx))?
                .into_iter()
                .filter(|((service, key), _)| {
                    service == SHARED_CONFIG_SERVICE && key == CONSOLE_ADMIN_SCOPES_KEY
                })
                .map(|((_, key), value)| {
                    json_value!({
                        "key": key,
                        "desired_value": value,
                        "pending_restart": false,
                    })
                })
                .collect::<Vec<_>>();
            json_value!({ "data": values })
        }
        ConsoleBridgeOperation::ConfigWrite {
            service,
            key,
            value,
        } => {
            let stored = upsert_value(&ctx.db, &service, &key, &value, Some(&actor.user_id))
                .await
                .map_err(|error| ApiErrorResponse::with_context(error, &request_ctx))?;
            json_value!({ "service": stored.service, "key": stored.key, "value": stored.value })
        }
        ConsoleBridgeOperation::ContributionsResolve { target, context } => {
            resolve_contributions(&registry, &target, &context, &actor.scopes)
        }
        ConsoleBridgeOperation::ModulesMetadata => json_value!({
            "modules": registry.modules.values().map(|module| &module.manifest).collect::<Vec<_>>()
        }),
    };
    Ok(json(ConsoleBridgeResponse { data }))
}

fn validate_request(
    module_name: &str,
    permission: &str,
    request: &ConsoleBridgeRequest,
    actor: &UserActor,
    request_ctx: &RequestContext,
) -> Result<(), ApiErrorResponse> {
    if request.permission != permission || module_name.is_empty() || permission.is_empty() {
        return Err(api_error(
            ErrorCode::Validation,
            "Console Bridge route binding is invalid",
            request_ctx,
        ));
    }
    if !valid_digest(&request.module_release_digest) || !valid_digest(&request.ui_artifact_digest) {
        return Err(api_error(
            ErrorCode::Validation,
            "Console Bridge release digests are invalid",
            request_ctx,
        ));
    }
    if !actor
        .scopes
        .iter()
        .any(|scope| scope == permission || scope == CONSOLE_ADMIN_SCOPE)
    {
        return Err(api_error(
            ErrorCode::Forbidden,
            "Console operator lacks the requested Module permission",
            request_ctx,
        ));
    }
    Ok(())
}

fn validate_permission(
    module: &ConsoleBridgeModule,
    permission: &str,
    operation: &ConsoleBridgeOperation,
    request_ctx: &RequestContext,
) -> Result<(), ApiErrorResponse> {
    let valid = match operation {
        ConsoleBridgeOperation::AdminDataList { entity, .. } => {
            entity_permission(&module.manifest, entity)
                .is_some_and(|declared| declared == permission)
        }
        ConsoleBridgeOperation::AdminActionInvoke { action, .. } => {
            action_permission(&module.manifest, action)
                .is_some_and(|declared| declared == permission)
        }
        ConsoleBridgeOperation::ConfigValues => {
            module.manifest.module_id == AUTH_MODULE && permission == "auth.users.manage"
        }
        ConsoleBridgeOperation::ConfigWrite { service, key, .. } => {
            module.manifest.module_id == AUTH_MODULE
                && permission == "auth.users.manage"
                && service == SHARED_CONFIG_SERVICE
                && key == CONSOLE_ADMIN_SCOPES_KEY
        }
        ConsoleBridgeOperation::ContributionsResolve { .. } => module
            .manifest
            .capabilities
            .iter()
            .any(|value| value == permission),
        ConsoleBridgeOperation::ModulesMetadata => {
            permission == MODULE_METADATA_READ
                && module
                    .manifest
                    .capabilities
                    .iter()
                    .any(|capability| capability == MODULE_METADATA_READ)
        }
    };
    if valid {
        Ok(())
    } else {
        Err(api_error(
            ErrorCode::Forbidden,
            "Console Bridge operation is not declared for the requested permission",
            request_ctx,
        ))
    }
}

fn entity_permission<'a>(manifest: &'a ModuleManifest, entity: &str) -> Option<&'a str> {
    let admin = manifest.admin.as_ref()?;
    let schema = match admin {
        AdminSurface::Schema(schema) => schema,
        AdminSurface::DeclarativeCustom(surface) => surface.fallback_schema.as_ref()?,
        AdminSurface::EmbeddedCustom(surface) => surface.fallback_schema.as_ref()?,
        _ => return None,
    };
    schema
        .entities
        .iter()
        .find(|candidate| candidate.name == entity)
        .map(|candidate| candidate.read_capability.as_str())
}

fn action_permission<'a>(manifest: &'a ModuleManifest, action: &str) -> Option<&'a str> {
    match manifest.admin.as_ref()? {
        AdminSurface::DeclarativeCustom(surface) => surface
            .actions
            .iter()
            .find(|candidate| candidate.name == action)
            .map(|candidate| candidate.capability.as_str()),
        _ => None,
    }
}

fn resolve_contributions(
    registry: &ConsoleBridgeRegistry,
    target: &str,
    context: &Value,
    scopes: &[String],
) -> Value {
    json_value!({ "data": registry
            .modules
            .values()
            .flat_map(|module| {
                module
                    .manifest
                    .console_contributions
                    .iter()
                    .filter_map(|contribution| {
                        if contribution.target != target
                            || !contribution.required_capabilities.iter().all(|required| {
                                scopes
                                    .iter()
                                    .any(|scope| scope == required || scope == CONSOLE_ADMIN_SCOPE)
                            })
                        {
                            return None;
                        }
                        let ConsoleContributionAction::AdminAction {
                            module: target_module,
                            name,
                            input_bindings,
                        } = &contribution.action
                        else {
                            return None;
                        };
                        let input = input_bindings
                            .iter()
                            .filter_map(|binding| {
                                let platform_module::ConsoleActionInputValue::SlotContext { path } =
                                    &binding.value
                                else {
                                    return None;
                                };
                                context
                                    .pointer(&format!("/{}", path.replace('.', "/")))
                                    .cloned()
                                    .map(|value| (binding.input.clone(), value))
                            })
                            .collect::<serde_json::Map<_, _>>();
                        Some(json_value!({
                            "kind": "admin_action",
                            "key": format!("{}:{}:{}", module.manifest.module_id, target, name),
                            "label": contribution.label,
                            "moduleName": target_module,
                            "actionName": name,
                            "input": input,
                            "icon": contribution.icon,
                            "requiredCapabilities": contribution.required_capabilities,
                        }))
                    })
            })
            .collect::<Vec<_>>() })
}

fn valid_digest(value: &str) -> bool {
    value
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

const fn default_limit() -> i64 {
    100
}

fn api_error(code: ErrorCode, message: &'static str, ctx: &RequestContext) -> ApiErrorResponse {
    ApiErrorResponse::with_context(AppError::new(code, message), ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_binding_is_exact() {
        assert!(valid_digest(&format!("sha256:{}", "a".repeat(64))));
        assert!(!valid_digest(&format!("sha256:{}", "a".repeat(63))));
        assert!(!valid_digest("auth-console"));
    }
}
