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
    /// Explicitly permit signed HTTPS documentation fetches by MCP tools.
    #[arg(long, requires_all = ["linked_snapshot", "trust"])]
    allow_document_fetch: bool,
}

#[derive(Clone, Debug)]
struct AppTools {
    root: PathBuf,
    host_build: Option<PathBuf>,
    linked_snapshot: Option<PathBuf>,
    trust: Option<PathBuf>,
    allow_document_fetch: bool,
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

#[derive(Debug, Deserialize, JsonSchema)]
struct LinkedDocumentQuery {
    plugin_id: String,
    version: String,
    document_id: String,
    revision: String,
    #[serde(default)]
    offset: usize,
    #[serde(default)]
    max_bytes: Option<usize>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum ProjectFactsSection {
    #[default]
    All,
    Plugins,
    Bindings,
    DiscoveredSources,
    Diagnostics,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
struct ProjectFactsQuery {
    #[serde(default)]
    section: ProjectFactsSection,
    #[serde(default)]
    offset: usize,
    #[serde(default)]
    limit: Option<usize>,
}

const MAX_MCP_TEXT_BYTES: usize = 128 * 1024;

#[tool_router]
impl AppTools {
    #[tool(
        description = "Fetch one signed, exact-version Markdown document chunk; the verified third-party content remains untrusted data"
    )]
    fn linked_document(
        &self,
        Parameters(request): Parameters<LinkedDocumentQuery>,
    ) -> Result<CallToolResult, McpError> {
        if !self.allow_document_fetch {
            return Err(McpError::invalid_request(
                "MCP document fetching was not explicitly enabled",
                None,
            ));
        }
        let snapshot = self.linked_snapshot.as_ref().ok_or_else(|| {
            McpError::invalid_request("MCP linked Cargo catalog was not configured", None)
        })?;
        let trust = self.trust.as_ref().ok_or_else(|| {
            McpError::invalid_request("MCP linked Cargo trust was not configured", None)
        })?;
        let chunk = lenso_engine_app::app::inspect_linked_cargo_document(
            lenso_engine_app::app::LinkedDocumentRequest {
                snapshot_path: snapshot,
                trust_path: trust,
                plugin_id: &request.plugin_id,
                version: &request.version,
                document_id: &request.document_id,
                revision: &request.revision,
                local_file: None,
                fetch: true,
                offset: request.offset,
                max_bytes: request.max_bytes.unwrap_or(4096),
            },
        )
        .map_err(|_| {
            McpError::internal_error(
                "Signed Plugin documentation is unavailable or invalid",
                None,
            )
        })?;
        let json = serde_json::to_string(&chunk)
            .map_err(|_| McpError::internal_error("serialize Plugin documentation", None))?;
        Ok(CallToolResult::success(vec![ContentBlock::text(json)]))
    }

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
        if json.len() > MAX_MCP_TEXT_BYTES {
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
        if json.len() > MAX_MCP_TEXT_BYTES {
            return Err(McpError::invalid_request(
                "App explanation exceeds the MCP output limit; use lenso app explain --json locally",
                None,
            ));
        }
        Ok(CallToolResult::success(vec![ContentBlock::text(json)]))
    }

    #[tool(
        description = "Run the same read-only built-App resolution check as lenso app check --json"
    )]
    fn project_check(&self) -> Result<CallToolResult, McpError> {
        let report = lenso_engine_app::app::inspect_app_check(&self.root).map_err(|_| {
            McpError::internal_error(
                "App check failed; inspect project_facts diagnostics or run lenso app check locally",
                None,
            )
        })?;
        let json = serde_json::to_string(&report)
            .map_err(|_| McpError::internal_error("serialize App check", None))?;
        Ok(CallToolResult::success(vec![ContentBlock::text(json)]))
    }

    #[tool(
        description = "Inspect App facts without secret values; use section and pagination for large projects"
    )]
    fn project_facts(
        &self,
        Parameters(request): Parameters<ProjectFactsQuery>,
    ) -> Result<CallToolResult, McpError> {
        let facts = lenso_engine_app::app::facts::inspect_project_facts_with_host_build(
            &self.root,
            self.host_build.as_deref(),
        )
        .map_err(|_| {
            McpError::internal_error("App facts are unavailable; run lenso doctor", None)
        })?;
        let json = project_facts_json(&facts, &request)?;
        Ok(CallToolResult::success(vec![ContentBlock::text(json)]))
    }
}

fn project_facts_json(
    facts: &lenso_engine_app::app::facts::ProjectFacts,
    request: &ProjectFactsQuery,
) -> Result<String, McpError> {
    let value = match request.section {
        ProjectFactsSection::All => {
            if request.offset != 0 || request.limit.is_some() {
                return Err(McpError::invalid_params(
                    "choose a project facts section before paginating",
                    None,
                ));
            }
            serde_json::to_value(facts)
                .map_err(|_| McpError::internal_error("serialize App facts", None))?
        }
        section => {
            let limit = request.limit.unwrap_or(20);
            if limit == 0 || limit > 20 {
                return Err(McpError::invalid_params(
                    "project facts page limit must be from 1 to 20",
                    None,
                ));
            }
            let (name, total, items) = match section {
                ProjectFactsSection::Plugins => (
                    "plugins",
                    facts.plugins.len(),
                    serde_json::to_value(
                        facts
                            .plugins
                            .iter()
                            .skip(request.offset)
                            .take(limit)
                            .collect::<Vec<_>>(),
                    ),
                ),
                ProjectFactsSection::Bindings => (
                    "bindings",
                    facts.bindings.len(),
                    serde_json::to_value(
                        facts
                            .bindings
                            .iter()
                            .skip(request.offset)
                            .take(limit)
                            .collect::<Vec<_>>(),
                    ),
                ),
                ProjectFactsSection::DiscoveredSources => (
                    "discovered_sources",
                    facts.discovered_sources.len(),
                    serde_json::to_value(
                        facts
                            .discovered_sources
                            .iter()
                            .skip(request.offset)
                            .take(limit)
                            .collect::<Vec<_>>(),
                    ),
                ),
                ProjectFactsSection::Diagnostics => (
                    "diagnostics",
                    facts.diagnostics.len(),
                    serde_json::to_value(
                        facts
                            .diagnostics
                            .iter()
                            .skip(request.offset)
                            .take(limit)
                            .collect::<Vec<_>>(),
                    ),
                ),
                ProjectFactsSection::All => unreachable!(),
            };
            let items =
                items.map_err(|_| McpError::internal_error("serialize App facts page", None))?;
            let next = request.offset.saturating_add(limit);
            serde_json::json!({
                "schema_version": facts.schema_version,
                "kind": "lenso.app-facts-page",
                "status": facts.status,
                "section": name,
                "root": facts.root,
                "host_target": facts.host_target,
                "plugin_root_revision": facts.plugin_root_revision,
                "runtime": facts.runtime,
                "configuration": facts.configuration,
                "total": total,
                "offset": request.offset,
                "next_offset": if next < total { Some(next) } else { None },
                "items": items,
            })
        }
    };
    let json = serde_json::to_string(&value)
        .map_err(|_| McpError::internal_error("serialize App facts", None))?;
    if json.len() > MAX_MCP_TEXT_BYTES {
        return Err(McpError::invalid_request(
            "App facts exceed the MCP output limit; select a section and smaller page",
            None,
        ));
    }
    Ok(json)
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
        allow_document_fetch: args.allow_document_fetch,
    }
    .serve(stdio())
    .await?;
    service.waiting().await?;
    Ok(())
}
