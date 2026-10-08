# Notes entry boundaries

The default application exposes its existing raw Fetch and native oRPC routes.
It does **not** mount `/api`, an OpenAPI specification or documentation UI.
CLI and MCP select their own existing operation declarations.

`src/openapi.ts` is an optional, read-only HTTP selection over the same Notes
router. Install `@orpc/openapi@2.0.0-beta.42` in deployments that enable it.
During the owning application's setup, after resolving the existing Notes and
authentication service instances:

```ts
import { createNotesRouter } from "./router";
import { createNotesOpenAPI } from "./openapi";

const router = createNotesRouter(notesService, authenticationService);
const api = createNotesOpenAPI(router);

// The native RPCHandler can use this same router at /rpc.
// The Web raw Fetch hook can explicitly return api.fetch(context).
const response = await api.handle(request, { request, signal: request.signal });
const spec = await api.generateSpec({
  info: { title: "Selected Notes API", version: "1.0.0" },
});
```

Only `GET /api/notes` and `POST /api/notes/read` are selected. Their existing
`requiredAuth` middleware verifies bearer evidence and the service rechecks its
audience/owner rules. The explicit admission callback delegates to that middleware;
it does not invent an actor from JSON. Writes, sessions, Files and private Manage
operations are absent from the selection and specification.

The two selected procedures share their input/output schemas with native RPC.
Missing notes remain successful `null` results, not a fabricated 404 contract.
Errors use RFC 9457 `application/problem+json`; native `/rpc` remains oRPC.
Publishing the returned specification requires a separate application policy.
Use the decoder and optional bounded Fetch guard from
`@lenso/web/openapi-client` for OpenAPILink, not for RPCLink.

`test/openapi.test.ts` exercises both clients against the actual SQLite-backed
Notes service, including unauthenticated and cross-owner calls and disabled
OpenAPI routes. No production database or credential is required.
