mod modules;

/// Host-owned module composition for this application.
///
/// Add project modules here with `HostBuilder::linked_module(...)`. The default
/// keeps Lenso's configured core/demo profile plus any remote modules from
/// environment configuration.
pub fn host_composition() -> lenso_host::HostComposition {
    lenso_host::HostBuilder::new()
        .linked_module(modules::app::linked_module())
        .build()
}
