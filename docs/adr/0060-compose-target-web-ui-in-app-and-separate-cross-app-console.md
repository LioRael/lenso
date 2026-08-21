# ADR 0060: Compose target Web UI in-App and separate cross-App Console

- Status: accepted
- Date: 2026-08-21
- Extends: ADR 0030, ADR 0031, ADR 0034, ADR 0043, ADR 0045, ADR 0057
- Supersedes: ADR 0044

## Context

ADR 0044 used **Console** for two products with different ownership and trust
requirements:

1. a Web UI owned by one target App, used to operate that App; and
2. an operator product that connects to and manages other Apps.

Requiring both products to be independent Apps makes the first case pay for a
Connector transport, a second process, target enrollment, delegated operator
identity, and duplicated lifecycle even when all UI and business Modules belong
to one composition. Treating both products as embedded has the opposite
problem: a durable multi-target operator product would inherit one target's
failure domain, identity policy, state, and release lifecycle.

The distinction is a product and composition decision, not a Kernel decision.
The Kernel already has the necessary runtime model: Module Instances provide
and require Capabilities, App Composition binds them, and authoring tools
materialize one immutable Resolved App Plan before boot. `Console`, `Web UI`,
and `Plugin` do not need to become peer runtime types.

Module authors also need a concrete way to ship an App-specific page without
running a Web server per Module or depending on a separate central product.
ADR 0043 established UI Contributions as ordinary Capabilities, but did not
settle which App owns the UI consumer or when a Connector is required.

## Decision

Lenso distinguishes a **target-owned App Web UI** from a **cross-App Console**.

- An App Web UI is an optional composition of ordinary Modules inside the
  target App. It is the default shape for a single App's user, administration,
  development, and diagnostic pages.
- A cross-App Console is an independent Lenso App only when it operates one or
  more target Apps through explicit portable Connector Capabilities.
- A Web-enabled development or product profile is an authoring recipe that
  selects ordinary Module Instances before plan resolution. It is not a Kernel
  mode or a runtime overlay.

The word **plugin** may describe distribution and product experience, but the
runtime model remains a Module package providing ordinary Capabilities. Lenso
does not add a `ConsolePlugin` type, plugin registry, nested Module host, or
post-boot graph mutation.

## Target-owned App Web UI

A target App may compose a Web Shell, a Browser Adapter, business Modules, and
zero or more UI Contribution providers into the same Resolved App Plan:

```text
Target App Composition

  orders ---------------- provides acme.orders.query@1
      ^                   provides acme.orders.command@1
      |
  orders-ui -------------- provides lenso.ui.contribution@1
      |                    requires acme.orders.query@1
      |                    requires acme.orders.command@1
      v
  web-shell -------------- requires many lenso.ui.contribution@1
      |
  browser-adapter -------- projects only the resolved portable requirements
      |
      v
  browser
```

The Web Shell owns route assembly, navigation assembly, contribution loading,
asset policy, collision detection, and UI readiness behind one small
Interface. It must not expose a global Capability Registry to browser code.

The Browser Adapter owns the browser-to-App transport and generated-client
projection. For each selected UI Contribution, it exposes only the portable
Capability Operations declared by that contribution and resolved by App
Composition. The target business Module remains the final authorization
authority. Browser code does not gain ambient App, Kernel, or administrator
authority merely because it is served by the App Web UI.

The browser remains a separate JavaScript execution realm even when its assets
are served by the target App. "In-App" describes product ownership and App
Composition; it does not imply that browser code can directly call a Rust or
Bun object without an Adapter.

Removing the Web Shell, Browser Adapter, and all UI Contribution providers must
leave non-UI Module behavior unchanged.

## Module-owned UI packaging

A Module author may ship business and UI artifacts in one package. The
recommended package shape uses explicit entrypoints and separate Module
Instances when the UI is optional or has a different lifecycle:

```text
@acme/orders
  ./backend  -> orders Module
  ./ui       -> orders-ui Module
  ./contract -> generated Capability bindings
```

The package can therefore be installed and versioned as one product while App
Composition keeps the responsibilities explicit:

- `orders` owns business behavior and provides business Capabilities;
- `orders-ui` provides the UI Contribution and declares the portable business
  Capabilities needed by its browser client; and
- `web-shell` consumes many UI Contributions without knowing Orders semantics.

A cohesive Module Instance may provide several Capabilities, including a UI
Contribution, as permitted by ADR 0031. It does not thereby contain another
Module, install another Module, or acquire a private runtime graph. Separate
entrypoints and Instances are preferred when they make optional installation,
dependency direction, lifecycle, or replacement clearer. "Same package" does
not require "same Module Instance."

## Plugin installation and plan immutability

Installing or enabling a UI extension is an authoring operation in v1:

1. the App author selects a package or configured contribution;
2. authoring tools add its Module Instances, configuration, and explicit
   Capability bindings to App Composition;
3. package managers and lockfiles resolve exact artifacts;
4. Lenso materializes and validates one Resolved App Plan; and
5. the Kernel boots that immutable Plan.

An authoring tool may present this as a one-command experience, for example
`lenso dev up --web`. The exact CLI spelling is not normative, but the command
must materialize the Web profile before boot and may open the browser only
after the App Ready Gate opens.

The initial model does not support:

- discovering or downloading executable plugins after boot;
- adding Module Instances or Capability bindings at runtime;
- a stringly typed `invoke-anything` browser endpoint;
- a Console-specific lifecycle or plugin registry;
- one Module starting an independently managed Web server per page; or
- fallback providers when a UI requirement cannot be resolved.

A Module may maintain mutable product data, such as user-selected dashboard
layout or a catalog of external links, without mutating App Composition. Such
data cannot grant new Capability clients or Operations beyond the immutable
bindings in the Resolved App Plan.

