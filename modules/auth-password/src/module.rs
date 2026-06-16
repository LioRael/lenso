use crate::config::AuthPasswordConfig;
use platform_core::AppContext;
use platform_http::ApiOpenApiRouter;
use platform_module::{
    LinkedBinding, LinkedHttpContribution, Module, ModuleHttpMethod, ModuleHttpRoute,
    ModuleManifest,
};

pub const MODULE_NAME: &str = "auth-password";
pub const AUTH_MODULE_DEPENDENCY: &str = "auth";

pub fn http_routes() -> Vec<ModuleHttpRoute> {
    vec![
        ModuleHttpRoute {
            method: ModuleHttpMethod::Post,
            path: "/v1/auth/password/register".to_owned(),
            capability: None,
            display_name: Some("Register With Password".to_owned()),
            story_title: Some("Password Registration".to_owned()),
        },
        ModuleHttpRoute {
            method: ModuleHttpMethod::Post,
            path: "/v1/auth/password/login".to_owned(),
            capability: None,
            display_name: Some("Login With Password".to_owned()),
            story_title: Some("Password Login".to_owned()),
        },
    ]
}

pub fn manifest() -> ModuleManifest {
    ModuleManifest::builder(MODULE_NAME)
        .dependencies(vec![AUTH_MODULE_DEPENDENCY.to_owned()])
        .http_routes(http_routes())
        .build()
}

pub fn merge_http(base: ApiOpenApiRouter) -> ApiOpenApiRouter {
    base.merge(crate::routes::router())
}

pub fn binding() -> LinkedBinding {
    LinkedBinding::builder()
        .http(LinkedHttpContribution {
            public_prefixes: &["/v1/auth/password/"],
            merge: merge_http,
        })
        .build()
}

pub fn module(_ctx: &AppContext) -> Module {
    Module::linked(manifest(), binding())
        .with_runtime_config_groups(crate::config::RUNTIME_CONFIG_GROUPS.as_slice())
        .with_runtime_config(crate::config::RUNTIME_CONFIG.as_slice())
}

/// Build a [`JwtActorResolver`] if auth-password is configured with `token_strategy = "jwt"`.
///
/// Returns `Ok(None)` when the strategy is `"session"` (the default) or when
/// the auth-password module is not enabled. Returns `Ok(Some(..))` with the
/// resolver wrapping the given `fallback`.
pub fn jwt_actor_resolver(
    ctx: &AppContext,
    fallback: std::sync::Arc<dyn platform_core::ActorResolver>,
) -> platform_core::AppResult<Option<std::sync::Arc<dyn platform_core::ActorResolver>>> {
    let config = AuthPasswordConfig::from_context(ctx)?;
    let jwt_config = match config.jwt_config()? {
        Some(cfg) => cfg,
        None => return Ok(None),
    };
    Ok(Some(std::sync::Arc::new(
        crate::resolver::JwtActorResolver::new(jwt_config, fallback),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use platform_module::{ModuleManifestLintSeverity, ModuleSource, lint_module_manifest};

    #[test]
    fn manifest_declares_password_routes() {
        let manifest = manifest();

        assert_eq!(manifest.name, MODULE_NAME);
        assert_eq!(manifest.http_routes, http_routes());

        let lints = lint_module_manifest(ModuleSource::Linked, &manifest);
        assert!(
            lints
                .iter()
                .all(|lint| lint.severity == ModuleManifestLintSeverity::Ok),
            "auth-password manifest should not have warning/error lints: {lints:?}"
        );
    }
}
