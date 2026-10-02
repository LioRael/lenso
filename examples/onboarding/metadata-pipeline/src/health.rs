use lenso_capability_http_endpoint::{prelude::*, response::Problem};

/// A separate liveness route keeps the Host useful after business removal.
#[lenso::plugin(id = "example.metadata-health", root_slot = "web")]
#[derive(Clone, Debug)]
pub struct Plugin {}

#[endpoint]
// Endpoint operations use the async contract even when this response is immediate.
#[allow(unknown_lints, clippy::unused_async, clippy::unused_async_trait_impl)]
impl Plugin {
    #[get("metadata.health", "/health")]
    async fn health(&self) -> Result<Json<String>, Problem> {
        Ok(Json("ok".to_owned()))
    }
}
