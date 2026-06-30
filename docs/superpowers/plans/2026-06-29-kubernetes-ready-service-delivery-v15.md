# Lenso Kubernetes-Ready Service Delivery V15 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Kubernetes a first-class production target for Lenso services while keeping Lenso's own model centered on services, modules, releases, host-owned runtime, Runtime Story, Remote Calls, and operator visibility.

**Architecture:** Lenso owns service/module/release semantics. Kubernetes owns production process scheduling, rollout state, networking, probes, and cluster-level availability. The CLI generates and reads Kubernetes deployment state; the host and Console read Lenso state files and never require Kubernetes credentials. Service providers stay out-of-process, and modules remain the installable business surface exposed by those providers.

**Tech Stack:** Rust 2024, clap, serde/serde_json, reqwest, std::process for kubectl integration, TypeScript, React, Vitest, TanStack Query/Router, pnpm, Kubernetes Deployment/Service/ConfigMap/Secret/Ingress/HPA/PDB/NetworkPolicy, Kustomize-compatible YAML.

---

## Product Decision

V15 should directly embrace Kubernetes as the production deployment target, but Lenso must not become a Kubernetes framework.

Lenso should provide:

- service environment records;
- Kubernetes manifest export;
- optional CLI-driven kubectl status reads;
- deployment observations persisted into `.lenso/service-deployments.json`;
- release plan/apply/promotion/rollback awareness per environment;
- Console command-center visibility for release and deployment drift;
- support-ticket Kubernetes proof.

Lenso should not provide in V15:

- CRDs or an Operator;
- service mesh policy;
- API gateway ownership;
- distributed transactions;
- schema registry;
- cloud account provisioning;
- automatic multi-cluster orchestration.

The user-facing framing is:

> Build modular first. Run services independently. Deploy mature services on Kubernetes when the boundary hardens.

## New User Journey

The main V15 path should work like this:

```sh
lenso service env add staging \
  --service support-suite-provider \
  --target kubernetes \
  --namespace lenso-staging \
  --image ghcr.io/example/support-suite-provider:0.4.0 \
  --public-base-url https://support-staging.example.com

lenso service release plan support-suite-provider \
  ./dist/lenso-service/support-suite-provider/lenso.service-package.json \
  --env staging \
  --output .lenso/support-suite-provider.staging.release-plan.json

lenso service deploy export support-suite-provider \
  --env staging \
  --target kubernetes \
  --output-dir dist/lenso-service/support-suite-provider/kubernetes/staging

lenso service deploy status support-suite-provider \
  --env staging \
  --write-state

lenso service release apply .lenso/support-suite-provider.staging.release-plan.json \
  --env staging
```

Console then shows:

- environment: `staging`;
- target: `kubernetes`;
- desired image;
- Kubernetes namespace;
- latest host release id;
- latest cluster-observed release id;
- drift state;
- rollout readiness;
- next action.

## Data Contracts

### Service Environments

File: `/Users/leosouthey/Projects/framework/lenso/.lenso/service-environments.json`

Shape:

```json
{
  "version": 1,
  "environments": [
    {
      "name": "staging",
      "serviceName": "support-suite-provider",
      "target": "kubernetes",
      "namespace": "lenso-staging",
      "kubeContext": "staging",
      "image": "ghcr.io/example/support-suite-provider:0.4.0",
      "publicBaseUrl": "https://support-staging.example.com",
      "manifestReference": "https://support-staging.example.com/lenso/service/v1/manifest",
      "releaseTrack": "staging",
      "config": {
        "replicas": 2,
        "port": 4110,
        "ingressHost": "support-staging.example.com"
      }
    }
  ]
}
```

Rules:

- `(serviceName, name)` is unique.
- `target` is `kubernetes` in V15, but keep the field open for later targets.
- `manifestReference` is optional; when missing, derive it from `publicBaseUrl`.
- `config` is a typed JSON object for target-specific settings.
- Secrets are never written into this file.

