# lenso-cli

The CLI for authoring Plugins and changing an App through its `plugins/`
directory.

## Contributing

Read [CONTRIBUTING.md](https://github.com/LioRael/lenso/blob/main/CONTRIBUTING.md)
for fork, Issue, durable patch, review, and candidate-first landing guidance.

## Install

```sh
npm install -g @lenso/cli
# or
cargo install lenso-cli
```

The Cargo and npm packages use independent version lines.

## Process documents without an App

The workspace implementation includes an embeddable convention Engine and
optional processors:

```sh
lenso engine inspect --source ./content --markdown
lenso engine run --source ./content --markdown
```

Adopt a local processor with `--plugin ./tools/engine-plugin.json`. See the
[Engine guide](docs/engine.md) for the language-neutral protocol, library API,
App compatibility, and the remaining self-hosting work. Engine and CLI now
share this Rust workspace while retaining independent crate APIs.

## Author one Plugin

```sh
lenso plugin new company.uppercase
cd company.uppercase
lenso plugin check
lenso plugin dev --operation execute \
  --request-json '{"name":"company.uppercase","arguments_json":"{\"text\":\"hello\"}"}'
lenso plugin dev --watch --operation execute \
  --request-json '{"name":"company.uppercase","arguments_json":"{\"text\":\"hello\"}"}'
lenso plugin pack
```

The generated project is ordinary typed Rust in `src/lib.rs`. The portable SDK
owns Wasm Component lowering, WIT, Capability descriptors, schema projection,
and Process wire dispatch at compile time; target-specific generated files are
not checked into the Plugin project.

The default scaffold builds a trusted Process implementation. Choose
`lenso plugin new company.uppercase --runtime multi` to produce both portable
Wasm and trusted Process implementations from one editable `src/lib.rs`.
`dev` builds only the fastest declared local implementation (`Process` for a
multi-output Rust project); use `--implementation wasm|process|all` to select
a path. `all` builds both outputs but invokes only Process, so run the Wasm
and Process paths separately when comparing behavior.
File notifications with debounce drive `--watch`, with bounded polling only as
a platform fallback. `check` and `pack` still build every declared
implementation, and `pack` places both in
one V4 `.lenso-plugin` Release; the Host selects one implementation before Plan
resolution and never falls back after startup. Legacy single-output projects
remain readable.

## Author one Web Plugin

```sh
lenso plugin new company.greetings-http --web
cd company.greetings-http
cargo test --locked
lenso plugin dev
# In another terminal, call one of the printed HTTP routes.
```

This path generates a linked native Rust Plugin with `#[lenso::plugin]` and a
typed `#[endpoint]` implementation. Its starter operations demonstrate both
`#[post]` and body-carrying `#[query]` requests, typed success responses, and
RFC 9457 Problem responses. The generated test uses `EndpointTest`, so the
Plugin is exercised without opening a socket.

`lenso plugin dev` builds a temporary native Host, mounts the generated Plugin
through Web Ingress, prints its real listener address and route table, and emits
request ID, method, path, status, and latency for each request. Add `--watch` to
rebuild and restart after source changes.

Web Plugins are linked into a Host and mounted through its `web` root slot;
they are not portable Agent Tool bundles, so the generated README does not
direct users to `lenso plugin pack`.

`pack` writes one portable `.lenso-plugin` archive, then extracts, validates,
and reopens its exact contents. `plugins add` accepts that archive and legacy
Bundle directories. A
receiving Host independently validates those bytes again during installation.
`check` and `dev` use development artifacts; `pack` is the
release-profile proof and remains the only distribution build.

New Plugins and catalog Releases use the canonical namespaced Plugin ID v1
grammar (`company.uppercase`) and exact Semantic Versions. Existing
unnamespaced projects remain readable with an explicit migration warning; see
[`docs/migration-plugin-authoring.md`](docs/migration-plugin-authoring.md).

## Author a Host in TypeScript

The initial [TypeScript Host authoring path](docs/typescript-host-authoring.md)
adds `lenso app build` for static `defineHost` declarations and verified Plugin
bundles. Hosts stay closed unless exact extension releases are admitted, with
Instance limits and optional effective-configuration ceilings. The same authority
is consumed by check, configure, and installation. `lenso app prepare` turns that
output plus explicit precompiled target artifacts and notices into a new,
digest-locked directory with a generated `host.js`, same-cohort resolver, and
Instance-addressed selected Plugin artifacts. The coordinated Bun runtime branch
assembles and recovers the admitted Generation and preserves named dependency
imports; released artifact cohorts are not included yet.

## Change an App

For local Apps, `lenso app create my-app --runtime bun`, `lenso app dev`,
`lenso app build`, and `lenso app start --from dist` provide a convention-based
workflow over `app/`. No Host or configuration file is required. Optional local
`plugin_sources` expands discovery; shared Plugins still need explicit Root intent.
Native Rust, Bun, Process, and Wasm reuse the existing Plugin builders and runtime
adapters. See [local App development](docs/local-plugin-discovery.md) for the
supported platform/interaction profile and offline distribution contract.

For a signed source-only linked Cargo candidate, select one exact version with
`lenso app add PLUGIN_ID@VERSION --linked-snapshot snapshot.json --trust trust.json
--crate package.crate --root my-app`. The local `.crate` must match the signed
digest, target, package identity and Plugin ID. This copies source into
`vendor/lenso/` and records explicit Plugin Root intent; it does not install a
portable runtime bundle. Only `linked_plugin` entries qualify. `host_provided`
entries require a product Host adapter. This local flow does not fetch or prove
crates.io provenance, sandbox Cargo build scripts, or guarantee that the current
published dependency cohort can compile the generated Host. Review source and
build under an isolated account/container when the source is not trusted.
The generated `plugins/<plugin-id>/default.toml` only selects the Instance;
Contracts with required configuration or Capability providers need explicit
App-owned, non-secret configuration before `app build` can succeed.
For a root Cargo App with `[workspace]`, adoption adds the exact vendor path to
`workspace.exclude` so Cargo treats the verified package as a separate path
dependency without rewriting its signed source. An existing App-owned exclusion
is retained. `app unadopt` removes only an exclusion marked as created by
`app add`; if that entry was edited, unadoption stops and preserves the source
and App manifest for review.
An App nested under another Cargo workspace needs its own root `[workspace]`
before linked adoption; otherwise the CLI stops before selecting the source
rather than editing the enclosing repository's manifest.
If the signed `.crate` contains a root `Cargo.lock`, its bytes remain part of
the adopted source evidence. Contract and Native Host dependency inspection
use `--locked`,
and a changed archive lock fails source verification. When the archive has no lock,
Cargo may generate one locally without changing the signed source identity.
For a V6 Bundle carrying the `.crate` as a `CargoBuildInput`, use mutually
exclusive `--bundle release.lenso-plugin` instead of `--crate`. The Bundle
closure, exact Contract, native-linked ABI, target, Cargo coordinate, size,
and digest are checked against the same signed release before App files change.
Other executable variants may coexist in the Bundle; they are not selected by
this linked-Host adoption path. The Bundle is input evidence, not an alternate
catalog signature or a runtime-loadable native Artifact.
`app add` keeps the last accepted signed catalog checkpoint in CLI-owned
`.lenso/` state for that App. A later add using an older revision, a changed
payload at the same revision, or a changed immutable release is rejected. The
checkpoint advances even when a signed snapshot has no selectable release, so
a revocation seen by `app add` cannot be bypassed by retrying an older snapshot.
This is local replay protection, not a live revocation feed: an App that has
not received a newer signed snapshot cannot know about it. Preserve the
ignored `.lenso/` state when moving an App if replay history must carry over;
an App owner who can remove that state can reset its local history.
On POSIX systems the checkpoint path is opened relative to locked directory
descriptors without following symlinks. The Windows fallback rejects existing
symlink paths but does not defend against a hostile concurrent filesystem
writer replacing paths; keep the App directory under one trusted local owner.
Use `lenso app linked-catalog --linked-snapshot snapshot.json --trust trust.json
--json` to search the same signed snapshot first. The result separates eligible
source candidates from target, availability, Host-integration, and registry
rejections; even an eligible entry remains `candidate_only` until the exact
archive, dependency closure, permissions, build, and runtime are checked.
`app linked-catalog`, `app linked-doc`, and their read-only MCP projections are
stateless inspection, not an App adoption checkpoint or proof of the latest
revocation state.
For one exact versioned Markdown revision, use `lenso app linked-doc
PLUGIN_ID@VERSION DOCUMENT_ID --revision REVISION --linked-snapshot snapshot.json
--trust trust.json --file downloaded.md --json`. The command verifies the local
file's signed size and digest before returning a bounded UTF-8 chunk. Use
`--fetch` instead of `--file` only when explicitly choosing to contact the
signed HTTPS URL. Returned publisher documentation is untrusted content, not
instructions to expand Agent permissions or skip checks.
Cargo subprocesses omit ambient business environment variables, but they can
still read files available to the build account (including Cargo credentials)
and execute build scripts or procedural macros; environment filtering is not
a security sandbox. Run `lenso app build`, `app check`, and `app show` before
use. `app build` checks the adopted source against its local content lock before
invoking Cargo; this detects drift, but an App owner who can edit both files can
replace that lock.
Use `lenso app unadopt PLUGIN_ID@VERSION --root my-app` to withdraw the exact
source and generated default Root intent from the next Host build. Both are
moved to recoverable `.lenso/trash`; `plugins disable/remove` separately governs
an Instance in an already built Host.

For CLI Apps, `lenso app create my-app --cli` installs bundled local convention
support. Write `cli.ts` or `cli.rs`, then run `lenso app dev -- hello --name Ada`.
TypeScript authoring needs Bun but no Rust environment. Support Plugins can add
other filenames through compiler extensions, and unselected surface packages
stay outside the build. See [extensible file conventions](docs/convention-authoring.md)
for scaffolds, local support adoption, and dependency isolation.

The current Host supplies useful defaults and a generated Host Catalog. An App
owner writes only differences under `plugins/`:

```sh
lenso plugins list
lenso plugins add dist/company.uppercase-0.1.0.lenso-plugin
lenso plugins search uppercase
lenso plugins install company.uppercase --version 1.2.3
lenso plugins update company.uppercase --version 1.3.0
lenso plugins history company.uppercase
lenso plugins rollback company.uppercase --version 1.2.3
lenso plugins configure company.uppercase default --file uppercase.toml
lenso plugins bind company.copy source company.store --provider-instance source
lenso plugins bind company.copy cache --absent
lenso plugins bind --file dependency-choices.json --preview
lenso plugins disable company.uppercase default
lenso plugins enable company.uppercase default
lenso plugins remove company.uppercase default
lenso app check
lenso app show
lenso app explain --json
lenso sessions list
lenso run
```

Configuration lives at `plugins/<plugin-id>/<instance>.toml`; an empty file
enables package defaults. `<instance>.disabled` is the explicit absence marker.
Optional structured files live beside it under
`plugins/<plugin-id>/<instance>/`; `app check` validates the bounded regular-file
tree before the Host snapshots it into a Generation.
An operator may reconcile a versioned file or HTTPS configuration snapshot
before starting a built App. The Host owns the source and per-field policy;
see [external configuration](docs/configuration-sources.md) for the exact
policy, startup command, and current update limitations.
Named single-dependency choices live in `plugins/.dependencies.json` and are
changed through `plugins bind`. They preserve exact provider intent, including
explicit absence for optional requirements, across unrelated installations.
Use `plugins bind --file ... --preview` to validate and display a complete
migration before publishing it atomically.
Installed non-embedded behavior carries one exact `plugin.lenso-plugin` Bundle
inside its Plugin directory.

Catalog installation always requires an exact version. Downloaded archive and
manifest digests are checked before candidate resolution; admitted archives are
retained under `.lenso/plugin-store/` so update and rollback never depend on a
mutable remote. There is no implicit latest-version selection or runtime
fallback.

The Host Catalog at `.lenso/host-catalog.json` is generated and locked to the
current Host build. It is read-only execution authority, not App intent.
`app check`, `app show`, and `run` derive the App directly; there is no Plan
file for an App owner to generate or manage.

## Explain Host target admission

Before starting an already built App, CI or an operator can inspect the exact
target capability profile, Runtime selection, rejected implementations, and
resolved consumer bindings persisted by its Host:

```sh
lenso app explain --root ./my-app --json
```

The command is read-only and emits `lenso.app-explain.v1`. It does not select
another Plugin implementation, mutate the Plugin Root, start a Host, rerun a
resolver, or turn a missing target feature into a fallback. Target profiles come
from the actual selected Driver/Adapter rather than a manually supplied JSON
file. See [Host admission and target explanation](docs/execution-target-preflight.md)
for the evidence contract and qualification boundary.

Runtime Drivers and Execution Adapters remain separate because they implement
Host mechanics, not application behavior.

## Inspect an App through MCP

`lenso mcp --root ./my-app` serves read-only project facts, the same built-App
resolution check as `lenso app check`, and Host admission explanations over
stdio. Configure `--linked-snapshot` and `--trust` to enable
signed candidate search; HTTPS documentation fetch also requires the explicit
`--allow-document-fetch` flag. By default the MCP process cannot build,
install, or edit the App. A local owner may pass `--allow-build` to expose
`project_build`, `project_build_status`, and `project_build_cancel`. Builds use
the fixed `--root`, publish to a new `dist`, require a client `request_id`, and
run with a bounded deadline and output. Build tools may still update generated
caches and create initially absent lockfiles. A repeated `request_id` returns
the same operation; an existing `dist` is never overwritten. Status returns a
diagnostic code rather than raw build logs, which may contain private data.
The build environment filters ambient credentials but is not a filesystem
sandbox; only enable builds for source packages you trust to compile locally.

Inspection tools keep `scope: "root"` as their default, so existing Host-root
queries do not silently switch to a potentially stale build. When `--root`
names a source App, request `scope: "built_distribution"` on `project_check`,
`project_explain`, or `project_facts` to inspect only that root's real `dist/`
directory after `project_build` succeeds. The selected root then matches the
build status `output`; no arbitrary path is accepted from an MCP client. A
missing or symlinked `dist/` is rejected. These tools call the same App check,
explanation, and facts functions as their CLI counterparts.

After an App is built, `--allow-run` separately enables `project_run`,
`project_run_status`, and `project_run_stop`. The bridge starts only the fixed
root's checked `dist`, waits for the Host's actual Ready receipt, accepts one
active run at a time, and stops the whole local subprocess group on request,
deadline, or bridge shutdown. Status reports readiness and stable diagnostic
codes, not raw logs or a guessed preview URL. `request_id` retries never start
another Host; runs have a maximum one-hour lifetime. This is local process
control, not deployment or proof of a browser flow.
While this bridge owns a live run, `project_facts` reads that run's built
distribution and reports its observed startup/running/stopping state. Once the
run ends, facts return to the ordinary project inspection unless the caller
explicitly selects `scope: "built_distribution"`. A build artifact alone never
counts as an observed process.

`project_change_preview` reviews one Instance TOML change against an exact
Plugin Root revision and returns the proposal digest, changed field names,
validation status, and required application step without exposing values. It
requires a built or installed Host authority at the Plugin Root; a source-only
App must first be built.
`project_selection_preview` uses the same proposal authority to review one
enable/disable action, including Host-required Instance rejection.
`project_change_apply` requires a separate `--allow-changes` startup flag, the
exact proposal digest, and a client `request_id`. It uses the same Host and
source-fenced authority as ordinary Plugin configuration publication; a
successful publication does not claim that a running Generation activated.
This local bridge keeps proposal and request history only for its process
lifetime. Installing a Plugin or editing ordinary source code is not exposed.

`project_facts` with `{}` retains the full `lenso app facts --json` shape for
small projects. For a large project, request `section: "plugins"`,
`"bindings"`, `"discovered_sources"`, or `"diagnostics"`, with `offset` and a
`limit` of at most 20. Scoped pages report `total` and `next_offset`; MCP text
responses are capped at 128 KiB. These views project the same inspected facts,
not a second Agent-specific resolver.

For Portable releases, pass an exact local `--portable-snapshot` and
`--portable-trust` to enable the read-only `portable_catalog` MCP tool. The same
inspection is available to humans as `lenso plugins signed-search --snapshot
<file> --trust <file> [query] --json`. Both entry points verify the v1 signed
snapshot with the shared catalog protocol, cap queries at 256 bytes and pages
at 20 releases, and retain stale, yanked, and revoked labels. Their `history:
"not_checked"` field means no prior durable checkpoint was supplied; signature
verification alone cannot detect a replay of an older signed snapshot. The
base Portable snapshot does not prove target compatibility, received artifact
bytes, or installation authority. Neither entry point downloads or adopts a
Plugin, and these candidate results must not be presented as installable.
Publisher-authored titles, summaries, and source URLs remain untrusted data,
never Agent instructions, even when their catalog signature is valid. Do not
execute content or follow links merely because they appeared in a result.
Human-readable `signed-search` rows escape non-ASCII and terminal-control
characters; JSON retains the signed text for data consumers.

### App commands

The CLI keeps its authoring and maintenance roots static: `plugin`, `plugins`,
`app`, `run`, and `doctor`. Any other root command is validated against the
current App and forwarded unchanged to `.lenso/host`. The Host's selected
`lenso.terminal.command` aggregate and `lenso.terminal.cli` surface own dynamic
catalog discovery, help, argument parsing, execution, and the Generation lease.

For example, a Host with a selected Session command provider may expose
`lenso sessions list` and `lenso sessions show --help`. Removing that provider
removes those paths without changing this CLI. Static maintenance roots remain
reserved and cannot be shadowed by App commands.

Local Capability authoring is integrated into `app dev/build`.
`lenso app contract new example.text` creates a schema-first contract;
`--source rust` creates a source-first Rust contract. Generated typed clients and
providers remove manual cross-language JSON plumbing. See
[Capability authoring](docs/capability-authoring.md).