## Extending an independent Console

An independent Console supports extensions through the same Module and
Capability model; it does not acquire a second plugin architecture. Its App
Composition may select Console-owned Modules that provide UI Contributions,
target Connector clients, identity providers, target catalogs, audit views, or
other operator features:

```text
Console App Composition

  console-web-shell ------- requires many lenso.ui.contribution@1
  runtime-inspector-ui ---- provides lenso.ui.contribution@1
  audit-ui ---------------- provides lenso.ui.contribution@1
  target-connector-client - provides explicitly bound target Capabilities
```

Installing one of these extensions is still an authoring-time Console project
change followed by a new Resolved App Plan and restart. A package may publish
separate target-side Connector and Console-side UI entrypoints, but each is
selected independently in the App where it executes. Connecting a target does
not install its package into Console.

## UI execution trust

A same-realm UI bundle has the authority of the page that loads it. ESM, CORS,
Web Components, and Shadow DOM do not sandbox hostile JavaScript. Therefore:

- trusted UI code should normally come from packages selected and locked by
  the App author;
- an explicitly configured mutable remote ESM remains an explicit trust in its
  publisher under ADR 0043, not a security boundary or automatic plugin
  mechanism; and
- a future untrusted external page requires an isolated browsing context and a
  separately reviewed, bounded Capability broker.

A cross-App Console must not automatically execute code advertised by a
target App. Target metadata may describe available Capabilities, but executable
UI becomes trusted Console code only when independently selected by the
Console App Composition. This prevents one connected target from gaining the
same-realm authority accumulated by a multi-target Console.

## Identity and authorization

A target-owned App Web UI uses the target App's explicitly composed identity
and access-policy Modules. It may serve end users, administrators, developers,
or an anonymous loopback profile, but none of those identities are implied by
Kernel or by the presence of a Web Shell.

A cross-App Console has a separate operator trust domain. It authenticates an
operator, authorizes access to an exact target, Capability, and Operation, and
attenuates the ActorAssertion before crossing the Connector. The target Module
still performs final authorization and never accepts a Console cookie or
session as a substitute for an ActorAssertion.

Runtime inspection, business invocation, and dangerous runtime control remain
separate authorities in both shapes.

## When Console becomes an independent App

An independent Console App is justified when at least one of these product
requirements is real:

- one UI manages multiple target Apps;
- the UI connects to remote targets rather than only its owning App;
- operator identity and policy must be independent from every target;
- Console state, history, or audit must survive target unavailability;
- Console and target Apps need independent deployment or release lifecycles; or
- Console performs cross-target aggregation or coordination.

The independent Console composes ordinary Modules such as Web Shell, Operator
Identity, Access Policy, Target Catalog, Audit, and optional Story providers.
State and PostgreSQL are required only by selected stateful Modules.

Each target explicitly opts into a thin Connector Module. The Connector
exports only portable Capabilities allowlisted by target App Composition and
does not expose a global registry, mutate the target plan, install Modules,
perform placement, or become a Control Plane. HTTP, WebSocket, UDS, and future
connection mechanisms remain Adapter choices outside Kernel.

## Shape selection

| Requirement | Selected shape |
| --- | --- |
| One App's product, admin, development, or diagnostic UI | Target-owned App Web UI |
| A Module ships its own optional page | UI Contribution Module, optionally in the same package |
| One command starts an App and its Web UI | Authoring-time Web composition profile |
| One operator product manages several Apps | Independent Console App plus target Connectors |
| Independent operator identity, durable history, or release lifecycle | Independent Console App |
| Every Module starts and secures its own Web server | Rejected |
| Runtime plugin installation mutates the running graph | Deferred and unsupported in v1 |

## Consequences

- A production App may own and serve its Web UI without installing a Connector
  or deploying a second Lenso App.
- Module authors can deliver cohesive backend and browser experiences while
  preserving explicit Module Instances and Capability bindings.
- The first Web UI proof can use direct in-App bindings and still exercise the
  same UI Contribution and generated Browser-client Interfaces needed later.
- The independent Console remains available for a genuinely cross-App operator
  product instead of becoming mandatory infrastructure for every Web page.
- App authors can remove all UI concerns without affecting Kernel or business
  Module operation.
- Package selection and plan materialization remain the reviewable extension
  boundary; v1 does not provide a runtime plugin marketplace or hot loading.
- A multi-target Console cannot silently inherit target-provided executable UI;
  installing such code is a separate Console trust decision.
- Some packages will publish multiple explicit Module entrypoints and must keep
  their package-level versioning and generated Capability contracts aligned.

## Rejected alternatives

### Require every production Web UI to be an independent App

This provides a separate process and trust domain even when the product needs
neither. It makes the common single-App case pay the Connector, deployment,
identity-delegation, and failure-handling costs of a multi-target operator
product.

### Embed every Console in a target App

This makes multi-target management, independent operator identity, durable
cross-target state, and independent release lifecycle awkward or impossible.

### Add a Console or Plugin runtime type

This duplicates Module lifecycle, Capability resolution, and App Composition,
then makes UI extensibility a privileged Kernel concern. Ordinary Modules and
Capabilities already provide the required seam.

### Let each Module expose an unrelated Web server

This fragments ports, routing, authentication, navigation, browser policy,
readiness, and generated-client handling across Module authors. A shared Web
Shell and Browser Adapter provide more leverage behind smaller Interfaces.

### Let connected targets push same-realm UI into Console

This composes the trust of every target inside one Console JavaScript realm. A
target connection grants only explicit portable Capability access; executable
Console UI requires an independent installation decision.
