//! Admin runtime-observability API for the Runtime Console.
//!
//! This module is split by responsibility:
//! - [`dto`]: request query params and response DTOs (re-exported for `OpenAPI`).
//! - [`handlers`]: Axum route handlers.
//! - [`rows`]: SQL row tuples/structs and their `From` conversions to DTOs.
//! - [`fetch`]: shared data-access helpers used by multiple handlers.
//! - [`stories`]: story graph assembly and naming.
//! - [`spans`]: telemetry-span → technical-operation mapping and PII redaction.
//! - [`support`]: small cross-cutting helpers (errors, pagination, limits).

use platform_http::{ApiOpenApiRouter, OpenApiRouter, routes};

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 100;

mod dto;
mod fetch;
mod handlers;
mod rows;
mod spans;
mod stories;
mod support;

pub use dto::*;
#[allow(clippy::wildcard_imports)]
use fetch::*;
#[allow(clippy::wildcard_imports)]
use handlers::*;
#[allow(clippy::wildcard_imports)]
use rows::*;
#[allow(clippy::wildcard_imports)]
use spans::*;
#[allow(clippy::wildcard_imports)]
use stories::*;
#[allow(clippy::wildcard_imports)]
use support::*;

pub fn router() -> ApiOpenApiRouter {
    OpenApiRouter::new()
        .routes(routes!(get_summary))
        .routes(routes!(get_timeline))
        .routes(routes!(get_heatmap))
        .routes(routes!(list_stories))
        .routes(routes!(get_story))
        .routes(routes!(get_story_technical_operations))
        .routes(routes!(get_execution_technical_operations))
        .routes(routes!(get_execution_payload))
        .routes(routes!(get_execution_logs))
        .routes(routes!(list_outbox))
        .routes(routes!(get_outbox_event))
        .routes(routes!(retry_outbox_event))
        .routes(routes!(list_function_runs))
        .routes(routes!(get_function_run))
        .routes(routes!(retry_function_run))
}
