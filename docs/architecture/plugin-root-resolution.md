# Plugin Root and App resolution

This contract defines the author-facing input and deterministic resolution
model accepted by [ADR 0070](../adr/0070-resolve-apps-from-plugin-roots.md).
It changes App authoring above Kernel; it does not change Plan or Kernel
semantics.

Implementation is tracked by [Plan 006](../../plans/006-migrate-embedded-behavior-to-plugins.md).
Until that plan closes, released Hosts and CLIs may still expose compatibility
App Definition and `Module*` inputs; this target contract is not a shipped-state
claim.

## The complete public model

- A **Host** is the runnable product. It supplies root Slots, embedded Plugin
  Releases, default Plugin Instances, and the policy that says which defaults
  are required or replaceable.
- A **Plugin** is all application behavior, regardless of whether its Release
  is embedded, bundled, or installed later.
- A **Plugin Root** is the App owner's optional collection of explicit Plugin
  Instances and exact non-embedded bundles.
- An **App** is the valid result of resolving one Host and one Plugin Root. It
  is not authored in a central manifest.

App Composition, exact bindings, execution variants, placements, Plan
Snapshots, Change Proposals, and App Generations remain derived operational
facts. They are inspectable, but they are not facts an ordinary App owner must
repeat.

## Plugin Root layout

The Host discovers at most one Plugin Root named `plugins` for the current App
project:

```text
plugins/
  lenso.agent.model.fixture/
    model.toml
  lenso.agent.loop/
    agent.toml
    subagent.toml
  lenso.agent.workspace-read/
    default.toml
  company.uppercase/
    plugin.lenso-plugin
    default.toml
```

The first directory name is the exact Plugin ID. A `*.toml` filename stem is
the App-local Instance key. Instance identity is the pair `(Plugin ID,
Instance key)`, so different Plugins may both have an Instance named
`default`. Its document is the direct Plugin configuration;
it has no `plugin`, `instance`, `slot`, binding, execution, or placement
wrapper:

```toml
model = "openai/gpt-5.1"
max_output_tokens = 2048
```

An empty TOML document explicitly enables an optional Plugin Instance with its
package defaults. A Host default Instance with only package defaults needs no
file. A configuration file matching a Host default Instance key replaces only
its configuration values; it does not create a duplicate Instance.

One optional `plugin.lenso-plugin` is the exact Release source when the Plugin
is not in the Host's immutable Plugin Catalog. Its declared Plugin ID must
equal the directory name. Replacing a Catalog Release with a root bundle also
requires explicit Host permission for that Plugin ID. A Plugin Root cannot
contain multiple active Releases for one Plugin ID. Acquisition commands may
retain immutable staged bundles for rollback, but that storage is not App
intent and does not select a Release. A bundle with no enabled Instance is
installed but inactive.

`<instance>.disabled` is an empty explicit absence marker. It may sit beside a
retained configuration file so disable and enable do not destroy settings.
Configuration and disabled markers for unknown Instances fail validation.
Required non-replaceable Host Instances cannot be disabled.

Reserved filenames are `plugin.lenso-plugin` and stems beginning with `.`.
Unknown files, duplicate normalized Plugin IDs or Instance keys, symlinks that
escape the Plugin Root, and case-colliding paths fail closed.

## Host contract

One immutable Host Catalog declares:

- the exact embedded Plugin Releases available to this Host build;
- root Slots, their attachment kind and cardinality;
- default Plugin Instances and any Host-owned configuration over package
  defaults;
- whether each default is required, optional, or replaceable;
- deterministic ordering for `many` and `intercept` Slots;
- execution constraints and maximum counts or resource ceilings; and
- the authority ceiling that an App-owner configuration may not exceed.

The Host Catalog is generated from Host and Plugin source and locked to the
Host build. It is not another App file. A Host may expose product-specific
commands that edit the Plugin Root, but those commands exercise the same
resolver as direct file edits.

## Deterministic resolution

Resolution never executes Plugin code and never guesses from names or install
order. Given identical Host Catalog, Plugin Root bytes, admitted bundles, and
secret references, it produces the same result:

1. Read the Host Catalog and materialize its default Plugin Instances.
2. Scan Plugin directories and require every identity to match its path and
   generated Descriptor.
3. Resolve exactly one Release per Plugin ID: the root bundle when present,
   otherwise the exact Host Catalog Release. Missing or competing Releases
   are errors.
