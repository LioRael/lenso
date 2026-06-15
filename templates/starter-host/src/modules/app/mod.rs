const APP_MIGRATIONS: &[lenso_host::Migration] = &[lenso_host::Migration {
    name: "app/0001_create_app_schema",
    sql: include_str!("migrations/0001_create_app_schema.sql"),
}];

/// Project-owned linked module skeleton.
///
/// Rename this module or add more modules beside it as your backend grows.
pub fn linked_module() -> lenso_host::HostLinkedModule {
    lenso_host::HostLinkedModule::manifest_only("app", manifest, APP_MIGRATIONS)
}

fn manifest() -> lenso_host::ModuleManifest {
    lenso_host::ModuleManifest::builder("app").build()
}
