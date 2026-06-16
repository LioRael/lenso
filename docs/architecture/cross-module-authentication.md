# Cross-Module Authentication

Modules in Lenso never import the `auth` module directly. Authentication and
authorization are platform-level concerns delivered through middleware and Axum
extractors. This document shows how to use them.

## Request-to-Handler Flow

```text
HTTP request
  → platform-http middleware extracts Authorization / Cookie headers
  → ActorResolver chain resolves an ActorContext
  → ActorContext stored in RequestContext.actor
  → Axum extension injected as HttpRequestContext
  → Handler extracts authenticated actor via platform_http::auth types
```

## Actor Variants

`ActorContext` (defined in `platform-core`) represents the calling identity:

```rust
pub enum ActorContext {
    Anonymous,
    User { user_id: String, scopes: Vec<String> },
    Service { service_id: String, scopes: Vec<String> },
    System,
}
```

The resolver chain determines which variant applies:

- **DevActorResolver** — local development only.
  - `Bearer dev-user:<user_id>` → `ActorContext::User`
  - `Bearer dev-service:<service_id>` → `ActorContext::Service`
- **AuthActorResolver** — production. Validates Bearer tokens and
  `lenso_session` cookies against `auth.sessions` / `auth.users`.

Composition in `app-bootstrap` wires the resolver chain based on the enabled
modules. Adding a new auth method means adding a new `ActorResolver`
implementation — no existing module changes required.

## Axum Extractors

All extractors live in `platform_http::auth`. They pull `ActorContext` from the
request extension and reject requests that do not match the required actor type.

| Extractor | Accepted variants | Rejection |
|---|---|---|
| `OptionalActor(ActorContext)` | All (including Anonymous) | Never rejects |
| `AuthenticatedActor(ActorContext)` | User, Service, System | 401 for Anonymous |
| `UserActor { user_id, scopes }` | User only | 401 for Anonymous, 403 for Service/System |
| `ServiceActor { service_id, scopes }` | Service only | 401 for Anonymous, 403 for User/System |
| `AdminActor` (enum) | Service, System | 401 for Anonymous, 403 for User |

### Usage in handlers

**Require an authenticated user:**

```rust
use platform_http::auth::UserActor;

async fn me(user: UserActor) -> impl IntoResponse {
    Json(json!({ "user_id": user.user_id }))
}
```

**Require a service or system actor (admin endpoint):**

```rust
use platform_http::auth::AdminActor;

async fn admin_endpoint(actor: AdminActor) -> impl IntoResponse {
    match actor {
        AdminActor::Service { service_id, scopes } => { /* ... */ }
        AdminActor::System => { /* ... */ }
    }
}
```

**Allow optional authentication:**

```rust
use platform_http::auth::OptionalActor;
use platform_core::ActorContext;

async fn public_with_optional_auth(actor: OptionalActor) -> impl IntoResponse {
    match actor.0 {
        ActorContext::User { user_id, .. } => { /* authenticated */ }
        ActorContext::Anonymous => { /* anonymous */ }
        _ => { /* other variants */ }
    }
}
```

## Scope and Capability Checks

Modules declare capabilities in `ModuleManifest.capabilities`:

```rust
// In a module manifest
capabilities: [
    "my-module.items.read",
    "my-module.items.write",
],
```

For schema-admin reads, `AdminSchema.entities[].read_capability` gates access.
The `AdminActor::Service` variant carries `scopes: Vec<String>`, and
`platform-admin-data` checks them:

```rust
// platform-admin-data/src/handlers.rs (simplified)
if !scopes.iter().any(|s| s == required_capability) {
    return Err(/* 403 */);
}
```

For module-specific authorization beyond the built-in extractors, read the
`scopes` field from the actor and check against your own capability strings.

## Module Wiring

No module imports `auth` for request-level authentication. The `auth` module
exports `AuthIdentity` and `AuthUserId` through `auth::public` solely for the
`identity` module's user-creation transaction flow — this is a structural
database dependency, not an HTTP auth dependency.

Modules receive authentication through the platform middleware that runs before
any handler, injected via `app-bootstrap`. The wiring is:

1. `app-bootstrap` checks the composition profile.
2. If the `auth` module is enabled, `AuthActorResolver` wraps the app context:
   `ctx.with_actor_resolver(actor_resolver)`.
3. The API app applies this at startup (`apps/api/src/lib.rs`).

## Summary

- **Do** import `platform_http::auth::{UserActor, AdminActor, …}` in your
  module routes.
- **Do** check `scopes` for fine-grained authorization within your module.
- **Don't** import the `auth` module crate from other modules.
- **Don't** add auth-centric logic to modules that don't own auth data.
- Extending auth means implementing `ActorResolver` and adding it to the chain
  in `app-bootstrap` — zero changes to existing modules.