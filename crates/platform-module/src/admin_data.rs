//! Schema-admin behavior seams: a module's read access to its admin entities
//! and host-owned invocation of declared admin actions.

use async_trait::async_trait;
use platform_core::AppResult;
use serde_json::Value;

/// A module's read access to its admin entities. Optional capability — only
/// modules with an admin surface implement it. Records cross as `Value` (the
/// only shape a generic renderer handles); strong types stay inside the impl.
#[async_trait]
pub trait AdminDataSource: std::fmt::Debug + Send + Sync {
    /// List records for `entity`, paginated. Returns a page of JSON objects.
    async fn list(&self, entity: &str, query: &AdminListQuery) -> AppResult<AdminPage>;

    /// Fetch one record by id. `Ok(None)` if not found.
    async fn get(&self, entity: &str, id: &str) -> AppResult<Option<Value>>;
}

/// A module's executable admin actions. Action declarations stay in
/// [`crate::AdminSurface`]; this seam only carries the behavior that varies by
/// loading source.
#[async_trait]
pub trait AdminActionSource: std::fmt::Debug + Send + Sync {
    /// Invoke a manifest-declared action by name with arbitrary JSON input.
    async fn invoke(&self, action: &str, input: Value) -> AppResult<Value>;
}

/// A module's read-only declarative query source. Query declarations stay in
/// [`crate::AdminSurface`]; this seam only returns the JSON value to render.
#[async_trait]
pub trait AdminQuerySource: std::fmt::Debug + Send + Sync {
    /// Execute a manifest-declared query by name.
    async fn query(&self, query: &str) -> AppResult<Value>;
}

/// Structured query — fields reserved for future filter/sort without changing
/// the method signature.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AdminListQuery {
    pub limit: i64,
    /// Opaque pagination cursor (NOT a timestamp — no entity-shape assumption).
    pub cursor: Option<String>,
}

/// One page of records.
#[derive(Debug, Clone)]
pub struct AdminPage {
    pub records: Vec<Value>,
    /// Opaque cursor for the next page; `None` at the end.
    pub next_cursor: Option<String>,
}

impl AdminListQuery {
    /// Convenience constructor.
    #[must_use]
    pub fn new(limit: i64, cursor: Option<String>) -> Self {
        Self { limit, cursor }
    }
}