### Service Deployment Observations

File: `/Users/leosouthey/Projects/framework/lenso/.lenso/service-deployments.json`

Shape:

```json
{
  "version": 1,
  "observations": [
    {
      "serviceName": "support-suite-provider",
      "environment": "staging",
      "target": "kubernetes",
      "observedAtUnixMs": 1803744000000,
      "state": "ready",
      "drift": "in_sync",
      "cluster": {
        "namespace": "lenso-staging",
        "deployment": "support-suite-provider",
        "readyReplicas": 2,
        "desiredReplicas": 2,
        "availableReplicas": 2,
        "image": "ghcr.io/example/support-suite-provider:0.4.0",
        "releaseId": "019c9f00-0000-7000-8000-000000000000",
        "manifestReference": "https://support-staging.example.com/lenso/service/v1/manifest",
        "serviceEndpoint": "support-suite-provider.lenso-staging.svc.cluster.local",
        "ingressHost": "support-staging.example.com"
      },
      "host": {
        "releaseId": "019c9f00-0000-7000-8000-000000000000",
        "candidateVersion": "0.4.0"
      },
      "checks": [
        {
          "name": "deployment_rollout",
          "status": "ok",
          "detail": "2/2 replicas ready"
        }
      ],
      "nextAction": "monitor rollout and Remote Calls"
    }
  ]
}
```

States:

- `ready`;
- `progressing`;
- `failed`;
- `unknown`.

Drift:

- `in_sync`: host release id and cluster release id match;
- `host_ahead`: host applied a release that has not been observed in the cluster;
- `cluster_ahead`: cluster has a release annotation newer than host state;
- `image_drift`: expected image and observed image differ;
- `unknown`: not enough evidence.

### Kubernetes Labels And Annotations

Use labels for stable selectors:

```yaml
labels:
  app.kubernetes.io/name: support-suite-provider
  app.kubernetes.io/part-of: lenso
  app.kubernetes.io/component: service-provider
  lenso.dev/service-provider: support-suite-provider
  lenso.dev/environment: staging
```

Use annotations for richer values:

```yaml
annotations:
  lenso.dev/modules: support-ticket
  lenso.dev/release-id: 019c9f00-0000-7000-8000-000000000000
  lenso.dev/manifest-reference: https://support-staging.example.com/lenso/service/v1/manifest
  lenso.dev/service-package-reference: ./dist/lenso-service/support-suite-provider/lenso.service-package.json
```

## Implementation Tracks

### Track 1: Shared Service Delivery Types

Repository: `/Users/leosouthey/Projects/framework/lenso`

Files:

- `/Users/leosouthey/Projects/framework/lenso/crates/lenso-service/src/lib.rs`

Steps:

- [ ] Add `ServiceDeploymentTarget` with `Kubernetes`.
- [ ] Add `ServiceEnvironment` and `ServiceEnvironmentsFile`.
- [ ] Add `ServiceDeploymentState`, `ServiceDeploymentDrift`, `ServiceDeploymentObservation`, and `ServiceDeploymentsFile`.
- [ ] Add `KubernetesDeploymentConfig` for replicas, port, ingress host, resource hints, autoscaling, disruption budget, and network policy toggles.
- [ ] Add `KubernetesDeploymentObservation` for namespace, deployment, image, ready replicas, desired replicas, service endpoint, ingress host, manifest reference, and release id.
- [ ] Keep all new structs serde-compatible with camelCase JSON.
- [ ] Add unit tests for serde round trips and drift enum spellings.

Important constraints:

- Do not add a Kubernetes client dependency to `lenso-service`.
- Do not store secret values in shared structs.
- Do not move module release semantics into service release structs.

Validation commands:

```sh
cd /Users/leosouthey/Projects/framework/lenso
cargo test -p lenso-service service_deployment
```

