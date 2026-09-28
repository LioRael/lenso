# Local App development

The local Host template implements [plan #727](https://github.com/LioRael/lenso/issues/727)
and [ADR 0075](https://github.com/LioRael/lenso/pull/730). This guide describes
the source-checkout workflow; see [CLI installation](../README.md#install) for
the distinction between candidate builds and released packages.

```sh
lenso app create my-app
cd my-app
lenso app dev
# In another terminal, or after stopping development:
lenso app build
lenso app start --from dist
```

No App configuration file, Host declaration, preset, or activation flag is required.
By default, `app create` (also `--lang rust`) makes the root Cargo package an
App-owned Process notes Plugin. It provides `POST /notes` and `GET /notes/{id}`
through the typed HTTP Endpoint Capability. Notes are in-memory development
data and do not survive a restart. Process Plugins are trusted native
executables, not sandboxed. Additional App-owned Plugin projects live in `app/`.
`plugins/` retains instance configuration, disabled markers, and named dependency
choices. `app create --web` selects the nested linked native Web scaffold with
Plugin-owned HTML. `--runtime process`
explicitly selects the nested Process starter; `bun`, `wasm`, `multi`, and `empty`
are also available. `--no-install` leaves normal language dependency installation
and the initial compile check to the developer.

Add **local discovery sources** only when needed:

```toml
# Optional lenso.toml, relative to this App root.
plugin_sources = ["../shared-plugins", "../packages/*", "../artifacts/example.lenso-plugin"]
```

Shared candidates are discoverable but are not built/admitted until an existing
Root file such as `plugins/example.audit/default.toml` explicitly selects them.
An empty/comment-only TOML file uses the Plugin's defaults. App-owned Plugins get
a disableable `default` Instance. Duplicate identities, ambiguous providers,
invalid configuration, and disabled required providers fail before publication.

## Exact linked Cargo catalog adoption

The source-only Marketplace channel is distinct from a portable Bundle. Given
an exact signed linked-Cargo snapshot, an independently configured public trust
file, and the matching registry `.crate` already downloaded by the operator:

```sh
lenso app add example.web@0.4.5 --root ./my-app \
  --linked-snapshot ./linked-cargo.json --trust ./catalog-trust.json \
  --crate ./example-web-plugin-0.4.5.crate
lenso app build --root ./my-app --out ./dist-web
lenso app check --root ./dist-web --json
lenso app show --root ./dist-web/intent --json
```

For a V6 Release Bundle whose selected native variant contains a `.crate`
`CargoBuildInput`, replace `--crate` with `--bundle ./release.lenso-plugin`.
This accepts a directory, not a registry lookup. The signed linked-Cargo
snapshot remains the authority for the exact Plugin ID, version, supported
target, package, and archive digest; Bundle verification additionally closes
the input size, Cargo identity, native ABI, and all other variant files. The
Host-build path requires one target-matching native build input. Other
executable variants may coexist but are not runtime fallback candidates for
this adoption. Requirements that this generated Host cannot prove are rejected
before any App source or Plugin Root change.

If publication is interrupted after verification, `app add` reports the
source, `lenso.toml`, and Plugin Root paths. Retry with the same exact signed
snapshot and archive or Bundle: the source and configuration are checked again,
and missing selection intent is published without overwriting an existing
Plugin Root entry. A detected conflicting edit is preserved for review. This is
recoverable multi-file publication, not an atomic filesystem transaction or a
lock respected by external editors. An external path replacement can still
race the final symlink check and rename; use a trusted App directory.

`app add` verifies the catalog signature, exact listed version, Native Host
target, `.crate` digest, archive contents, and linked Plugin source identity.
It vendors the exact source under `vendor/lenso/`, records the source digest,
and selects it through the App's Plugin Root. It does not download the crate,
prove registry provenance by itself, or make a `host_provided` integration a
generic candidate. Review build-time code before compiling it.

Selection creates a comment-only `plugins/<plugin-id>/default.toml`, not a
ready-to-run configuration for every Plugin. If the selected Contract requires
fields or other Capabilities, fill this App-owned intent with the required
non-secret values and explicitly select/configure the providers before
`app build`; keep secret values outside Plugin Root. `app build` fails closed on
missing fields or providers. A modified intent is user-owned: `app unadopt`
refuses to remove it until those changes are resolved explicitly.

To remove an unchanged adopted source, use `lenso app unadopt
example.web@0.4.5 --root ./my-app`, then rebuild and check the App. Unadoption
moves both source and default Plugin Root intent to a recoverable `.lenso/trash/`
entry. It refuses edited source or user-modified Plugin Root intent; resolve
those changes explicitly before retrying. An upgrade uses an explicit
unadopt/add/build/check sequence for two exact versions, not a runtime fallback
or implicit semver selection. A signed catalog listing alone does not prove a
release is installed, buildable, or running.

## Exact signed Portable catalog adoption

`plugins signed-search` only browses a signed Portable snapshot. To select
one exact listed Bundle for a source App, provide the same snapshot, its
independently configured public trust file, and the exact archive bytes:

```sh
lenso plugins signed-search --snapshot ./portable.json --trust ./catalog-trust.json --json
lenso app add example.tools@0.1.0 --root ./my-app \
  --portable-snapshot ./portable.json --trust ./catalog-trust.json \
  --archive ./example-tools-0.1.0.lenso-plugin
lenso app build --root ./my-app --out ./dist-tools
lenso app check --root ./dist-tools --json
lenso app show --root ./dist-tools/intent --json
lenso app start --from ./dist-tools --check
```

If that release provides `lenso.agent.tool-provider@2`, a portable-only Host
binds its exact Instance to the local Tool CLI. After checking the binding with
`app show`, a user can call it explicitly. The execution example assumes the
catalog lists an `uppercase` tool accepting `text`:

```sh
lenso app tools catalog --from ./dist-tools --provider example.tools/default
lenso app tools execute --from ./dist-tools --provider example.tools/default \
  --name uppercase --arguments-json '{"text":"Lenso plugin"}'
```

This path does not expose the provider to a model or MCP client. A mixed
generated-native Host currently rejects `app tools`; it does not silently
execute an unbound provider. Do not put secrets in `--arguments-json`: command
arguments can be visible to other local processes.

`--origin HTTPS_ORIGIN` may replace `--archive`; it explicitly permits only
that HTTPS origin for the signed artifact URL. A listing or URL alone never
grants download, source selection, or Host installation authority. Adoption
verifies the original signed payload bytes, accepted catalog history,
availability, exact ID/version, archive size and SHA-256, and Bundle manifest
identity before changing the App. It vendors the archive under
`vendor/lenso/portable/`, writes an exact artifact lock, and selects the
source with `lenso.toml` plus `plugins/<id>/default.toml`. A failed validation
does not add source or Plugin Root intent. Retry with the same bytes is
idempotent; a conflicting existing source is preserved, not replaced.

To remove the source App selection, use `lenso app unadopt
example.tools@0.1.0 --portable --root ./my-app`, then build to a new output
and check/show that output. This moves the unmodified generated intent to
`.lenso/trash/portable/` and retains the exact archive under `vendor/` for
audit or explicit cleanup. It refuses user-modified intent. Source App
upgrades use explicit unadopt/add/build/check, not `--replace` or runtime
fallback. During later builds, the local lock detects archive drift; it is
not an independent signature authority if the App owner changes both the
archive and lock. Keep the public trust decision outside untrusted source
edits when signer provenance matters.

For an already built Plugin Root, use the separate Host mutation:

```sh
lenso plugins signed-install example.tools --version 0.1.0 \
  --snapshot ./portable.json --trust ./catalog-trust.json \
  --archive ./example-tools-0.1.0.lenso-plugin --root ./dist-tools/intent
lenso plugins configure example.tools default --root ./dist-tools/intent
lenso app check --root ./dist-tools --json
```

Built-root installation does not silently create an Instance; configuration
and dependency choices remain App-owned. Both commands use the same signed
release and archive verification, but source adoption and built-root mutation
are different permissions and artifacts.

## Exact signed editable content

An optional `lenso.marketplace.release-content.v2` snapshot can attach a
template or development extension to the same exact Plugin ID and version as
a signed Portable, linked Cargo, or npm-only package release. It can also
describe a `content_only` release with no executable or package base. It is a
separate signature and checkpoint; the v1 release bytes and adoption behavior
do not change. Use `--linked-snapshot`, `--portable-snapshot`, or
`--package-snapshot` for the respective signed base. Omit a base snapshot only
for a signed `content_only` release with publisher, purpose, exact source
revision, and versioned digest-bound Markdown metadata. The content archive is a local, downloaded
`.tar.gz`; this command does not fetch its signed URL.
This content-copy path currently requires Unix filesystem primitives.

```sh
lenso app add example.web@1.2.3 --root ./my-app \
  --linked-snapshot ./linked.json --trust ./catalog-trust.json \
  --content-snapshot ./content.json --content-id react-template \
  --content-archive ./react-template.tar.gz \
  --content-destination frontend/from-example --content-preview

# After reviewing the exact file plan, repeat without --content-preview.
lenso app add example.web@1.2.3 --root ./my-app \
  --linked-snapshot ./linked.json --trust ./catalog-trust.json \
  --content-snapshot ./content.json --content-id react-template \
  --content-archive ./react-template.tar.gz \
  --content-destination frontend/from-example

# A content-only template has no fake runtime/package base.
lenso app add example.starter@1.0.0 --root ./my-app \
  --trust ./catalog-trust.json --content-snapshot ./content.json \
  --content-id react-template --content-archive ./react-template.tar.gz \
  --content-destination frontend/from-starter

# A package-only release can supply optional editable source without
# installing its npm distribution.
lenso app add example.web@1.2.3 --root ./my-app \
  --package-snapshot ./package.json --trust ./catalog-trust.json \
  --content-snapshot ./content.json --content-id react-template \
  --content-archive ./react-template.tar.gz \
  --content-destination frontend/from-package
```

Preview verifies both current signed snapshots, the immutable base identity,
and the complete archive before showing the file list. It writes nothing to
the App. Copying advances the two independent local checkpoints and publishes
only a new App-relative directory with `.lenso-release-content.json`
provenance. Existing destinations, symlinks, path traversal, duplicate archive
entries and non-file archive entries are rejected. Copied files belong to the
App owner; a later exact adoption never overwrites edits or replaces the
directory. Updating them is a reviewed source change, not an automatic
Marketplace update.

A `development_extension` copy remains inert. The destination cannot be in
auto-discovered or generated App paths. Its archive must contain one source
Plugin with the exact signed ID/version and explicit convention declarations.
After reviewing that source, run
`lenso app add ./my-app/extensions/example-web --root ./my-app --no-install`
as a separate, explicit selection. Only that
selection permits ordinary App discovery and a later build to invoke the
extension's compiler; the signed content copy itself grants no execution
permission. The local source remains editable and is not a sandbox.

## Build, inspect, and run

```sh
lenso app discover --json
lenso app build --out ./dist-release
lenso app check --root ./dist-release
lenso app show --root ./dist-release --json
lenso app start --from ./dist-release --check
lenso app start --from ./dist-release
```

Build creates a new output directory; it never overwrites an existing one. Source
builds use the existing Plugin builders and normal installed language dependencies.
Existing Cargo, Bun, npm, pnpm and Yarn lockfiles at the App and selected
project roots are pinned from build planning through output publication. If a
package manager changes one during the build, the build rejects publication;
update the lockfile explicitly and retry. A first build may generate a
previously absent lockfile.
Native Plugins need Cargo and expose the SDK-generated `link_plugin` anchor.
Their normal Cargo contract dependencies supply typed runtime codecs; no parallel
handwritten Capability schema is required. Incompatible codec cohorts fail with
an error. Bun-only Apps use the precompiled CLI runtime and need no Rust toolchain.

The output includes the executable Host, resolver, selected artifacts, Bun when
needed, exact Host authority, Root intent, and integrity metadata. Startup verifies
immutable runtime files and uses the ordinary resolver/Kernel path. The distribution
runs without the source tree, Cargo, Bun on PATH, or runtime downloads. It is a
runtime closure, not a portable source-reproduction archive. `intent/plugins/`
remains the editable Root snapshot; `--root PATH` can select another already
initialized Plugin Root. `--check` performs real activation and clean shutdown.

`app assemble --out PATH` retains the authoring-only path for portable Plugins;
`--executable` requests the runnable closure. Native assembly necessarily generates
a Host. Existing `app build --source host.ts --target TARGET --out PATH`, custom
Hosts, and `app prepare` retain their own contracts. Dynamic terminal Plugin command
names remain available because convenience commands live under `app`.

Linked Plugins that need a private, request-pinned business policy can provide a
[business snapshot binding](business-snapshot-binding.md). Its policy and source
lifecycle belong to the Plugin, not the generated Host or the App's Plan.

## Development loop

`app dev` watches App source, Root intent, optional local sources, and native Cargo
path dependencies. Changes are debounced, rebuilt into a separate directory, then
restart the Host with graceful shutdown. Failed builds keep the last running App.
A successful build followed by a startup failure is reported; automatic rollback
or zero-downtime switching is not claimed. Ctrl-C stops the active build/Host.
Generated output and dependency/cache trees are excluded from watching.

Web routes and assets belong to the Plugin. Editing the default Process notes
Guest rebuilds its Process artifact while reusing the precompiled Host; use the
same CLI binary for build and start. The explicit `--web` starter embeds its own
HTML; editing it triggers the ordinary rebuild/restart. Native instance resources
are loaded from the Root snapshot. By default, no separate frontend process or
implicit business route registry runs in the Host.

### Explicit React/Vite development process

An App with an App-owned `frontend/` directory may opt into one separate
frontend development process by adding `frontend/lenso.dev.toml`:

```toml
schema = "lenso.frontend-dev.v1"
command = ["bun", "run", "dev"]
url = "http://127.0.0.1:5173/"
backend_url_mode = "file"
```

The declared command runs in `frontend/` without a shell; configure Vite itself
to listen on that exact loopback port with `strictPort`. Use a non-default HTTP
port such as 5173; explicit `:80` is normalized away and unsupported. `lenso dev` starts this
process only when the file is present. The command is trusted App-owned code,
not a sandbox for an unreviewed package: it can access files available to the
developer account. Its environment contains only basic toolchain variables
plus `LENSO_API_URL` for initial compatibility and `LENSO_API_URL_FILE`, which
names `.lenso/dev-backend-url` under the App root. Host secret environment
variables are not inherited by the frontend process. The file is updated
atomically whenever the Web Host generation changes.
Only one `app dev` session may own an App root at a time. The persistent
`.lenso/dev.lock` file is locked for the session and is not removed on exit;
a second session exits before building or changing the backend URL file.

The frontend must read `LENSO_API_URL_FILE` for each API proxy request and expose
`GET /__lenso/backend` with HTTP 200 and an unchunked plain-text body equal to
the current file value (an optional trailing newline is accepted). This
handshake is part of the explicit dev contract: it proves the frontend has
observed the candidate backend URL before the previous Host is stopped.
`lenso dev` reports the preview URL only after the Host Ready Gate, frontend
HTTP `/` readiness, and this backend handshake all pass. A failed frontend
candidate retains the previous running generation. A backend rebuild reuses
the same frontend process and refreshes its backend URL through the file;
frontend source changes are left to the declared frontend's HMR without recompiling the Host.
Changing the dev command/configuration requires restarting `lenso dev`.

This is a source-development preview, not a static-asset build. The built Host
continues to serve its last explicitly built Plugin-owned assets until a
frontend build copies new assets there. A custom frontend that reports the
handshake but proxies API traffic through a different target violates its own
contract; the handshake is not a general browser-flow test.

## Supported local runtime profile

| Source | Build/runtime path | Boundary |
| --- | --- | --- |
| Rust native linked | Generated Host + normal SDK factories | Cargo required at build time |
| TypeScript/Bun | Existing Bun builder + shipped Bun Adapter | Bun required at build time; bundled for deployment |
| Rust Process | Existing release builder + Process Adapter | Native build target |
| Rust Wasm | Existing Wasm builder + Wasm Component Adapter | Existing SDK target/toolchain required |
| Composite Rust/Bun | Existing composite builder and Contract equivalence | One deterministic implementation selected |
| Web | Native HTTP Endpoint Plugin + generated ingress | Assets remain Plugin-owned |
| QuickJS / dylib | Discovery can inspect verified Bundles | Local source/runtime integration deferred |
| Python / other languages | No SDK path established here | Not claimed by this workflow |

The current executable profile supports macOS ARM64, Linux x86_64, and Linux
ARM64. Other platforms fail explicitly instead of choosing a different runtime.
Pure portable Capabilities support Request interactions through verified
generated Descriptor evidence. Stream/Event boundaries require typed codecs
from native contract projections. Old Bun archives without embedded generated
Descriptor evidence need repacking for the generic portable Host; custom typed
Hosts remain available.

## Discovery contract

- Paths are relative to the App root, independent of the calling directory.
  Absolute paths and component globs (`*`, `?`, character classes) are supported.
  A directory is scanned recursively; recursive `**` globs are rejected to keep
  traversal bounded. Missing explicit roots and unmatched patterns are errors.
- `app/` candidates have `app_owned` provenance. Extra roots have `shared`
  provenance. Neither value means enabled or admitted. Scanning never writes
  Plugin Root files or starts a build, package script, Plugin, or Generation.
- Cargo and npm workspace roots select declared members (including Cargo
  excludes). Otherwise directories are containers. Workspace members outside
  their workspace require an explicit source entry. pnpm-only workspace YAML
  parsing is deferred; configure its package directory/glob explicitly.
- Recognized Plugin projects terminate traversal. Composite implementation
  subprojects belong to their parent Plugin, not additional default Instances.
- Ignore `.git`, `.lenso`, `target`, `node_modules`, `dist`, `build`, `.next`,
  `.venv`, `__pycache__`, `plugins`, and hidden directories during traversal.
  An explicitly named root can still point to an artifact in an output directory.
- Canonicalize roots and deduplicate repeated same-role paths. Reject any
  encountered overlap between App-owned and shared roots. Skip nested symlinks;
  an explicit source symlink resolves to its canonical root. Metadata files must
  be regular files. Traversal is limited to 64 levels and 50,000 visited entries;
  metadata is limited to 4 MiB. Existing archive verification bounds still apply.
- Sort candidates by Plugin ID. Distinct projects with the same Plugin ID fail
  even if their versions differ. Filesystem order never selects a Release or
  dependency provider. Unsupported declarations and malformed metadata fail
  with paths/context instead of becoming execution authority.

## Reused metadata and evidence

Cargo projects use existing `[package.metadata.lenso]` (`plugin-id`, `root-slot`)
and optional `[package.metadata.lenso-cli]` implementation declarations. Versions
may inherit `workspace.package.version`. Native projects are recognized from their normal `lenso`/`lenso-native-adapter`
SDK dependency or explicit `runtime = "native-linked"`. Contract-only crates are
not Plugins. Existing Web scaffold metadata remains supported; language,
execution class, and business Capability remain independent.

Bun projects use existing `package.json` `lenso.pluginId`, `lenso.runtime`, and
package version. SDK packages exposing only `lenso.build` are not business
Plugins. A project declaring both Cargo and Bun Plugin metadata is ambiguous;
use the existing composite declaration and separate implementation directories.
Composite identities and versions must agree; generated Contract equivalence
still requires the existing build/pack checks.

Source results say `source_metadata_only`: they identify build locators and
declared runtime choices, not extracted Capabilities, executable availability,
or successful admission. Local Bundle directories/archives use the existing
Bundle verifier and report `verified_bundle_not_admitted`. Bundle implementation
runtime values are exact execution-class IDs; source values are authoring runtime
names. Assembly normalizes these before explicit implementation selection.
No new handwritten Descriptor, Schema, or plugin manifest is introduced.

## Optional surface packages

Local convention selection is an authoring/build feature. It selects existing
Plugin packages or invokes an adopted compiler for standalone entry files.
See [convention authoring](convention-authoring.md) for the complete CLI example.
Run `lenso app inspect --json` to inspect the selection without executing code.

A support Plugin declares recognized filenames in its existing Lenso metadata
(`lenso` in package.json, or `package.metadata.lenso` in Cargo.toml):

```json
{"conventions":[{"id":"example.cli","entries":["cli.ts","cli.rs"]}]}
```

An owning Plugin declares independently buildable contributions in that same
metadata:

```json
{"surfaces":[
  {"entry":"cli/src/cli.ts","project":"cli"},
  {"entry":"tui/src/tui.rs","project":"tui","required":false}
]}
```

The support must have an active App-owned default or an explicit, non-disabled
Plugin Root instance. An unselected shared source grants no support. Recognition
conflicts fail regardless of filesystem ordering. Required surfaces without
support fail before compilation; optional ones report `support_not_adopted`.
Inactive package manifests are not parsed, and their dependencies are not passed
to package managers by this workflow. Dependencies deliberately placed in the
core package or its workspace remain the author's responsibility.

For mixed-language products, an optional `plugin.json` owns nested packages:

```json
{
  "schema":"lenso.plugin-project.v1",
  "core":"core",
  "surfaces":[{"entry":"cli/src/cli.ts","project":"cli"}]
}
```

The core package supplies the logical identity and version. Every selected
surface is an ordinary Plugin with a distinct runtime identity and the same
release version. Its own package manifest controls compilation. Selected
contributions receive disableable defaults and retain normal descriptor, binding,
and permission validation. There is no automatic cross-language source import,
configuration forwarding, permission inheritance, or descriptor merging.

This first profile requires one active owner instance. A multi-instance owner
fails rather than silently sharing a surface instance. Owner/support activation
is evaluated when building; changing it requires rebuilding. Runtime editing of
a distribution's Plugin Root does not recompute the build selection. Generated
`.lenso/conventions.json` records selection provenance. A changed selection during
a build aborts publication of that output.

Simple single-package Plugins need no composite manifest. Existing multi-runtime
implementations remain alternatives for one contract, separate from additive
surface packages. Compiler extensions lower standalone entries to ordinary source packages or
verified Bundles. Bundled CLI support recognizes `cli.ts` and `cli.rs`; bare
App-owned directories also work without a package manifest. Registry installation
is outside this local-source workflow.

Bun packages may set `lenso.source` to a package-relative entry file, for example
`"src/cli.ts"`. The default remains `src/plugin.ts`. The source must resolve to a
file inside its package. This changes the authoring entry location, not the
runtime contract: the entry still exports an ordinary `definePlugin` declaration.
