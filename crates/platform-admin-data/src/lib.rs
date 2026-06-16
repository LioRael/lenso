//! Schema-admin data API: generic endpoints that render any module's declared
//! admin entities and invoke manifest-declared admin actions. Depends on NO
//! concrete module crate: it works only through injected platform-module seams,
//! mirroring `platform-admin`'s seam-only discipline.

use platform_core::{AppError, ErrorCode, RequestContext, StoryDisplayDescriptor};
use platform_http::{ApiErrorResponse, ApiOpenApiRouter, OpenApiRouter, routes};
use platform_module::{
    AdminActionSource, AdminDataSource, AdminSchema, AdminSurface, ConsoleSurface, EventSurface,
    LifecycleSurface, ModuleHttpRoute, ModuleLoadStatus, ModuleSource, RuntimeSurface,
};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Instant;

mod dto;
mod handlers;

pub use dto::*;
#[allow(clippy::wildcard_imports)]
use handlers::*;

/// One module's admin capability: its declared surface plus live data/action
/// sources.
#[derive(Clone, Debug)]
pub struct AdminModule {
    /// The owning module's stable name, e.g. "identity".
    pub module_name: String,
    /// The loading source that produced this module.
    pub source: ModuleSource,
    /// Current load state. The first remote slice only installs loaded modules;
    /// error entries are reserved for degraded loading in a later slice.
    pub load_status: ModuleLoadStatus,
    /// The module's schema-admin surface or custom surface fallback schema.
    pub schema: AdminSchema,
    /// The full declared admin surface, retained for action validation.
    pub admin: Option<AdminSurface>,
    /// Whether this module should appear in the generic schema-admin discovery
    /// endpoint. Declarative custom surfaces may still be readable through
    /// fallback schema entities without being advertised as plain schema-admin.
    pub listed_in_schema: bool,
    /// Live read access to the module's records. Missing for degraded modules
    /// whose manifest/data source failed to load.
    pub data_source: Option<Arc<dyn AdminDataSource>>,
    /// Live behavior for manifest-declared admin actions.
    pub action_source: Option<Arc<dyn AdminActionSource>>,
}

/// One module's registry metadata, independent of whether schema-admin
/// list/detail reads or an admin surface are available.
#[derive(Clone, Debug)]
pub struct AdminModuleMetadata {
    /// The owning module's stable name, e.g. "identity".
    pub module_name: String,
    /// The loading source that produced this module.
    pub source: ModuleSource,
    /// Current load state.
    pub load_status: ModuleLoadStatus,
    /// Declared module-owned HTTP routes. Metadata only; not mounted by
    /// platform-admin-data.
    pub http_routes: Vec<ModuleHttpRoute>,
    /// Declared runtime functions. Metadata only; runtime registration belongs
    /// to the source-specific binding and worker composition.
    pub runtime: Option<RuntimeSurface>,
    /// Declared event handlers. Metadata only; event dispatch registration
    /// belongs to the source-specific binding and worker composition.
    pub events: Option<EventSurface>,
    /// Declared lifecycle checks and activation jobs. Metadata only; worker startup
    /// owns validation and enqueueing.
    pub lifecycle: Option<LifecycleSurface>,
    /// Declared trusted Runtime Console frontend surfaces.
    pub console: Vec<ConsoleSurface>,
    /// Declared story-display mappings for runtime story titles and node
    /// labels.
    pub story_display: Vec<StoryDisplayDescriptor>,
    /// Declared capability strings owned by the module.
    pub capabilities: Vec<String>,
    /// Declared module dependencies.
    pub dependencies: Vec<String>,
    /// The declared admin surface. Missing for modules with no admin surface
    /// and degraded failed remotes whose manifest could not be loaded.
    pub admin: Option<AdminSurface>,
    /// Source-level diagnostics known to the host. Remote modules include
    /// endpoint and load metadata; linked modules usually leave this empty.
    pub source_diagnostics: Option<AdminModuleSourceDiagnostics>,
}

#[derive(Clone, Debug)]
pub enum AdminModuleSourceDiagnostics {
    Remote(AdminRemoteModuleDiagnostics),
}