### Track 2: CLI Environment Registry

Repository: `/Users/leosouthey/Projects/framework/lenso-cli`

Files:

- `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs`
- `/Users/leosouthey/Projects/framework/lenso-cli/src/module.rs`
- Optional pure helpers: `/Users/leosouthey/Projects/framework/lenso-cli/src/service_delivery.rs`

CLI surface:

```sh
lenso service env list [--service <provider>] [--json]
lenso service env add <env> --service <provider> --target kubernetes --namespace <ns> --image <image> [--kube-context <ctx>] [--public-base-url <url>] [--manifest-reference <url>] [--replicas <n>] [--port <n>]
lenso service env remove <env> --service <provider> [--dry-run]
lenso service env verify <env> --service <provider> [--json]
```

Steps:

- [ ] Add `ServiceEnvCommand` in `src/main.rs`.
- [ ] Add options structs for list/add/remove/verify.
- [ ] Add path constant `.lenso/service-environments.json`.
- [ ] Implement load/write/upsert/remove helpers with stable sorting by `serviceName` then `name`.
- [ ] `env add` should create `.lenso/` when missing.
- [ ] `env verify` should check service install receipt, environment uniqueness, target fields, derived manifest URL, and missing image/namespace for Kubernetes.
- [ ] Human output should use operator wording: "environment configured", "image missing", "namespace missing", "manifest reference derived".
- [ ] JSON output should print the normalized environment plus checks.

Tests:

- [ ] Parse tests for each command in `src/main.rs`.
- [ ] Unit tests for upsert/remove stability.
- [ ] Verify scenarios: missing file, ready env, missing image, missing namespace.

Validation commands:

```sh
cd /Users/leosouthey/Projects/framework/lenso-cli
cargo test service_env
```

### Track 3: Kubernetes Manifest Export

Repository: `/Users/leosouthey/Projects/framework/lenso-cli`

Files:

- `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs`
- `/Users/leosouthey/Projects/framework/lenso-cli/src/module.rs`
- Optional pure helpers: `/Users/leosouthey/Projects/framework/lenso-cli/src/kubernetes_export.rs`

CLI surface:

```sh
lenso service deploy export <provider> --env staging --target kubernetes --output-dir dist/lenso-service/<provider>/kubernetes/staging
lenso service deploy export <provider> --env prod --image ghcr.io/acme/support-suite-provider:0.4.0 --namespace lenso-prod --ingress-host support.example.com
```

Generated files:

```text
deployment.yaml
service.yaml
configmap.yaml
secret.example.yaml
ingress.yaml
hpa.yaml
pdb.yaml
networkpolicy.yaml
kustomization.yaml
README.md
```

Steps:

- [ ] Add `ServiceDeployCommand::Export`.
- [ ] Resolve provider from install receipt or service environment.
- [ ] Resolve modules from service manifest snapshot.
- [ ] Resolve image/namespace/port/ingress/replicas from CLI args first, then environment file, then package defaults.
- [ ] Generate Kustomize-compatible YAML.
- [ ] Include readiness and liveness probes. Prefer service manifest health/status endpoint when present; otherwise use `/lenso/service/v1/status`.
- [ ] Put required non-secret env names in `configmap.yaml` with empty or documented values.
- [ ] Put required secret env names in `secret.example.yaml` with placeholder values.
- [ ] Add optional `hpa.yaml` when autoscaling config exists or `--hpa` is passed.
- [ ] Add optional `pdb.yaml` when replicas are greater than one or `--pdb` is passed.
- [ ] Add optional `networkpolicy.yaml` when `--network-policy` is passed or env config enables it.
- [ ] Print the next commands:

```sh
kubectl apply -k <output-dir>
lenso service deploy status <provider> --env <env> --write-state
```

Generation rules:

- Keep generated files deterministic.
- Do not shell out to kubectl in `export`.
- Do not include actual secret values.
- Use annotations for release id only when a release plan or latest release is available.
- If no release id exists, write `lenso.dev/release-id: pending`.

