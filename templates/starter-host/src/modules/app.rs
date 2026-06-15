/// Project-owned linked module skeleton.
///
/// Rename this module or add more modules beside it as your backend grows.
pub fn linked_module() -> lenso_host::HostLinkedModule {
    lenso_host::HostLinkedModule::manifest_only("app", manifest, &[])
}

fn manifest() -> lenso_host::ModuleManifest {
    lenso_host::ModuleManifest::builder("app").build()
}