#[derive(Clone, Debug)]
pub struct AdminRemoteModuleDiagnostics {
    pub base_url: String,
    pub manifest_url: String,
    pub timeout_ms: u64,
    pub auth_configured: bool,
    pub load_duration_ms: Option<u64>,
    pub last_checked_at: Option<String>,
    pub last_load_error: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct AdminModuleMetadataSnapshot {
    modules: Vec<AdminModuleMetadata>,
    refreshed_at: Option<String>,
    refresh_error: Option<String>,
    refresh_history: Vec<AdminModuleMetadataRefreshRecord>,
}

#[derive(Clone, Debug)]
pub struct AdminModuleMetadataRefreshRecord {
    pub id: String,
    pub status: AdminModuleMetadataRefreshStatus,
    pub started_at: String,
    pub completed_at: String,
    pub duration_ms: u64,
    pub module_count: usize,
    pub error: Option<String>,
    pub module_results: Vec<AdminModuleMetadataRefreshModuleResult>,
}

#[derive(Clone, Debug)]
pub struct AdminModuleMetadataRefreshModuleResult {
    pub module_name: String,
    pub source: ModuleSource,
    pub status: AdminModuleMetadataRefreshModuleStatus,
    pub duration_ms: Option<u64>,
    pub endpoint: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub enum AdminModuleMetadataRefreshModuleStatus {
    Loaded,
    Error,
}

#[derive(Clone, Copy, Debug)]
pub enum AdminModuleMetadataRefreshStatus {
    Success,
    Error,
}

static ADMIN_REGISTRY: OnceLock<RwLock<Vec<AdminModule>>> = OnceLock::new();
static ADMIN_METADATA_REGISTRY: OnceLock<RwLock<AdminModuleMetadataSnapshot>> = OnceLock::new();
static ADMIN_REFRESHER: OnceLock<RwLock<Option<Arc<dyn AdminModuleRefresher>>>> = OnceLock::new();
static ADMIN_METADATA_REFRESHER: OnceLock<RwLock<Option<Arc<dyn AdminModuleMetadataRefresher>>>> =
    OnceLock::new();

#[async_trait::async_trait]
pub trait AdminModuleRefresher: Send + Sync {
    async fn refresh_admin_modules(&self) -> platform_core::AppResult<Vec<AdminModule>>;
}

#[async_trait::async_trait]
pub trait AdminModuleMetadataRefresher: Send + Sync {
    async fn refresh_admin_module_metadata(
        &self,
    ) -> platform_core::AppResult<Vec<AdminModuleMetadata>>;
}

struct StaticAdminModuleRefresher<F>(F);
struct StaticAdminModuleMetadataRefresher<F>(F);

#[async_trait::async_trait]
impl<F, Fut> AdminModuleRefresher for StaticAdminModuleRefresher<F>
where
    F: Fn() -> Fut + Send + Sync,
    Fut: std::future::Future<Output = platform_core::AppResult<Vec<AdminModule>>> + Send,
{
    async fn refresh_admin_modules(&self) -> platform_core::AppResult<Vec<AdminModule>> {
        (self.0)().await
    }
}

#[async_trait::async_trait]
impl<F, Fut> AdminModuleMetadataRefresher for StaticAdminModuleMetadataRefresher<F>
where
    F: Fn() -> Fut + Send + Sync,
    Fut: std::future::Future<Output = platform_core::AppResult<Vec<AdminModuleMetadata>>> + Send,
{
    async fn refresh_admin_module_metadata(
        &self,
    ) -> platform_core::AppResult<Vec<AdminModuleMetadata>> {
        (self.0)().await
    }
}

/// Install the admin-capable module registry. Called once by the composition
/// root before the router serves traffic. Later calls replace the registry,
/// which keeps tests isolated and leaves room for explicit refresh later.
pub fn install_admin_modules(modules: Vec<AdminModule>) {
    let registry = ADMIN_REGISTRY.get_or_init(|| RwLock::new(Vec::new()));
    *registry.write().expect("admin registry lock poisoned") = modules;
}

/// Install the metadata registry for every module.
pub fn install_admin_module_metadata(modules: Vec<AdminModuleMetadata>) {
    let registry =
        ADMIN_METADATA_REGISTRY.get_or_init(|| RwLock::new(AdminModuleMetadataSnapshot::default()));
    *registry
        .write()
        .expect("admin metadata registry lock poisoned") = AdminModuleMetadataSnapshot {
        modules,
        refreshed_at: Some(current_timestamp()),
        refresh_error: None,
        refresh_history: Vec::new(),
    };
}

pub(crate) fn record_admin_module_metadata_refresh_success(
    modules: Vec<AdminModuleMetadata>,
    started_at: String,
    started: Instant,
) -> AdminModuleMetadataSnapshot {
    let registry =
        ADMIN_METADATA_REGISTRY.get_or_init(|| RwLock::new(AdminModuleMetadataSnapshot::default()));
    let mut snapshot = registry
        .write()
        .expect("admin metadata registry lock poisoned");
    let completed_at = current_timestamp();
    let record = AdminModuleMetadataRefreshRecord {
        id: format!(
            "module_refresh_{}",
            completed_at.replace([':', '.', '+'], "_")
        ),
        status: AdminModuleMetadataRefreshStatus::Success,
        started_at,
        completed_at: completed_at.clone(),
        duration_ms: duration_ms(started),
        module_count: modules.len(),
        error: None,
        module_results: refresh_module_results(&modules),
    };
    snapshot.modules = modules;
    snapshot.refreshed_at = Some(completed_at);
    snapshot.refresh_error = None;
    push_refresh_record(&mut snapshot.refresh_history, record);
    snapshot.clone()
}

/// Install the callback used by the explicit admin refresh endpoint.
///
/// Kept as an injected seam so this platform crate does not depend on the
/// composition root that knows how to load linked and remote modules.
pub fn install_admin_module_refresher(refresher: Arc<dyn AdminModuleRefresher>) {
    let registry = ADMIN_REFRESHER.get_or_init(|| RwLock::new(None));
    *registry.write().expect("admin refresher lock poisoned") = Some(refresher);
}

pub fn install_admin_module_refresh_fn<F, Fut>(refresh: F)
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = platform_core::AppResult<Vec<AdminModule>>> + Send + 'static,
{
    install_admin_module_refresher(Arc::new(StaticAdminModuleRefresher(refresh)));
}

/// Install the callback used to refresh module registry metadata.
pub fn install_admin_module_metadata_refresher(refresher: Arc<dyn AdminModuleMetadataRefresher>) {
    let registry = ADMIN_METADATA_REFRESHER.get_or_init(|| RwLock::new(None));
    *registry
        .write()
        .expect("admin metadata refresher lock poisoned") = Some(refresher);
}

pub fn install_admin_module_metadata_refresh_fn<F, Fut>(refresh: F)
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = platform_core::AppResult<Vec<AdminModuleMetadata>>>
        + Send
        + 'static,
{
    install_admin_module_metadata_refresher(Arc::new(StaticAdminModuleMetadataRefresher(refresh)));
}

fn admin_modules() -> Vec<AdminModule> {
    ADMIN_REGISTRY
        .get()
        .map(|registry| {
            registry
                .read()
                .expect("admin registry lock poisoned")
                .clone()
        })
        .unwrap_or_default()
}

fn admin_module_metadata_snapshot() -> AdminModuleMetadataSnapshot {
    ADMIN_METADATA_REGISTRY
        .get()
        .map(|registry| {
            registry
                .read()
                .expect("admin metadata registry lock poisoned")
                .clone()
        })
        .unwrap_or_default()
}

pub(crate) fn record_admin_module_metadata_refresh_error(
    error: String,
    started_at: String,
    started: Instant,
) -> AdminModuleMetadataSnapshot {
    let registry =
        ADMIN_METADATA_REGISTRY.get_or_init(|| RwLock::new(AdminModuleMetadataSnapshot::default()));
    let mut snapshot = registry
        .write()
        .expect("admin metadata registry lock poisoned");
    snapshot.refresh_error = Some(error);
    let completed_at = current_timestamp();
    let record = AdminModuleMetadataRefreshRecord {
        id: format!(
            "module_refresh_{}",
            completed_at.replace([':', '.', '+'], "_")
        ),
        status: AdminModuleMetadataRefreshStatus::Error,
        started_at,
        completed_at,
        duration_ms: duration_ms(started),
        module_count: snapshot.modules.len(),
        error: snapshot.refresh_error.clone(),
        module_results: Vec::new(),
    };
    push_refresh_record(&mut snapshot.refresh_history, record);
    snapshot.clone()
}

pub(crate) fn current_timestamp() -> String {
    use platform_core::Clock;
    platform_core::SystemClock.now().to_rfc3339()
}

fn duration_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn push_refresh_record(
    history: &mut Vec<AdminModuleMetadataRefreshRecord>,
    record: AdminModuleMetadataRefreshRecord,
) {
    history.insert(0, record);
    history.truncate(10);
}

fn refresh_module_results(
    modules: &[AdminModuleMetadata],
) -> Vec<AdminModuleMetadataRefreshModuleResult> {
    modules
        .iter()
        .map(|module| {
            let remote = match &module.source_diagnostics {
                Some(AdminModuleSourceDiagnostics::Remote(remote)) => Some(remote),
                None => None,
            };
            AdminModuleMetadataRefreshModuleResult {
                module_name: module.module_name.clone(),
                source: module.source,
                status: match module.load_status {
                    ModuleLoadStatus::Loaded => AdminModuleMetadataRefreshModuleStatus::Loaded,
                    ModuleLoadStatus::Error { .. } => AdminModuleMetadataRefreshModuleStatus::Error,
                },
                duration_ms: remote.and_then(|diagnostics| diagnostics.load_duration_ms),
                endpoint: remote.map(|diagnostics| diagnostics.base_url.clone()),
                error: match &module.load_status {
                    ModuleLoadStatus::Loaded => {
                        remote.and_then(|diagnostics| diagnostics.last_load_error.clone())
                    }
                    ModuleLoadStatus::Error { message } => Some(message.clone()),
                },
            }
        })
        .collect()
}

fn admin_refresher() -> Option<Arc<dyn AdminModuleRefresher>> {
    ADMIN_REFRESHER.get().and_then(|registry| {
        registry
            .read()
            .expect("admin refresher lock poisoned")
            .clone()
    })
}

fn admin_metadata_refresher() -> Option<Arc<dyn AdminModuleMetadataRefresher>> {
    ADMIN_METADATA_REFRESHER.get().and_then(|registry| {
        registry
            .read()
            .expect("admin metadata refresher lock poisoned")
            .clone()
    })
}

fn find_module(module: &str, ctx: &RequestContext) -> Result<AdminModule, ApiErrorResponse> {
    admin_modules()
        .into_iter()
        .find(|m| m.module_name == module)
        .ok_or_else(|| {
            ApiErrorResponse::with_context(
                AppError::new(ErrorCode::NotFound, format!("unknown module: {module}")),
                ctx,
            )
        })
}

fn find_loaded_module(module: &str, ctx: &RequestContext) -> Result<AdminModule, ApiErrorResponse> {
    let admin_module = find_module(module, ctx)?;
    if admin_module.data_source.is_some() {
        Ok(admin_module)
    } else {
        Err(ApiErrorResponse::with_context(
            AppError::new(
                ErrorCode::ExternalDependency,
                format!("module {module} is not loaded"),
            )
            .retryable(),
            ctx,
        ))
    }
}

fn find_loaded_action_module(
    module: &str,
    ctx: &RequestContext,
) -> Result<AdminModule, ApiErrorResponse> {
    let admin_module = find_module(module, ctx)?;
    if matches!(admin_module.load_status, ModuleLoadStatus::Loaded) {
        Ok(admin_module)
    } else {
        Err(ApiErrorResponse::with_context(
            AppError::new(
                ErrorCode::ExternalDependency,
                format!("module {module} is not loaded"),
            )
            .retryable(),
            ctx,
        ))
    }
}

/// The schema-admin router, mounted by the API app.
pub fn router() -> ApiOpenApiRouter {
    OpenApiRouter::new()
        .routes(routes!(list_modules))
        .routes(routes!(refresh_modules))
        .routes(routes!(available_modules))
        .routes(routes!(module_registry_snapshot))
        .routes(routes!(list_schemas))
        .routes(routes!(refresh_schemas))
        .routes(routes!(invoke_action))
        .routes(routes!(list_records))
        .routes(routes!(get_record))
}
