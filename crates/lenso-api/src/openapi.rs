//! `OpenAPI` document assembly.
//!
//! Paths and component schemas are derived directly from the
//! `#[utoipa::path]`-annotated handlers via `utoipa-axum`'s `OpenApiRouter`, so
//! there is a single source of truth per endpoint. This module contributes the
//! document-level metadata (info, tags) and normalizes shared platform error
//! responses after linked/module routers are merged.

use lenso_bootstrap::CompositionProfile;
use platform_core::AppContext;
use platform_http::{ApiOpenApiRouter, OpenApiRouter, base_router};
use utoipa::OpenApi;
use utoipa::openapi::RefOr;
use utoipa::openapi::content::Content;
use utoipa::openapi::path::Operation;
use utoipa::openapi::response::Response;

/// Document-level `OpenAPI` metadata shared by every endpoint.
///
/// Intentionally declares no `paths` and no per-endpoint `schemas`: those are
/// collected automatically from the annotated handlers when the router is split
/// into its parts.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Lenso API",
        version = "1.0.0",
        description = "Rust-first modular monolith API contract"
    ),
    tags(
        (name = "auth", description = "Auth module development session APIs"),
        (name = "admin-runtime", description = "Read-only runtime console APIs"),
        (name = "admin-config", description = "Editable configuration console APIs"),
        (name = "admin-data", description = "Schema-driven admin data console APIs"),
        (name = "system-delivery", description = "Production delivery authority APIs")
    )
)]
struct ApiDoc;

/// Assemble the full `OpenAPI` router: base probes, linked module routes, and
/// admin/runtime routers, seeded with the document-level metadata.
///
/// Context-free: route registration and `OpenAPI` metadata never touch the
/// database, so callers can either serve it (after `with_state` +
/// `split_for_parts`) or extract the `OpenAPI` document alone.
pub(crate) fn api_router() -> ApiOpenApiRouter {
    api_router_for_profile(CompositionProfile::default())
}

pub(crate) fn api_router_for_profile(profile: CompositionProfile) -> ApiOpenApiRouter {
    let base = OpenApiRouter::with_openapi(openapi_document_for_profile_with_composition(
        profile,
        &lenso_bootstrap::HostComposition::default(),
    ))
    .merge(base_router());
    lenso_bootstrap::merge_linked_http_for_profile(base, profile)
        .merge(platform_admin::router())
        .merge(platform_admin_data::router())
        .merge(platform_module_remote::router())
        .merge(crate::system_delivery::router())
}

pub(crate) fn api_router_for_context_with_composition(
    ctx: &AppContext,
    composition: &lenso_bootstrap::HostComposition,
) -> platform_core::AppResult<ApiOpenApiRouter> {
    let profile = CompositionProfile::from_config(&ctx.config)?;
    let base = OpenApiRouter::with_openapi(openapi_document_for_profile_with_composition(
        profile,
        composition,
    ))
    .merge(base_router());
    Ok(
        lenso_bootstrap::merge_linked_http_for_context_with_composition(base, ctx, composition)?
            .merge(platform_admin::router())
            .merge(platform_admin_data::router())
            .merge(platform_module_remote::router())
            .merge(crate::system_delivery::router()),
    )
}

fn openapi_document_for_profile_with_composition(
    profile: CompositionProfile,
    composition: &lenso_bootstrap::HostComposition,
) -> utoipa::openapi::OpenApi {
    let mut document = ApiDoc::openapi();
    if let Some(tags) = &mut document.tags {
        let has_auth = profile == CompositionProfile::Demo
            || composition
                .linked_modules()
                .iter()
                .any(|module| module.module_name == "auth");
        match profile {
            CompositionProfile::Core => tags.retain(|tag| has_auth || tag.name != "auth"),
            CompositionProfile::Demo => {}
        }
    }
    document
}

/// The committed `OpenAPI` document, derived from the annotated handlers.
#[must_use]
pub fn openapi_document() -> utoipa::openapi::OpenApi {
    let mut document = api_router().to_openapi();
    normalize_error_response_content_types(&mut document);
    document
}

pub(crate) fn normalize_error_response_content_types(document: &mut utoipa::openapi::OpenApi) {
    for path_item in document.paths.paths.values_mut() {
        normalize_operation_error_responses(path_item.get.as_mut());
        normalize_operation_error_responses(path_item.put.as_mut());
        normalize_operation_error_responses(path_item.post.as_mut());
        normalize_operation_error_responses(path_item.delete.as_mut());
        normalize_operation_error_responses(path_item.options.as_mut());
        normalize_operation_error_responses(path_item.head.as_mut());
        normalize_operation_error_responses(path_item.patch.as_mut());
        normalize_operation_error_responses(path_item.trace.as_mut());
    }
}

fn normalize_operation_error_responses(operation: Option<&mut Operation>) {
    let Some(operation) = operation else {
        return;
    };
    for response in operation.responses.responses.values_mut() {
        if let RefOr::T(response) = response {
            normalize_response_error_content_type(response);
        }
    }
}

fn normalize_response_error_content_type(response: &mut Response) {
    let Some(content) = response.content.get("application/json") else {
        return;
    };
    if !is_error_response_content(content) {
        return;
    }

    let content = response
        .content
        .shift_remove("application/json")
        .expect("application/json content should exist");
    response
        .content
        .insert("application/problem+json".to_owned(), content);
}

fn is_error_response_content(content: &Content) -> bool {
    matches!(
        &content.schema,
        Some(RefOr::Ref(reference))
            if reference.ref_location == "#/components/schemas/ErrorResponse"
    )
}
