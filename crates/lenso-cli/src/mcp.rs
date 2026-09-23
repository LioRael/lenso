//! Read-only project operations shared with the App authoring API.
use std::path::PathBuf;

use anyhow::Context;
use clap::Args;
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    model::{CallToolResult, ContentBlock},
    tool, tool_handler, tool_router,
    transport::stdio,
};

#[derive(Clone, Debug, Args)]
pub(crate) struct McpArgs {
    /// One local App root this MCP process may inspect.
    #[arg(long)]
    root: PathBuf,
    /// Exact distribution Host build when the root is external to its distribution.
    #[arg(long)]
    host_build: Option<PathBuf>,
}

#[derive(Clone, Debug)]
struct AppTools {
    root: PathBuf,
    host_build: Option<PathBuf>,
}

#[tool_router]
impl AppTools {
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
    }
    .serve(stdio())
    .await?;
    service.waiting().await?;
    Ok(())
}
