# Local source App development

Read this branch for zero-configuration App scaffolding, local discovery,
optional support, or precompiled development Hosts. This is the source-based
authoring layer over the ordinary Host Catalog and Plugin Root.

## Establish available tooling

Locate the selected CLI help and Engine/Console owner examples. As of
2026-09-20, Engine core 0.1.1 and its optional App, Runtime, Authoring, Markdown,
Worker and Host packages at 0.1.0 are published on crates.io. CLI npm versions
and Console kit releases are separate delivery streams: inspect their actual
release assets and the installed command help before claiming availability.
Use an explicitly supplied local build when requested.

With a matching source CLI, `lenso app create my-app --lang rust`, `app dev`,
`app build`, and `app start --from dist` form the default Rust workflow. The
root Cargo package can be the App-owned business Plugin; `app/` remains available
for additional local Plugins. `--runtime bun|process|wasm|multi` is the legacy
nested starter path, not the default Rust project layout. `plugins/` keeps
Instance intent. Only additional shared directories/globs/bundles need
`plugin_sources` in `lenso.toml`. These sources are not marketplace endpoints;
shared candidates require explicit Root selection. App-owned Plugins have
disableable default Instances when they use the single-Plugin package metadata
fallback. Independently source-declared Rust Plugins in public library modules
are discovered separately and require explicit `plugins/<id>/<instance>.toml`
selection. Their generated Host links the selected module anchors and retains
the ordinary configuration and dependency resolver. Existing custom file
conventions for CLI/Console/Agent remain separate supported sources; default Rust
discovery does not replace or execute those processors. See
[multiple Plugins](../../../../examples/multiple-plugins/README.md).
In a source App, use
`lenso plugins disable <plugin-id> default --root <source>` or the matching
`enable` command to change only that App-owned source marker. These commands
reject shared Plugins and Host defaults;
an existing `dist` keeps its previous state. Run `lenso app build`, then
`lenso app check` and `lenso app show` on the new distribution to verify the
change. Use `--root <dist>` for an existing built Plugin Root.

For an exact signed linked Cargo release, inspect `lenso app add --help` and
adopt the selected version with its signed snapshot, trust file, and matching
`.crate` or V6 Bundle. Rebuild, then run `app check` and `app show`; use
`app unadopt` to withdraw that source from the next build. Check registry
availability separately: a locally built CLI may select package versions that
have not yet been published, and a source-mode path dependency is not a
packaged-consumer result.

Inspect the source App with `app discover --json`. Run `app check` and
`app show` against the built distribution, whose Host Catalog now exists.
Build output must be a new directory. Keep custom Hosts and lower-level APIs
available; generated Plans remain diagnostic artifacts.

## Optional surfaces and no-Rust Hosts

The source App `--web` starter uses `[package.metadata.lenso.web] preset = "v1"`.
It infers one root Plugin provider, reads optional explicit `src/routes` handlers
and filesystem `src/app/**/route.rs` handlers, and stages existing Endpoint
bindings before native Host compilation. The default source needs no authored
processor list, build script or generated include. Use App build/dev (or Plugin
dev) to lower it; plain Cargo on its authored package does not run App lowering.
Keep legacy build-script projects on their existing explicit path. See the
[Web/Contracts APIs](../../../docs/architecture/engine-web-contracts.md) for
method attributes, stable IDs, additive middleware scopes and overrides.

Select support before expecting its files to have meaning. For CLI support use
the supplied `app create --cli` or `app add @lenso/cli` path; that adoption
uses bundled support, not a marketplace lookup. For Console use the matching
Console development kit's `lenso app create my-app --console`. Its launcher
provides Engine, Bun, native Host and SDK/compiler closure; open the HTTP address
printed by `app dev`. No Agent process is required.

The Console scaffold records the installed kit's local `development_host` and
`plugin_sources` paths. Relocation requires updating both. Native Rust additions
require a compatible precompiled Host or a source build with Cargo. Target,
source-identity and integrity admission must fail explicitly rather than falling
back to compilation. Initial page dependency installation needs registry access;
the built runtime distribution must run without source/toolchain downloads.

## Completion

For incremental development, keep using `app dev`. Existing Instance TOML
edits may reuse locked execution artifacts; source, contracts, dependency or
resource changes retain the normal build path. Configuration still activates
through a fresh checked Host generation. A selected convention compiler keeps
its build semantics. An explicitly configured frontend keeps its own reload
loop. Agents may read the existing MCP entry's `project_dev_feedback` tool for
the last classification and measured result; no generated snapshot inspection
is required. See [the small dev App](../../../../examples/incremental-dev/README.md).

Prove source discovery, a real build/start and one observable operation. Disable
support and verify its contributions and private compilation disappear. A failed
development build keeps the previous generation; successful builds restart the
Host, potentially on a new dynamic port. This is not React Fast Refresh or a
zero-downtime promise. The Console kit has native macOS ARM64 and Linux x64
CI evidence, including extracted-archive, clean-PATH and real HTTP allow/deny
checks. Windows is outside this POSIX package. Verify the downloaded archive's
checksum and preserve executable modes by extracting its tar.gz payload. The
SDK is bundled with the kit; a kit release does not imply an npm SDK alias.
