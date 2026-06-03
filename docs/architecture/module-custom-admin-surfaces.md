# Module Custom Admin Surfaces

This note specifies the admin-surface direction for the module framework.
`AdminSurface::Schema` is the generic data-admin surface. `EmbeddedCustom`
currently has a first Runtime Console iframe renderer with origin checks,
sandbox attributes, and no host bridge. `DeclarativeCustom` remains a contract
only until the host-rendered component protocol is implemented.

## Why Two Custom Modes

Custom admin UI has two different meanings:

- Host-rendered custom UI: the module declares screens, sections, actions, and
  data bindings as structured manifest data. The Runtime Console renders those
  declarations with its own trusted React components.
- Module-owned custom UI: the module provides a UI artifact that is embedded or
  executed across a stricter boundary, such as a sandboxed iframe first, then
  possibly Wasm later.

These should not be one `Custom` variant. They have different security models,
runtime behavior, and manifest fields. A single variant would produce many
mutually exclusive fields such as `entry_url` for iframe surfaces and `pages`
for declarative surfaces.

The target shape is:

```rust
pub enum AdminSurface {
    Schema(AdminSchema),
    DeclarativeCustom(AdminDeclarativeSurface),
    EmbeddedCustom(AdminEmbeddedSurface),
}
```

## `DeclarativeCustom`

`DeclarativeCustom` is a host-rendered lane. The module contributes data, not
code. It is appropriate for richer screens that still fit the platform's UI
grammar: forms, tables, detail panels, charts, status blocks, action buttons,
tabs, filters, and workflow steps.

Target shape:

```rust
pub struct AdminDeclarativeSurface {
    pub pages: Vec<AdminDeclarativePage>,
    pub actions: Vec<AdminAction>,
    pub fallback_schema: Option<AdminSchema>,
}
```

Rules:

- The manifest is pure serializable data.
- The Runtime Console owns rendering, styling, accessibility, layout, and
  component behavior.
- Declarative actions call host-defined admin action endpoints. They do not run
  module-provided frontend code.
- Data reads may use `AdminDataSource` where the surface maps onto schema-admin
  entities, or a later action/query protocol when it does not.
- `fallback_schema` lets the host offer generic schema-admin access when the
  declarative renderer cannot support a page.

Deferred from the first implementation:

- Arbitrary expression languages.
- Module-authored JavaScript.
- Unbounded layout primitives.
- Direct module access to host tokens.

## `EmbeddedCustom`

`EmbeddedCustom` is the module-owned UI lane. It is the right model when a module
needs its own frontend runtime, complex visualization, or product-specific
workflow that cannot be expressed declaratively.

Target shape:

```rust
pub struct AdminEmbeddedSurface {
    pub runtime: AdminEmbeddedRuntime,
    pub entry: AdminEmbeddedEntry,
    pub sandbox: AdminSandboxPolicy,
    pub permissions: Vec<AdminPermission>,
    pub fallback_schema: Option<AdminSchema>,
}

pub enum AdminEmbeddedRuntime {
    Iframe,
    Wasm,
    JsBundle,
}
```

The initial implementation allows only `Iframe`. `Wasm` and `JsBundle` are
reserved names until they have separate execution, signing, and lifecycle specs.

Rules:

- The Runtime Console embeds the surface behind a sandbox boundary.
- Iframe entries must be absolute URLs whose origins match an explicit allowlist.
- The host must not pass bearer tokens, service credentials, or database access
  into the embedded surface.
- Any host/module communication must use a versioned message protocol. Do not
  add ad hoc `postMessage` commands.
- Permissions are declared in the manifest and enforced by the host before any
  bridge or action endpoint is exposed.
- `fallback_schema` gives operators a host-rendered escape hatch when an
  embedded surface is unavailable or blocked by policy.

Deferred from the first implementation:

- Bidirectional host/module action bridge.
- Remote JavaScript bundle loading.
- Wasm component execution.
- Marketplace install, signature verification, and provenance policy.
- Module-owned routing inside the Runtime Console shell.

## Relationship To Existing Surfaces

`Schema` remains the plain business-entity lane. It should stay narrow and
generic: entity fields, list/detail reads, and later controlled writes.

`DeclarativeCustom` can reuse schema-admin data, but it is not just CRUD. It is
for host-rendered workflows built from trusted components.

`EmbeddedCustom` is for module-owned UI and requires a stronger sandbox and
permission model. Its first useful slice is a visible, sandboxed iframe with no
host bridge.

The loading source axis remains separate:

- Linked modules may declare any admin surface.
- Remote modules may declare custom surfaces once the protocol and host policy
  are implemented.
- Wasm as a module source is separate from `EmbeddedCustom::Wasm`; one describes
  how a module is loaded, the other describes how its admin UI runs.

## Implementation Order

1. Add manifest data types for `DeclarativeCustom` and `EmbeddedCustom` without
   rendering either mode.
2. Teach schema/admin metadata endpoints and generated SDK types to preserve the
   new variants.
3. Implement `EmbeddedCustom::Iframe` as a read-only visible surface with origin
   checks and `sandbox` attributes, but no message bridge. Done for the Runtime
   Console Data page.
4. Implement a small `DeclarativeCustom` renderer for one or two trusted
   components.
5. Specify a versioned host/module message and action protocol before enabling
   any embedded surface to call back into the host.
