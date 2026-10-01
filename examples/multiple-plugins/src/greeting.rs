use lenso_capability_http_endpoint::prelude::*;

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, lenso::PluginConfig)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    #[lenso(default = "Hello")]
    pub message: String,
}

#[lenso::plugin(id = "example.greeting", root_slot = "web")]
#[derive(Clone, Debug)]
pub struct Plugin {
    #[config]
    config: Configuration,
}

#[endpoint]
#[allow(unknown_lints, clippy::unused_async, clippy::unused_async_trait_impl)]
impl Plugin {
    #[get("greeting", "/greeting")]
    async fn greeting(
        &self,
    ) -> Result<Json<String>, lenso_capability_http_endpoint::response::Problem> {
        Ok(Json(self.config.message.clone()))
    }
}
