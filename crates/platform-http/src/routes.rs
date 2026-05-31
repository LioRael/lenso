use crate::health::{livez, readyz};
use axum::routing::get;
use axum::Router;
use platform_core::AppContext;

pub type ApiRouter = Router<AppContext>;

#[derive(Debug, Clone)]
pub struct DomainHttp {
    pub name: &'static str,
    pub router: ApiRouter,
}

pub fn base_router(ctx: AppContext) -> ApiRouter {
    Router::new()
        .route("/livez", get(livez))
        .route("/readyz", get(readyz))
        .with_state(ctx)
}
