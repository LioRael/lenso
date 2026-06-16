# Auth Module

The `auth` module is Lenso's host-owned authentication anchor. It owns the
minimum tables and HTTP routes needed to turn credentials into an
`ActorContext::User`; it does not own product user profiles.

## Boundary

`auth.users` is an authentication anchor:

- `id` is the stable user actor id returned by the host auth resolver.
- `created_at` records when the actor anchor was created.
- `disabled_at` blocks future session resolution.

Application-specific user data belongs outside this module. A product can define
its own `users`, `profiles`, `accounts`, or tenant membership tables and key them
by the auth user id, or keep a separate mapping if it needs different ids. The
auth module should not grow profile fields such as name, email, avatar, bio,
plan, or organization membership.

`identity` remains a demo fixture. It is useful for examples and integration
tests, but it is not a dependency of `auth`.

## Product User Tables

Applications should treat `auth.users.id` as the actor id and define their own
business tables around it. A simple app can keep the same id:

```sql
create table app.users (
    id text primary key,
    display_name text not null,
    created_at timestamptz not null
);
```

The app creates or updates this row in its own module after registration, invite
acceptance, or onboarding. The auth module does not insert it automatically.

Apps that need a different profile/account id can keep a mapping instead:

```sql
create table app.accounts (
    id text primary key,
    auth_user_id text not null unique,
    name text not null,
    created_at timestamptz not null
);
```

Routes that need the product profile should read `ActorContext::User { user_id,
.. }` from the request context and query the app-owned table by `user_id`. If no
row exists, return the app's onboarding or not-found behavior. Do not add those
fields to `auth.users`.

## Installation

Today `auth` is a linked module registered by `crates/app-bootstrap`:

- Product hosts should use `LENSO_COMPOSITION_PROFILE=core` and explicitly add
  `builtins::auth()` plus provider modules such as `builtins::auth_password()`
  to their host composition.
- The `demo` linked profile enables it by default.
- `modules.auth.enabled = false` disables its migrations, HTTP routes, admin
  data, and actor resolver install.
- The `core` linked profile does not install it.

The current HTTP surface is intentionally small:

- `POST /v1/auth/dev/sessions` creates a local-development session for a user id.
- `POST /v1/auth/sessions/revoke` revokes the current bearer or cookie session.

The first provider is the separate linked `auth-password` module:

- `modules.auth-password.enabled = false` disables the password provider.
- It depends on `auth`; if `auth` is disabled, `auth-password` is not installed.
- Its `ModuleManifest.dependencies` declares `["auth"]`, and
  `/admin/data/modules` exposes that dependency for diagnostics and installers.
- `POST /v1/auth/password/register` registers `identifier + password`.
- `POST /v1/auth/password/login` creates a session for a password identity.

The password provider stores provider-specific credential hashes in its own
`auth_password` schema. It uses `auth::public` helpers to create auth users,
identities, and sessions, so the auth core remains the owner of those tables.

The actor resolver accepts a bearer session token or `lenso_session` cookie,
checks `auth.sessions`, and returns only:

```text
ActorContext::User { user_id, scopes: [] }
```

Authorization, product profile lookup, tenant membership, and richer claims
belong to the installing application or later focused auth slices.

## Loading Model

Keep core auth linked/in-process for now. Session resolution is a host trust
boundary and runs on every request. First-party linked provider modules can sit
beside it, as `auth-password` does. Remote modules may later provide external
provider connectors, but they should not own host sessions or receive caller
bearer tokens.