Tests:

- [ ] Unit test generated Deployment labels, annotations, probes, image, port, and env names.
- [ ] Snapshot-like test for support-ticket Kubernetes export using string comparisons, not broad golden-file churn.
- [ ] Command parse tests for export args.

Validation commands:

```sh
cd /Users/leosouthey/Projects/framework/lenso-cli
cargo test kubernetes_export
```

### Track 4: Kubernetes Status Reader And Deployment State Cache

Repository: `/Users/leosouthey/Projects/framework/lenso-cli`

Files:

- `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs`
- `/Users/leosouthey/Projects/framework/lenso-cli/src/module.rs`
- Optional pure helpers: `/Users/leosouthey/Projects/framework/lenso-cli/src/kubernetes_status.rs`

CLI surface:

```sh
lenso service deploy status <provider> --env staging
lenso service deploy status <provider> --env staging --write-state
lenso service deploy status <provider> --env staging --from-file fixtures/k8s-status-ready.json --write-state
```

Steps:

- [ ] Add `ServiceDeployCommand::Status`.
- [ ] Resolve Kubernetes context, namespace, deployment name, service name, and ingress name from env/config.
- [ ] Use `kubectl` through `std::process::Command` only in the CLI.
- [ ] Read:
  - `kubectl get deployment <name> -o json`;
  - `kubectl get service <name> -o json`;
  - `kubectl get ingress <name> -o json` when ingress is configured.
- [ ] Add `--from-file` for tests and offline demos.
- [ ] Parse JSON with serde_json and compute state.
- [ ] Compute drift against `.lenso/service-releases.json` latest release for the service and the observed `lenso.dev/release-id` annotation.
- [ ] With `--write-state`, upsert `.lenso/service-deployments.json`.
- [ ] Human output should say what is wrong:
  - deployment missing;
  - rollout progressing;
  - pods not ready;
  - image drift;
  - host ahead;
  - cluster ahead;
  - ingress missing.

State rules:

- Ready when desired replicas are greater than zero and ready replicas equal desired replicas.
- Progressing when observed generation or ready replicas are behind.
- Failed when deployment conditions include unavailable/progress deadline.
- Unknown when kubectl cannot provide enough evidence.

Tests:

- [ ] Parse ready deployment fixture.
- [ ] Parse progressing deployment fixture.
- [ ] Parse failed deployment fixture.
- [ ] Drift computation: in sync, host ahead, cluster ahead, image drift, unknown.
- [ ] Write-state upsert keeps only the newest observation for `(serviceName, environment)`.

Validation commands:

```sh
cd /Users/leosouthey/Projects/framework/lenso-cli
cargo test kubernetes_status
```

### Track 5: Release Plan Environment Awareness

Repository: `/Users/leosouthey/Projects/framework/lenso-cli`

Files:

- `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs`
- `/Users/leosouthey/Projects/framework/lenso-cli/src/module.rs`

CLI changes:

```sh
lenso service release plan <provider> <manifest-or-package> --env staging --output .lenso/<provider>.staging.release-plan.json
lenso service release check .lenso/<provider>.staging.release-plan.json
lenso service release apply .lenso/<provider>.staging.release-plan.json --env staging
```

Plan additions:

```json
{
  "environment": {
    "name": "staging",
    "target": "kubernetes",
    "namespace": "lenso-staging",
    "image": "ghcr.io/example/support-suite-provider:0.4.0",
    "manifestReference": "https://support-staging.example.com/lenso/service/v1/manifest"
  }
}
```

Steps:

