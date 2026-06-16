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
