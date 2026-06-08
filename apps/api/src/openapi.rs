//! `OpenAPI` document assembly.
//!
//! Paths and component schemas are derived directly from the
//! `#[utoipa::path]`-annotated handlers via `utoipa-axum`'s `OpenApiRouter`, so
//! there is a single source of truth per endpoint. This module only contributes
//! the document-level metadata (info, tags) that is not tied to any one route.

use app_bootstrap::CompositionProfile;
use platform_core::AppContext;
use platform_http::{ApiOpenApiRouter, OpenApiRouter, base_router};
use utoipa::OpenApi;

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
        (name = "identity", description = "Identity module fixture APIs"),
        (name = "admin-runtime", description = "Read-only runtime console APIs"),
        (name = "admin-config", description = "Editable configuration console APIs"),
        (name = "admin-data", description = "Schema-driven admin data console APIs")
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
    let base =
        OpenApiRouter::with_openapi(openapi_document_for_profile(profile)).merge(base_router());
    app_bootstrap::merge_linked_http_for_profile(base, profile)
        .merge(story::backend::router())
        .merge(platform_admin::router())
        .merge(platform_admin_data::router())
        .merge(platform_module_remote::router())
}

pub(crate) fn api_router_for_context(
    ctx: &AppContext,
) -> platform_core::AppResult<ApiOpenApiRouter> {
    let profile = CompositionProfile::from_config(&ctx.config)?;
    let base =
        OpenApiRouter::with_openapi(openapi_document_for_profile(profile)).merge(base_router());
    Ok(app_bootstrap::merge_linked_http_for_context(base, ctx)?
        .merge(story::backend::router())
        .merge(platform_admin::router())
        .merge(platform_admin_data::router())
        .merge(platform_module_remote::router()))
}

fn openapi_document_for_profile(profile: CompositionProfile) -> utoipa::openapi::OpenApi {
    let mut document = ApiDoc::openapi();
    if profile == CompositionProfile::Core {
        if let Some(tags) = &mut document.tags {
            tags.retain(|tag| tag.name != "identity");
        }
    }
    document
}

/// The committed `OpenAPI` document, derived from the annotated handlers.
#[must_use]
pub fn openapi_document() -> utoipa::openapi::OpenApi {
    api_router().to_openapi()
}