- [ ] Add `--env` to `ServiceReleasePlanArgs`, `ServiceReleaseCheckArgs`, and `ServiceReleaseApplyArgs`.
- [ ] When `--env` is passed to `plan`, load the service environment and embed normalized environment metadata.
- [ ] `check` should validate environment metadata if present, but must still accept V14 plans without it.
- [ ] `apply --env` should ensure the plan environment matches the requested environment.
- [ ] Release ledger entries should record `environment`, `target`, and expected image when available.
- [ ] `print_service_release_plan` should show `environment: staging (kubernetes/lenso-staging)` when present.

Tests:

- [ ] V14 plans remain valid.
- [ ] V15 env-aware plans validate.
- [ ] Applying a staging plan with `--env prod` fails clearly.
- [ ] Ledger stores environment fields.

Validation commands:

```sh
cd /Users/leosouthey/Projects/framework/lenso-cli
cargo test service_release
```

### Track 6: Promotion And Rollback By Environment

Repository: `/Users/leosouthey/Projects/framework/lenso-cli`

Files:

- `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs`
- `/Users/leosouthey/Projects/framework/lenso-cli/src/module.rs`

CLI surface:

```sh
lenso service release promote support-suite-provider --from staging --to prod --output .lenso/support-suite-provider.prod.release-plan.json
lenso service release rollback support-suite-provider --env prod
lenso service release rollback support-suite-provider --env prod --to <release-id>
```

Steps:

- [ ] Add `ServiceReleaseCommand::Promote`.
- [ ] Add `ServiceReleaseCommand::Rollback`.
- [ ] `promote` should find the latest applied release in the source environment and create a target-environment plan that points at the same candidate package/manifest.
- [ ] `promote` should not apply automatically.
- [ ] `rollback --env` should create and optionally apply a plan that targets the previous release for the same environment.
- [ ] `rollback --to <release-id>` should validate that the release exists for the same service.
- [ ] If rollback uses a local package path that no longer exists, fail with a fix message: rebuild or provide the manifest/package reference explicitly.

Tests:

- [ ] Promote staging to prod with existing source release.
- [ ] Promote fails when source has no release.
- [ ] Rollback picks previous same-environment release.
- [ ] Rollback refuses cross-service release id.
- [ ] Rollback fails cleanly when package reference is unavailable.

Validation commands:

```sh
cd /Users/leosouthey/Projects/framework/lenso-cli
cargo test service_release_promote
cargo test service_release_rollback
```

### Track 7: Host Admin Data Surface

Repository: `/Users/leosouthey/Projects/framework/lenso`

Files:

- `/Users/leosouthey/Projects/framework/lenso/crates/platform-admin-data/src/dto.rs`
- `/Users/leosouthey/Projects/framework/lenso/crates/platform-admin-data/src/handlers.rs`
- `/Users/leosouthey/Projects/framework/lenso/crates/lenso-api/tests/admin_data_console.rs`

Steps:

- [ ] Add DTOs for service environments and deployment observations.
- [ ] Extend `AdminServiceModuleLifecycleModuleDto` with:
  - `environments`;
  - `deployments`;
  - `deploymentDrift`;
  - `deploymentNextAction`.
- [ ] Read `.lenso/service-environments.json` and `.lenso/service-deployments.json` beside the existing release ledger read.
- [ ] Group environments and observations by `serviceName`.
- [ ] Attach all matching environments to every module provided by the service.
- [ ] Attach only the newest observation per environment.
- [ ] Never shell out to kubectl from host code.
- [ ] When deployment state exists but release history does not, surface drift as `unknown` and next action as "run service release apply or refresh deployment status".

Tests:

- [ ] Existing admin data console fixture still exposes release history.
- [ ] New fixture exposes environment and deployment observation.
- [ ] Missing deployment file returns empty arrays, not errors.
- [ ] Malformed deployment file degrades to fixes/needs_attention without panicking.

Validation commands:

```sh
cd /Users/leosouthey/Projects/framework/lenso
HTTP_HOST=127.0.0.1 cargo test -p lenso-api --test admin_data_console service_deployment
```

### Track 8: Runtime Console Command Center

