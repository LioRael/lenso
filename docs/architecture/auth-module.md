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

Product profile data stays outside `auth`; install or write an application
module when user-facing profiles are needed.

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

Today `auth` is a linked module registered by `crates/lenso-bootstrap`:

- Product hosts should use `LENSO_COMPOSITION_PROFILE=core` and explicitly add
  `builtins::auth()` plus provider modules such as `builtins::auth_password()`
  or `builtins::auth_phone()` to their host composition.
- The `demo` linked profile enables it by default.
- `modules.auth.enabled = false` disables its migrations, HTTP routes, admin
  data, and actor resolver install.
- The `core` linked profile does not install it.

The current HTTP surface is intentionally small:

- `POST /v1/auth/dev/sessions` creates a local-development session for a user id.
- `POST /v1/auth/sessions/revoke` revokes the current bearer or cookie session.

The password provider is the separate linked `auth-password` module:

- `modules.auth-password.enabled = false` disables the password provider.
- It depends on `auth`; if `auth` is disabled, `auth-password` is not installed.
- Its `ModuleManifest.dependencies` declares `["auth"]`, and
  `/admin/data/modules` exposes that dependency for diagnostics and installers.
- `POST /v1/auth/password/register` registers `identifier + password`.
- `POST /v1/auth/password/login` creates a session for a password identity.

The password provider stores provider-specific credential hashes in its own
`auth_password` schema. It uses `auth::public` helpers to create auth users,
identities, and sessions, so the auth core remains the owner of those tables.

The phone provider is the separate linked `auth-phone` module:

- `modules.auth-phone.enabled = false` disables the phone provider.
- It depends on `auth`; if `auth` is disabled, `auth-phone` is not installed.
- Its `ModuleManifest.dependencies` declares `["auth"]`, and
  `/admin/data/modules` exposes that dependency for diagnostics and installers.
- `POST /v1/auth/phone/otp/start` starts a phone OTP challenge.
- `POST /v1/auth/phone/otp/verify` verifies an OTP and creates a session.
- `POST /v1/auth/phone/password/set` sets or replaces the current phone
  identity's password.
- `POST /v1/auth/phone/password/login` creates a session for a phone password
  identity.

The phone provider stores provider-specific identities, OTP challenges,
password hashes, and password failure counters in its own `auth_phone` schema.
OTP policy and password length are editable runtime config under
`auth-phone.otp` and `auth-phone.password`; OTP secrets stay in module-local
host config, for example `LENSO_MODULE_AUTH_PHONE__OTP_SECRET=<secret>`.
Outside local development, OTP start and verify fail closed if no module-local
secret is configured.

The `auth-anonymous` provider creates an auth user, an `anonymous` identity, and
a normal `auth.sessions` row without collecting PII:

- `POST /v1/auth/anonymous/login` creates an anonymous session.
- `auth.users.is_anonymous` marks whether the user still has only anonymous
  credentials.
- Provider modules can call `auth::public::link_identity_to_anonymous_user_in_tx`
  to bind a real credential to the same auth user, which keeps downstream
  module data attached to the same `auth_user_id`.

The `auth-oidc` provider exposes the host as an OIDC provider for the hosted
Runtime Console:

- `/.well-known/openid-configuration`
- `/.well-known/jwks.json`
- `/oauth/authorize`
- `/oauth/token`

It depends on `auth` and reuses the same auth sessions. A Console browser first
signs in through password auth or an existing session cookie, then the OIDC
authorization-code + PKCE flow issues an access token backed by `auth.sessions`.
The normal auth resolver turns that access token back into
`ActorContext::User { user_id, scopes }`.

Enable it with module-local config on `auth-oidc`:

```sh
LENSO_MODULE_AUTH_OIDC__ENABLED=true
LENSO_MODULE_AUTH_OIDC__ISSUER=https://app.example.com
LENSO_MODULE_AUTH_OIDC__CONSOLE_REDIRECT_URIS='["https://app.example.com/console/oidc/callback"]'
LENSO_MODULE_AUTH_OIDC__JWKS='{"keys":[...]}'
LENSO_MODULE_AUTH_OIDC__ID_TOKEN_PRIVATE_KEY_PEM="$OIDC_SIGNING_KEY_PEM"
```

`console_client_id` defaults to `lenso-console`. Keep
`id_token_private_key_pem` in the host secret store, and make `jwks` the public
key set that matches it.

The actor resolver accepts a bearer session token or `lenso_session` cookie,
checks `auth.sessions`, and returns:

```text
ActorContext::User { user_id, scopes: [] }
```

By default `scopes` is empty. Hosts that expose the Runtime Console through
normal user login can set `auth.console_admin_user_scopes` to a JSON object that
maps auth user ids to explicit scopes, for example:

```json
{
  "usr_admin": ["console.admin", "auth.users.read", "identity.users.read"]
}
```

`console.admin` is required before a user can enter admin HTTP endpoints; other
capabilities are still checked per admin data query, action, or remote route.
For Runtime Console stories, add `runtime.stories.read`. Bootstrap the first
production Console admin from the host root after the auth user exists:

```sh
lenso console bootstrap-admin --user-id usr_admin --scope runtime.stories.read
```

For password auth, `--identifier admin@example.com` can resolve the auth user id.
Restart the API and worker after bootstrapping.

Do not embed `dev-user`, `dev-service`, or other service bearer tokens in a
browser Runtime Console build.
Authorization beyond these explicit Console scopes, product profile lookup,
tenant membership, and richer claims belong to the installing application or
later focused auth slices.

## Loading Model

Keep core auth linked/in-process for now. Session resolution is a host trust
boundary and runs on every request. First-party linked provider modules can sit
beside it, as `auth-password` and `auth-phone` do. Remote modules may later
provide external provider connectors, but they should not own host sessions or
receive caller bearer tokens.
