use platform_core::AppContext;
use platform_module::{
    ConsoleArea, ConsolePackage, ConsoleSurface, LinkedBinding, Module, ModuleManifest,
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

/// The loaded Story module.
///
/// The first extraction slice makes Story discoverable through the module
/// framework while preserving the existing platform-admin backend routes.
pub fn module(_ctx: &AppContext) -> Module {
    Module::linked(manifest(), LinkedBinding::builder().build())
}

#[cfg(test)]
mod tests {
    use super::*;
    use platform_module::{ModuleManifestLintSeverity, ModuleSource, lint_module_manifest};

    #[test]
    fn manifest_declares_story_console_surface() {
        let manifest = manifest();
        let console_surface_contract: serde_json::Value = serde_json::from_str(include_str!(
            "../../../apps/runtime-console/packages/story-console/console-surface.json"
        ))
        .expect("story console surface contract should be valid json");

        assert_eq!(manifest.name, console_surface_contract["id"]);
        assert_eq!(manifest.admin, None);
        assert_eq!(manifest.capabilities, vec![STORY_CONSOLE_CAPABILITY]);
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
