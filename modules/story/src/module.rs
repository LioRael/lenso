use platform_core::AppContext;
use platform_http::ApiOpenApiRouter;
use platform_module::{
    ConsoleArea, ConsolePackage, ConsoleSurface, LinkedBinding, LinkedHttpContribution, Module,
    ModuleHttpMethod, ModuleHttpRoute, ModuleManifest,
};

pub const MODULE_NAME: &str = "platform-story";
pub const STORY_CONSOLE_CAPABILITY: &str = "runtime.stories.read";

pub fn http_routes() -> Vec<ModuleHttpRoute> {
    vec![
        ModuleHttpRoute {
            method: ModuleHttpMethod::Get,
            path: "/admin/runtime/stories".to_owned(),
            capability: Some(STORY_CONSOLE_CAPABILITY.to_owned()),
            display_name: Some("List Runtime Stories".to_owned()),
            story_title: Some("Runtime Stories".to_owned()),
            operation: None,
        },
        ModuleHttpRoute {
            method: ModuleHttpMethod::Get,
            path: "/admin/runtime/stories/{correlation_id}".to_owned(),
            capability: Some(STORY_CONSOLE_CAPABILITY.to_owned()),
            display_name: Some("Runtime Story Detail".to_owned()),
            story_title: Some("Runtime Story Detail".to_owned()),
            operation: None,
        },
        ModuleHttpRoute {
            method: ModuleHttpMethod::Get,
            path: "/admin/runtime/stories/{correlation_id}/heatmap".to_owned(),
            capability: Some(STORY_CONSOLE_CAPABILITY.to_owned()),
            display_name: Some("Runtime Story Heatmap".to_owned()),
            story_title: Some("Runtime Story Heatmap".to_owned()),
            operation: None,
        },
        ModuleHttpRoute {
            method: ModuleHttpMethod::Get,
            path: "/admin/runtime/stories/{correlation_id}/technical-operations".to_owned(),
            capability: Some(STORY_CONSOLE_CAPABILITY.to_owned()),
            display_name: Some("Runtime Story Technical Operations".to_owned()),
            story_title: Some("Runtime Story Technical Operations".to_owned()),
            operation: None,
        },
    ]
}

/// Context-free manifest for the Runtime Story system module.
pub fn manifest() -> ModuleManifest {
    ModuleManifest::builder(MODULE_NAME)
        .capabilities(vec![STORY_CONSOLE_CAPABILITY.to_owned()])
        .http_routes(http_routes())
        .console(vec![ConsoleSurface {
            name: "stories".to_owned(),
            label: "Stories".to_owned(),
            area: ConsoleArea::Runtime,
            route: "/runtime/stories".to_owned(),
            package: ConsolePackage {
                name: "@lenso/story-console".to_owned(),
                export: "storyConsoleModule".to_owned(),
            },
            icon: Some("workflow".to_owned()),
            required_capabilities: vec![STORY_CONSOLE_CAPABILITY.to_owned()],
            navigation: None,
        }])
        .build()
}

pub fn merge_http(base: ApiOpenApiRouter) -> ApiOpenApiRouter {
    base.merge(crate::backend::router())
}

pub fn binding() -> LinkedBinding {
    LinkedBinding::builder()
        .http(LinkedHttpContribution {
            public_prefixes: &["/admin/runtime/stories"],
            merge: merge_http,
        })
        .build()
}

/// The loaded Story module.
pub fn module(_ctx: &AppContext) -> Module {
    Module::linked(manifest(), binding())
}

#[cfg(test)]
mod tests {
    use super::*;
    use platform_module::{ModuleManifestLintSeverity, ModuleSource, lint_module_manifest};

    #[test]
    fn manifest_declares_story_console_surface() {
        let manifest = manifest();
        let console_surface_contract: serde_json::Value =
            serde_json::from_str(include_str!("../console/console-surface.json"))
                .expect("story console surface contract should be valid json");

        assert_eq!(manifest.name, console_surface_contract["id"]);
        assert_eq!(manifest.admin, None);
        assert_eq!(manifest.capabilities, vec![STORY_CONSOLE_CAPABILITY]);
        assert_eq!(manifest.http_routes, http_routes());
        assert_eq!(manifest.console.len(), 1);

        let surface = &manifest.console[0];
        let surface_json =
            serde_json::to_value(surface).expect("story console surface should serialize");

        assert_eq!(surface.name, console_surface_contract["surfaceName"]);
        assert_eq!(surface.label, console_surface_contract["label"]);
        assert_eq!(surface.area, ConsoleArea::Runtime);
        assert_eq!(surface_json["area"], console_surface_contract["area"]);
        assert_eq!(surface.route, console_surface_contract["route"]);
        assert_eq!(
            surface.package.name,
            console_surface_contract["packageName"]
        );
        assert_eq!(
            surface.package.export,
            console_surface_contract["exportName"]
        );
        assert_eq!(surface_json["icon"], console_surface_contract["icon"]);
        assert_eq!(surface.navigation, None);
        assert!(console_surface_contract.get("navigation").is_none());
        assert_eq!(
            surface.required_capabilities,
            vec![STORY_CONSOLE_CAPABILITY]
        );

        let lints = lint_module_manifest(ModuleSource::Linked, &manifest);
        assert!(
            lints
                .iter()
                .all(|lint| lint.severity == ModuleManifestLintSeverity::Ok),
            "platform-story manifest should not have warning/error lints: {lints:?}"
        );
    }
}
