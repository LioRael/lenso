# @lenso/service-kit

Helpers for building Lenso Provider Services that provide one or more Modules.

## Capability tier

`@lenso/service-kit` implements the Provider tier only through
`lenso.service.v1`. It does not provide Autonomous Service parity for
`lenso.service.v2`; the current Autonomous Service runtime and its Service-owned
storage, direct HTTP/gRPC, Event Contract, Durable Workflow, Workload Identity,
and Delegated Actor Context capabilities are Rust only.

See the framework's
[Service Capability Tiers](https://github.com/LioRael/lenso/blob/main/docs/architecture/service-capability-tiers.md)
for the exact ownership boundary.

```ts
import {
  defineModule,
  defineKubernetesDeployment,
  defineModuleContract,
  defineModuleRelease,
  defineService,
  defineServiceContract,
  defineServicePackage,
  defineServiceReleasePlan,
  defineServiceWorkspace,
  serviceReleaseRestartRequired,
  serviceEnv,
  serviceWorkspaceBaseUrl,
  serviceWorkspaceToModuleServices,
  serveService,
} from "@lenso/service-kit";

const supportTicket = defineModule({
  name: "support-ticket",
  version: "0.1.0",
  capabilities: ["support_ticket.tickets.read"],
});

export const moduleContract = defineModuleContract({
  name: supportTicket.name,
  version: supportTicket.version ?? "0.1.0",
  source: "service",
  capabilities: supportTicket.capabilities,
});

export const contract = defineServiceContract({
  name: "support-suite-provider",
  version: "0.2.0",
  deployment: defineKubernetesDeployment({
    ingressHost: "support-staging.example.com",
    port: 4110,
    replicas: 2,
  }),
  env: [serviceEnv("PORT", { example: "4110", required: true })],
  modules: [{ name: supportTicket.name, version: supportTicket.version }],
});

export const manifest = defineService({
  name: contract.name,
  version: contract.version,
  modules: [supportTicket],
});

export const servicePackage = defineServicePackage({
  name: contract.name,
  version: contract.version ?? "0.1.0",
  modules: [supportTicket.name],
});

export const workspace = defineServiceWorkspace({
  services: [
    {
      command: "pnpm start",
      cwd: "services/support-suite-provider",
      lang: "ts",
      manifest: "lenso.service.json",
      modules: [supportTicket.name],
      name: contract.name,
      readyUrl: "http://127.0.0.1:4110/lenso/service/v1/status",
    },
  ],
});

export const serviceStartFile = serviceWorkspaceToModuleServices(workspace);
export const localBaseUrl = serviceWorkspaceBaseUrl(workspace.services[0]);

export const moduleRelease = defineModuleRelease({
  name: supportTicket.name,
  version: supportTicket.version ?? "0.1.0",
  provider: { name: contract.name },
  capabilities: supportTicket.capabilities,
});

export const releasePlan = defineServiceReleasePlan({
  service: { name: contract.name },
  current: {
    name: contract.name,
    version: "0.1.0",
    manifestReference: "https://example.com/support/v1/lenso.service.json",
    modules: [supportTicket.name],
  },
  candidate: {
    name: contract.name,
    version: contract.version,
    manifestReference: "https://example.com/support/v2/lenso.service.json",
    packageReference:
      "https://example.com/support/v2/lenso.service-package.json",
    modules: [supportTicket.name],
  },
  diff: {
    capabilities: [],
    config: { added: [], removed: [] },
    env: { added: ["SUPPORT_API_KEY"], removed: [] },
    modules: { added: [], removed: [] },
    operations: [],
  },
  restartRequired: serviceReleaseRestartRequired({
    capabilities: [],
    config: { added: [], removed: [] },
    env: { added: ["SUPPORT_API_KEY"], removed: [] },
    modules: { added: [], removed: [] },
    operations: [],
  }),
});

serveService(manifest, { modules: {} });
```

```js
import {
  defineModule,
  defineService,
  getRoute,
  runtimeFunction,
  serveService,
} from "@lenso/service-kit";

const supportTicket = defineModule({
  capabilities: ["support_ticket.tickets.read"],
  httpRoutes: [
    getRoute("/tickets/{id}", {
      capability: "support_ticket.tickets.read",
      displayName: "Get Ticket",
      storyTitle: "Get Ticket",
    }),
  ],
  name: "support-ticket",
  runtimeFunctions: [runtimeFunction("support-ticket.escalate-ticket.v1")],
});

const service = defineService({
  install: {
    services: [
      {
        command: "pnpm start",
        name: "support-service",
        readyUrl: "http://127.0.0.1:4110/lenso/service/v1/status",
      },
    ],
  },
  modules: [supportTicket],
  name: "support-service",
  requiredEnv: ["PORT"],
});

const server = await serveService(service, {
  modules: {
    "support-ticket": {
      http: {
        "GET /tickets/{id}": ({ params }) => ({ ticket: { id: params.id } }),
      },
    },
  },
});

console.log(server.manifestUrl);
console.log(server.statusUrl);
```

`serveService()` serves:

- `GET /lenso/service/v1/manifest`
- `GET /lenso/service/v1/status`
- module handlers below `/lenso/service/v1/modules/{moduleName}`

When `providerV1.moduleReleases` supplies the exact release for an export,
`serveService()` also serves it from
`GET /lenso/provider/v1/exports/{exportKey}/module-release`. The public Provider
descriptor continues to contain only the locked release digest. Provider V1
HTTP handlers receive the Host-authenticated `actor` in their handler context;
authorization remains a Host boundary and the Service does not parse client
credentials itself.

## Durable Provider invocations

Provider V1 uses a process-local invocation Store by default so existing local
Services keep working, but the descriptor does not advertise
`durable_invocations` for that Store. A Service that can perform side effects
must inject a `ProviderInvocationStore` backed by durable storage:

```ts
import type { ProviderInvocationStore } from "@lenso/service-kit";

const invocationStore: ProviderInvocationStore = {
  durability: "durable",
  async claim(input) {
    // Atomically insert invocationId + requestDigest + pendingOutcome, or load
    // the existing row and return replay/conflict after comparing the digest.
    throw new Error(`Implement durable claim for ${input.invocationId}`);
  },
  async get(invocationId) {
    // Return the durable row used by timeout recovery.
    throw new Error(`Implement durable recovery for ${invocationId}`);
  },
  async complete(input) {
    // Compare requestDigest and atomically persist the immutable outcome.
    throw new Error(`Implement durable completion for ${input.invocationId}`);
  },
  async acknowledge(input) {
    // Compare outcomeDigest and persist an idempotent acknowledgement.
    throw new Error(`Implement durable acknowledgement for ${input.invocationId}`);
  },
};

await serveService(service, {
  providerV1: {
    // ...the exact locked Provider release and exports
    invocationStore,
  },
});
```

A production implementation should put these fields in one invocation table:
the invocation id as a unique key, canonical request digest, execution phase,
complete outcome JSON and digest, created/updated timestamps, and nullable
acknowledgement timestamp/digest. `claim`, `complete`, and `acknowledge` must be
atomic across every Service instance. An identical claim replays its stored
outcome; the same id with a different request digest returns `conflict` without
executing the handler. `complete` must reject request-digest mismatches and must
not replace one final outcome with another. When a new pending or final outcome
replaces an acknowledged executing/pending outcome, `complete` must clear that
stale acknowledgement. `acknowledge` must compare the exact outcome digest and
be idempotent. Do not delete acknowledged rows until the Service's documented
recovery and audit retention period has elapsed.
Declaring `durability: "durable"` is an adapter assertion; the Service Kit
cannot infer storage guarantees from an implementation. Provider packages
should run the exported mutating conformance vector against an isolated Store:

```ts
import { verifyProviderInvocationStoreConformance } from "@lenso/service-kit";

await verifyProviderInvocationStoreConformance({
  createStore: () => createPostgresInvocationStore(testPool),
});
```

Each `createStore` call must return a fresh adapter connected to the same test
backend. The vector leaves a small group of namespaced rows and proves
cross-adapter claim and completion races, conflict non-mutation, pending-to-final
transitions, failed/rejected retry metadata and persisted receipt round trips,
idempotent acknowledgement, and recovery through fresh adapters.

Handlers remain backward-compatible: returning an ordinary value produces a
successful outcome with no effects. Explicit helpers expose the complete
Provider V1 result vocabulary:

```ts
import {
  providerFailed,
  providerPending,
  providerRejected,
  providerSucceeded,
} from "@lenso/service-kit";

return providerSucceeded(result, {
  hostEffects: { events: [deliveredEvent] },
});

return providerFailed({
  code: "upstream_unavailable",
  message: "The upstream provider is unavailable",
  retryable: true,
  retryAfterMs: 1_000,
  providerTraceReference: remoteRequestId,
});
```

Use `providerSucceeded` for a known business observation, including an
upstream-declared transient or permanent business result. `providerFailed`
means the Provider operation itself failed before it could establish that
business observation, so it may enter the Host's technical retry rail. One
stable `functionRunId` is one owning-Module business attempt; outer Provider
invocation ids vary by Host technical attempt and must not multiply the
owning Module's retry policy.

`pending`, `rejected`, and `failed` outcomes cannot include Host effects;
`rejected` is always permanent. Effect records and successful Host Event or
Runtime Function effects are JSON-validated and bounded before the outcome is
made durable. Recovery uses
`GET /lenso/provider/v1/invocations/{invocationId}`; acknowledgement uses
`POST /lenso/provider/v1/invocations/{invocationId}:ack` with the exact outcome
digest.

When `serveService()` binds Provider V1 outside loopback, configure
`providerV1.bearerToken`. Descriptor, health, invocation, recovery, and
acknowledgement routes then require that bearer, and non-loopback startup fails
closed without it. Runtime Host effects must preserve the exact invocation
actor, tenant, and trace context so a Service cannot mint Host authority.

## Local Provider Core identity

For a local Console enrollment check, `serveService()` can expose the Provider's
identity from the same loopback origin at the fixed
`GET /system-plane/v1` path:

```ts
const bearerToken = process.env.LENSO_LOCAL_ENROLLMENT_TOKEN;
if (!bearerToken) {
  throw new Error("LENSO_LOCAL_ENROLLMENT_TOKEN is required");
}

const served = await serveService(service, {
  providerCore: {
    bearerToken,
    serviceId: "support-service",
    servicePrincipal: "service:support-service",
    serviceRevision: "release:sha256:0123456789abcdef",
  },
});

console.log(served.systemPlaneCoreUrl);
```

The option is disabled by default and accepts only a loopback `host`. When it is
enabled, the route requires the exact `Authorization: Bearer ...` credential;
missing or incorrect credentials receive the same fixed `401` response. The
token is never included in the Core document, manifest, error body, or returned
server handle.

The response is the strict camelCase `lenso.system-plane.v1` Core identity with
`serviceId`, `servicePrincipal`, and `serviceRevision`. It advertises no
capabilities and does not add Autonomous Service runtime behavior to the
TypeScript Provider tier.

Install it into a host with:

```sh
lenso service install http://127.0.0.1:4110/lenso/service/v1/manifest
```

Package a running service manifest for handoff with:

```sh
lenso service package --manifest http://127.0.0.1:4110/lenso/service/v1/manifest
```

The package command writes `lenso.service-package.json` plus one
`modules/<module>/lenso.module-release.json` artifact per provided module.

## Scripts

- `pnpm build`: emit JavaScript and declarations into `dist/`.
- `pnpm pack --dry-run`: build and inspect the publish tarball without uploading.

## Publishing

This package is released independently with Changesets. Add a changeset from
the repository root with `pnpm changeset`; the Changesets workflow opens a
version pull request and publishes the merged version through npm Trusted
Publishing. Local checks are available with `pnpm --dir sdk/typescript check`.
