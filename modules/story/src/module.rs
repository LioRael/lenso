use platform_core::AppContext;
use platform_module::{
    CONSOLE_BRIDGE_PROTOCOL, ConsoleSurface, ConsoleSurfacePresentation, LinkedBinding, Module,
    ModuleManifest,
};

pub const MODULE_NAME: &str = "platform-story";
pub const STORY_CONSOLE_CAPABILITY: &str = "runtime.stories.read";

/// Context-free manifest for the Runtime Story system module.
pub fn manifest() -> ModuleManifest {
    ModuleManifest::builder(MODULE_NAME)
        .capabilities(vec![STORY_CONSOLE_CAPABILITY.to_owned()])
        .console(vec![ConsoleSurface {
            name: "stories".to_owned(),
            label: "Stories".to_owned(),
            route: "/runtime/stories".to_owned(),
            presentation: ConsoleSurfacePresentation::Isolated {
                entry: "runtime-stories".to_owned(),
                bridge_protocol: CONSOLE_BRIDGE_PROTOCOL.to_owned(),
            },
            icon: Some("workflow".to_owned()),
            required_capabilities: vec![STORY_CONSOLE_CAPABILITY.to_owned()],
            navigation: None,
        }])
        .build()
}

pub fn binding() -> LinkedBinding {
    LinkedBinding::builder().build()
}

/// The loaded Story module.
pub fn module(_ctx: &AppContext) -> Module {
    Module::linked(manifest(), binding())
}

#[cfg(test)]
mod tests {
    use super::*;
    use platform_module::{ModuleManifestLintSeverity, lint_module_manifest};

    #[test]
    fn manifest_declares_story_console_surface() {
        let manifest = manifest();
        let console_surface_contract: serde_json::Value =
            serde_json::from_str(include_str!("../console/console-surface.json"))
                .expect("story console surface contract should be valid json");

        assert_eq!(manifest.module_id, console_surface_contract["id"]);
        assert_eq!(manifest.admin, None);
        assert_eq!(manifest.capabilities, vec![STORY_CONSOLE_CAPABILITY]);
        assert!(manifest.http_routes.is_empty());
        assert_eq!(manifest.console.len(), 1);

        let surface = &manifest.console[0];
        let surface_json =
            serde_json::to_value(surface).expect("story console surface should serialize");

        assert_eq!(surface.name, console_surface_contract["surfaceName"]);
        assert_eq!(surface.label, console_surface_contract["label"]);
        assert_eq!(surface.route, console_surface_contract["route"]);
        assert_eq!(
            surface_json["presentation"],
            console_surface_contract["presentation"]
        );
        assert_eq!(surface_json["icon"], console_surface_contract["icon"]);
        assert_eq!(surface.navigation, None);
        assert!(console_surface_contract.get("navigation").is_none());
        assert_eq!(
            surface.required_capabilities,
            vec![STORY_CONSOLE_CAPABILITY]
        );

        let lints = lint_module_manifest(&manifest);
        assert!(
            lints
                .iter()
                .all(|lint| lint.severity == ModuleManifestLintSeverity::Ok),
            "platform-story manifest should not have warning/error lints: {lints:?}"
        );
    }
}
