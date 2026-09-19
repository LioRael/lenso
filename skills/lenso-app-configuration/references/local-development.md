# Local source App development

Read this branch for zero-configuration App scaffolding, local discovery,
optional support, or precompiled development Hosts. This is the source-based
authoring layer over the ordinary Host Catalog and Plugin Root.

## Establish available tooling

Locate the selected CLI help and Engine/Console owner examples. The Engine,
Console convention and precompiled Console kit are implemented locally as of
2026-09-20; registry availability must be verified separately. Use an explicitly
supplied local build when requested. Do not assume an npm upgrade installs them.

With a matching CLI, `lenso app create my-app --runtime bun`, `app dev`,
`app build`, and `app start --from dist` form the ordinary source workflow.
The default source is `app/`; `plugins/` keeps Instance intent. Only additional
local directories/globs/bundles need `plugin_sources` in `lenso.toml`. These
sources are not marketplace endpoints. Shared candidates require explicit Root
selection; App-owned Plugins have disableable default Instances.

Inspect the source App with `app discover --json`. Run `app check` and
`app show` against the built distribution, whose Host Catalog now exists.
Build output must be a new directory. Keep custom Hosts and lower-level APIs
available; generated Plans remain diagnostic artifacts.

## Optional surfaces and no-Rust Hosts

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

Prove source discovery, a real build/start and one observable operation. Disable
support and verify its contributions and private compilation disappear. A failed
development build keeps the previous generation; successful builds restart the
Host, potentially on a new dynamic port. This is not React Fast Refresh or a
zero-downtime promise. The local Console kit has macOS ARM64 evidence; Linux
native validation remains pending and Windows is outside this POSIX package.