4. Apply disabled markers and direct Instance configuration. Merge package
   defaults first, then Host-owned values for a default Instance, then explicit
   root values; arrays and scalar values replace and objects merge recursively.
   Unknown fields and invalid complete values fail the Plugin-owned Schema.
5. Offer every enabled Instance through its generated Slot Entries. The Host
   Catalog, not a Plugin publisher, decides admissibility and order.
6. For a replaceable `one` Slot, one explicit candidate replaces the default;
   zero keeps the default and two or more fail. For `many`, all legal explicit
   entries join the defaults in Host-defined stable order. Required missing
   Slots fail.
7. Bind every Plugin Port by exact Capability compatibility. A unique legal
   provider binds automatically; no provider or multiple providers for a
   `one` Port fails with the affected Instances and legal remedies. There is no
   hand-authored binding escape hatch.
8. Select the one legal execution variant and placement under Host policy,
   derive the App Composition, and validate the complete immutable Plan.
9. Produce a Change Proposal explaining additions, removals, replacements,
   permission changes, configuration provenance, and whether a hot Transition
   or App Generation swap is required.
10. Only an exact ready proposal may pass the Ready Gate and become the next
    App. Failure preserves the Plugin Root transaction and currently running
    Plan.

"Automatic" therefore means omitted known defaults and uniquely determined
bindings are filled in. It never means selecting among multiple valid choices.
Ambiguity is removed by adding, disabling, or removing the relevant Plugin
Instance, not by writing internal bindings.

## Configuration and authority

The Plugin Descriptor owns the typed Schema and safe package defaults. The
Plugin Root owns explicit App values. Secret material remains outside TOML;
configuration contains only typed secret references.

The authored TOML profile has no generic null value. Omitting an optional field
means no explicit override. A Plugin that needs a meaningful "clear" choice
models it in its own Schema, for example as `mode = "none"`; the resolver does
not invent a universal deletion token. Canonical derived Plan data may still
contain JSON null where the generated Plugin Schema requires it.

Positive filesystem, network, process, credential, or external-effect
authority cannot appear through a package default. It must be declared by the
Host for a default Instance or admitted by the Host authority ceiling and
explicit in the Instance configuration or an installation proposal.
Configuration changes always resolve a candidate Plan and pass the Ready Gate
before they affect the running App.

There is no generic `lenso.local.toml` overlay and no second local-settings
allowlist. A product that needs per-user configuration may Git-ignore selected
Instance TOML files or provide an editor for them, but it may not introduce a
parallel configuration authority. Dynamic business configuration remains
Plugin-owned state behind a Capability and is not Plan configuration.

## Author and App-owner workflows

Plugin authors keep one lifecycle:

```text
lenso plugin new -> check -> dev -> pack
```

`pack` validates the exact bundle it creates. There is no separate `verify`
authoring command; an installing Host independently verifies untrusted input.

App owners use the current project and derived App:

```text
lenso plugins list
lenso plugins add <bundle>
lenso plugins configure <plugin-id> [instance]
lenso plugins disable <plugin-id> [instance]
lenso plugins enable <plugin-id> [instance]
lenso plugins remove <plugin-id> [instance]
lenso app check
lenso app show
lenso run
```

Mutation commands first build and Ready-check the complete candidate, then
commit the smallest Plugin Root change atomically. `app check` and `app show`
accept no Definition path. An advanced `lenso app resolve --output <path>` may
export derived evidence for debugging or deployment; its output is never the
next authoring input.

## Migration from App Definitions

Migration is mechanical and fail-closed:

- every `plugins[]` or compatibility `modules[]` entry becomes
  `plugins/<plugin-id>/<instance>.toml`;
- non-embedded exact Releases become `plugin.lenso-plugin` in their Plugin
  directory;
- package defaults disappear from project files;
- manual Capability bindings, lane assignments, execution classes, package
  tables, Host package metadata, decisions, and generated manifests disappear
  from author input and are re-derived;
- a choice that the new resolver cannot determine uniquely blocks migration
  with the conflicting Plugin Instances and Host Slot involved; and
- once migrated, `lenso.app.json`, `lenso.app.toml`, `--definition`, and
  `app add/remove --definition` produce an actionable retirement diagnostic.

The migration tool must prove that the old and new inputs resolve to the same
effective Plugin Instances, configuration, bindings, authority, and Plan before
it removes the old file.
