# ADR 0068: Layer Module configuration sources without creating a generic overlay

- Status: accepted
- Date: 2026-08-27
- Extends: ADR 0031, ADR 0045, ADR 0066, ADR 0067
- Related: ADR 0064

## Context

The App Definition currently carries the complete configuration object for
every Module Instance. This keeps execution reproducible, but it conflates
three different owners:

- the Module package repeats stable implementation defaults in every App;
- the App author selects product behavior, policy, and authority; and
- a local user may need a different model, profile, output preference, or
  bounded resource choice without committing that choice for every user.

Treating every difference as an arbitrary local JSON or TOML merge patch is
unsafe. Existing Module configuration includes filesystem roots, network
origins, executable allowlists, environment-variable allowlists, resource
ceilings, secret references, UI content, and ordinary preferences. A generic
overlay could silently widen reviewed authority, redirect data, or produce a
configuration that no longer matches the exact package-owned Schema.

Moving complete configuration into an operating-system App Data directory is
also the wrong ownership model. It hides state, makes a checkout incomplete,
and creates another authority users cannot easily inspect or remove.

## Decision

**Resolve one complete Module configuration from three typed sources: locked
package defaults, reviewed App values, and explicitly admitted local settings.
No source is a generic patch authority, and the Kernel receives only the final
canonical configuration in an immutable Plan Snapshot.**

### Source ownership

| Source | Owner | Persistence | Purpose |
| --- | --- | --- | --- |
| package defaults | Module publisher | generated Module Descriptor in the locked package | stable, safe defaults that should not be repeated by every App |
| App values | App author | `lenso.app.json`, reviewed in source control | product behavior, policy, authority, and overrides of package defaults |
| local settings | local user through the product Host | one visible, product-owned, Git-ignored file beside the App Definition | admitted per-user choices only |
| secret values | Secret Provider | provider-owned storage | resolved only from secret references after Plan admission |

Dynamic business configuration remains Module-owned state behind a typed
Capability. It is not Plan configuration and is not added to these sources.

### Package defaults make the App Definition partial, not ambiguous

A generated Module Descriptor gains a canonical `configuration_defaults`
object beside its package-owned `configuration_schema`. Defaults are derived
from the Module's typed source and are part of the locked release. The
Descriptor build validates the defaults against the Schema.

The App Definition's `configuration` becomes a partial object. Resolution
recursively overlays App object fields on package defaults; scalar, array, and
`null` values replace the value at that field. There is no deletion operator.
The resulting complete base configuration must satisfy the package-owned
Schema before any local settings are considered.

An authority-bearing default must be deny-safe. A value that grants positive
filesystem, network, process, credential, or external-effect authority remains
an explicit App-owned value.

This keeps a clean checkout independently valid: local settings may override a
complete base but may not supply a missing required value.

### Local adjustment requires two independent permissions

The package-owned Schema declares the comparison semantics for an individual
configuration field. The default is `fixed`. A field may instead declare one
local rule:

| Rule | Admitted local value |
| --- | --- |
| `replace` | any value valid for the field; intended for non-authority preferences |
| `at_most` | a valid numeric value no greater than the reviewed base value |
| `subset` | a valid duplicate-free array whose members all occur in the reviewed base array |
| `descendant` | a path that canonically remains within the reviewed base path |
| `secret_ref` | another syntactically valid secret reference, never secret material |

The Descriptor encodes the rule as an `x-lenso-local` Schema annotation. An
authority-bearing field also carries `x-lenso-authority: true`; such a field
cannot use `replace`. For example:

```json
{
  "configuration_defaults": {
    "max_output_tokens": 4096
  },
  "configuration_schema": {
    "type": "object",
    "properties": {
      "max_output_tokens": {
        "type": "integer",
        "minimum": 1,
        "x-lenso-local": "at_most"
      }
    },
    "additionalProperties": false
  }
}
```

The first version applies rules only at explicit leaf paths. Objects are
traversed; arrays are values rather than index-addressable patch targets.
Unknown fields, unannotated fields, array-index edits, and incompatible rule or
Schema combinations fail closed.

The App author then admits an exact JSON Pointer for one keyed Instance under
that Instance's local-settings policy. For example:

```json
{
  "key": "agent",
  "package": "lenso.agent.loop",
  "configuration": {
    "model": "openai/gpt-5",
    "max_output_tokens": 4096
  },
  "local_settings": {
    "allow": ["/model", "/max_output_tokens"]
  }
}
```

The package cannot grant local mutability by itself, and the App cannot invent
comparison semantics absent from the locked package Descriptor. Package
upgrade, Instance rename, missing path, or rule change forces a fresh review
because resolution revalidates both permissions against the exact Descriptor.

### Product-owned local document

Core libraries define a typed local-settings input but do not choose a global
path or create an App Data database. Each Host owns discovery and persistence.
A source-backed product may use a visible file beside its App Definition. For
example, the Agent Harness can extend its existing `lenso.local.toml`:

