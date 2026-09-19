---
status: accepted
---

# Discover local sources as generated Host input

This decision adopts the zero-configuration App direction in issue #727. It
amends ADR 0070 only for an explicitly generated local development Host profile;
existing custom Hosts and their admission policies keep their current meaning.
Acceptance is not evidence that the complete CLI or distribution workflow ships.

## Authoring and authority

The conventional `app/` directory supplies App-owned Plugin source projects.
Optional tooling configuration names additional local source roots. Discovery
produces candidates with canonical source provenance; it neither executes
business lifecycle code nor grants execution authority. Generated source
Descriptors and verified Bundle metadata remain the Contract authority under
ADR 0066. No handwritten App manifest or duplicate Plugin Contract is added.

The generated local Host gives each App-owned Plugin one default Instance named
`default`. Identity comes from the Plugin ID and Instance key, never directory
order or basename. Existing Plugin Root configuration overrides and disabled
markers apply to that same identity. Adding/removing an App-owned source changes
the next generated Host, not an already running graph. Removing a source with
remaining explicit Root intent is an actionable error, not silent intent loss.

Shared roots supply candidates only. An explicit Plugin Root Instance selects a
shared Plugin. Neither dependency inference nor discovery activates another
shared Plugin automatically. A missing provider is reported with candidate
provenance, leaving adoption to the App owner. An adopted shared source is built
and locked by the same pipeline as App-owned source; it need not first be
published or installed from a marketplace.

The local Host template explicitly permits `many` offers in its source-derived
root Slots. This policy does not change Capability requirement cardinality:
`one`, optional, and `many` bindings are still resolved by the existing resolver.
Ambiguous single dependencies fail unless an existing Host-permitted saved
choice resolves them. No provider is selected by discovery order or version.
Custom Hosts retain their own Slot cardinality and admission ceilings.

## Generated profile

The local Host profile has a distinct versioned generated authority schema.
Its exact built source Releases and selected implementations define its admission
set. App-owned defaults are disableable; explicit Root instances cannot invent
unknown identities or substitute unapproved artifacts. A disabled required
provider can make the candidate App invalid, which must fail before activation.
Custom/closed Host profiles must not acquire these policies implicitly.

Template identity/version, selected implementation target, source content identity,
runtime cohort and artifact digests are recorded in generated build/distribution
evidence. The creation command writes no required App configuration file. The
tool's versioned template supplies defaults; a build records the exact policy so
reproduction does not depend on whichever CLI happens to run later.

Languages, execution classes and business Capabilities remain orthogonal. One
Release can provide several implementations of one Contract under ADR 0071;
Host policy selects one before resolution. Native linked implementations require
generated Host compilation. Process, Bun, Wasm and other implementations require
their real supported Adapters and target-specific proof. Unsupported combinations
fail without fallback or claims based only on successful discovery.

## Execution and production

Development orchestrates discovery, build, validation, readiness and managed
shutdown above Kernel. Structural edits use a new Generation or a controlled
restart. Stateful seamless hot replacement is not promised. Failed preparation
must not publish a partial Host/Plugin Root; downtime and cleanup failures remain
observable when replacement requires stopping the old Generation first.

Production consumes locked artifacts and the existing immutable Plan mechanism,
not live source discovery. External local source paths must be captured into the
build closure; starting a prepared artifact must not require sibling checkouts
or implicit downloads. Replacing the generated Host with a custom Host uses the
same Plugin Root and resolver boundary; it does not require rewriting Plugins.

## Delivery boundary

The current read-only discovery slice is tracked by #728. Subsequent owner-local
tickets must prove generated defaults/disablement, explicit shared adoption,
mixed native/Bun invocation, remaining supported execution classes, managed
development restart, and offline distribution before #727 closes. CLI commands
must account for existing reserved/dynamic roots; this ADR does not silently
reassign a product-owned terminal command.
