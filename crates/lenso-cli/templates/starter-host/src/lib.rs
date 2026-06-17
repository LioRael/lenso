mod modules;

use lenso_host::prelude::*;

/// Host-owned module composition for this application.
///
/// Add project modules here with `HostBuilder::linked_module(...)`. The default
/// keeps Lenso's configured linked profile plus any remote modules from
/// environment configuration.
pub fn host_composition() -> HostComposition {
    HostBuilder::new()
        .linked_module(builtins::auth())
        .linked_module(builtins::auth_password())
        .linked_module(modules::app::linked_module())
        .build()
}
