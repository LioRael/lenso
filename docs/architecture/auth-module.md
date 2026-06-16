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

## Installation

Today `auth` is a linked module registered by `crates/app-bootstrap`:

- The `demo` linked profile enables it by default.
- `modules.auth.enabled = false` disables its migrations, HTTP routes, admin
  data, and actor resolver install.
- The `core` linked profile does not install it.

The current HTTP surface is intentionally small:

- `POST /v1/auth/dev/sessions` creates a local-development session for a user id.
- `POST /v1/auth/sessions/revoke` revokes the current bearer or cookie session.

The actor resolver accepts a bearer session token or `lenso_session` cookie,
checks `auth.sessions`, and returns only:

```text
ActorContext::User { user_id, scopes: [] }
```

Authorization, product profile lookup, tenant membership, and richer claims
belong to the installing application or later focused auth slices.

## Loading Model

Keep core auth linked/in-process for now. Session resolution is a host trust
boundary and runs on every request. Remote modules may later provide provider
connectors, but they should not own host sessions or receive caller bearer
tokens.
