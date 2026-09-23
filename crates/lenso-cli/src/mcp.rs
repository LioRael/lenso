//! Read-only project operations shared with the App authoring API.
use std::path::PathBuf;

use anyhow::Context;
use clap::Args;
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock},
    tool, tool_handler, tool_router,
    transport::stdio,
};
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Clone, Debug, Args)]
pub(crate) struct McpArgs {
    /// One local App root this MCP process may inspect.
    #[arg(long)]
    root: PathBuf,
    /// Exact distribution Host build when the root is external to its distribution.
    #[arg(long)]
    host_build: Option<PathBuf>,
    /// Optional exact signed linked Cargo snapshot for read-only candidate search.
    #[arg(long, requires = "trust")]
    linked_snapshot: Option<PathBuf>,
    /// Public trust configuration for --linked-snapshot.
    #[arg(long, requires = "linked_snapshot")]
    trust: Option<PathBuf>,
}

#[derive(Clone, Debug)]
struct AppTools {
    root: PathBuf,
    host_build: Option<PathBuf>,
    linked_snapshot: Option<PathBuf>,
    trust: Option<PathBuf>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct LinkedCatalogQuery {
    #[serde(default)]
    query: String,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    offset: usize,
    #[serde(default)]
    limit: Option<usize>,
}

#[tool_router]
impl AppTools {
    #[tool(
        description = "Search an explicitly configured signed linked Cargo snapshot; results are candidates, not verified installable releases"
    )]
    fn linked_catalog(
        &self,
        Parameters(request): Parameters<LinkedCatalogQuery>,
    ) -> Result<CallToolResult, McpError> {
        let snapshot = self.linked_snapshot.as_ref().ok_or_else(|| {
            McpError::invalid_request("MCP linked Cargo catalog was not configured", None)
        })?;
        let trust = self.trust.as_ref().ok_or_else(|| {
            McpError::invalid_request("MCP linked Cargo trust was not configured", None)
        })?;
        let target = request
            .target
            .as_deref()
            .unwrap_or(lenso_app_authoring::native_host_target());
        let mut report = lenso_engine_app::app::inspect_linked_cargo_catalog(
            snapshot,
            trust,
            &request.query,
            target,
        )
        .map_err(|_| {
            McpError::internal_error(
                "Signed linked Cargo catalog is unavailable or invalid",
                None,
            )
        })?;
        let releases = report["releases"]
            .as_array_mut()
            .ok_or_else(|| McpError::internal_error("invalid linked Cargo catalog report", None))?;
        let total = releases.len();
        let limit = request.limit.unwrap_or(20);
        if limit == 0 || limit > 20 {
            return Err(McpError::invalid_params(
                "linked Cargo page limit must be from 1 to 20",
                None,
            ));
        }
        let page = releases
            .iter()
            .skip(request.offset)
            .take(limit)
            .cloned()
            .collect::<Vec<_>>();
        let next = request.offset.saturating_add(page.len());
        *releases = page;
        report["total_releases"] = total.into();
        report["next_offset"] = if next < total {
            next.into()
        } else {
            serde_json::Value::Null
        };
        let json = serde_json::to_string(&report)
            .map_err(|_| McpError::internal_error("serialize linked Cargo catalog", None))?;
        if json.len() > 128 * 1024 {
            return Err(McpError::invalid_params(
                "linked Cargo page exceeds output limit; use a narrower query or smaller limit",
                None,
            ));
        }
        Ok(CallToolResult::success(vec![ContentBlock::text(json)]))
    }

    #[tool(
        description = "Explain persisted Host target admission, implementation selection, and consumer requirements for a built App"
    )]
    fn project_explain(&self) -> Result<CallToolResult, McpError> {
        let explanation = lenso_engine_app::app::inspect_app_explanation(&self.root).map_err(|_| {
            McpError::internal_error(
                "App explanation is unavailable; run lenso app explain --json on the built Host root",
                None,
            )
        })?;
        let json = serde_json::to_string(&explanation)
            .map_err(|_| McpError::internal_error("serialize App explanation", None))?;
        Ok(CallToolResult::success(vec![ContentBlock::text(json)]))
    }

    #[tool(
        description = "Inspect exact App, Plugin, source, binding and diagnostic facts without reading secret values"
    )]
    fn project_facts(&self) -> Result<CallToolResult, McpError> {
        let facts = lenso_engine_app::app::facts::inspect_project_facts_with_host_build(
            &self.root,
            self.host_build.as_deref(),
        )
        .map_err(|_| {
            McpError::internal_error("App facts are unavailable; run lenso doctor", None)
        })?;
        let json = serde_json::to_string(&facts)
            .map_err(|_| McpError::internal_error("serialize App facts", None))?;
        Ok(CallToolResult::success(vec![ContentBlock::text(json)]))
    }
}

// The SDK generates an async handler for the synchronous tool router.
#[allow(clippy::unused_async_trait_impl)]
#[tool_handler]
impl ServerHandler for AppTools {}

pub(crate) async fn serve(args: McpArgs) -> anyhow::Result<()> {
    let root = std::fs::canonicalize(&args.root).context("resolve MCP App root")?;
    if !root.is_dir() {
        anyhow::bail!("MCP App root must be a directory");
    }
    let service = AppTools {
        root,
        host_build: args.host_build,
        linked_snapshot: args.linked_snapshot,
        trust: args.trust,
    }
    .serve(stdio())
    .await?;
    service.waiting().await?;
    Ok(())
}
