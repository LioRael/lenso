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
