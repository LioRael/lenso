# Web and Auth

Use [Web contracts](https://github.com/LioRael/lenso/blob/6e239c71a38279885facce133ceb847bbfe12f2f/packages/web/README.md) and [Auth contracts](https://github.com/LioRael/lenso/blob/6e239c71a38279885facce133ceb847bbfe12f2f/packages/auth/README.md) for the changed boundary, not a copied adapter implementation.

## Assemble around the existing service

- Reuse the shape of [createNotesApplication](https://github.com/LioRael/lenso/blob/6e239c71a38279885facce133ceb847bbfe12f2f/examples/notes/src/application.ts), [startNotesServer](https://github.com/LioRael/lenso/blob/6e239c71a38279885facce133ceb847bbfe12f2f/examples/notes/src/server.ts), and the [Notes Web adapter](https://github.com/LioRael/lenso/blob/6e239c71a38279885facce133ceb847bbfe12f2f/examples/notes/src/web.ts). For Bun, `createBunListenerPlugin` from `@lenso/web/bun` already owns listener setup/cleanup; declare the exact Web instance and explicit ingress policy. Notes' loopback policy is not production ingress.
- The listener is merged in this baseline; external use requires a verified packed/released build exporting `/bun`. If absent, report the artifact gap and keep existing supported assembly rather than write replacement listener glue.
- Use the same business schema/service across Fetch, oRPC and CLI. Browser clients import `@lenso/web/client`, not server assembly. Read the installed oRPC contract before changing routers; apply authentication middleware once because this baseline does not deduplicate it.
- A returned Response means headers ready, not completed work. Register acquired request resources with `WebContext.onCleanup`, independent producer work with `waitUntil`, and propagate its signal to providers. Consume/cancel bodies in tests. Deadlines and abort are cooperative; cleanup must wait for actual work settlement.

## Choose the identity/session owner

- Auth's root verifies sources and derives audience-specific actors; `/plugin` wires lifecycle. `/sessions` supplies optional managed opaque sessions; `/session-source` bridges an existing session owner. `/fetch`, `/orpc` and the documented `/drizzle/*` entries are public adapters, not mandatory dependencies.
- Keep one session owner. Managed sessions need explicit store/lifetime and cleanup; bridging existing sessions does not transfer their ownership. Follow [Notes Auth assembly](https://github.com/LioRael/lenso/blob/6e239c71a38279885facce133ceb847bbfe12f2f/examples/notes/src/application-auth.ts); its configured demo login keys are not a production account system.
- Preserve exact Auth-instance provenance and operation audience. JSON actors, copied/spread actors and actors from another instance/audience cannot authorize. Verify trusted evidence at each entry and revalidate through shared service policy.
- Load the actual object before choosing tenant/owner policy; client tenant/owner claims are not evidence. Cover wrong audience, forged provenance, revoked credentials and another owner's record when changing authorization.
- For credential-bearing HTTP endpoints, retain explicit extraction and Origin/Host policy; Origin checks do not replace authentication. Keep raw credentials and session-bearing URLs out of logs and discovery metadata.
