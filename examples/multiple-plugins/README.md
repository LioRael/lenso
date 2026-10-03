# Multiple Plugins in one Cargo package

Each public Rust module owns its Plugin identity, descriptor, configuration,
dependencies and generated factory. The Cargo package supplies the release
version. No package-level Plugin identity or hand-written registration is needed.

Run `cargo run -p lenso-multiple-plugins-example`, then request
`http://127.0.0.1:8080/health`. Only Health is selected; `/greeting` is absent.
A custom Host can select Greeting with `.plugin::<greeting::Plugin>()` or
configure it with `.plugin_with::<greeting::Plugin>(configuration)?`.

The default source App workflow uses the same declarations. Run `lenso app
discover --root examples/multiple-plugins --json`, then `lenso app build` with
that root and a fresh output directory. `plugins/example.health/default.toml`
selects Health. To also select Greeting, add
`plugins/example.greeting/default.toml` containing `message = "Welcome"`,
then rebuild. Discovery alone never enables every Plugin in the crate.

Declare `#[lenso::plugin(id = "example.health", root_slot = "web")]` on a
public named-field struct in a public module reachable from the library target.
Separate `plugin.rs` modules can use `#[path = "../plugins/health/plugin.rs"]`
from `src/lib.rs`; the filename itself has no discovery meaning. Use one
Plugin declaration per module. Source discovery reads declarations without
running Cargo or build scripts; private and conditional modules, tests,
examples and binaries are outside this default library surface. Feature-gated
Plugins require a custom Host. Existing package metadata remains the fallback
for single-Plugin libraries. Custom CLI/Console/Agent conventions are unchanged.

Health keeps its source at `plugins/example.health/plugin.rs` beside its
`default.toml` selection. A source App build projects only the exact discovered
Rust module files out of the runtime Plugin Root; unknown files still fail
validation. The built `intent/plugins/` contains configuration and resources,
and a prepared Host still rejects source files placed into its runtime Root.
The conventional `app/` source projects and optional shared roots remain valid;
this layout needs no additional source-directory configuration.
