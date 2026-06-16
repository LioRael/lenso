use lenso_host::prelude::*;

pub const MODULE_NAME: &str = "app";
pub const APP_DATA_READ_CAPABILITY: &str = "app.data.read";

const APP_MIGRATIONS: &[Migration] = &[Migration {
    name: "app/0001_create_app_schema",
    sql: include_str!("migrations/0001_create_app_schema.sql"),
}];

/// Project-owned linked module skeleton.
///
/// Rename this module or add more modules beside it as your backend grows.
pub fn linked_module() -> HostLinkedModule {
    HostLinkedModule::manifest_only(MODULE_NAME, manifest, APP_MIGRATIONS)
}

fn manifest() -> ModuleManifest {
    ModuleManifest::builder(MODULE_NAME)
        .capabilities(vec![APP_DATA_READ_CAPABILITY.to_owned()])
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linked_module_exposes_starter_metadata() {
        let module = linked_module();
        let manifest = (module.manifest)();

        assert_eq!(module.module_name, MODULE_NAME);
        assert_eq!(manifest.name, MODULE_NAME);
        assert_eq!(manifest.capabilities, vec![APP_DATA_READ_CAPABILITY]);
        assert!(module
            .migrations
            .iter()
            .any(|migration| migration.name == "app/0001_create_app_schema"));
    }
}
