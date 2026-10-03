use lenso_capability_secrets as secrets;
use lenso_capability_secrets::{ResolveError, ResolveRequest, ResolveResponse, Secrets};
use lenso_kernel::{InvocationContext, NativeRequestFuture};

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, lenso::PluginConfig)]
#[serde(deny_unknown_fields)]
pub struct FixtureSecretsConfig {
    pub database_env: String,
    pub signing_env: String,
    pub pepper_env: String,
}

#[lenso::plugin(id = "example.fixture-secrets", root_slot = "secrets")]
#[derive(Clone, Debug)]
pub struct Plugin {
    #[config]
    config: FixtureSecretsConfig,
}

#[lenso::provides(secrets::Secrets)]
impl Plugin {}

impl Plugin {
    fn resolve(
        &self,
        context: InvocationContext,
        request: ResolveRequest,
    ) -> NativeRequestFuture<Secrets> {
        let result = if context.caller_instance() != Some("lenso.auth.api-token/default") {
            Err(ResolveError::UnknownReference)
        } else {
            let variable = match request.reference.as_str() {
                "fixture/database" => Some(&self.config.database_env),
                "fixture/signing" => Some(&self.config.signing_env),
                "fixture/pepper" => Some(&self.config.pepper_env),
                _ => None,
            };
            variable
                .and_then(|name| std::env::var(name).ok())
                .map(|value| ResolveResponse { value })
                .ok_or(ResolveError::UnknownReference)
        };
        Box::pin(async move { Ok(result) })
    }
}
