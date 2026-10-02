//! An independent Endpoint keeps the Host observable when jobs are removed.
use lenso_capability_http_endpoint::{prelude::*, response::Problem};

#[lenso::plugin(id = "example.background-health", root_slot = "web")]
#[derive(Clone, Debug)]
pub struct Plugin {}

#[endpoint]
#[allow(unknown_lints, clippy::unused_async, clippy::unused_async_trait_impl)]
impl Plugin {
    #[get("background.health", "/health")]
    async fn health(&self) -> Result<Json<&'static str>, Problem> {
        Ok(Json("ok"))
    }
}