```toml
schema_version = 1

[plugins]
enabled = ["codex-direct@1"]

[modules.agent.configuration]
model = "openai/gpt-5.1"
max_output_tokens = 2048
```

Instance keys are the only Module selectors. The document cannot add or remove
Modules, change packages, entrypoints, bindings, lanes, execution classes, or
Plugin grants. An empty local document is removed. Older Hosts reject the new
section as unknown instead of silently ignoring it.

### Resolution and provenance

The authoring and Host pipeline performs these steps before staging:

1. resolve the exact package lock and generated Descriptor;
2. materialize package defaults and overlay reviewed App values;
3. validate the complete base configuration;
4. match each local value to an existing Instance, App allowlist path, and
   package-owned local rule;
5. compare constrained values against the complete reviewed base;
6. materialize and validate the complete effective configuration;
7. canonicalize it into the next Resolved App Plan Snapshot; and
8. stage readiness before applying an ADR 0067 Transition or whole-App
   Generation swap.

The Kernel and Module receive only the complete effective configuration. They
do not read local files, merge values, or interpret source precedence.

The Host retains in-memory provenance for each effective leaf — package
default, App value, or local setting — plus the three source digests for the
candidate Generation. An authoring or Host status surface may expose paths,
rules, source labels, and digests but never configuration or secret values by
default. Kernel Runtime Diagnostics remain configuration-free.

### Initial Agent Harness classification

The current Agent Harness demonstrates why classification is field-specific:

| Existing field kind | Initial ownership or rule |
| --- | --- |
| stable conservative byte/count/time defaults | package default |
| model identity or reasoning preference | App value; local `replace` only when that Instance explicitly admits it |
| output, step, Tool-call, and parallelism ceilings | App value with optional local `at_most` |
| allowed origins, programs, environment names, or tools | explicit App authority with optional local `subset` |
| workspace/process roots | explicit App authority; local `descendant` only where the Module uses canonical path confinement |
| prompt text, TUI commands, and static panels | App-owned product behavior, fixed by default |
| credential identifier | secret reference; local `secret_ref` only when the App admits per-user credentials |
| Plugin enabled set | product-owned Plugin intent, not Module configuration |

This classification is not inferred from field names. Each Module must declare
the semantics in generated source metadata, and each App Instance must admit
the specific local path.

### Authoring and user experience

Repository validation remains deterministic:

- `lenso app check` validates package defaults plus App values and does not
  require a local file;
- a Host startup or explicit local check validates the effective local result;
- `lenso config get <instance> <path>` reports the effective value when it is
  not sensitive;
- `lenso config explain <instance> <path>` reports source, local rule, and why
  an override is or is not admitted; and
- `lenso config set --local <instance> <path> <value>` performs the complete
  candidate resolve and Ready Gate before committing the local file.

Directly editing the visible local document remains supported. Invalid edits
fail startup or status closed and never replace the running Plan Snapshot.

## Rejected alternatives

### Move all Module configuration to local storage

This makes product behavior and authority machine-specific, prevents clean
checkout validation, and hides required state.

### Apply an unrestricted local deep merge

JSON shape validation alone does not establish whether an override widens
authority. It also gives the local file implicit control over every future
field added by a package upgrade.

### Let the Module Schema alone decide local mutability

A package publisher owns field semantics but does not own the App author's
product policy. Local adjustment therefore requires both the package rule and
the App Instance allowlist.

### Put per-user values in environment variables

Environment variables are untyped, poorly discoverable, and easy to leak.
They remain an implementation detail of Secret Providers or an explicit Host
integration, not a universal configuration layer.

## Consequences

- `lenso.app.json` becomes smaller mainly because package defaults stop being
  repeated, while product and authority decisions remain reviewable.
- Different users can keep admitted preferences in one visible local file
  without producing Git diffs.
- Module authoring gains default and local-rule metadata; App authoring gains
  an Instance-scoped allowlist; resolver and Host APIs gain typed overlay and
  provenance inputs.
- A local configuration edit always produces a new immutable effective Plan
  Snapshot. It never mutates a running Module or Kernel-owned configuration.
- Existing Apps remain valid: absent defaults mean `{}`, absent local rules
  mean `fixed`, and absent local settings preserve current behavior.
- The initial safe deployment can support only `replace` on reviewed
  non-authority preferences and add constrained rules after conformance tests.

## Phased proof

1. Add Descriptor defaults and prove two Apps with omitted repeated defaults
   resolve to canonical complete configuration.
2. Add schema local rules, App Instance allowlists, and resolver conformance
   tests for every rule and every fail-closed mismatch.
3. Extend the Agent Harness local document with one non-authority preference;
   prove two local users resolve different effective Plans while
   `lenso.app.json` remains byte-identical.
4. Prove failed validation or Ready Gate preserves both the local file and the
   currently routed Plan Snapshot.
5. Add `get`, `explain`, and transactional `set --local` UX before admitting
   more Module fields.
