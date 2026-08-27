# Source-derived App Definition recipe

`lenso.app.json` is the only hand-authored App composition input. Inspect the
installed `lenso --help` and selected package versions before copying examples;
the CLI owns edit mechanics while Module packages own their generated
Descriptors and ordinary package managers own dependency locks.

## Ownership map

| Input or artifact | Owner |
| --- | --- |
| `lenso.app.json` | App name, selected Module packages, keyed Instances, product configuration differences, admitted local-setting paths, lane choices, real ambiguity decisions |
| `Cargo.toml` / package manifest | requested package dependencies |
| package lock | exact selected releases |
| Module Descriptor, defaults, and Schemas | generated from Module source and locked by the Module package |
| derived App Composition | resolver output, never hand-authored |
| Resolved App Plan | canonical generated Host input |

Do not copy Capability IDs, Operations, Ports, execution classes, lifecycle
policy, or unambiguous bindings into the App Definition. Those facts come from
the selected package-owned Descriptor.

## Worked App Definition

This example selects one Module Instance and provides only App-owned intent:

```json
{
  "schema_version": 1,
  "manifest": "Cargo.toml",
  "packages": {
    "example.greeting": "greeting-module"
  },
  "app": {
    "name": "example",
    "modules": [
      {
        "key": "greeter",
        "package": "example.greeting",
        "configuration": { "prefix": "Hello" }
      }
    ],
    "decisions": []
  }
}
```

If exactly one compatible provider exists, the resolver binds it. Add a
decision only when more than one valid provider leaves a real App-owner choice.
The same package may appear under several keys with different configuration.

## Authoring commands

Read `lenso --help` first. Add and remove express App intent; check and resolve
remain advanced App-owner and Host operations:

```sh
lenso app add greeting-module \
  --definition lenso.app.json \
  --version '^1.0' \
  --configuration '{"prefix":"Hello"}' \
  --dry-run

lenso app add greeting-module \
  --definition lenso.app.json \
  --version '^1.0' \
  --configuration '{"prefix":"Hello"}'

lenso app check --definition lenso.app.json
lenso app resolve --definition lenso.app.json \
  --output .lenso/resolved-plan.json

lenso app remove greeter --definition lenso.app.json --dry-run
lenso app remove greeter --definition lenso.app.json --uninstall
```

Package-owned safe defaults are materialized first, so an App Definition does
not repeat them. Its `configuration` object contains only App-owned differences
and must still resolve to a complete Schema-valid value without local settings.

`app add` delegates acquisition and exact selection to Cargo, reads the
package-owned Descriptor without executing Module code, and applies the App
Definition edit transactionally. `--dry-run` performs the complete build and
resolution check, reports touched files, and restores them byte-for-byte.

`app resolve` emits canonical Plan bytes for a product-owned Runner or Host.
There is no generic author-facing `lenso run --plan` command and no recipe or
fragment document that becomes a second composition authority.

## Secret and configuration rule

Configuration is Module-owned opaque data. Non-empty configuration follows its
generated Schema. Sensitive values use a secret reference rather than raw
secret material. Secret values stay out of the App Definition, Resolved Plan,
Invocation Context, and Runtime Diagnostics.
