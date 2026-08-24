# Plugin-system developer experience research — 2026-08-25

## Scope

This note compares five mature extensibility systems using only first-party
documentation and source:

- Visual Studio Code Extensions;
- IntelliJ Platform Plugins;
- Obsidian Community Plugins;
- WordPress Plugins; and
- Backstage Plugins and Modules.

It also includes a focused addendum on DeepSeek Harness. DeepSeek Harness is
kept outside the mature-system comparison because its own README labels it a
rapidly changing developer preview; the addendum audits official source at
commit `b150a551b8d465e31e418e1b2eaf5e79bbb7d28e`.
[DeepSeek Harness status](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/README.md#L5-L11)

The comparison focuses on the user-visible artifact, the relationship between
internal modules and installable plugins, extension and dependency wiring,
lifecycle, isolation, and the path from scaffold to publication. It does not
make a Lenso architecture decision. Sections explicitly labelled **Inference**
are conclusions drawn across the cited systems rather than claims made by any
one source.

## Direct conclusion

The mature systems do not ask ordinary users to enumerate all possible module
combinations or author a complete runtime graph. Their common surface is much
smaller:

1. the user installs, enables, disables, updates, or removes a named Plugin or
   Extension;
2. the Plugin declares its identity, compatibility, dependencies, entrypoints,
   and contributions;
3. the host discovers those declarations and connects them to named extension
   points or host APIs; and
4. internal source modules, services, classloaders, processes, and wiring stay
   behind that product boundary.

Backstage is the clearest example of an explicit Plugin/Module distinction:
Plugins provide independent base features, while a Module may extend only one
target Plugin and must use that Plugin's Extension Points. VS Code, Obsidian,
and WordPress expose only the Extension or Plugin as the user lifecycle unit.
IntelliJ's modular-plugin format keeps multiple modules inside one installed
Plugin and still describes that format as experimental. [Backstage backend
architecture](https://backstage.io/docs/backend-system/architecture/index/),
[Backstage Plugin Modules](https://backstage.io/docs/backend-system/architecture/modules/),
[IntelliJ modular plugins](https://plugins.jetbrains.com/docs/intellij/modular-plugins.html)

## Comparison at a glance

| System | User-visible artifact | Module versus Plugin | How composition happens | Lifecycle granularity | Runtime boundary | Primary development loop |
|---|---|---|---|---|---|---|
| VS Code | Extension directory or VSIX with `package.json` | Internal TypeScript/npm modules are implementation details; Extension is the installable unit | `contributes`, activation events, named Extension dependencies, and VS Code APIs; no user-authored binding graph | Install, global/workspace enable/disable, auto/manual update, uninstall; changes commonly restart the Extension Host | Desktop Extensions run outside the renderer but share an Extension Host; Web Extensions run in a browser worker with no Node APIs | `yo code` → F5 Extension Development Host → reload/debug/test → `vsce package/publish` |
| IntelliJ | Plugin ZIP/JAR with `META-INF/plugin.xml` | Plugin is the installed unit; experimental modular Plugins may contain required/optional modules | Required/optional Plugin dependencies plus implementations of named Extension Points | Restart-free load/update/unload only when strict dynamic-plugin constraints hold; otherwise restart | Per-Plugin classloader in the IDE JVM; split-mode modules may run in frontend/backend processes | IDE wizard → sandbox IDE via `runIde` → auto-reload/test/verify → build/sign/publish Gradle tasks |
| Obsidian | Plugin directory or release assets: `main.js`, `manifest.json`, optional `styles.css` | No public Module unit; one Plugin is one toggle and entrypoint | Commands, views, events, and settings are registered in `Plugin.onload()`; no dependency or general capability resolver in the manifest | Install, enable, disable, update; source changes require Plugin or App reload | Plugin code shares the App environment; desktop-only Plugins may use Node/Electron APIs | Official template in a test vault → `npm run dev` → reload Obsidian → GitHub Release/community submission |
| WordPress | At minimum one PHP file with a Plugin Header, normally a directory | No separate public Module unit; add-ons are themselves Plugins | Core and Plugin-defined Actions/Filters; `Requires Plugins` adds named installation dependencies, not service wiring | Install, activate, deactivate, update, delete/uninstall; active code is loaded again for each request | Active Plugin PHP executes in the WordPress/PHP request environment | Hand-written file or `wp scaffold plugin` → edit and refresh/request → Plugin Check/PHPUnit → WordPress.org release |
| Backstage | Usually one or more npm packages with Backstage package-role metadata | Explicit distinction: Plugin is independent functionality; Module is a constrained extension of one Plugin | Typed services and Extension Points; frontend feature discovery can auto-install dependency packages | Primarily package install, build, deploy, and startup wiring; not an end-user hot-unload marketplace | Default backend may host many Plugins in one Node process; Plugins may be split into separate deployments | `yarn new` → small frontend dev App or backend dev host → test/build → publish standard npm packages |

## System evidence

### Visual Studio Code Extensions

#### Artifact and declaration

Every Extension has a `package.json` manifest. Its `publisher` and `name` form
the Extension ID; `engines.vscode` states host compatibility; `main` or
`browser` selects the entrypoint; `activationEvents` states when executable
code is needed; and `contributes` statically declares commands, views, themes,
languages, settings, and other host-owned contribution types. A packaged
Extension is a VSIX, and the same `vsce` tool packages and publishes it.
[Extension manifest](https://code.visualstudio.com/api/references/extension-manifest),
[Extension anatomy](https://code.visualstudio.com/api/get-started/extension-anatomy),
[Publishing Extensions](https://code.visualstudio.com/api/working-with-extensions/publishing-extension)

VS Code does not expose its internal source modules as separately installable
units. The platform itself is modular and many core features are built as
Extensions, but the user-visible identity remains the Extension. [VS Code
source organization](https://github.com/microsoft/vscode/wiki/source-code-organization),
[VS Code Extension API](https://code.visualstudio.com/api/)

#### Wiring and lifecycle

An author declares a contribution against a known Contribution Point and then
binds its implementation with the corresponding VS Code API. Activation events
let the host load executable code lazily. The manifest can also declare named
`extensionDependencies` or an `extensionPack`, while an activated Extension can
export an API to another Extension in the same Extension Host. This is named,
explicit integration, not a general dependency-injection or capability-graph
resolver. [Extension anatomy](https://code.visualstudio.com/api/get-started/extension-anatomy),
[Activation Events](https://code.visualstudio.com/api/references/activation-events),
[Extension manifest](https://code.visualstudio.com/api/references/extension-manifest),
[VS Code API `extensions`](https://code.visualstudio.com/api/references/vscode-api#extensions)

Users can install, disable globally or per workspace, enable, update, and
uninstall through the UI or CLI. Disabling, updating, or uninstalling normally
prompts the user to restart Extensions, which restarts the Extension Host rather
than requiring every Extension to support arbitrary in-place graph mutation.
The Extension entrypoint has one `activate()` call and an optional
`deactivate()` cleanup hook. [Extension Marketplace management](https://code.visualstudio.com/docs/configure/extensions/extension-marketplace),
[Extension anatomy](https://code.visualstudio.com/api/get-started/extension-anatomy)

#### Isolation and developer experience

Desktop Extensions execute in local or remote Node Extension Host processes,
separate from the renderer, and are lazily activated. This protects UI startup
and responsiveness, but all Extensions for a window can share an Extension
Host, so it is not a per-Extension security sandbox. Web Extensions instead run
in a browser WebWorker and cannot use Node APIs or spawn child processes.
[Extension Host](https://code.visualstudio.com/api/advanced-topics/extension-host),
[Extension capabilities](https://code.visualstudio.com/api/extension-capabilities/overview),
[Web Extensions](https://code.visualstudio.com/api/extension-guides/web-extensions)

The official path is deliberately one continuous loop: `yo code` scaffolds the
manifest, TypeScript entrypoint, build tasks, and launch configuration; F5 opens
a separate Extension Development Host; the same editor provides breakpoints
and reload; `@vscode/test-electron` or `@vscode/test-web` runs integration tests
inside a real host; and `vsce package` or `vsce publish` produces the release.
[Your First Extension](https://code.visualstudio.com/api/get-started/your-first-extension),
[Testing Extensions](https://code.visualstudio.com/api/working-with-extensions/testing-extension),
[Publishing Extensions](https://code.visualstudio.com/api/working-with-extensions/publishing-extension)

The simple surface comes from one manifest, a stable catalog of named
Contribution Points, host-owned discovery and activation, and a dev host that
behaves like production. The trade-offs are a static contribution schema,
explicit cross-Extension coupling, broad authority for desktop Extensions, and
coarse Extension Host restarts for many lifecycle changes.

### IntelliJ Platform Plugins

#### Artifact and declaration

The user installs a Plugin ZIP/JAR whose `META-INF/plugin.xml` contains identity,
version and platform compatibility, required or optional Plugin dependencies,
registered Extensions, and Extension Points published for other Plugins. The
IntelliJ Platform Gradle Plugin supplies build, verification, signing, and
publication tasks; `buildPlugin` produces the installable ZIP and
`publishPlugin` uploads it. [Plugin configuration file](https://plugins.jetbrains.com/docs/intellij/plugin-configuration-file.html),
[IntelliJ Platform Gradle tasks](https://plugins.jetbrains.com/docs/intellij/tools-intellij-platform-gradle-plugin-tasks.html),
[Publishing a Plugin](https://plugins.jetbrains.com/docs/intellij/publishing-plugin.html)

IntelliJ has an experimental modular-Plugin format in which one installed
Plugin contains one or more required or optional Modules with separate
descriptors and classloaders. Modules remain content of the Plugin rather than
independently installed products, and the documentation recommends the format
primarily for Remote Development. The platform also uses the word “module” for
built-in, non-removable product functionality, which further distinguishes it
from a user-installed Plugin. [Modular Plugins](https://plugins.jetbrains.com/docs/intellij/modular-plugins.html),
[Plugin compatibility and modules](https://plugins.jetbrains.com/docs/intellij/plugin-compatibility.html#modules)

#### Wiring and lifecycle

`plugin.xml` declares required and optional dependencies. Optional dependencies
can load additional descriptor files only when the dependency Plugin is
present. Extensions implement named Extension Points provided by the platform
or another declared dependency; the platform instantiates and calls those
implementations. Authors can publish their own Extension Points and mark an
Extension Point `dynamic="true"` only when it satisfies the unload contract.
[Plugin dependencies](https://plugins.jetbrains.com/docs/intellij/plugin-dependencies.html),
[Extensions](https://plugins.jetbrains.com/docs/intellij/plugin-extensions.html),
[Plugin configuration file](https://plugins.jetbrains.com/docs/intellij/plugin-configuration-file.html)

The IDE supports install, enable, disable, update, and uninstall. Restart-free
installation, update, and removal exist, but only for Plugins that obey the
Dynamic Plugin restrictions. Unload can fail because of resource or classloader
leaks, in which case the IDE asks the user to restart. Native libraries can
require restart, and third-party paid Plugins cannot be changed without one.
[Managing Plugins](https://www.jetbrains.com/help/idea/managing-plugins.html),
[Dynamic Plugins](https://plugins.jetbrains.com/docs/intellij/dynamic-plugins.html)

#### Isolation and developer experience

Classic Plugins receive dedicated classloaders, which isolate dependency
visibility and allow different library versions. They still execute inside the
IDE JVM; therefore, treating the classloader as an OS security sandbox would be
incorrect. This is an inference from the documented runtime and classloader
model. Split-mode modular Plugins can place frontend and backend Modules in
different Remote Development processes, but the modular format remains
experimental. [Plugin classloaders](https://plugins.jetbrains.com/docs/intellij/plugin-class-loaders.html),
[Modular Plugins](https://plugins.jetbrains.com/docs/intellij/modular-plugins.html),
[Plugin security](https://plugins.jetbrains.com/docs/marketplace/understanding-plugin-security.html)

The IDE Plugin wizard or web generator creates the Gradle project and a “Run
IDE with Plugin” configuration. `runIde` starts a separate sandbox IDE; dynamic
Plugins can auto-reload after a build; and the Gradle Plugin provides `test`,
`testIde`, compatibility verification, build, signing, and publishing tasks.
[Creating a Plugin Project](https://plugins.jetbrains.com/docs/intellij/creating-plugin-project.html),
[Development instance](https://plugins.jetbrains.com/docs/intellij/ide-development-instance.html),
[IntelliJ Platform Gradle tasks](https://plugins.jetbrains.com/docs/intellij/tools-intellij-platform-gradle-plugin-tasks.html)

The simple surface comes from one Plugin identity, a large typed catalog of
Extension Points with IDE completion and inspections, and one Gradle toolchain
that owns the target IDE and sandbox. The cost is a large versioned platform
API, explicit dependency and Extension Point declarations, and strict cleanup
constraints for reliable dynamic unload.

### Obsidian Community Plugins

#### Artifact and declaration

An Obsidian Plugin is distributed as `main.js`, `manifest.json`, and optionally
`styles.css`. The manifest holds identity, version, minimum App version, author
metadata, and whether the Plugin is desktop-only. After the initial directory
submission, Obsidian installs matching assets from a GitHub Release whose tag
equals the manifest version. [Obsidian API Plugin structure](https://github.com/obsidianmd/obsidian-api#plugin-structure),
[Manifest reference](https://docs.obsidian.md/Reference/Manifest),
[Submit your Plugin](https://docs.obsidian.md/plugins/releasing/submit-plugin)

There is no separately installable Module concept in the official artifact.
Authors may split TypeScript internally, but the build produces one entrypoint,
and the user installs and toggles the whole Plugin.

#### Wiring and lifecycle

The Plugin subclass registers commands, views, events, settings, and other
behavior against the Obsidian API from `onload()`. The API's registration
helpers arrange cleanup for many registered resources on unload. The manifest
does not declare Plugin dependencies, provided/required Capabilities, or a
general automatic binding model. [Obsidian API architecture](https://github.com/obsidianmd/obsidian-api#app-architecture),
[Build a Plugin](https://docs.obsidian.md/Plugins/Getting%20started/Build%20a%20plugin)

Users can install, enable, disable, and update Community Plugins. During
development, source changes are rebuilt continuously, but they take effect only
after disabling and enabling the Plugin or reloading the App; the official
tutorial points to a third-party Hot-Reload Plugin as an optional convenience.
[Build a Plugin](https://docs.obsidian.md/Plugins/Getting%20started/Build%20a%20plugin)

#### Isolation and developer experience

Desktop-only Plugins may use Node.js and Electron APIs, and the Community
policy requires disclosures for network use and access outside the vault. These
facts show that the manifest is not a least-authority capability grant; it
should not be interpreted as a hostile-code sandbox. That conclusion is an
inference from the documented API availability and disclosure policy.
[Submission requirements](https://docs.obsidian.md/community-directory/submission-requirements-for-plugins),
[Developer policies](https://docs.obsidian.md/community-directory/developer-policies)

The official sample repository is a template. Authors clone it into a dedicated
test vault, run `npm install` and `npm run dev`, enable it in the real App, and
reload after edits. Publication is a GitHub Release plus automated Community
Directory review. [Build a Plugin](https://docs.obsidian.md/Plugins/Getting%20started/Build%20a%20plugin),
[Official sample Plugin](https://github.com/obsidianmd/obsidian-sample-plugin),
[Submit your Plugin](https://docs.obsidian.md/plugins/releasing/submit-plugin)

The surface feels simple because one bundle, one subclass, one manifest, and
one toggle cover the whole lifecycle. The cost is weak dependency semantics,
no general sub-extension composition, broad desktop authority, and a reload
rather than a first-party hot-development host.

### WordPress Plugins

#### Artifact and declaration

At its simplest, a WordPress Plugin is one PHP file with a Plugin Header;
larger Plugins use a directory. The header supplies the identity and can state
WordPress/PHP compatibility and comma-separated `Requires Plugins` slugs.
WordPress discovers Plugins by scanning the Plugins directory for those headers.
[Plugin Basics](https://developer.wordpress.org/plugins/plugin-basics/),
[Header requirements](https://developer.wordpress.org/plugins/plugin-basics/header-requirements/)

WordPress does not expose a second installable Module unit. Internal files,
classes, and Composer packages are implementation organization; separately
selectable add-ons are normally Plugins themselves.

#### Wiring and lifecycle

Actions and Filters are named global extension points used by Core, themes, and
Plugins. A Plugin registers callbacks against Core hooks and can publish its own
custom hooks for other Plugins. `Requires Plugins` lets Core detect unmet or
circular named Plugin dependencies and enforce activation ordering, but it does
not provide version solving, typed service injection, or automatic operation
binding. [Hooks](https://developer.wordpress.org/plugins/hooks/),
[Custom Hooks](https://developer.wordpress.org/plugins/hooks/custom-hooks/),
[`WP_Plugin_Dependencies`](https://developer.wordpress.org/reference/classes/wp_plugin_dependencies/)

WordPress Admin and WP-CLI provide install, activate, deactivate, update,
uninstall, and delete operations. Plugin authors can register distinct
activation, deactivation, and uninstall hooks so temporary state and durable
data are not conflated. [WP-CLI Plugin commands](https://developer.wordpress.org/cli/commands/plugin/),
[Activation and deactivation hooks](https://developer.wordpress.org/plugins/plugin-basics/activation-deactivation-hooks/),
[Uninstall methods](https://developer.wordpress.org/plugins/plugin-basics/uninstall-methods/)

#### Isolation and developer experience

Core loads active Plugin PHP files into the WordPress bootstrap for a request.
Consequently, Plugins share the PHP request environment rather than receiving a
per-Plugin process sandbox, and disabling one changes subsequent requests rather
than unloading a long-lived instance. This is an inference from the official
loading description and Core bootstrap source. [Plugin loading](https://developer.wordpress.org/plugins/plugin-basics/#how-wordpress-loads-plugins),
[`wp-settings.php`](https://github.com/WordPress/WordPress/blob/master/wp-settings.php)

An author can begin with one PHP file or use `wp scaffold plugin`, which can
also generate unit-test support. PHP edits are visible on a later request;
official tools include Plugin Check, PHPUnit scaffolding, and WordPress
Playground-based test environments. A Plugin accepted into the official
directory is released by committing its files and version tags to the
Plugin's WordPress.org Subversion repository. [WP-CLI scaffold](https://developer.wordpress.org/cli/commands/scaffold/),
[Plugin Check](https://developer.wordpress.org/plugins/developer-tools/helper-plugins/),
[Testing with Playground](https://developer.wordpress.org/playground/handbook/guides/phpunit-testing/),
[WordPress.org Subversion workflow](https://developer.wordpress.org/plugins/wordpress-org/how-to-use-subversion/)

The surface feels simple because directory discovery, interpreted code, and
globally named hooks eliminate a separate compile-and-compose step. The same
choices create its main limitations: shared global state, callback ordering and
priority conflicts, weak dependency semantics, and no isolation between
Plugins in a request.

### Backstage Plugins and Modules

#### Artifact and declaration

Backstage uses npm packages both for distribution and architectural roles.
Package metadata identifies frontend, backend, common, node-library, and Module
roles. A complete product Plugin can span several packages sharing a Plugin ID,
with optional frontend and backend Module packages. [Package metadata](https://backstage.io/docs/tooling/package-metadata/),
[Architecture overview](https://backstage.io/docs/overview/architecture-overview/),
[Plugin Package Structure ADR](https://backstage.io/docs/architecture-decisions/adrs-adr011/)

Backstage explicitly separates the terms. A backend Plugin provides independent
base functionality and is designed like a logical microservice. A Module adds
or changes behavior through Extension Points of one target Plugin, must be
deployed in the same backend instance, and shares the target Plugin's scoped
services. A Module is therefore a constrained technical extension unit, not a
second independent product boundary. [Backend Plugins](https://backstage.io/docs/backend-system/architecture/plugins/),
[Plugin Modules](https://backstage.io/docs/backend-system/architecture/modules/)

#### Wiring and lifecycle

Backend Plugins and Modules declare typed dependencies on services and
Extension Points. All Modules for a Plugin initialize before the Plugin, so
they can register their contributions before the target starts. A Module
depends on an Extension Point exported from a Plugin's node-library package
rather than on the Plugin implementation package itself. [Plugin Modules](https://backstage.io/docs/backend-system/architecture/modules/),
[Backend Extension Points](https://backstage.io/docs/backend-system/architecture/extension-points/)

In the new frontend system, adding a Plugin as an App-package dependency is
usually sufficient: recommended feature discovery finds and installs it.
Configuration can then enable, disable, or override individual Extensions.
Manual installation remains available when the App owner needs explicit
ordering or control. [Installing frontend Plugins](https://backstage.io/docs/frontend-system/building-apps/installing-plugins/),
[Configuring Extensions](https://backstage.io/docs/frontend-system/building-apps/configuring-extensions/)

Backstage's main lifecycle is package installation followed by build,
deployment, and startup wiring; it is not a general end-user hot-unload Plugin
store. Backend lifecycle services provide startup and shutdown hooks, while the
new backend system explicitly does not support backend hot-module reload.
[Building Backends](https://backstage.io/docs/backend-system/building-backends/),
[Lifecycle service](https://backstage.io/docs/backend-system/core-services/lifecycle/),
[Backend migration](https://backstage.io/docs/next/backend-system/building-backends/migrating/)

#### Isolation and developer experience

Many backend Plugins can run in one Node process by default. Architecture rules
prevent direct Plugin-to-Plugin code calls, and a deployment can split Plugins
into separate backends so they communicate over the network. This is a
deployment option, not a per-Plugin sandbox automatically created by package
installation. [Backend Plugins](https://backstage.io/docs/next/backend-system/architecture/plugins/),
[Building Backends](https://backstage.io/docs/next/backend-system/building-backends/index/)

`yarn new` scaffolds frontend Plugins, backend Plugins, and Modules. A frontend
Plugin includes a small dev App that starts and hot-reloads faster than a full
Backstage installation; backend development can use a dedicated development
backend. The Backstage CLI standardizes build, lint, test, packaging, and npm
publication while intentionally keeping its customization surface small.
[New Module](https://backstage.io/docs/tooling/cli/module-new/),
[Building backend Plugins and Modules](https://backstage.io/docs/next/backend-system/building-plugins-and-modules/index/),
[Backstage CLI](https://backstage.io/docs/tooling/cli/overview/)

Backstage gives the strongest evidence that a system can preserve rich internal
composition while offering a much smaller installation experience. Its cost is
corresponding structural weight: one product Plugin may span several packages,
and changes still pass through package installation, build, and deployment.

## Focused addendum: DeepSeek Harness is not a flat one-concept system

### Verdict

DeepSeek Harness does use **Plugin** as its one general runtime extension noun.
It does not introduce an architecture-level **Module** beside Plugin, and its
official architecture says that the model adapter, tool registry, session log,
and agent loop are all Plugins. However, “everything is a Plugin” does not mean
that the system has only one undifferentiated layer.

The official code and documentation expose at least six distinct roles:

1. an npm **Bundle** is the distributable/installable package that contributes
   a configuration layer;
2. a **Profile** is the named runnable composition and ordered Bundle stack;
3. a configuration **plugin row** names and configures one Cordis Plugin;
4. each loaded Plugin instance owns a runtime **Fiber** and an unload boundary;
5. Plugins publish **Services** or contribute definitions to domain registries;
   and
6. typed **Events** and reversible **Effects** are the hook and ownership
   substrate used inside the running tree.

The official publishing tutorial explicitly calls Bundle and Profile “two
concepts, two manifests”: a Bundle answers what the package contributes, while
a Profile answers which Bundles compose one setup and in what order. It also
states that a Bundle is what an author distributes and a Profile is what a user
boots. [DeepSeek Plugin packaging tutorial](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/docs/user/develop/basic/publish.md#L5-L16)

```text
npm Bundle
  package.json + cordis.patch.yml + Plugin code
                         |
                         v
Profile
  ordered Bundle layers + user patch overlays
                         |
                         v
Cordis Loader tree
  plugin rows -> loaded Plugin Fibers
                         |
                         v
shared Context
  Services + Registries + typed Events + reversible Effects
                         |
                         v
domain contributions
  tools, agents, prompts, policies, adapters, UI nodes, ...
```

This makes DeepSeek a counterexample to the claim that a good Plugin system
must expose a separately named Module concept. It is not a counterexample to
the underlying separation of responsibilities: those responsibilities still
exist, but DeepSeek names them Bundle, Profile, Plugin row/Fiber, Service,
Registry, Event, Effect, and scoped contribution.

### Distribution and composition are outside the Plugin function

The installable artifact is a Bundle with a `dsh.bundle` manifest pointing to a
`cordis.patch.yml`. The patch inserts or replaces Plugin rows. A package without
that Bundle declaration can still be installed as a plain library dependency,
but it activates no layer. The CLI maintains the Profile manifest, so ordinary
users run `dsh plugin --profile <name> add/remove/update ...` rather than writing
the ordered Bundle list by hand. Advanced users can still inspect and patch the
exact row tree. [Bundle artifact and row insertion](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/docs/user/develop/basic/publish.md#L18-L64),
[Profile installation](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/docs/user/develop/basic/publish.md#L66-L110)

Composition is therefore real and visible, not absent. Bundle patches, Profile
patches, home patches, and command overlays are applied in order to an empty
entry list. The production implementation's `composeEntries()` calls
`applyEntryPatches()` to produce that effective row list, while `boot()` creates
a root Cordis Context, installs the Loader, mounts the row tree, awaits
activation, and disposes the partial tree on failure.
[Composition implementation](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/boot/app-boot/src/profile.ts#L405-L419),
[Loader boot path](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/boot/app-boot/src/index.ts#L757-L800)

Changing Bundle membership is a startup boundary: after add, remove, or update,
the user restarts the Profile. Edits to the Profile or home patch layer can hot
reload transactionally. Thus even DeepSeek's dynamic experience distinguishes
artifact-set changes from live configuration-tree replacement.
[DeepSeek CLI Plugin lifecycle](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/apps/cli/reference/README.md#L41-L55)

### Plugin code contributes through internal services and registries

The official architecture describes Cordis Plugins as contributors of Services,
typed Events, and reversible Effects to a shared Context. It calls Events the
extension points and lists the Tool registry, Agent registry, Session store,
prompt assembly, scope primitive, and LLM seam as separate core packages and
Context services. [DeepSeek Harness architecture](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/docs/architecture.md#L9-L61)

The Tool implementation makes the inner contribution layer concrete:

- `ToolRuntime` is a Cordis `Service` that owns scoped registration layers and
  one guarded execution pipeline;
- each layer aggregates named tools, restrictions, guards, and presentation;
- `ctx.tools.register()` inserts one `ToolDefinition` into the correct layer,
  rejects conflicts, and returns the disposer; and
- scope resolution shadows global registrations with nearer contributions
  instead of mounting a second Tool-runtime Plugin for every tool.

[Tool registry service and layers](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/core/tools/src/index.ts#L713-L817),
[`tools.register()`](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/core/tools/src/index.ts#L1031-L1061)

The Agent side follows the same pattern. `AgentRegistry` is a Service holding
live Agent entries and a single swappable `AgentFactory`; the concrete loop is a
Plugin that registers that factory through an effect-scoped `setFactory()`.
Consumers program against `ctx.agents`, not the loop implementation package.
[Agent Registry](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/core/agent/src/index.ts#L244-L297),
[swappable Agent factory](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/core/agent/src/index.ts#L360-L387)

These registries are not global bags detached from lifecycle. `ScopedLayers`
derives both visibility and ownership from the calling Cordis Context, records
an undo action through `ctx.effect()`, notifies on change, and removes an empty
scope during disposal. [Scoped contribution ownership](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/core/scope/src/store.ts#L152-L266)

### Events are its hook/extension-point layer

DeepSeek does not need a separately packaged “Extension” type because typed
Cordis Events perform that internal role. The official tutorial defines five
dispatch modes: broadcast, parallel, serial, bail, and waterfall. Waterfalls
are ordered around-middleware that can transform or short-circuit a request;
the Harness uses them for model requests and approval decisions. Event
listeners are effects and disappear with the owning Plugin.
[Cordis Events](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/docs/cordis-tutorial/04-events.md#L44-L96),
[waterfall interception](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/docs/cordis-tutorial/04-events.md#L94-L140)

The runtime lifecycle is Fiber- and Effect-based. A config edit, hot reload,
explicit disposal, or loss of a required Service can unload a Plugin. Cordis
then reverses registered effects, recursively disposes child Plugins, and
drives the Fiber through pending/loading/active/unloading/disposed or failed
states. Registries such as `ctx.tools.register()` participate in that same
ownership chain. [Cordis Plugin lifecycle and effects](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/docs/cordis-tutorial/02-lifecycle-and-effects.md#L5-L94)

### What this changes in the Lenso comparison — inference

DeepSeek shows that Lenso has two coherent vocabulary options:

1. keep **Plugin** as the product/distribution/lifecycle boundary and **Module**
   as the typed implementation/Composition unit; or
2. call every mountable implementation unit a **Plugin**, but then introduce
   equally explicit names for the installable Bundle, runnable Profile/App,
   runtime instance, registry contribution, and generated graph.

The first option keeps user Plugin lifecycle distinct from internal Module
composition. The second is closer to Cordis, but “one noun” does not remove
complexity; it redistributes the complexity into Bundle/Profile manifests,
plugin rows, service injection, registries, events, effects, scopes, and Fibers.

DeepSeek also sets a more composition-visible precedent than VS Code or
Obsidian. It hides Profile-manifest maintenance behind `dsh plugin`, but lets
users dump and patch the effective Plugin-row tree. A Lenso design can therefore
keep exact generated Composition inspectable and patchable for experts without
making it the ordinary install/enable interface.

## Cross-system conclusions — inference

### 1. Plugin is a product and lifecycle boundary

Across all five systems, the installable unit has one stable identity and is the
subject of discovery, compatibility checks, enable/disable state, updates,
diagnostics, and publication. That makes Plugin more than a synonym for a code
module. It is the distribution, trust, configuration, and lifecycle boundary
that users can reason about.

### 2. Module is either private implementation or a constrained subunit

VS Code, Obsidian, and WordPress do not make internal source modules part of the
ordinary product vocabulary. IntelliJ contains Modules inside an installed
Plugin. Backstage exposes Modules to developers, but restricts each Module to a
single target Plugin. None of the systems asks end users to assemble arbitrary
internal Modules into a complete execution graph.

### 3. “Freely composable” means selecting Plugins, not drawing bindings

Users experience composition as set membership: install this Plugin, disable
that one, choose a setting, or install a declared dependency. Plugin authors
target stable named extension points. The host owns discovery, ordering,
activation, conflict checks, and invocation. Advanced systems preserve an
escape hatch for App owners, but the full binding graph is not the default
authoring interface.

This does not mean every combination must work. Compatibility versions,
required dependencies, duplicate identities, extension-point contracts,
permissions, and conflicts bound the valid space. Good Plugin UX makes an
invalid combination explainable instead of requiring users to precompute valid
graphs themselves.

### 4. Dynamic UX does not require in-place mutation

VS Code restarts the Extension Host for many disable, update, and uninstall
operations. IntelliJ dynamically unloads only Plugins that meet strict
constraints and falls back to IDE restart. Obsidian reloads the Plugin or App.
Backstage applies changes through build, deploy, and startup. WordPress benefits
from a request-based execution model rather than a general long-lived unload
protocol.

The cross-system lesson is that a smooth “enable/update and continue” product
experience may be implemented by replacing a bounded host, process, App
generation, or request context. It need not promise mutation of arbitrary live
objects.

### 5. Composability and isolation are separate axes

The easiest systems are not necessarily the safest. WordPress and Obsidian gain
simple authoring from shared execution environments. IntelliJ classloaders and
the VS Code Extension Host improve namespace or fault separation but are not
per-Plugin hostile-code sandboxes. Strong isolation appears only when a system
chooses a restricted execution class, such as a Web Extension, or a separate
deployment/process.

A Plugin manifest must therefore express both what the Plugin contributes and
which execution/trust class it requires. Auto-wiring alone cannot provide a
security boundary.

### 6. The best developer loops collapse seven decisions into one path

The strongest workflows consistently provide:

1. one supported scaffold;
2. one manifest and stable Plugin ID;
3. one representative dev host;
4. watch/reload with debugger support;
5. host-level test utilities;
6. one package/verify command; and
7. one publication path using the same artifact users install.

Their simplicity is not the absence of internal architecture. It comes from
the host and toolchain owning the repetitive composition, activation, test-host,
and packaging decisions.

## Implications for a future Lenso design — inference

### Recommended vocabulary boundary

The evidence supports keeping Module and Plugin as distinct concepts:

- **Module** — a strongly typed implementation and App Composition unit that
  provides and requires Capabilities. A Module may be deep and internally
  complex, but it should remain coherent, removable, and independently
  testable. It is not automatically a marketplace or user lifecycle unit.
- **Plugin** — a named, versioned, distributable product artifact that owns the
  install, enable, disable, update, configuration, permission, trust, and
  rollback experience. A Plugin may package one or more Modules, assets, and
  Execution Adapter requirements.
- **App Composition** — the exact Module Instances and Capability bindings for
one App generation. It remains authoritative compiler/resolver output and an
expert App-author seam, not the routine Plugin-user interface.

Plugin lifecycle should not collapse Store, App intent, runtime realization,
and executable cleanup into one mutable flag or one universal callback. One
Release may be retained but disabled in one App while active in another, and an
old and new realization may overlap during replacement. Store admission and
removal remain inert platform operations; enablement changes App intent;
Generation replacement owns atomic activation; and executable cleanup remains
ordinary Module lifecycle. A simple Plugin SDK may project familiar
`prepare`/`activate`/`deactivate` hooks, but those hooks should compile to a
Module rather than create a second Kernel lifecycle.

The ordinary path must also remain an on-ramp rather than a ceiling. A Tool or
panel builder may generate a hidden Module, while an advanced author must still
be able to implement a public Capability with an explicit Module and publish it
as a candidate for a replaceable product role. Product Extension Points need
more than additive collection: they need explicit Provider selection, typed
ordered interception, and closed scoped mounts when the product exposes those
semantics. Otherwise the result matches a contribution marketplace but cannot
replace a deep subsystem such as an Agent Loop.

The important refinement is that a Module should not itself own an arbitrary
concrete composition graph. Its Descriptor should declare what it provides and
requires. A Plugin may declare package-level dependencies, optional features,
default intent, configuration schemas, permissions, and constraints. The App
resolver should turn those declarations plus App policy into the exact Module
Instances and bindings.

```text
User intent
  install / enable / disable / configure Plugin
                         |
                         v
Plugin artifact
  identity + version + dependencies + features
  permissions + trust class + packaged Modules
                         |
                         v
App resolver
  compatibility + policy + ambiguity + conflict checks
                         |
                         v
Exact App Composition / immutable Resolved App Plan
                         |
                         v
App generation start, readiness, switch, drain or rollback
```

### Complexity that should remain

Lenso should retain exact Capability contracts, cardinality, deterministic
bindings, configuration validation, Execution Adapter selection, and immutable
resolved generations. Those mechanisms are useful internal authority and
reproducibility boundaries.

What should change is who writes them:

| Persona | Default surface |
|---|---|
| Plugin user/operator | Install, enable, disable, configure, update, inspect requested authority, and rollback a Plugin |
| Plugin author | Scaffold one Plugin, declare contributions/requirements, implement one or more Modules, run a representative dev App, test, package, and publish |
| App author/operator | Set policy and make explicit choices only when multiple compatible providers or permissions require a decision |
| Lenso resolver | Discover packaged Modules, solve deterministic bindings, validate constraints, and emit the exact Composition and Plan |
| Framework/runtime author | Maintain Capability, Kernel, Driver, Adapter, generation, and isolation contracts |

An advanced App author may override a binding or pin an exact Module release,
just as Backstage permits manual installation and IntelliJ permits explicit
ordering. That escape hatch should not force every Plugin user to understand the
complete graph.

### Proposed developer-experience acceptance bar

A competitive Lenso Plugin path should be able to reach this shape:

```sh
lenso plugin new example-search
cd example-search
lenso plugin dev
lenso plugin test
lenso plugin pack
lenso plugin publish
```

And the App-owner path should be correspondingly small:

```sh
lenso plugin add example.search
lenso plugin enable example.search
lenso plugin update example.search
lenso plugin disable example.search
```

The commands are illustrative, not a proposed CLI contract. The acceptance
criteria behind them are more important:

- no hand-written Module Instance or binding JSON in the ordinary path;
- one Plugin Definition supports both small generated Contributions and
  explicit user-authored Capability Modules without a second Plugin system;
- additive, replacement, interception, and scoped-mount semantics are visible
  through typed product builders and produce explainable resolver decisions;
- the dev host runs the same packaged Plugin contract as production;
- changes rebuild and replace a bounded development generation automatically;
- the host explains missing dependencies, ambiguous providers, conflicts,
  compatibility failures, and requested authority in Plugin vocabulary;
- packaging produces the same signed/locked artifact that installation uses;
- an exact generated Composition and Plan remain inspectable and diffable; and
- unsafe code receives a genuinely constrained Adapter or process boundary,
  rather than treating manifest metadata as a sandbox.

### Design warning

Copying only the surface simplicity of Obsidian or WordPress would discard
Lenso's strongest internal properties. Copying only Backstage's structural
precision would preserve those properties but could leave the user with a
multi-package composition exercise. The useful target is the combination:

> Obsidian/VS Code simplicity at the Plugin boundary, Backstage-like separation
> between Plugin and Module, and Lenso's exact generated App Composition beneath
> both.
