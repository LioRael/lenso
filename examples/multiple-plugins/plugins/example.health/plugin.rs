use lenso_capability_http_endpoint::prelude::*;

#[lenso::plugin(id = "example.health", root_slot = "web")]
#[derive(Clone, Debug)]
pub struct Plugin {}

#[endpoint]
#[allow(unknown_lints, clippy::unused_async, clippy::unused_async_trait_impl)]
impl Plugin {
    #[get("health", "/health")]
    async fn health(
        &self,
    ) -> Result<Json<String>, lenso_capability_http_endpoint::response::Problem> {
        Ok(Json("ok".into()))
    }
}
