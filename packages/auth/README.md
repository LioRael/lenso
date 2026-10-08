# @lenso/auth

Independent authentication plugin; Lenso core has no auth dependency.
`createAuthPlugin({ provider, id? })` exposes
`authenticate({ request, signal? }): Promise<Identity | null>`. Providers verify
credentials through their supported API and return `anonymous`, `invalid`, or
`authenticated` with `{ subject, tenantId? }`. Missing identity is anonymous;
invalid identity throws `UNAUTHORIZED`; provider failures throw sanitized
`SERVICE_UNAVAILABLE`. Results are projected, frozen, and cached per Request.
Tokens, sessions and provider errors never enter the public identity.

For an already configured Better Auth instance, use its documented session API:

```ts
const auth = createAuthPlugin({
  provider: createSessionProvider({
    getSession: (input) => betterAuth.api.getSession(input),
    identity: ({ user }) => ({ subject: user.id }),
  }),
});
const web = createWebPlugin({
  requires: [auth, notes],
  router: (plugins) => ({
    read: os
      .$context<WebContext>()
      .use(requiredAuth(plugins.get(auth)))
      .input(z.object({ id: z.string() }))
      .handler(({ context, input }) => plugins.get(notes).read(context.identity, input.id)),
  }),
});
```

`requiredAuth(service)` is typed oRPC middleware providing non-null `identity`.
`optionalAuth(service)` explicitly permits anonymous callers; invalid identities
still fail closed. Both map `AuthError` from the business service to safe oRPC
errors. They never return a provider session. Neither implements account,
password, login, cookie issuance or OAuth flows; configure those with the provider.
The session adapter passes only headers to `getSession`, matching that API. It
checks cancellation before/after, but cannot interrupt a getSession call that
doesn't support signals. A custom provider receives `signal` for supported APIs.

Authentication is not object authorization. Inside the business service, load
trusted object ownership and call `authorize(identity, policy)` before accessing
data, including direct non-HTTP calls. For example, compare both
`identity.tenantId === object.tenantId` and `identity.subject === object.ownerId`.
Never use a client-supplied tenant/object ID as proof of ownership or membership.
Only exact `true` grants access; false throws `FORBIDDEN`, policy failure throws
sanitized `SERVICE_UNAVAILABLE`. Trusted callers must pass provider-verified
identities. Applications own membership lookup, revocation and authorization
policy. A subject alone conveys no tenant membership.

Cookie-based providers must retain their own secure-cookie, origin and CSRF
protections. This plugin adds no CORS/GET bypass or automatic credential logging.
The test fixture's `x-test-identity` header is **only a fixture**, never production
authentication. No live provider credentials are created by this package.

Supported API references: [Better Auth session](https://better-auth.com/docs/basic-usage),
[oRPC v1 integration](https://v1.orpc.dev/docs/integrations/better-auth),
[typed middleware](https://v1.orpc.dev/docs/middleware).
