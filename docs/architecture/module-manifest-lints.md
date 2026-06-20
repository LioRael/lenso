# Module Manifest Lints

`platform-module` owns module manifest lint rules. Backends expose the resulting
`manifest_lints` through module metadata endpoints, and the Runtime Console only
filters, groups, and renders those results.

Each lint has:

- `severity`: `error`, `warning`, or `ok`.
- `subject`: stable enough for operator grouping and search.
- `message`: human-readable problem summary.
- `suggestion`: human-readable next action.

## Subject Categories

The Runtime Console derives categories from `subject`:

| Subject pattern | Console category |
| --- | --- |
| `routes` or `METHOD /path` | `routes` |
| `capability ...` or `capability...` | `capability` |
| `admin.schema...` | `admin.schema` |
| `admin.declarative...` | `admin.declarative` |
| `admin.embedded...` | `admin.embedded` |
| `runtime...` | `runtime` |
| `events...` | `events` |
| `lifecycle...` | `lifecycle` |
| `console...` | `console` |
| `module...` | `module` |
| anything else | `manifest` |

When adding a lint rule, choose a subject that fits this catalog or update the
Console category mapping and tests in the same change.

## Current Catalog

| Severity | Subject | Meaning |
| --- | --- | --- |
| `error` | `module.name` | Manifest name is missing. |
| `warning` | `capability {value}` | Capability name is not dot-separated lowercase. |
| `warning` | `capability.reference.{surface}` | HTTP route, admin read, or admin action capability is referenced but not declared in the module manifest. |
| `ok` / `warning` | `routes` | Empty route declaration state. Remote modules warn; linked modules are OK. |
| `error` | `METHOD /path` | Duplicate HTTP route method/path. |
| `warning` | `METHOD /path` | Route display, story title, or remote capability metadata is missing. |
| `warning` | `admin.schema` | Schema surface declares no entities. |
| `warning` | `admin.schema.{entity}` | Schema entity is missing read capability. |
| `warning` | `admin.declarative.pages` | Declarative surface declares no pages. |
| `warning` | `admin.declarative.fallback_schema` | Declarative fallback schema declares no entities. |
| `warning` | `admin.declarative.fallback_schema.{entity}` | Declarative fallback entity is missing read capability. |
| `warning` | `admin.declarative.section.{section}` | Declarative section references an entity missing from `fallback_schema`. |
| `warning` | `admin.declarative.query.{query}` | Declarative query value is missing a stable name, value path, or read capability. |
| `warning` | `admin.embedded.runtime` | Embedded runtime is reserved by current host policy. |
| `warning` | `admin.embedded.entry.url` | Embedded entry URL is not HTTPS outside local development. |
| `warning` | `admin.embedded.entry.allowed_origins` | Embedded surface has no origin allowlist. |
| `warning` | `admin.embedded.fallback_schema` | Embedded fallback schema declares no entities. |
| `warning` | `admin.embedded.fallback_schema.{entity}` | Embedded fallback entity is missing read capability. |
| `warning` | `admin.embedded.permission.{entity}` | Embedded read permission references an entity missing from `fallback_schema`. |
| `error` | `console.surface.{surface}` | Console surface is missing a name or duplicates another surface name. |
| `error` | `console.surface.{surface}.route` | Console surface route is invalid or duplicates another route. |
| `warning` | `console.surface.{surface}.label` | Console surface is missing an operator-facing label. |
| `warning` | `console.surface.{surface}.package` | Console surface package name is not shaped like an npm package. |
| `warning` | `console.surface.{surface}.package.export` | Console surface package export is missing. |
| `warning` | `console.surface.{surface}.navigation.workspace.id` | Console workspace id is invalid or uses the host-reserved `system` id. |
| `warning` | `console.surface.{surface}.navigation.workspace.label` | Console workspace is missing an operator-facing label. |
| `warning` | `console.surface.{surface}.navigation.group.id` | Console navigation group id is invalid. |
| `warning` | `console.surface.{surface}.navigation.group.label` | Console navigation group is missing an operator-facing label. |
| `warning` | `runtime.functions` | Runtime surface declares no functions. |
| `error` | `runtime.function` | Runtime function declaration is missing a name. |
| `warning` | `runtime.function.{name}` | Runtime function name is not path-safe, queue is missing, or another declaration quality issue applies. |
| `error` | `runtime.function.{name}` | Runtime function name is declared more than once. |
| `warning` | `runtime.function.{name}.input_schema` | Runtime function input schema does not match the function name. |
| `warning` | `runtime.function.{name}.retry_policy` | Runtime retry policy declares zero attempts. |
| `warning` | `events.handlers` | Event surface declares no handlers. |
| `error` | `events.handler` | Event handler declaration is missing a name. |
| `warning` | `events.handler.{name}` | Event handler name is not path-safe. |
| `error` | `events.handler.{name}` | Event handler name is declared more than once. |
| `error` | `events.handler.{name}.event_name` | Event handler declaration is missing an event name. |
| `warning` | `events.handler.{name}.event_name` | Event name is not path-safe. |
| `warning` | `lifecycle` | Lifecycle surface declares no startup checks or activation jobs. |
| `warning` | `lifecycle.startup_check` | Lifecycle startup check is missing a name. |
| `error` | `lifecycle.startup_check.function_registered.{function}` | Lifecycle startup check references an unknown runtime function. |
| `warning` | `lifecycle.startup_check.capability.{capability}` | Lifecycle startup check references an undeclared capability. |
| `warning` | `lifecycle.activation_job` | Lifecycle activation job is missing a name. |
| `error` | `lifecycle.activation_job` | Lifecycle activation job is missing a function name. |
| `error` | `lifecycle.activation_job.{job}` | Lifecycle activation job references an unknown runtime function. |
| `ok` | `manifest` | No other lint result was produced. |
