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
| `capability ...` | `capability` |
| `admin.schema...` | `admin.schema` |
| `admin.declarative...` | `admin.declarative` |
| `admin.embedded...` | `admin.embedded` |
| `module...` | `module` |
| anything else | `manifest` |

When adding a lint rule, choose a subject that fits this catalog or update the
Console category mapping and tests in the same change.

## Current Catalog

| Severity | Subject | Meaning |
| --- | --- | --- |
| `error` | `module.name` | Manifest name is missing. |
| `warning` | `capability {value}` | Capability name is not dot-separated lowercase. |
| `ok` / `warning` | `routes` | Empty route declaration state. Remote modules warn; linked modules are OK. |
| `error` | `METHOD /path` | Duplicate HTTP route method/path. |
| `warning` | `METHOD /path` | Route display, story title, or remote capability metadata is missing. |
| `warning` | `admin.schema` | Schema surface declares no entities. |
| `warning` | `admin.schema.{entity}` | Schema entity is missing read capability. |
| `warning` | `admin.declarative.pages` | Declarative surface declares no pages. |
| `warning` | `admin.declarative.fallback_schema` | Declarative fallback schema declares no entities. |
| `warning` | `admin.declarative.fallback_schema.{entity}` | Declarative fallback entity is missing read capability. |
| `warning` | `admin.declarative.section.{section}` | Declarative section references an entity missing from `fallback_schema`. |
| `warning` | `admin.embedded.runtime` | Embedded runtime is reserved by current host policy. |
| `warning` | `admin.embedded.entry.url` | Embedded entry URL is not HTTPS outside local development. |
| `warning` | `admin.embedded.entry.allowed_origins` | Embedded surface has no origin allowlist. |
| `warning` | `admin.embedded.fallback_schema` | Embedded fallback schema declares no entities. |
| `warning` | `admin.embedded.fallback_schema.{entity}` | Embedded fallback entity is missing read capability. |
| `warning` | `admin.embedded.permission.{entity}` | Embedded read permission references an entity missing from `fallback_schema`. |
| `ok` | `manifest` | No other lint result was produced. |