Repository: `/Users/leosouthey/Projects/framework/lenso-runtime-console`

Files:

- `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/available-modules-model.ts`
- `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/services-model.ts`
- `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/services-model.test.ts`
- `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/services-page.tsx`
- `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/data/available-modules.ts`

UI requirements:

- Add environment selector to the service detail panel.
- Show deployment target and namespace.
- Show desired image and observed image.
- Show rollout state.
- Show drift badge.
- Show latest host release id and cluster release id.
- Show release promotion path.
- Show rollback target.
- Show copyable commands:
  - `lenso service deploy export ...`;
  - `kubectl apply -k ...`;
  - `lenso service deploy status ... --write-state`;
  - `lenso service release promote ...`;
  - `lenso service release rollback ...`.
- Keep links to Remote Calls, Runtime Story, Technical Operations, and Runtime Functions.

Design constraints:

- This is an operations console, not a marketing page.
- Keep the existing dense service-detail layout.
- Do not add nested cards.
- Do not add cluster mutation buttons in V15.
- Commands can be displayed and copied; applying stays in the CLI.

Steps:

- [ ] Extend TypeScript DTOs with environment/deployment/drift fields.
- [ ] Add model functions:
  - `serviceEnvironmentRows`;
  - `selectedServiceEnvironment`;
  - `serviceDeploymentSummary`;
  - `deploymentNextAction`.
- [ ] Add tests for drift labels and selected environment behavior.
- [ ] Update service detail panel sections:
  - `deployment environments`;
  - `kubernetes rollout`;
  - `release drift`;
  - `operator commands`.
- [ ] Update sample data with support-ticket staging/prod examples.

Validation commands:

```sh
cd /Users/leosouthey/Projects/framework/lenso-runtime-console
pnpm check
```

### Track 9: SDK And Authoring Helpers

Repositories:

- `/Users/leosouthey/Projects/framework/lenso`
- `/Users/leosouthey/Projects/framework/lenso-runtime-console`

Files:

- `/Users/leosouthey/Projects/framework/lenso/crates/lenso-service/src/lib.rs`
- `/Users/leosouthey/Projects/framework/lenso-runtime-console/packages/service-kit/src/index.ts`
- `/Users/leosouthey/Projects/framework/lenso-runtime-console/packages/service-kit/src/index.test.ts`
- `/Users/leosouthey/Projects/framework/lenso-runtime-console/packages/service-kit/README.md`

TypeScript helpers:

```ts
defineKubernetesDeployment({
  port: 4110,
  replicas: 2,
  ingressHost: "support-staging.example.com",
  env: ["SUPPORT_TICKET_QUEUE"],
  secrets: ["SUPPORT_TICKET_TOKEN"],
});
```

Rust helpers:

```rust
ServiceDeployment::kubernetes()
    .port(4110)
    .replicas(2)
    .ingress_host("support-staging.example.com");
```

Steps:

- [ ] Add TS types for Kubernetes deployment hints.
- [ ] Add `defineKubernetesDeployment` helper.
- [ ] Add Rust structs/builders only if they fit the current `lenso-service` style; otherwise add plain structs first.
- [ ] Ensure service manifests can carry deployment hints without making them required.
- [ ] Ensure existing service contracts continue serializing the same when no deployment hints are passed.
- [ ] Document that SDK helpers do not deploy anything; they only make CLI export better.

Validation commands:

```sh
cd /Users/leosouthey/Projects/framework/lenso-runtime-console
pnpm --filter @lenso/service-kit test

cd /Users/leosouthey/Projects/framework/lenso
cargo test -p lenso-service
```

### Track 10: Support-Ticket Kubernetes Proof

Repository: `/Users/leosouthey/Projects/framework/lenso-examples`

Files:

