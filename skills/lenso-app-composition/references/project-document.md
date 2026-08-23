# App project document recipe

`lenso.json` is the language-independent authoring source. Inspect the selected
`lenso-app-plan`/`lenso-authoring` versions before copying this example; fields
and CLI flags are owned by those packages.

## Ownership map

| Section | Owns |
| --- | --- |
| `composition.modules` | keyed Instances, entrypoints, configuration, endpoints, requirements, execution class, role, lane |
| `composition.bindings` | explicit consumer requirement to provider Instance edges |
| `composition.execution_lanes` | App-local single-owner Kernel lane identities |
| `packages` | reviewable package-manager inputs and lock locations |
| `contracts` | exact Capability Descriptor versions and checked-in generated artifacts |
| `profiles` | pre-resolution Web authoring recipes |

Package managers acquire code and own locks. `lenso.json` records the inputs
needed to verify and resolve those selections; it is not another package lock.

## Worked provider/consumer project

This example selects one native greeting provider and one consumer. Non-empty
configuration carries a Schema, and the binding names the exact provider key.

```json
{
  "schema_version": 1,
  "composition": {
    "modules": [
      {
        "key": "greeter",
        "package": "example.greeting",
        "entrypoint": "default",
        "configuration": { "prefix": "Hello" },
        "configuration_schema": "modules/greeting/config.schema.json",
        "provides": [
          {
            "capability_id": "example.greeting@1",
            "descriptor_version": "1.0.0",
            "operations": ["greet"]
          }
        ],
        "requires": [],
        "execution_class": "lenso.native-rust@1"
      },
      {
        "key": "welcome-flow",
        "package": "example.welcome-flow",
        "entrypoint": "default",
        "configuration": {},
        "provides": [],
        "requires": [
          {
            "capability_id": "example.greeting@1",
            "descriptor_version": "1.0.0",
            "cardinality": "one"
          }
        ],
        "execution_class": "lenso.native-rust@1"
      }
    ],
    "bindings": [
      {
        "consumer": "welcome-flow",
        "capability_id": "example.greeting@1",
        "descriptor_version": "1.0.0",
        "provider": "greeter"
      }
    ],
    "execution_lanes": [{ "id": "main" }]
  },
  "packages": {
    "example.greeting": {
      "name": "example.greeting",
      "package_name": "greeting-module",
      "source": "cargo",
      "version": "0.1.0",
      "manifest": "Cargo.toml",
      "lockfile": "Cargo.lock"
    },
    "example.welcome-flow": {
      "name": "example.welcome-flow",
      "package_name": "welcome-flow-module",
      "source": "cargo",
      "version": "0.1.0",
      "manifest": "Cargo.toml",
      "lockfile": "Cargo.lock"
    }
  },
  "contracts": [
    {
      "capability_id": "example.greeting@1",
      "descriptor_version": "1.0.0",
      "descriptor": "contracts/greeting/capability.json",
      "rust": "contracts/greeting/src/generated.rs",
      "typescript": "contracts/greeting/generated/bindings.ts"
    }
  ],
  "profiles": {}
}
```

The exact package version/revision must agree with the ordinary lockfile and,
for native Modules, the linked factory's `package_id()`/`package_version()`.
For a Bun/npm package, select `lenso.bun-process@1` when that is the installed
Adapter class and set `entrypoint` to the executable script. OCI inputs need an
immutable `sha256:` digest and an explicitly supported execution class.

## Authoring command shape

Read `lenso --help` first. The current workflow has this shape:

```sh
lenso add --project lenso.json \
  --key greeter \
  --package example.greeting \
  --package-name greeting-module \
  --source cargo \
  --version 0.1.0 \
  --manifest Cargo.toml \
  --lockfile Cargo.lock

lenso check --project lenso.json \
  --execution-class lenso.native-rust@1

lenso resolve --project lenso.json \
  --execution-class lenso.native-rust@1 \
  --output .lenso/resolved-plan.json

lenso run --plan .lenso/resolved-plan.json --root .
```

`add` is a reviewable project/package edit, not permission to install into a
running App. Manually add the endpoint/requirement/contract/binding data that
the installed CLI does not yet author, then run `check`.

## Secret and configuration rule

Configuration is Module-owned opaque data. Non-empty configuration needs its
declared JSON Schema. Fields marked `x-lenso-sensitive: true` accept a
`{"secret_ref":"NAME"}` reference, not a raw secret. Secret values stay out
of Composition, Resolved Plan, Invocation Context, and Runtime Diagnostics.