- `/Users/leosouthey/Projects/framework/lenso-examples/examples/support-ticket/src/module.ts`
- `/Users/leosouthey/Projects/framework/lenso-examples/examples/support-ticket/README.md`
- `/Users/leosouthey/Projects/framework/lenso-examples/docs/support-ticket-service-module-run.md`
- `/Users/leosouthey/Projects/framework/lenso-examples/package.json`
- Optional generated proof directory: `/Users/leosouthey/Projects/framework/lenso-examples/examples/support-ticket/kubernetes/staging`

Steps:

- [ ] Add Kubernetes deployment hints to the support-ticket service definition.
- [ ] Add npm script:

```json
{
  "service-deploy-export:support-ticket": "lenso service deploy export support-suite-provider --env staging --target kubernetes --output-dir examples/support-ticket/kubernetes/staging"
}
```

- [ ] Add a documented sample `.lenso/service-environments.json` snippet.
- [ ] Add a checked-in Kubernetes export example only if the output is stable and small.
- [ ] Add an offline status fixture for `lenso service deploy status --from-file`.
- [ ] Update docs to show:
  - package service;
  - add env;
  - plan release with env;
  - export K8s files;
  - apply with kubectl;
  - write deployment state;
  - inspect Console.

Validation commands:

```sh
cd /Users/leosouthey/Projects/framework/lenso-examples
pnpm --filter @lenso/example-support-ticket smoke
pnpm service-package:support-ticket
```

Note: do not run repeated host/API smoke checks during every subtask. Use focused unit checks while building V15, then run one end-to-end support-ticket proof only after the CLI, Host DTO, and Console surfaces are ready.

### Track 11: Public Docs And CLI Reference

Repository: `/Users/leosouthey/Projects/framework/lenso-site`

Files:

- `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(host)/deployment.mdx`
- `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(host)/cli-reference.mdx`
- `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(module)/examples.mdx`
- Optional new page: `/Users/leosouthey/Projects/framework/lenso-site/content/docs/(host)/kubernetes-service-delivery.mdx`

Steps:

- [ ] Reframe Kubernetes from "use only if you already operate it" to "first-class production target for mature service boundaries".
- [ ] Keep the warning that Lenso is not a service mesh or cluster orchestrator.
- [ ] Document environment files, deployment export, status cache, promotion, rollback, and Console command center.
- [ ] Add CLI reference rows for:
  - `lenso service env list/add/remove/verify`;
  - `lenso service deploy export/status`;
  - `lenso service release plan --env`;
  - `lenso service release promote`;
  - `lenso service release rollback --env`.
- [ ] Update examples page with support-ticket Kubernetes path.

Validation commands:

```sh
cd /Users/leosouthey/Projects/framework/lenso-site
pnpm lint
```

### Track 12: Cross-Repo Integration And Branching

Branches:

- `/Users/leosouthey/Projects/framework/lenso`: `feat/kubernetes-ready-delivery-v15`
- `/Users/leosouthey/Projects/framework/lenso-cli`: `feat/kubernetes-ready-delivery-v15`
- `/Users/leosouthey/Projects/framework/lenso-runtime-console`: `feat/kubernetes-ready-delivery-v15`
- `/Users/leosouthey/Projects/framework/lenso-examples`: `feat/kubernetes-ready-delivery-v15`
- `/Users/leosouthey/Projects/framework/lenso-site`: `feat/kubernetes-ready-delivery-v15`

Steps:

- [ ] Create or switch every repo to the V15 branch.
- [ ] Keep commits repo-scoped.
- [ ] Commit shared Rust service types before CLI code that depends on them.
- [ ] Commit Console model/UI after Host DTO tests pass.
- [ ] Commit examples after CLI export is stable.
- [ ] Commit site docs last, after command names settle.

Suggested commit order:

```sh
# lenso
git add crates/lenso-service crates/platform-admin-data crates/lenso-api/tests docs/superpowers/plans
git commit -m "feat: add kubernetes service delivery state"

# lenso-cli
git add src
git commit -m "feat: add kubernetes service deployment commands"

# lenso-runtime-console
git add src packages/service-kit
git commit -m "feat: show kubernetes service delivery state"

# lenso-examples
git add examples/support-ticket docs package.json
git commit -m "docs: add support ticket kubernetes delivery proof"

# lenso-site
git add content
git commit -m "docs: document kubernetes-ready service delivery"
```

## Acceptance Criteria

V15 is done when:

- `lenso service env add/list/verify/remove` works against `.lenso/service-environments.json`.
- `lenso service deploy export` generates Kubernetes manifests for support-ticket.
- `lenso service deploy status --from-file --write-state` writes `.lenso/service-deployments.json`.
- `lenso service deploy status` can read a real cluster through kubectl when kubeconfig is available.
- `lenso service release plan --env` embeds environment metadata.
- `lenso service release apply --env` writes env-aware release ledger entries.
- `lenso service release promote` produces a target-env release plan.
- `lenso service release rollback --env` can target the previous same-env release.
- Host admin data exposes environments, deployment observations, and drift.
- Console shows Kubernetes rollout, release drift, and operator commands.
- TS and Rust authoring helpers expose deployment hints without making deployment mandatory.
- support-ticket demonstrates the Kubernetes path.
- Site docs describe Kubernetes as first-class production target without promising Operator/service mesh behavior.

## Verification Plan

Use focused checks while implementing. Avoid running broad smoke checks after every small edit.

Core checks:

```sh
cd /Users/leosouthey/Projects/framework/lenso
cargo test -p lenso-service
HTTP_HOST=127.0.0.1 cargo test -p lenso-api --test admin_data_console

cd /Users/leosouthey/Projects/framework/lenso-cli
cargo test

cd /Users/leosouthey/Projects/framework/lenso-runtime-console
pnpm check

cd /Users/leosouthey/Projects/framework/lenso-examples
pnpm --filter @lenso/example-support-ticket smoke
pnpm service-package:support-ticket

cd /Users/leosouthey/Projects/framework/lenso-site
pnpm lint
```

One final proof, after all tracks are merged locally:

```sh
cd /Users/leosouthey/Projects/framework/lenso-examples
pnpm service-package:support-ticket

cd /Users/leosouthey/Projects/framework/lenso
lenso service env add staging \
  --service support-suite-provider \
  --target kubernetes \
  --namespace lenso-staging \
  --image ghcr.io/example/support-suite-provider:0.4.0 \
  --public-base-url https://support-staging.example.com

lenso service release plan support-suite-provider \
  ../lenso-examples/dist/lenso-service/support-suite-provider/lenso.service-package.json \
  --env staging \
  --output .lenso/support-suite-provider.staging.release-plan.json

lenso service deploy export support-suite-provider \
  --env staging \
  --target kubernetes \
  --output-dir ../lenso-examples/examples/support-ticket/kubernetes/staging

lenso service deploy status support-suite-provider \
  --env staging \
  --from-file ../lenso-examples/examples/support-ticket/fixtures/k8s-ready.json \
  --write-state
```

## Rollout Notes

- V15 should be implemented as a stacked train on top of V14 if V14 has not landed.
- Keep V14 commands compatible.
- Keep `lenso module install` for module artifacts. V15 does not remove module install.
- Do not migrate users from module install to service deploy. These are different layers:
  - module install controls business module availability in a Lenso host;
  - service deploy controls the remote provider process in Kubernetes.
- Console should present both layers clearly: installed modules, service releases, deployment environments, and cluster observations.

## Future V16 Candidates

Only after V15 has a working Kubernetes delivery path:

- CRD and Operator for `LensoServiceProvider`;
- Helm chart generation;
- multi-cluster environment sets;
- deployment history from cluster events;
- policy-as-code hooks;
- progressive delivery integration;
- service mesh adapters;
- signed service packages;
- protocol version negotiation.
