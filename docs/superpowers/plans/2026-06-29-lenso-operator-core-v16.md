# Lenso Operator Core V16 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the first Kubernetes-native Lenso Operator core so a service provider can be declared as a `LensoServiceProvider` custom resource, reconciled inside Kubernetes, observed through CLI status, and shown in Runtime Console without giving the Host Kubernetes credentials.

**Architecture:** V16 builds on V15's service environment, release, deployment export, and deployment observation files. The new `lenso-operator` crate owns Kubernetes reconciliation for provider process resources only; the CLI exports CRDs and bridges CRD status back into `.lenso/service-deployments.json`; Host admin data and Console continue reading local Lenso state. Host-owned runtime, auth, queues, retries, Outbox, Runtime Story, Remote Calls, and Technical Operations stay outside the operator.

**Tech Stack:** Rust 2024, kube 4, k8s-openapi 0.28, schemars 1, serde/serde_json/serde_yaml, tokio, tracing, clap, std::process `kubectl` integration, TypeScript, React, Vitest, pnpm, Kubernetes CRD/Deployment/Service/Ingress/HPA/PDB/NetworkPolicy, Kustomize-compatible YAML.

---

## Product Decision

V16 should make Lenso Kubernetes-native without turning Lenso into a full Kubernetes platform.

Build:

- `LensoServiceProvider` CRD, group `lenso.dev`, version `v1alpha1`;
- Rust operator binary that reconciles provider process resources;
- CLI install bundle export for the operator itself;
- CLI `--target operator` provider CR export;
- CLI `--source operator` status read and local observation write;
- Host/Console visibility for operator-managed observations;
- support-ticket operator proof docs and fixtures;
- site docs explaining when to use raw Kubernetes export versus operator-managed delivery.

Do not build in V16:

- Helm chart engine;
- admission webhooks;
- cert-manager integration;
- cloud account provisioning;
- multi-cluster orchestration;
- service mesh adapters;
- gateway ownership;
- distributed transactions;
- CRDs for individual modules;
- automatic module install from Kubernetes;
- operator writes into Host runtime tables;
- Host reads kubeconfig.

User-facing framing:

> Services can now be Kubernetes-native: define one `LensoServiceProvider`, let the Lenso Operator reconcile the provider process, then keep Host and Console observability connected to releases, Runtime Story, Remote Calls, and Technical Operations.

## File Structure

### Repository: `/Users/leosouthey/Projects/framework/lenso`

- Modify `/Users/leosouthey/Projects/framework/lenso/Cargo.toml`
  - Add workspace member `crates/lenso-operator`.
  - Add workspace dependencies only when shared by more than one crate.
- Create `/Users/leosouthey/Projects/framework/lenso/crates/lenso-operator/Cargo.toml`
  - Own all Kubernetes dependencies here so Host/runtime crates stay clean.
- Create `/Users/leosouthey/Projects/framework/lenso/crates/lenso-operator/src/lib.rs`
  - Export CRD, resource builder, status, and reconcile modules.
- Create `/Users/leosouthey/Projects/framework/lenso/crates/lenso-operator/src/crd.rs`
  - Define `LensoServiceProviderSpec`, optional spec blocks, status, and condition types.
- Create `/Users/leosouthey/Projects/framework/lenso/crates/lenso-operator/src/resources.rs`
  - Build desired Deployment, Service, optional Ingress, HPA, PDB, and NetworkPolicy from one CR.
- Create `/Users/leosouthey/Projects/framework/lenso/crates/lenso-operator/src/reconcile.rs`
  - Reconcile one CR into owned Kubernetes resources and derive status.
- Create `/Users/leosouthey/Projects/framework/lenso/crates/lenso-operator/src/main.rs`
  - Run the controller with namespace configuration and structured logging.
- Create `/Users/leosouthey/Projects/framework/lenso/crates/lenso-operator/tests/crd_contract.rs`
  - Verify CRD JSON/YAML contract and status serialization.
- Create `/Users/leosouthey/Projects/framework/lenso/crates/lenso-operator/tests/resource_builders.rs`
  - Verify generated Kubernetes resource shapes, labels, owner references, probes, and annotations.
- Modify `/Users/leosouthey/Projects/framework/lenso/crates/platform-admin-data/src/dto.rs`
  - Add operator status and condition DTOs to deployment observations.
- Modify `/Users/leosouthey/Projects/framework/lenso/crates/platform-admin-data/src/handlers.rs`
  - Parse operator observations from `.lenso/service-deployments.json`.
- Modify `/Users/leosouthey/Projects/framework/lenso/crates/lenso-api/tests/admin_data_console.rs`
  - Add API coverage for operator-managed service observations.

### Repository: `/Users/leosouthey/Projects/framework/lenso-cli`

- Create `/Users/leosouthey/Projects/framework/lenso-cli/src/operator.rs`
  - Export operator install bundle files: CRD, RBAC, Deployment, kustomization, README.
- Modify `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs`
  - Add top-level `lenso operator export-crd`.
  - Add service deploy target `operator`.
  - Add service deploy status source `operator`.
- Modify `/Users/leosouthey/Projects/framework/lenso-cli/src/module.rs`
  - Reuse V15 service environment/release/deployment helpers for provider CR export and status parsing.
  - Add operator CR YAML generation and CRD status observation conversion.
- Modify `/Users/leosouthey/Projects/framework/lenso-cli/Cargo.toml`
  - Add `serde_yaml` only if it is not already available to the CLI package.
- Add or extend unit tests in `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs` and `/Users/leosouthey/Projects/framework/lenso-cli/src/module.rs`.

### Repository: `/Users/leosouthey/Projects/framework/lenso-runtime-console`

- Modify `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/available-modules-model.ts`
  - Extend deployment observation types with `operator`.
- Modify `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/services-model.ts`
  - Add operator-managed row helpers, condition summaries, and command generation.
- Modify `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/services-model.test.ts`
  - Cover operator-managed rows and CRD condition display.
- Modify `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/services-page.tsx`
  - Show operator managed badge, CRD state, observed generation, conditions, release/image drift, and operator commands.

### Repository: `/Users/leosouthey/Projects/framework/lenso-examples`

- Create `/Users/leosouthey/Projects/framework/lenso-examples/examples/support-ticket/kubernetes/operator/staging/lensoserviceprovider.yaml`
  - Committed support-ticket provider CR fixture.
- Create `/Users/leosouthey/Projects/framework/lenso-examples/examples/support-ticket/kubernetes/operator/staging/kustomization.yaml`
  - Minimal fixture kustomization.
- Modify `/Users/leosouthey/Projects/framework/lenso-examples/examples/support-ticket/README.md`
  - Add operator-managed delivery path.
- Modify `/Users/leosouthey/Projects/framework/lenso-examples/docs/support-ticket-service-module-run.md`
  - Add the operator sequence after the V15 raw Kubernetes path.
- Modify `/Users/leosouthey/Projects/framework/lenso-examples/package.json`
  - Add a repository-local check command that validates the fixture shape without a live cluster.

### Repository: `/Users/leosouthey/Projects/framework/lenso-site`

- Modify the current service/Kubernetes docs page under `/Users/leosouthey/Projects/framework/lenso-site`
  - Explain raw Kubernetes export versus operator-managed delivery.
- Add a V16 operator doc page if the existing docs IA has a service delivery section.
  - Use site-local routing/sidebar conventions; inspect first and keep the page in the current docs group.

## Data Contracts

### `LensoServiceProvider` CRD

Committed API target:

```yaml
apiVersion: lenso.dev/v1alpha1
kind: LensoServiceProvider
metadata:
  name: support-suite-provider
  namespace: lenso-staging
  labels:
    app.kubernetes.io/part-of: lenso
    app.kubernetes.io/component: service-provider
    lenso.dev/service-provider: support-suite-provider
    lenso.dev/environment: staging
spec:
  serviceName: support-suite-provider
  environment: staging
  image: ghcr.io/acme/support-suite-provider:0.4.0
  releaseId: rel_staging
  manifestReference: https://support-staging.example.com/lenso/service/v1/manifest
  modules:
    - support-ticket
  replicas: 2
  port: 4110
  envFrom:
    configMap: support-suite-provider-config
    secret: support-suite-provider-secrets
  ingress:
    host: support-staging.example.com
  autoscaling:
    enabled: true
    minReplicas: 2
    maxReplicas: 6
    targetCpuUtilization: 70
  disruptionBudget:
    enabled: true
    minAvailable: 1
  networkPolicy:
    enabled: true
status:
  state: ready
  observedGeneration: 3
  observedReleaseId: rel_staging
  observedImage: ghcr.io/acme/support-suite-provider:0.4.0
  readyReplicas: 2
  desiredReplicas: 2
  availableReplicas: 2
  manifestReference: https://support-staging.example.com/lenso/service/v1/manifest
  conditions:
    - type: Reconciled
      status: "True"
      reason: ResourcesApplied
      message: Deployment and Service are in sync.
      lastTransitionTime: "2026-06-29T00:00:00Z"
    - type: Ready
      status: "True"
      reason: DeploymentAvailable
      message: 2/2 replicas are ready.
      lastTransitionTime: "2026-06-29T00:00:00Z"
```

Rules:

- CR name uses the Kubernetes-safe provider name from the CLI.
- `spec.serviceName`, `spec.environment`, `spec.image`, and `spec.port` are required by operator validation.
- `spec.modules` is informational and supports Console/operator diagnostics.
- `spec.envFrom.configMap` and `spec.envFrom.secret` are references only; no secret values appear in generated files.
- `status.observedReleaseId` and `status.observedImage` are the cluster-side truth used for drift.
- The operator owns only resources with owner references to the CR.

### `.lenso/service-deployments.json` Operator Observation

The CLI writes operator observations in the same file V15 introduced:

```json
{
  "version": 1,
  "observations": [
    {
      "serviceName": "support-suite-provider",
      "environment": "staging",
      "target": "operator",
      "observedAtUnixMs": 1803744000000,
      "state": "ready",
      "drift": "in_sync",
      "operator": {
        "resource": "support-suite-provider",
        "namespace": "lenso-staging",
        "observedGeneration": 3,
        "conditions": [
          {
            "type": "Ready",
            "status": "True",
            "reason": "DeploymentAvailable",
            "message": "2/2 replicas are ready.",
            "lastTransitionTime": "2026-06-29T00:00:00Z"
          }
        ]
      },
      "cluster": {
        "namespace": "lenso-staging",
        "deployment": "support-suite-provider",
        "readyReplicas": 2,
        "desiredReplicas": 2,
        "availableReplicas": 2,
        "image": "ghcr.io/acme/support-suite-provider:0.4.0",
        "releaseId": "rel_staging",
        "manifestReference": "https://support-staging.example.com/lenso/service/v1/manifest"
      },
      "host": {
        "releaseId": "rel_staging",
        "candidateVersion": "0.4.0"
      },
      "checks": [
        {
          "name": "operator_reconcile",
          "status": "ok",
          "detail": "LensoServiceProvider/support-suite-provider is ready"
        }
      ],
      "nextAction": "monitor operator conditions, Remote Calls, and Runtime Story"
    }
  ]
}
```

Drift rules:

- `in_sync`: Host release id equals CRD observed release id and expected image equals observed image.
- `host_ahead`: Host release id exists and CRD observed release id is missing or different.
- `cluster_ahead`: CRD observed release id exists and Host release id is missing.
- `image_drift`: environment image differs from CRD observed image.
- `unknown`: required evidence is missing.

State rules:

- `ready`: CRD status state is `ready`.
- `progressing`: CRD status state is `progressing`.
- `failed`: CRD status state is `failed`, the CRD is missing, or status parsing fails.
- `unknown`: CRD status state is empty and no stronger failure evidence exists.

## Task 0: Branches And Baseline

**Files:**

- Inspect: all five repositories under `/Users/leosouthey/Projects/framework`

- [ ] **Step 1: Confirm clean V15 base branches**

Run:

```sh
for repo in lenso lenso-cli lenso-runtime-console lenso-examples lenso-site; do
  git -C /Users/leosouthey/Projects/framework/$repo status --short --branch
done
```

Expected:

```text
## feat/operator-core-v16
## feat/kubernetes-ready-delivery-v15
## feat/kubernetes-ready-delivery-v15
## feat/kubernetes-ready-delivery-v15
## feat/kubernetes-ready-delivery-v15
```

If a repo has unrelated local changes, inspect them with `git -C <repo> diff --stat` and leave them untouched unless they block the V16 files listed above.

- [ ] **Step 2: Create V16 branches for repos that are still on V15**

Run:

```sh
for repo in lenso-cli lenso-runtime-console lenso-examples lenso-site; do
  git -C /Users/leosouthey/Projects/framework/$repo switch feat/kubernetes-ready-delivery-v15
  git -C /Users/leosouthey/Projects/framework/$repo switch -c feat/operator-core-v16
done
```

Expected:

```text
Switched to branch 'feat/kubernetes-ready-delivery-v15'
Switched to a new branch 'feat/operator-core-v16'
```

- [ ] **Step 3: Record implementation anchor**

Run:

```sh
git -C /Users/leosouthey/Projects/framework/lenso log --oneline -3
```

Expected includes:

```text
1832c12 docs: design lenso operator core v16
```

Commit: none.

## Task 1: Operator Crate And CRD Contract

**Files:**

- Modify: `/Users/leosouthey/Projects/framework/lenso/Cargo.toml`
- Create: `/Users/leosouthey/Projects/framework/lenso/crates/lenso-operator/Cargo.toml`
- Create: `/Users/leosouthey/Projects/framework/lenso/crates/lenso-operator/src/lib.rs`
- Create: `/Users/leosouthey/Projects/framework/lenso/crates/lenso-operator/src/crd.rs`
- Create: `/Users/leosouthey/Projects/framework/lenso/crates/lenso-operator/tests/crd_contract.rs`

- [ ] **Step 1: Add workspace member**

Edit `/Users/leosouthey/Projects/framework/lenso/Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = [
    "fixtures/remote-module",
    "crates/lenso-contracts",
    "crates/lenso",
    "crates/lenso-api",
    "crates/lenso-migrate",
    "crates/lenso-worker",
    "crates/lenso-bootstrap",
    "crates/lenso-service",
    "crates/lenso-operator",
    "crates/platform-admin",
    "crates/platform-admin-data",
    "crates/platform-core",
    "crates/platform-module",
    "crates/platform-module-remote",
    "crates/platform-http",
    "crates/platform-runtime",
    "crates/platform-testing",
    "modules/story",
    "tools/generate-contracts",
    "tools/arch-check",
    "tools/otel-smoke",
]
```

- [ ] **Step 2: Create operator crate manifest**

Create `/Users/leosouthey/Projects/framework/lenso/crates/lenso-operator/Cargo.toml`:

```toml
[package]
name = "lenso-operator"
version = "0.1.0"
edition.workspace = true
license.workspace = true
publish.workspace = true
rust-version.workspace = true

[dependencies]
anyhow.workspace = true
chrono.workspace = true
futures = "0.3"
k8s-openapi = { version = "0.28", features = ["v1_36", "schemars"] }
kube = { version = "4", features = ["client", "derive", "runtime", "rustls-tls"] }
schemars = "1"
serde.workspace = true
serde_json.workspace = true
serde_yaml.workspace = true
thiserror.workspace = true
tokio.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true

[dev-dependencies]
insta = { version = "1", features = ["yaml"] }

[lints]
workspace = true
```

- [ ] **Step 3: Create library entrypoint**

Create `/Users/leosouthey/Projects/framework/lenso/crates/lenso-operator/src/lib.rs`:

```rust
#![forbid(unsafe_code)]

pub mod crd;
pub mod reconcile;
pub mod resources;

pub use crd::{
    LensoServiceProvider, LensoServiceProviderAutoscalingSpec,
    LensoServiceProviderCondition, LensoServiceProviderDisruptionBudgetSpec,
    LensoServiceProviderEnvFromSpec, LensoServiceProviderIngressSpec,
    LensoServiceProviderNetworkPolicySpec, LensoServiceProviderSpec,
    LensoServiceProviderState, LensoServiceProviderStatus,
};
```

- [ ] **Step 4: Add CRD types**

Create `/Users/leosouthey/Projects/framework/lenso/crates/lenso-operator/src/crd.rs`:

```rust
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, CustomResource, Deserialize, JsonSchema, Serialize)]
#[kube(
    group = "lenso.dev",
    version = "v1alpha1",
    kind = "LensoServiceProvider",
    plural = "lensoserviceproviders",
    namespaced,
    status = "LensoServiceProviderStatus",
    derive = "PartialEq",
    printcolumn = r#"{"name":"State","type":"string","jsonPath":".status.state"}"#,
    printcolumn = r#"{"name":"Release","type":"string","jsonPath":".status.observedReleaseId"}"#,
    printcolumn = r#"{"name":"Image","type":"string","jsonPath":".status.observedImage"}"#,
    printcolumn = r#"{"name":"Ready","type":"integer","jsonPath":".status.readyReplicas"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct LensoServiceProviderSpec {
    pub service_name: String,
    pub environment: String,
    pub image: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_reference: Option<String>,
    #[serde(default)]
    pub modules: Vec<String>,
    #[serde(default = "default_replicas")]
    pub replicas: i32,
    pub port: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env_from: Option<LensoServiceProviderEnvFromSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingress: Option<LensoServiceProviderIngressSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autoscaling: Option<LensoServiceProviderAutoscalingSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disruption_budget: Option<LensoServiceProviderDisruptionBudgetSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_policy: Option<LensoServiceProviderNetworkPolicySpec>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LensoServiceProviderEnvFromSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_map: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LensoServiceProviderIngressSpec {
    pub host: String,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LensoServiceProviderAutoscalingSpec {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_replicas")]
    pub min_replicas: i32,
    #[serde(default = "default_max_replicas")]
    pub max_replicas: i32,
    #[serde(default = "default_target_cpu")]
    pub target_cpu_utilization: i32,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LensoServiceProviderDisruptionBudgetSpec {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_min_available")]
    pub min_available: i32,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LensoServiceProviderNetworkPolicySpec {
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LensoServiceProviderStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<LensoServiceProviderState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_generation: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_release_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ready_replicas: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desired_replicas: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_replicas: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_reference: Option<String>,
    #[serde(default)]
    pub conditions: Vec<LensoServiceProviderCondition>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LensoServiceProviderState {
    Ready,
    Progressing,
    Failed,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LensoServiceProviderCondition {
    #[serde(rename = "type")]
    pub type_: String,
    pub status: String,
    pub reason: String,
    pub message: String,
    pub last_transition_time: String,
}

const fn default_replicas() -> i32 {
    1
}

const fn default_max_replicas() -> i32 {
    3
}

const fn default_target_cpu() -> i32 {
    70
}

const fn default_min_available() -> i32 {
    1
}
```

- [ ] **Step 5: Write CRD contract tests**

Create `/Users/leosouthey/Projects/framework/lenso/crates/lenso-operator/tests/crd_contract.rs`:

```rust
use kube::CustomResourceExt;
use lenso_operator::{
    LensoServiceProvider, LensoServiceProviderCondition,
    LensoServiceProviderEnvFromSpec, LensoServiceProviderSpec,
    LensoServiceProviderState, LensoServiceProviderStatus,
};

#[test]
fn crd_uses_lenso_group_and_provider_kind() {
    let crd = LensoServiceProvider::crd();
    assert_eq!(crd.spec.group, "lenso.dev");
    assert_eq!(crd.spec.names.kind, "LensoServiceProvider");
    assert_eq!(crd.spec.names.plural, "lensoserviceproviders");
    assert_eq!(crd.spec.scope, "Namespaced");
    assert_eq!(crd.spec.versions[0].name, "v1alpha1");
}

#[test]
fn provider_spec_serializes_camel_case() {
    let spec = LensoServiceProviderSpec {
        service_name: "support-suite-provider".to_owned(),
        environment: "staging".to_owned(),
        image: "ghcr.io/acme/support-suite-provider:0.4.0".to_owned(),
        release_id: Some("rel_staging".to_owned()),
        manifest_reference: Some(
            "https://support-staging.example.com/lenso/service/v1/manifest".to_owned(),
        ),
        modules: vec!["support-ticket".to_owned()],
        replicas: 2,
        port: 4110,
        env_from: Some(LensoServiceProviderEnvFromSpec {
            config_map: Some("support-suite-provider-config".to_owned()),
            secret: Some("support-suite-provider-secrets".to_owned()),
        }),
        ingress: None,
        autoscaling: None,
        disruption_budget: None,
        network_policy: None,
    };

    let value = serde_json::to_value(spec).expect("spec serializes");
    assert_eq!(value["serviceName"], "support-suite-provider");
    assert_eq!(value["releaseId"], "rel_staging");
    assert_eq!(value["manifestReference"], "https://support-staging.example.com/lenso/service/v1/manifest");
    assert_eq!(value["envFrom"]["configMap"], "support-suite-provider-config");
}

#[test]
fn status_serializes_ready_state_and_conditions() {
    let status = LensoServiceProviderStatus {
        state: Some(LensoServiceProviderState::Ready),
        observed_generation: Some(3),
        observed_release_id: Some("rel_staging".to_owned()),
        observed_image: Some("ghcr.io/acme/support-suite-provider:0.4.0".to_owned()),
        ready_replicas: Some(2),
        desired_replicas: Some(2),
        available_replicas: Some(2),
        manifest_reference: Some(
            "https://support-staging.example.com/lenso/service/v1/manifest".to_owned(),
        ),
        conditions: vec![LensoServiceProviderCondition {
            type_: "Ready".to_owned(),
            status: "True".to_owned(),
            reason: "DeploymentAvailable".to_owned(),
            message: "2/2 replicas are ready.".to_owned(),
            last_transition_time: "2026-06-29T00:00:00Z".to_owned(),
        }],
    };

    let value = serde_json::to_value(status).expect("status serializes");
    assert_eq!(value["state"], "ready");
    assert_eq!(value["observedGeneration"], 3);
    assert_eq!(value["conditions"][0]["type"], "Ready");
}
```

- [ ] **Step 6: Verify tests fail before implementation is complete**

Run before Step 4 if practicing strict TDD:

```sh
cargo test -p lenso-operator crd_contract -- --nocapture
```

Expected:

```text
error: package ID specification `lenso-operator` did not match any packages
```

After Steps 1-5, run again.

Expected:

```text
test result: ok. 3 passed
```

- [ ] **Step 7: Commit**

Run:

```sh
git -C /Users/leosouthey/Projects/framework/lenso add Cargo.toml crates/lenso-operator
git -C /Users/leosouthey/Projects/framework/lenso commit -m "feat: add lenso service provider crd"
```

## Task 2: Operator Desired Resource Builders

**Files:**

- Create: `/Users/leosouthey/Projects/framework/lenso/crates/lenso-operator/src/resources.rs`
- Create: `/Users/leosouthey/Projects/framework/lenso/crates/lenso-operator/tests/resource_builders.rs`

- [ ] **Step 1: Write resource builder tests**

Create `/Users/leosouthey/Projects/framework/lenso/crates/lenso-operator/tests/resource_builders.rs`:

```rust
use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::autoscaling::v2::HorizontalPodAutoscaler;
use k8s_openapi::api::core::v1::Service;
use k8s_openapi::api::networking::v1::{Ingress, NetworkPolicy};
use k8s_openapi::api::policy::v1::PodDisruptionBudget;
use kube::ResourceExt;
use lenso_operator::crd::{
    LensoServiceProvider, LensoServiceProviderAutoscalingSpec,
    LensoServiceProviderDisruptionBudgetSpec, LensoServiceProviderEnvFromSpec,
    LensoServiceProviderIngressSpec, LensoServiceProviderNetworkPolicySpec,
    LensoServiceProviderSpec,
};
use lenso_operator::resources::{
    build_deployment, build_horizontal_pod_autoscaler, build_ingress,
    build_network_policy, build_pod_disruption_budget, build_service,
};

fn provider() -> LensoServiceProvider {
    LensoServiceProvider::new(
        "support-suite-provider",
        LensoServiceProviderSpec {
            service_name: "support-suite-provider".to_owned(),
            environment: "staging".to_owned(),
            image: "ghcr.io/acme/support-suite-provider:0.4.0".to_owned(),
            release_id: Some("rel_staging".to_owned()),
            manifest_reference: Some(
                "https://support-staging.example.com/lenso/service/v1/manifest".to_owned(),
            ),
            modules: vec!["support-ticket".to_owned()],
            replicas: 2,
            port: 4110,
            env_from: Some(LensoServiceProviderEnvFromSpec {
                config_map: Some("support-suite-provider-config".to_owned()),
                secret: Some("support-suite-provider-secrets".to_owned()),
            }),
            ingress: Some(LensoServiceProviderIngressSpec {
                host: "support-staging.example.com".to_owned(),
            }),
            autoscaling: Some(LensoServiceProviderAutoscalingSpec {
                enabled: true,
                min_replicas: 2,
                max_replicas: 6,
                target_cpu_utilization: 70,
            }),
            disruption_budget: Some(LensoServiceProviderDisruptionBudgetSpec {
                enabled: true,
                min_available: 1,
            }),
            network_policy: Some(LensoServiceProviderNetworkPolicySpec { enabled: true }),
        },
    )
}

#[test]
fn deployment_contains_lenso_labels_annotations_probes_and_env_refs() {
    let deployment: Deployment = build_deployment(&provider()).expect("deployment builds");
    assert_eq!(deployment.name_any(), "support-suite-provider");
    assert_eq!(deployment.metadata.namespace.as_deref(), None);
    let labels = deployment.metadata.labels.as_ref().expect("labels");
    assert_eq!(labels["app.kubernetes.io/part-of"], "lenso");
    assert_eq!(labels["app.kubernetes.io/component"], "service-provider");
    assert_eq!(labels["lenso.dev/service-provider"], "support-suite-provider");
    assert_eq!(labels["lenso.dev/environment"], "staging");

    let annotations = deployment.metadata.annotations.as_ref().expect("annotations");
    assert_eq!(annotations["lenso.dev/release-id"], "rel_staging");
    assert_eq!(annotations["lenso.dev/modules"], "support-ticket");

    let spec = deployment.spec.as_ref().expect("deployment spec");
    assert_eq!(spec.replicas, Some(2));
    let pod = spec.template.spec.as_ref().expect("pod spec");
    let container = &pod.containers[0];
    assert_eq!(container.image.as_deref(), Some("ghcr.io/acme/support-suite-provider:0.4.0"));
    assert!(container.readiness_probe.is_some());
    assert!(container.liveness_probe.is_some());
    assert_eq!(container.env_from.as_ref().expect("env_from").len(), 2);
}

#[test]
fn service_selects_provider_pods() {
    let service: Service = build_service(&provider()).expect("service builds");
    assert_eq!(service.name_any(), "support-suite-provider");
    let spec = service.spec.as_ref().expect("service spec");
    assert_eq!(spec.selector.as_ref().expect("selector")["app.kubernetes.io/name"], "support-suite-provider");
    assert_eq!(spec.ports.as_ref().expect("ports")[0].port, 4110);
}

#[test]
fn optional_resources_build_when_enabled() {
    let provider = provider();
    let ingress: Ingress = build_ingress(&provider).expect("ingress builds").expect("enabled");
    assert_eq!(ingress.spec.as_ref().expect("spec").rules.as_ref().expect("rules")[0].host.as_deref(), Some("support-staging.example.com"));

    let hpa: HorizontalPodAutoscaler = build_horizontal_pod_autoscaler(&provider).expect("hpa builds").expect("enabled");
    assert_eq!(hpa.spec.as_ref().expect("spec").min_replicas, Some(2));
    assert_eq!(hpa.spec.as_ref().expect("spec").max_replicas, 6);

    let pdb: PodDisruptionBudget = build_pod_disruption_budget(&provider).expect("pdb builds").expect("enabled");
    assert!(pdb.spec.as_ref().expect("spec").min_available.is_some());

    let policy: NetworkPolicy = build_network_policy(&provider).expect("policy builds").expect("enabled");
    assert_eq!(policy.spec.as_ref().expect("spec").policy_types.as_ref().expect("types"), &vec!["Ingress".to_owned()]);
}
```

- [ ] **Step 2: Run tests and confirm builder module is missing**

Run:

```sh
cargo test -p lenso-operator resource_builders -- --nocapture
```

Expected includes:

```text
unresolved import `lenso_operator::resources`
```

- [ ] **Step 3: Implement resource builders**

Create `/Users/leosouthey/Projects/framework/lenso/crates/lenso-operator/src/resources.rs` with these public functions and helper responsibilities:

```rust
use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec};
use k8s_openapi::api::autoscaling::v2::{
    CrossVersionObjectReference, HorizontalPodAutoscaler, HorizontalPodAutoscalerSpec,
    MetricSpec, MetricTarget, ResourceMetricSource,
};
use k8s_openapi::api::core::v1::{
    ConfigMapEnvSource, Container, EnvFromSource, HTTPGetAction, PodSpec, PodTemplateSpec,
    Probe, SecretEnvSource, Service, ServicePort, ServiceSpec,
};
use k8s_openapi::api::networking::v1::{
    HTTPIngressPath, HTTPIngressRuleValue, Ingress, IngressBackend, IngressRule,
    IngressServiceBackend, IngressSpec, NetworkPolicy, NetworkPolicyIngressRule,
    NetworkPolicyPort, NetworkPolicySpec, ServiceBackendPort,
};
use k8s_openapi::api::policy::v1::{PodDisruptionBudget, PodDisruptionBudgetSpec};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, ObjectMeta};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::ResourceExt;

use crate::crd::LensoServiceProvider;

pub fn build_deployment(provider: &LensoServiceProvider) -> Result<Deployment> {
    validate_provider(provider)?;
    let name = provider.name_any();
    let labels = labels(provider);
    let annotations = annotations(provider);
    let mut container = Container {
        name: name.clone(),
        image: Some(provider.spec.image.clone()),
        ports: Some(vec![k8s_openapi::api::core::v1::ContainerPort {
            container_port: provider.spec.port,
            ..Default::default()
        }]),
        readiness_probe: Some(http_probe(provider.spec.port)),
        liveness_probe: Some(http_probe(provider.spec.port)),
        ..Default::default()
    };
    if let Some(env_from) = &provider.spec.env_from {
        let mut sources = Vec::new();
        if let Some(config_map) = &env_from.config_map {
            sources.push(EnvFromSource {
                config_map_ref: Some(ConfigMapEnvSource {
                    name: Some(config_map.clone()),
                    ..Default::default()
                }),
                ..Default::default()
            });
        }
        if let Some(secret) = &env_from.secret {
            sources.push(EnvFromSource {
                secret_ref: Some(SecretEnvSource {
                    name: Some(secret.clone()),
                    optional: Some(true),
                }),
                ..Default::default()
            });
        }
        if !sources.is_empty() {
            container.env_from = Some(sources);
        }
    }

    Ok(Deployment {
        metadata: ObjectMeta {
            name: Some(name.clone()),
            labels: Some(labels.clone()),
            annotations: Some(annotations),
            ..Default::default()
        },
        spec: Some(DeploymentSpec {
            replicas: Some(provider.spec.replicas),
            selector: LabelSelector {
                match_labels: Some(selector_labels(&name)),
                ..Default::default()
            },
            template: PodTemplateSpec {
                metadata: Some(ObjectMeta {
                    labels: Some(labels),
                    ..Default::default()
                }),
                spec: Some(PodSpec {
                    containers: vec![container],
                    ..Default::default()
                }),
            },
            ..Default::default()
        }),
        ..Default::default()
    })
}
```

Add the remaining builder functions in the same file:

```rust
pub fn build_service(provider: &LensoServiceProvider) -> Result<Service> {
    validate_provider(provider)?;
    let name = provider.name_any();
    Ok(Service {
        metadata: ObjectMeta {
            name: Some(name.clone()),
            labels: Some(labels(provider)),
            ..Default::default()
        },
        spec: Some(ServiceSpec {
            selector: Some(selector_labels(&name)),
            ports: Some(vec![ServicePort {
                name: Some("http".to_owned()),
                port: provider.spec.port,
                target_port: Some(IntOrString::Int(provider.spec.port)),
                ..Default::default()
            }]),
            ..Default::default()
        }),
        ..Default::default()
    })
}

pub fn build_ingress(provider: &LensoServiceProvider) -> Result<Option<Ingress>> {
    validate_provider(provider)?;
    let Some(ingress) = &provider.spec.ingress else {
        return Ok(None);
    };
    let name = provider.name_any();
    Ok(Some(Ingress {
        metadata: ObjectMeta {
            name: Some(name.clone()),
            labels: Some(labels(provider)),
            ..Default::default()
        },
        spec: Some(IngressSpec {
            rules: Some(vec![IngressRule {
                host: Some(ingress.host.clone()),
                http: Some(HTTPIngressRuleValue {
                    paths: vec![HTTPIngressPath {
                        path: Some("/".to_owned()),
                        path_type: Some("Prefix".to_owned()),
                        backend: IngressBackend {
                            service: Some(IngressServiceBackend {
                                name: name.clone(),
                                port: Some(ServiceBackendPort {
                                    number: Some(provider.spec.port),
                                    ..Default::default()
                                }),
                            }),
                            ..Default::default()
                        },
                    }],
                }),
            }]),
            ..Default::default()
        }),
        ..Default::default()
    }))
}

pub fn build_horizontal_pod_autoscaler(
    provider: &LensoServiceProvider,
) -> Result<Option<HorizontalPodAutoscaler>> {
    validate_provider(provider)?;
    let Some(autoscaling) = &provider.spec.autoscaling else {
        return Ok(None);
    };
    if !autoscaling.enabled {
        return Ok(None);
    }
    let name = provider.name_any();
    Ok(Some(HorizontalPodAutoscaler {
        metadata: ObjectMeta {
            name: Some(name.clone()),
            labels: Some(labels(provider)),
            ..Default::default()
        },
        spec: Some(HorizontalPodAutoscalerSpec {
            scale_target_ref: CrossVersionObjectReference {
                api_version: Some("apps/v1".to_owned()),
                kind: "Deployment".to_owned(),
                name: name.clone(),
            },
            min_replicas: Some(autoscaling.min_replicas),
            max_replicas: autoscaling.max_replicas,
            metrics: Some(vec![MetricSpec {
                type_: "Resource".to_owned(),
                resource: Some(ResourceMetricSource {
                    name: "cpu".to_owned(),
                    target: MetricTarget {
                        type_: "Utilization".to_owned(),
                        average_utilization: Some(autoscaling.target_cpu_utilization),
                        ..Default::default()
                    },
                }),
                ..Default::default()
            }]),
            ..Default::default()
        }),
        ..Default::default()
    }))
}
```

Continue with PDB, NetworkPolicy, labels, annotations, selector, probe, and validation:

```rust
pub fn build_pod_disruption_budget(
    provider: &LensoServiceProvider,
) -> Result<Option<PodDisruptionBudget>> {
    validate_provider(provider)?;
    let Some(disruption_budget) = &provider.spec.disruption_budget else {
        return Ok(None);
    };
    if !disruption_budget.enabled {
        return Ok(None);
    }
    let name = provider.name_any();
    Ok(Some(PodDisruptionBudget {
        metadata: ObjectMeta {
            name: Some(name.clone()),
            labels: Some(labels(provider)),
            ..Default::default()
        },
        spec: Some(PodDisruptionBudgetSpec {
            min_available: Some(IntOrString::Int(disruption_budget.min_available)),
            selector: Some(LabelSelector {
                match_labels: Some(selector_labels(&name)),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    }))
}

pub fn build_network_policy(provider: &LensoServiceProvider) -> Result<Option<NetworkPolicy>> {
    validate_provider(provider)?;
    let Some(network_policy) = &provider.spec.network_policy else {
        return Ok(None);
    };
    if !network_policy.enabled {
        return Ok(None);
    }
    let name = provider.name_any();
    Ok(Some(NetworkPolicy {
        metadata: ObjectMeta {
            name: Some(name.clone()),
            labels: Some(labels(provider)),
            ..Default::default()
        },
        spec: Some(NetworkPolicySpec {
            pod_selector: LabelSelector {
                match_labels: Some(selector_labels(&name)),
                ..Default::default()
            },
            policy_types: Some(vec!["Ingress".to_owned()]),
            ingress: Some(vec![NetworkPolicyIngressRule {
                ports: Some(vec![NetworkPolicyPort {
                    port: Some(IntOrString::Int(provider.spec.port)),
                    protocol: Some("TCP".to_owned()),
                    ..Default::default()
                }]),
                ..Default::default()
            }]),
            ..Default::default()
        }),
        ..Default::default()
    }))
}

fn labels(provider: &LensoServiceProvider) -> BTreeMap<String, String> {
    let name = provider.name_any();
    BTreeMap::from([
        ("app.kubernetes.io/name".to_owned(), name.clone()),
        ("app.kubernetes.io/part-of".to_owned(), "lenso".to_owned()),
        ("app.kubernetes.io/component".to_owned(), "service-provider".to_owned()),
        ("lenso.dev/service-provider".to_owned(), provider.spec.service_name.clone()),
        ("lenso.dev/environment".to_owned(), provider.spec.environment.clone()),
    ])
}

fn selector_labels(name: &str) -> BTreeMap<String, String> {
    BTreeMap::from([("app.kubernetes.io/name".to_owned(), name.to_owned())])
}

fn annotations(provider: &LensoServiceProvider) -> BTreeMap<String, String> {
    let mut annotations = BTreeMap::new();
    annotations.insert("lenso.dev/modules".to_owned(), provider.spec.modules.join(","));
    if let Some(release_id) = &provider.spec.release_id {
        annotations.insert("lenso.dev/release-id".to_owned(), release_id.clone());
    }
    if let Some(manifest_reference) = &provider.spec.manifest_reference {
        annotations.insert(
            "lenso.dev/manifest-reference".to_owned(),
            manifest_reference.clone(),
        );
    }
    annotations
}

fn http_probe(port: i32) -> Probe {
    Probe {
        http_get: Some(HTTPGetAction {
            path: Some("/lenso/service/v1/status".to_owned()),
            port: IntOrString::Int(port),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn validate_provider(provider: &LensoServiceProvider) -> Result<()> {
    if provider.spec.service_name.trim().is_empty() {
        return Err(anyhow!("spec.serviceName is required"));
    }
    if provider.spec.environment.trim().is_empty() {
        return Err(anyhow!("spec.environment is required"));
    }
    if provider.spec.image.trim().is_empty() {
        return Err(anyhow!("spec.image is required"));
    }
    if provider.spec.port <= 0 {
        return Err(anyhow!("spec.port must be greater than zero"));
    }
    Ok(())
}
```

- [ ] **Step 4: Run resource tests**

Run:

```sh
cargo test -p lenso-operator resource_builders -- --nocapture
```

Expected:

```text
test result: ok. 3 passed
```

- [ ] **Step 5: Commit**

Run:

```sh
git -C /Users/leosouthey/Projects/framework/lenso add crates/lenso-operator/src/resources.rs crates/lenso-operator/tests/resource_builders.rs
git -C /Users/leosouthey/Projects/framework/lenso commit -m "feat: build operator managed resources"
```

## Task 3: Operator Reconciliation And Status

**Files:**

- Create: `/Users/leosouthey/Projects/framework/lenso/crates/lenso-operator/src/reconcile.rs`
- Create: `/Users/leosouthey/Projects/framework/lenso/crates/lenso-operator/src/main.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso/crates/lenso-operator/src/lib.rs`
- Add tests in: `/Users/leosouthey/Projects/framework/lenso/crates/lenso-operator/tests/resource_builders.rs`

- [ ] **Step 1: Add status derivation tests**

Append to `/Users/leosouthey/Projects/framework/lenso/crates/lenso-operator/tests/resource_builders.rs`:

```rust
use lenso_operator::reconcile::{deployment_status_to_provider_status, invalid_spec_status};

#[test]
fn deployment_status_maps_to_ready_provider_status() {
    let mut deployment = build_deployment(&provider()).expect("deployment builds");
    deployment.status = Some(k8s_openapi::api::apps::v1::DeploymentStatus {
        ready_replicas: Some(2),
        replicas: Some(2),
        available_replicas: Some(2),
        ..Default::default()
    });

    let status = deployment_status_to_provider_status(&provider(), Some(&deployment));

    assert_eq!(status.state.expect("state"), lenso_operator::LensoServiceProviderState::Ready);
    assert_eq!(status.observed_release_id.as_deref(), Some("rel_staging"));
    assert_eq!(status.observed_image.as_deref(), Some("ghcr.io/acme/support-suite-provider:0.4.0"));
    assert_eq!(status.ready_replicas, Some(2));
    assert_eq!(status.desired_replicas, Some(2));
    assert_eq!(status.conditions[0].type_, "Ready");
}

#[test]
fn invalid_spec_status_explains_missing_image() {
    let mut provider = provider();
    provider.spec.image.clear();

    let status = invalid_spec_status(&provider, "spec.image is required");

    assert_eq!(status.state.expect("state"), lenso_operator::LensoServiceProviderState::Failed);
    assert_eq!(status.conditions[0].reason, "SpecInvalid");
    assert_eq!(status.conditions[0].message, "spec.image is required");
}
```

- [ ] **Step 2: Implement status helpers and reconcile skeleton**

Create `/Users/leosouthey/Projects/framework/lenso/crates/lenso-operator/src/reconcile.rs`:

```rust
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use chrono::Utc;
use k8s_openapi::api::apps::v1::Deployment;
use kube::api::{Api, Patch, PatchParams};
use kube::runtime::controller::Action;
use kube::{Client, ResourceExt};
use serde::Serialize;
use thiserror::Error;

use crate::crd::{
    LensoServiceProvider, LensoServiceProviderCondition, LensoServiceProviderState,
    LensoServiceProviderStatus,
};
use crate::resources::{
    build_deployment, build_horizontal_pod_autoscaler, build_ingress, build_network_policy,
    build_pod_disruption_budget, build_service,
};

#[derive(Clone, Debug)]
pub struct ReconcileContext {
    pub client: Client,
}

#[derive(Debug, Error)]
pub enum ReconcileError {
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
    #[error(transparent)]
    Kube(#[from] kube::Error),
}

pub async fn reconcile(
    provider: Arc<LensoServiceProvider>,
    context: Arc<ReconcileContext>,
) -> Result<Action, ReconcileError> {
    let namespace = provider.namespace().unwrap_or_else(|| "default".to_owned());
    let status_api: Api<LensoServiceProvider> = Api::namespaced(context.client.clone(), &namespace);

    if let Err(error) = validate(&provider) {
        patch_status(&status_api, &provider, invalid_spec_status(&provider, &error.to_string())).await?;
        return Ok(Action::requeue(Duration::from_secs(300)));
    }

    apply(&context.client, &namespace, build_deployment(&provider)?).await?;
    apply(&context.client, &namespace, build_service(&provider)?).await?;
    if let Some(ingress) = build_ingress(&provider)? {
        apply(&context.client, &namespace, ingress).await?;
    }
    if let Some(hpa) = build_horizontal_pod_autoscaler(&provider)? {
        apply(&context.client, &namespace, hpa).await?;
    }
    if let Some(pdb) = build_pod_disruption_budget(&provider)? {
        apply(&context.client, &namespace, pdb).await?;
    }
    if let Some(policy) = build_network_policy(&provider)? {
        apply(&context.client, &namespace, policy).await?;
    }

    let deployments: Api<Deployment> = Api::namespaced(context.client.clone(), &namespace);
    let deployment = deployments.get_opt(&provider.name_any()).await?;
    patch_status(
        &status_api,
        &provider,
        deployment_status_to_provider_status(&provider, deployment.as_ref()),
    )
    .await?;

    Ok(Action::requeue(Duration::from_secs(60)))
}

pub fn error_policy(
    _provider: Arc<LensoServiceProvider>,
    error: &ReconcileError,
    _context: Arc<ReconcileContext>,
) -> Action {
    tracing::warn!(error = %error, "lenso service provider reconcile failed");
    Action::requeue(Duration::from_secs(30))
}
```

Add helper functions in the same file:

```rust
pub fn deployment_status_to_provider_status(
    provider: &LensoServiceProvider,
    deployment: Option<&Deployment>,
) -> LensoServiceProviderStatus {
    let desired = Some(provider.spec.replicas);
    let ready = deployment
        .and_then(|deployment| deployment.status.as_ref())
        .and_then(|status| status.ready_replicas);
    let available = deployment
        .and_then(|deployment| deployment.status.as_ref())
        .and_then(|status| status.available_replicas);
    let observed_generation = provider.metadata.generation;
    let state = if ready == desired && desired.unwrap_or_default() > 0 {
        LensoServiceProviderState::Ready
    } else if deployment.is_some() {
        LensoServiceProviderState::Progressing
    } else {
        LensoServiceProviderState::Unknown
    };
    let condition = match state {
        LensoServiceProviderState::Ready => condition(
            "Ready",
            "True",
            "DeploymentAvailable",
            format!("{}/{} replicas are ready.", ready.unwrap_or_default(), desired.unwrap_or_default()),
        ),
        LensoServiceProviderState::Progressing => condition(
            "Ready",
            "False",
            "DeploymentProgressing",
            format!("{}/{} replicas are ready.", ready.unwrap_or_default(), desired.unwrap_or_default()),
        ),
        LensoServiceProviderState::Failed => condition(
            "Ready",
            "False",
            "ApplyFailed",
            "deployment reconcile failed".to_owned(),
        ),
        LensoServiceProviderState::Unknown => condition(
            "Ready",
            "Unknown",
            "DeploymentUnknown",
            "deployment has not been observed yet".to_owned(),
        ),
    };

    LensoServiceProviderStatus {
        state: Some(state),
        observed_generation,
        observed_release_id: provider.spec.release_id.clone(),
        observed_image: Some(provider.spec.image.clone()),
        ready_replicas: ready,
        desired_replicas: desired,
        available_replicas: available,
        manifest_reference: provider.spec.manifest_reference.clone(),
        conditions: vec![condition],
    }
}

pub fn invalid_spec_status(
    provider: &LensoServiceProvider,
    message: &str,
) -> LensoServiceProviderStatus {
    LensoServiceProviderStatus {
        state: Some(LensoServiceProviderState::Failed),
        observed_generation: provider.metadata.generation,
        observed_release_id: provider.spec.release_id.clone(),
        observed_image: None,
        ready_replicas: Some(0),
        desired_replicas: Some(provider.spec.replicas),
        available_replicas: Some(0),
        manifest_reference: provider.spec.manifest_reference.clone(),
        conditions: vec![condition("Reconciled", "False", "SpecInvalid", message.to_owned())],
    }
}

fn validate(provider: &LensoServiceProvider) -> Result<()> {
    if provider.spec.service_name.trim().is_empty() {
        anyhow::bail!("spec.serviceName is required");
    }
    if provider.spec.environment.trim().is_empty() {
        anyhow::bail!("spec.environment is required");
    }
    if provider.spec.image.trim().is_empty() {
        anyhow::bail!("spec.image is required");
    }
    if provider.spec.port <= 0 {
        anyhow::bail!("spec.port must be greater than zero");
    }
    Ok(())
}

fn condition(
    type_: &str,
    status: &str,
    reason: &str,
    message: String,
) -> LensoServiceProviderCondition {
    LensoServiceProviderCondition {
        type_: type_.to_owned(),
        status: status.to_owned(),
        reason: reason.to_owned(),
        message,
        last_transition_time: Utc::now().to_rfc3339(),
    }
}

async fn patch_status(
    api: &Api<LensoServiceProvider>,
    provider: &LensoServiceProvider,
    status: LensoServiceProviderStatus,
) -> Result<LensoServiceProvider, kube::Error> {
    api.patch_status(
        &provider.name_any(),
        &PatchParams::apply("lenso-operator").force(),
        &Patch::Apply(serde_json::json!({
            "apiVersion": "lenso.dev/v1alpha1",
            "kind": "LensoServiceProvider",
            "status": status
        })),
    )
    .await
}

async fn apply<K>(client: &Client, namespace: &str, resource: K) -> Result<K, kube::Error>
where
    K: Clone + ResourceExt + Serialize + serde::de::DeserializeOwned + std::fmt::Debug,
    <K as kube::Resource>::DynamicType: Default,
{
    let api: Api<K> = Api::namespaced(client.clone(), namespace);
    api.patch(
        &resource.name_any(),
        &PatchParams::apply("lenso-operator").force(),
        &Patch::Apply(resource),
    )
    .await
}
```

- [ ] **Step 3: Add controller binary**

Create `/Users/leosouthey/Projects/framework/lenso/crates/lenso-operator/src/main.rs`:

```rust
use std::sync::Arc;

use anyhow::Result;
use futures::StreamExt;
use kube::api::Api;
use kube::runtime::controller::Controller;
use kube::runtime::watcher::Config;
use kube::{Client, CustomResourceExt};
use lenso_operator::crd::LensoServiceProvider;
use lenso_operator::reconcile::{error_policy, reconcile, ReconcileContext};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "lenso_operator=info,kube=info".to_owned()),
        )
        .init();

    if std::env::args().any(|arg| arg == "--print-crd") {
        println!("{}", serde_yaml::to_string(&LensoServiceProvider::crd())?);
        return Ok(());
    }

    let client = Client::try_default().await?;
    let providers: Api<LensoServiceProvider> = match std::env::var("LENSO_OPERATOR_NAMESPACE") {
        Ok(namespace) if !namespace.trim().is_empty() => Api::namespaced(client.clone(), &namespace),
        _ => Api::all(client.clone()),
    };
    let context = Arc::new(ReconcileContext { client });

    Controller::new(providers, Config::default())
        .run(reconcile, error_policy, context)
        .for_each(|result| async move {
            match result {
                Ok(object_ref) => tracing::info!(?object_ref, "reconciled lenso service provider"),
                Err(error) => tracing::warn!(error = %error, "reconcile stream error"),
            }
        })
        .await;

    Ok(())
}
```

- [ ] **Step 4: Verify operator crate**

Run:

```sh
cargo test -p lenso-operator
cargo run -p lenso-operator -- --print-crd | sed -n '1,40p'
```

Expected:

```text
test result: ok
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
```

- [ ] **Step 5: Commit**

Run:

```sh
git -C /Users/leosouthey/Projects/framework/lenso add crates/lenso-operator
git -C /Users/leosouthey/Projects/framework/lenso commit -m "feat: reconcile lenso service providers"
```

## Task 4: CLI Operator Install Bundle

**Files:**

- Create: `/Users/leosouthey/Projects/framework/lenso-cli/src/operator.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs`

- [ ] **Step 1: Add command parsing test**

Append to existing parser tests in `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs`:

```rust
#[test]
fn parses_operator_export_crd() {
    let cli = Cli::try_parse_from([
        "lenso",
        "operator",
        "export-crd",
        "--output",
        "dist/lenso-operator/crds",
    ])
    .expect("operator export-crd parses");

    let Command::Operator { command } = cli.command else {
        panic!("expected operator command");
    };
    let OperatorCommand::ExportCrd(args) = command;
    assert_eq!(args.output, std::path::PathBuf::from("dist/lenso-operator/crds"));
}
```

Run:

```sh
cargo test parses_operator_export_crd
```

Expected includes:

```text
no variant named `Operator`
```

- [ ] **Step 2: Add CLI command types**

Modify `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs`:

```rust
mod host;
mod module;
mod operator;
mod service;
```

Add the top-level command variant:

```rust
#[derive(Debug, Subcommand)]
enum Command {
    Serve(ServeArgs),
    Host {
        #[command(subcommand)]
        command: HostCommand,
    },
    Module {
        #[command(subcommand)]
        command: ModuleCommand,
    },
    Service {
        #[command(subcommand)]
        command: ServiceCommand,
    },
    Operator {
        #[command(subcommand)]
        command: OperatorCommand,
    },
    Console {
        #[command(subcommand)]
        command: ConsoleCommand,
    },
}

#[derive(Debug, Subcommand)]
enum OperatorCommand {
    /// Export the Lenso Kubernetes Operator install bundle.
    ExportCrd(OperatorExportCrdArgs),
}

#[derive(Debug, Args, Clone)]
struct OperatorExportCrdArgs {
    /// Output directory for CRD, RBAC, deployment, kustomization, and README.
    #[arg(long)]
    output: std::path::PathBuf,

    /// Operator image to put in deployment.yaml.
    #[arg(long, default_value = "ghcr.io/lenso-dev/lenso-operator:latest")]
    image: String,

    /// Namespace for operator install resources.
    #[arg(long, default_value = "lenso-system")]
    namespace: String,

    /// Print machine-readable JSON.
    #[arg(long)]
    json: bool,
}
```

Add dispatch near the existing command match:

```rust
Command::Operator { command } => match command {
    OperatorCommand::ExportCrd(args) => {
        operator::export_crd_bundle((&args).into())?;
    }
},
```

- [ ] **Step 3: Implement operator bundle writer**

Create `/Users/leosouthey/Projects/framework/lenso-cli/src/operator.rs`:

```rust
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde_json::json;

#[derive(Clone, Debug)]
pub struct ExportCrdBundleOptions {
    pub output: PathBuf,
    pub image: String,
    pub namespace: String,
    pub json: bool,
}

impl From<&crate::OperatorExportCrdArgs> for ExportCrdBundleOptions {
    fn from(args: &crate::OperatorExportCrdArgs) -> Self {
        Self {
            output: args.output.clone(),
            image: args.image.clone(),
            namespace: args.namespace.clone(),
            json: args.json,
        }
    }
}

pub fn export_crd_bundle(options: ExportCrdBundleOptions) -> Result<()> {
    fs::create_dir_all(&options.output)
        .with_context(|| format!("create directory {}", options.output.display()))?;

    let files = [
        ("lenso.dev_lensoserviceproviders.yaml", crd_yaml()),
        ("rbac.yaml", rbac_yaml(&options.namespace)),
        ("deployment.yaml", deployment_yaml(&options.namespace, &options.image)),
        ("kustomization.yaml", kustomization_yaml()),
        ("README.md", readme(&options.namespace)),
    ];

    for (name, contents) in files {
        fs::write(options.output.join(name), contents)
            .with_context(|| format!("write {}", options.output.join(name).display()))?;
    }

    if options.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "output": options.output,
                "namespace": options.namespace,
                "files": [
                    "lenso.dev_lensoserviceproviders.yaml",
                    "rbac.yaml",
                    "deployment.yaml",
                    "kustomization.yaml",
                    "README.md"
                ]
            }))?
        );
    } else {
        println!("Wrote Lenso Operator bundle: {}", options.output.display());
        println!("next: kubectl apply -k {}", options.output.display());
    }

    Ok(())
}
```

Add file content helpers in the same file. The CRD can be a static YAML copy generated from the operator crate in Task 3. The minimum accepted helper names are:

```rust
fn crd_yaml() -> String
fn rbac_yaml(namespace: &str) -> String
fn deployment_yaml(namespace: &str, image: &str) -> String
fn kustomization_yaml() -> String
fn readme(namespace: &str) -> String
```

The `kustomization_yaml()` output must contain:

```yaml
resources:
  - lenso.dev_lensoserviceproviders.yaml
  - rbac.yaml
  - deployment.yaml
```

The `deployment_yaml()` output must set:

```yaml
env:
  - name: RUST_LOG
    value: lenso_operator=info,kube=info
```

- [ ] **Step 4: Verify operator bundle export**

Run:

```sh
cargo test parses_operator_export_crd
cargo run -- operator export-crd --output /tmp/lenso-operator-v16 --namespace lenso-system --image ghcr.io/acme/lenso-operator:test
find /tmp/lenso-operator-v16 -maxdepth 1 -type f -print | sort
```

Expected:

```text
/tmp/lenso-operator-v16/README.md
/tmp/lenso-operator-v16/deployment.yaml
/tmp/lenso-operator-v16/kustomization.yaml
/tmp/lenso-operator-v16/lenso.dev_lensoserviceproviders.yaml
/tmp/lenso-operator-v16/rbac.yaml
```

- [ ] **Step 5: Commit**

Run:

```sh
git -C /Users/leosouthey/Projects/framework/lenso-cli add src/main.rs src/operator.rs
git -C /Users/leosouthey/Projects/framework/lenso-cli commit -m "feat: export lenso operator bundle"
```

## Task 5: CLI Provider CR Export And Operator Status

**Files:**

- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso-cli/src/module.rs`

- [ ] **Step 1: Extend deploy command parser tests**

Modify existing deploy parser tests in `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs` and add:

```rust
#[test]
fn parses_service_deploy_export_operator_target() {
    let cli = Cli::try_parse_from([
        "lenso",
        "service",
        "deploy",
        "export",
        "support-suite-provider",
        "--env",
        "staging",
        "--target",
        "operator",
        "--output-dir",
        "dist/operator/staging",
    ])
    .expect("service deploy export parses");

    let Command::Service { command: ServiceCommand::Deploy { command: ServiceDeployCommand::Export(args) } } = cli.command else {
        panic!("expected service deploy export");
    };
    assert_eq!(args.target, ServiceDeploymentTargetArg::Operator);
}

#[test]
fn parses_service_deploy_status_operator_source() {
    let cli = Cli::try_parse_from([
        "lenso",
        "service",
        "deploy",
        "status",
        "support-suite-provider",
        "--env",
        "staging",
        "--source",
        "operator",
        "--from-file",
        "fixtures/operator-status.json",
    ])
    .expect("service deploy status parses");

    let Command::Service { command: ServiceCommand::Deploy { command: ServiceDeployCommand::Status(args) } } = cli.command else {
        panic!("expected service deploy status");
    };
    assert_eq!(args.source, ServiceDeploymentSourceArg::Operator);
}
```

- [ ] **Step 2: Add target and source args**

Modify `/Users/leosouthey/Projects/framework/lenso-cli/src/main.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum ServiceDeploymentTargetArg {
    Kubernetes,
    Operator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum ServiceDeploymentSourceArg {
    Kubernetes,
    Operator,
}
```

Add to `ServiceDeployStatusArgs`:

```rust
/// Deployment status source.
#[arg(long, value_enum, default_value_t = ServiceDeploymentSourceArg::Kubernetes)]
source: ServiceDeploymentSourceArg,
```

Update option conversion:

```rust
impl From<&ServiceDeployStatusArgs> for module::ServiceDeployStatusOptions {
    fn from(args: &ServiceDeployStatusArgs) -> Self {
        Self {
            environment_name: args.environment_name.clone(),
            from_file: args.from_file.clone(),
            json: args.json,
            repo_root: args.repo_root.clone(),
            service_name: args.service_name.clone(),
            source: service_deployment_source_arg(args.source).to_owned(),
            write_state: args.write_state,
        }
    }
}

const fn service_deployment_target_arg(target: ServiceDeploymentTargetArg) -> &'static str {
    match target {
        ServiceDeploymentTargetArg::Kubernetes => "kubernetes",
        ServiceDeploymentTargetArg::Operator => "operator",
    }
}

const fn service_deployment_source_arg(source: ServiceDeploymentSourceArg) -> &'static str {
    match source {
        ServiceDeploymentSourceArg::Kubernetes => "kubernetes",
        ServiceDeploymentSourceArg::Operator => "operator",
    }
}
```

- [ ] **Step 3: Extend module options**

Modify `/Users/leosouthey/Projects/framework/lenso-cli/src/module.rs`:

```rust
pub struct ServiceDeployStatusOptions {
    pub environment_name: String,
    pub from_file: Option<PathBuf>,
    pub json: bool,
    pub repo_root: Option<PathBuf>,
    pub service_name: String,
    pub source: String,
    pub write_state: bool,
}
```

Keep `ServiceDeployExportOptions.target` as the existing string field.

- [ ] **Step 4: Add operator export tests**

Add a unit test near the Kubernetes export tests in `/Users/leosouthey/Projects/framework/lenso-cli/src/module.rs`:

```rust
#[test]
fn service_deploy_export_operator_writes_provider_cr() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo = temp.path();
    std::fs::create_dir_all(repo.join(".lenso")).expect("create .lenso");
    std::fs::write(
        repo.join(".lenso/service-environments.json"),
        serde_json::json!({
            "version": 1,
            "environments": [{
                "name": "staging",
                "serviceName": "support-suite-provider",
                "target": "operator",
                "namespace": "lenso-staging",
                "image": "ghcr.io/acme/support-suite-provider:0.4.0",
                "manifestReference": "https://support-staging.example.com/lenso/service/v1/manifest",
                "config": {
                    "port": 4110,
                    "replicas": 2,
                    "ingressHost": "support-staging.example.com",
                    "autoscaling": true,
                    "disruptionBudget": true,
                    "networkPolicy": true
                }
            }]
        })
        .to_string(),
    )
    .expect("write envs");
    std::fs::write(
        repo.join(".lenso/service-releases.json"),
        serde_json::json!({
            "version": 1,
            "releases": [{
                "id": "rel_staging",
                "serviceName": "support-suite-provider",
                "environment": {"name": "staging", "target": "operator"},
                "candidate": {"version": "0.4.0"}
            }]
        })
        .to_string(),
    )
    .expect("write releases");
    let output_dir = repo.join("dist/operator/staging");

    export_service_deployment(ServiceDeployExportOptions {
        environment_name: "staging".to_owned(),
        image: None,
        ingress_host: None,
        json: false,
        namespace: None,
        output_dir: output_dir.clone(),
        hpa: false,
        port: None,
        pdb: false,
        network_policy: false,
        replicas: None,
        repo_root: Some(repo.to_path_buf()),
        service_name: "support-suite-provider".to_owned(),
        target: "operator".to_owned(),
    })
    .expect("export succeeds");

    let cr = std::fs::read_to_string(output_dir.join("lensoserviceprovider.yaml"))
        .expect("read provider cr");
    assert!(cr.contains("kind: LensoServiceProvider"));
    assert!(cr.contains("serviceName: support-suite-provider"));
    assert!(cr.contains("releaseId: rel_staging"));
    assert!(cr.contains("targetCpuUtilization: 70"));
    assert!(output_dir.join("kustomization.yaml").exists());
}
```

- [ ] **Step 5: Implement provider CR export path**

In `/Users/leosouthey/Projects/framework/lenso-cli/src/module.rs`, update `export_service_deployment`:

```rust
pub fn export_service_deployment(options: ServiceDeployExportOptions) -> Result<()> {
    match options.target.as_str() {
        "kubernetes" => export_kubernetes_service_deployment(options),
        "operator" => export_operator_service_provider(options),
        other => bail!("Unsupported deployment target `{other}`; expected kubernetes or operator"),
    }
}
```

Move the current body into:

```rust
fn export_kubernetes_service_deployment(options: ServiceDeployExportOptions) -> Result<()> {
    // Existing V15 implementation moves here unchanged except for the function name.
}
```

Add:

```rust
fn export_operator_service_provider(options: ServiceDeployExportOptions) -> Result<()> {
    let repo_root = resolve_repo_root(options.repo_root.as_deref())?;
    let environment =
        find_service_environment(&repo_root, &options.service_name, &options.environment_name)?
            .unwrap_or_else(|| {
                json!({
                    "name": options.environment_name,
                    "serviceName": options.service_name,
                    "target": "operator",
                })
            });
    let namespace = options
        .namespace
        .clone()
        .or_else(|| string_at(&environment, "/namespace"))
        .ok_or_else(|| anyhow!("Kubernetes namespace is required; pass --namespace or configure service env"))?;
    let image = options
        .image
        .clone()
        .or_else(|| string_at(&environment, "/image"))
        .ok_or_else(|| anyhow!("Kubernetes image is required; pass --image or configure service env"))?;
    let port = options
        .port
        .or_else(|| u16_at(&environment, "/config/port"))
        .unwrap_or(4100);
    let replicas = options
        .replicas
        .or_else(|| u32_at(&environment, "/config/replicas"))
        .unwrap_or(1);
    let ingress_host = options
        .ingress_host
        .clone()
        .or_else(|| string_at(&environment, "/config/ingressHost"));
    let include_hpa = options.hpa || bool_at(&environment, "/config/autoscaling").unwrap_or(false);
    let include_pdb = options.pdb || bool_at(&environment, "/config/disruptionBudget").unwrap_or(replicas > 1);
    let include_network_policy =
        options.network_policy || bool_at(&environment, "/config/networkPolicy").unwrap_or(false);
    let manifest_reference = service_environment_manifest_reference(&environment);
    let release = latest_service_release(&repo_root, &options.service_name)?;
    let release_id = release
        .as_ref()
        .and_then(|release| release.get("id").and_then(Value::as_str))
        .unwrap_or("pending");
    let service_manifest = installed_service_receipt(&repo_root, &options.service_name)
        .ok()
        .and_then(|receipt| receipt.get("serviceManifestSnapshot").cloned())
        .unwrap_or(Value::Null);
    let modules = if service_manifest.is_null() {
        Vec::new()
    } else {
        service_module_name_set(&service_manifest).into_iter().collect()
    };
    let deployment_name = kubernetes_name(&options.service_name);
    let context = KubernetesExportContext {
        deployment_name: &deployment_name,
        env_names: &service_manifest_env_names(&service_manifest),
        image: &image,
        ingress_host: ingress_host.as_deref(),
        manifest_reference: manifest_reference.as_deref().unwrap_or(""),
        modules: &modules,
        namespace: &namespace,
        port,
        release_id,
        replicas,
        service_name: &options.service_name,
        environment_name: &options.environment_name,
    };
    let output_dir = resolve_path(&repo_root, &options.output_dir);
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("create directory {}", output_dir.display()))?;

    write_file(
        &output_dir.join("lensoserviceprovider.yaml"),
        operator_provider_cr_yaml(&context, include_hpa, include_pdb, include_network_policy).as_bytes(),
    )?;
    write_file(
        &output_dir.join("kustomization.yaml"),
        "resources:\n  - lensoserviceprovider.yaml\n".as_bytes(),
    )?;
    write_file(
        &output_dir.join("README.md"),
        operator_provider_export_readme(&context).as_bytes(),
    )?;

    if options.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "serviceName": options.service_name,
                "environment": options.environment_name,
                "target": "operator",
                "outputDir": output_dir,
                "files": ["lensoserviceprovider.yaml", "kustomization.yaml", "README.md"],
            }))?
        );
    } else {
        println!("Wrote LensoServiceProvider files: {}", output_dir.display());
        println!("next: kubectl apply -k {}", output_dir.display());
        println!(
            "next: lenso service deploy status {} --env {} --source operator --write-state",
            context.service_name, context.environment_name
        );
    }
    Ok(())
}
```

Add `operator_provider_cr_yaml` and README helpers beside the existing Kubernetes YAML helpers.

- [ ] **Step 6: Add operator status fixture test**

Add a test in `/Users/leosouthey/Projects/framework/lenso-cli/src/module.rs`:

```rust
#[test]
fn service_deploy_status_operator_maps_crd_status_to_observation() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo = temp.path();
    std::fs::create_dir_all(repo.join(".lenso")).expect("create .lenso");
    std::fs::write(
        repo.join(".lenso/service-environments.json"),
        serde_json::json!({
            "version": 1,
            "environments": [{
                "name": "staging",
                "serviceName": "support-suite-provider",
                "target": "operator",
                "namespace": "lenso-staging",
                "image": "ghcr.io/acme/support-suite-provider:0.4.0"
            }]
        })
        .to_string(),
    )
    .expect("write envs");
    std::fs::write(
        repo.join(".lenso/service-releases.json"),
        serde_json::json!({
            "version": 1,
            "releases": [{
                "id": "rel_staging",
                "serviceName": "support-suite-provider",
                "environment": {"name": "staging", "target": "operator"},
                "candidate": {"version": "0.4.0"}
            }]
        })
        .to_string(),
    )
    .expect("write releases");
    let fixture = repo.join("operator-status.json");
    std::fs::write(
        &fixture,
        serde_json::json!({
            "apiVersion": "lenso.dev/v1alpha1",
            "kind": "LensoServiceProvider",
            "metadata": {
                "name": "support-suite-provider",
                "namespace": "lenso-staging",
                "generation": 3
            },
            "status": {
                "state": "ready",
                "observedGeneration": 3,
                "observedReleaseId": "rel_staging",
                "observedImage": "ghcr.io/acme/support-suite-provider:0.4.0",
                "readyReplicas": 2,
                "desiredReplicas": 2,
                "availableReplicas": 2,
                "manifestReference": "https://support-staging.example.com/lenso/service/v1/manifest",
                "conditions": [{
                    "type": "Ready",
                    "status": "True",
                    "reason": "DeploymentAvailable",
                    "message": "2/2 replicas are ready.",
                    "lastTransitionTime": "2026-06-29T00:00:00Z"
                }]
            }
        })
        .to_string(),
    )
    .expect("write fixture");

    status_service_deployment(ServiceDeployStatusOptions {
        environment_name: "staging".to_owned(),
        from_file: Some(fixture),
        json: false,
        repo_root: Some(repo.to_path_buf()),
        service_name: "support-suite-provider".to_owned(),
        source: "operator".to_owned(),
        write_state: true,
    })
    .expect("status succeeds");

    let observations = std::fs::read_to_string(repo.join(".lenso/service-deployments.json"))
        .expect("deployment observations");
    assert!(observations.contains("\"target\":\"operator\""));
    assert!(observations.contains("\"resource\":\"support-suite-provider\""));
    assert!(observations.contains("\"drift\":\"in_sync\""));
}
```

- [ ] **Step 7: Implement operator status source**

Update `status_service_deployment` in `/Users/leosouthey/Projects/framework/lenso-cli/src/module.rs`:

```rust
pub fn status_service_deployment(options: ServiceDeployStatusOptions) -> Result<()> {
    match options.source.as_str() {
        "kubernetes" => status_kubernetes_service_deployment(options),
        "operator" => status_operator_service_deployment(options),
        other => bail!("Unsupported deployment source `{other}`; expected kubernetes or operator"),
    }
}
```

Move the existing body into:

```rust
fn status_kubernetes_service_deployment(options: ServiceDeployStatusOptions) -> Result<()> {
    // Existing V15 implementation moves here unchanged except for the function name.
}
```

Add:

```rust
fn status_operator_service_deployment(options: ServiceDeployStatusOptions) -> Result<()> {
    let repo_root = resolve_repo_root(options.repo_root.as_deref())?;
    let environment =
        find_service_environment(&repo_root, &options.service_name, &options.environment_name)?
            .ok_or_else(|| {
                anyhow!(
                    "Service environment not found: {}/{}",
                    options.service_name,
                    options.environment_name
                )
            })?;
    let provider = if let Some(from_file) = options.from_file.as_deref() {
        read_json(from_file)?
    } else {
        kubectl_get_lenso_service_provider(&environment, &options.service_name)?
    };
    let observation = operator_service_deployment_observation(
        &repo_root,
        &options.service_name,
        &options.environment_name,
        &environment,
        &provider,
    )?;
    if options.write_state {
        upsert_service_deployment_observation(
            &repo_root.join(SERVICE_DEPLOYMENTS_PATH),
            observation.clone(),
        )?;
    }
    if options.json {
        println!("{}", serde_json::to_string_pretty(&observation)?);
    } else {
        println!("Service deployment: {}/{}", options.service_name, options.environment_name);
        println!("state: {}", observation.get("state").and_then(Value::as_str).unwrap_or("-"));
        println!("drift: {}", observation.get("drift").and_then(Value::as_str).unwrap_or("-"));
        println!("next action: {}", observation.get("nextAction").and_then(Value::as_str).unwrap_or("-"));
    }
    Ok(())
}
```

`operator_service_deployment_observation` must set:

- `target: "operator"`;
- `operator.resource` from metadata name;
- `operator.namespace` from metadata namespace or environment namespace;
- `operator.observedGeneration` from status observedGeneration;
- `operator.conditions` from status conditions;
- `cluster.image` from status observedImage;
- `cluster.releaseId` from status observedReleaseId;
- `cluster.readyReplicas`, `desiredReplicas`, `availableReplicas` from status;
- `checks[0].name = "operator_reconcile"`;
- `nextAction` based on state and drift.

- [ ] **Step 8: Verify CLI changes**

Run:

```sh
cargo test parses_service_deploy_export_operator_target parses_service_deploy_status_operator_source service_deploy_export_operator_writes_provider_cr service_deploy_status_operator_maps_crd_status_to_observation
cargo test
```

Expected:

```text
test result: ok
```

- [ ] **Step 9: Commit**

Run:

```sh
git -C /Users/leosouthey/Projects/framework/lenso-cli add src/main.rs src/module.rs
git -C /Users/leosouthey/Projects/framework/lenso-cli commit -m "feat: add operator service deployment flow"
```

## Task 6: Host Admin Data Operator Observations

**Files:**

- Modify: `/Users/leosouthey/Projects/framework/lenso/crates/platform-admin-data/src/dto.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso/crates/platform-admin-data/src/handlers.rs`
- Modify: `/Users/leosouthey/Projects/framework/lenso/crates/lenso-api/tests/admin_data_console.rs`

- [ ] **Step 1: Add API test for operator observations**

Append to `/Users/leosouthey/Projects/framework/lenso/crates/lenso-api/tests/admin_data_console.rs` near the existing deployment state test:

```rust
#[tokio::test]
async fn service_modules_include_operator_managed_deployment_state() {
    let _guard = ADMIN_DATA_CONSOLE_TEST_LOCK.lock().await;
    let _env = FileFixture::write(".env", "REMOTE_MODULES=support-ticket=grpc://example.com:50051\n");
    let _ledger = FileFixture::remove(".lenso/module-installs.json");
    let _services = FileFixture::remove(".lenso/module-services.json");
    let _release_ledger = FileFixture::write(
        ".lenso/service-releases.json",
        serde_json::json!({
            "version": 1,
            "releases": [{
                "id": "rel_staging",
                "serviceName": "support-suite-provider",
                "appliedAtUnixMs": 200,
                "risk": "safe",
                "environment": {
                    "name": "staging",
                    "target": "operator",
                    "namespace": "lenso-staging",
                    "image": "ghcr.io/acme/support-suite-provider:0.4.0"
                },
                "candidate": { "version": "0.4.0" }
            }]
        })
        .to_string(),
    );
    let _environments = FileFixture::write(
        ".lenso/service-environments.json",
        serde_json::json!({
            "version": 1,
            "environments": [{
                "name": "staging",
                "serviceName": "support-suite-provider",
                "target": "operator",
                "namespace": "lenso-staging",
                "image": "ghcr.io/acme/support-suite-provider:0.4.0",
                "manifestReference": "https://support-staging.example.com/lenso/service/v1/manifest"
            }]
        })
        .to_string(),
    );
    let _deployments = FileFixture::write(
        ".lenso/service-deployments.json",
        serde_json::json!({
            "version": 1,
            "observations": [{
                "serviceName": "support-suite-provider",
                "environment": "staging",
                "target": "operator",
                "observedAtUnixMs": 300,
                "state": "ready",
                "drift": "in_sync",
                "operator": {
                    "resource": "support-suite-provider",
                    "namespace": "lenso-staging",
                    "observedGeneration": 3,
                    "conditions": [{
                        "type": "Ready",
                        "status": "True",
                        "reason": "DeploymentAvailable",
                        "message": "2/2 replicas are ready.",
                        "lastTransitionTime": "2026-06-29T00:00:00Z"
                    }]
                },
                "cluster": {
                    "namespace": "lenso-staging",
                    "deployment": "support-suite-provider",
                    "readyReplicas": 2,
                    "desiredReplicas": 2,
                    "availableReplicas": 2,
                    "image": "ghcr.io/acme/support-suite-provider:0.4.0",
                    "releaseId": "rel_staging"
                },
                "host": {
                    "releaseId": "rel_staging",
                    "candidateVersion": "0.4.0"
                },
                "checks": [{
                    "name": "operator_reconcile",
                    "status": "ok",
                    "detail": "LensoServiceProvider/support-suite-provider is ready"
                }],
                "nextAction": "monitor operator conditions, Remote Calls, and Runtime Story"
            }]
        })
        .to_string(),
    );
    install_admin_module_metadata(vec![]);
    let ctx = AppContext::new(
        AppConfig::from_env(),
        lazy_failing_db(),
        Arc::new(LoggingEventPublisher),
    );
    let app = build_router(ctx);

    let response = app
        .oneshot(admin_get("/admin/data/service-modules"))
        .await
        .expect("service modules request completes");

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let module = &body["modules"][0];
    assert_eq!(module["environments"][0]["target"], "operator");
    assert_eq!(module["deployments"][0]["target"], "operator");
    assert_eq!(module["deployments"][0]["operator"]["resource"], "support-suite-provider");
    assert_eq!(module["deployments"][0]["operator"]["observedGeneration"], 3);
    assert_eq!(module["deployments"][0]["operator"]["conditions"][0]["reason"], "DeploymentAvailable");
    assert_eq!(module["deployments"][0]["cluster"]["availableReplicas"], 2);
}
```

- [ ] **Step 2: Add DTOs**

Modify `/Users/leosouthey/Projects/framework/lenso/crates/platform-admin-data/src/dto.rs`:

```rust
#[derive(Clone, Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminServiceDeploymentObservationDto {
    pub service_name: String,
    pub environment: String,
    pub target: String,
    pub observed_at_unix_ms: Option<u64>,
    pub state: String,
    pub drift: String,
    pub operator: Option<AdminServiceDeploymentOperatorObservationDto>,
    pub cluster: Option<AdminKubernetesDeploymentObservationDto>,
    pub host: Option<AdminServiceDeploymentHostObservationDto>,
    pub checks: Vec<AdminServiceDeploymentCheckDto>,
    pub next_action: Option<String>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminServiceDeploymentOperatorObservationDto {
    pub resource: Option<String>,
    pub namespace: Option<String>,
    pub observed_generation: Option<u64>,
    pub conditions: Vec<AdminServiceDeploymentOperatorConditionDto>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminServiceDeploymentOperatorConditionDto {
    #[serde(rename = "type")]
    pub type_: Option<String>,
    pub status: Option<String>,
    pub reason: Option<String>,
    pub message: Option<String>,
    pub last_transition_time: Option<String>,
}
```

- [ ] **Step 3: Parse operator observations**

Modify `/Users/leosouthey/Projects/framework/lenso/crates/platform-admin-data/src/handlers.rs` imports to include the new DTOs, then update:

```rust
fn service_deployment_from_value(value: &Value) -> Option<AdminServiceDeploymentObservationDto> {
    Some(AdminServiceDeploymentObservationDto {
        service_name: json_string(value, "serviceName")?,
        environment: json_string(value, "environment")?,
        target: json_string(value, "target").unwrap_or_else(|| "kubernetes".to_owned()),
        observed_at_unix_ms: value.get("observedAtUnixMs").and_then(Value::as_u64),
        state: json_string(value, "state").unwrap_or_else(|| "unknown".to_owned()),
        drift: json_string(value, "drift").unwrap_or_else(|| "unknown".to_owned()),
        operator: value.get("operator").map(operator_deployment_from_value),
        cluster: value.get("cluster").map(kubernetes_deployment_from_value),
        host: value.get("host").map(service_deployment_host_from_value),
        checks: value
            .get("checks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(service_deployment_check_from_value)
            .collect(),
        next_action: json_string(value, "nextAction"),
    })
}

fn operator_deployment_from_value(
    value: &Value,
) -> AdminServiceDeploymentOperatorObservationDto {
    AdminServiceDeploymentOperatorObservationDto {
        resource: json_string(value, "resource"),
        namespace: json_string(value, "namespace"),
        observed_generation: value.get("observedGeneration").and_then(Value::as_u64),
        conditions: value
            .get("conditions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(operator_condition_from_value)
            .collect(),
    }
}

fn operator_condition_from_value(value: &Value) -> AdminServiceDeploymentOperatorConditionDto {
    AdminServiceDeploymentOperatorConditionDto {
        type_: json_string(value, "type"),
        status: json_string(value, "status"),
        reason: json_string(value, "reason"),
        message: json_string(value, "message"),
        last_transition_time: json_string(value, "lastTransitionTime"),
    }
}
```

- [ ] **Step 4: Verify host admin data**

Run:

```sh
HTTP_HOST=127.0.0.1 cargo test -p lenso-api --test admin_data_console service_modules_include_operator_managed_deployment_state
HTTP_HOST=127.0.0.1 cargo test -p lenso-api --test admin_data_console service_modules_include_service_environment_and_deployment_state
```

Expected:

```text
test result: ok
```

- [ ] **Step 5: Commit**

Run:

```sh
git -C /Users/leosouthey/Projects/framework/lenso add crates/platform-admin-data/src/dto.rs crates/platform-admin-data/src/handlers.rs crates/lenso-api/tests/admin_data_console.rs
git -C /Users/leosouthey/Projects/framework/lenso commit -m "feat: expose operator deployment observations"
```

## Task 7: Console Operator Managed View

**Files:**

- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/available-modules-model.ts`
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/services-model.ts`
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/services-model.test.ts`
- Modify: `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/services-page.tsx`

- [ ] **Step 1: Add model test**

Append to `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/services-model.test.ts`:

```ts
it("surfaces operator-managed deployment conditions and commands", () => {
  const response = {
    version: 1,
    status: "ready",
    modules: [
      {
        configured: true,
        deploymentDrift: "in_sync",
        deploymentNextAction:
          "monitor operator conditions, Remote Calls, and Runtime Story",
        deployments: [
          {
            serviceName: "support-suite-provider",
            environment: "staging",
            target: "operator",
            observedAtUnixMs: 300,
            state: "ready",
            drift: "in_sync",
            operator: {
              resource: "support-suite-provider",
              namespace: "lenso-staging",
              observedGeneration: 3,
              conditions: [
                {
                  type: "Ready",
                  status: "True",
                  reason: "DeploymentAvailable",
                  message: "2/2 replicas are ready.",
                  lastTransitionTime: "2026-06-29T00:00:00Z",
                },
              ],
            },
            cluster: {
              namespace: "lenso-staging",
              readyReplicas: 2,
              desiredReplicas: 2,
              availableReplicas: 2,
              image: "ghcr.io/acme/support-suite-provider:0.4.0",
            },
          },
        ],
        environments: [
          {
            name: "staging",
            serviceName: "support-suite-provider",
            target: "operator",
            namespace: "lenso-staging",
            image: "ghcr.io/acme/support-suite-provider:0.4.0",
          },
        ],
        fixes: [],
        installed: true,
        loaded: true,
        manifestStatus: "reachable",
        moduleName: "support-ticket",
        providerName: "support-suite-provider",
        restartPending: false,
        services: [],
        status: "ready",
      },
    ],
  } satisfies ServiceModuleLifecycleResponse;

  const [row] = serviceCenterRows(response);

  expect(row?.operatorManaged).toBe(true);
  expect(row?.operatorConditions).toEqual([
    "Ready=True DeploymentAvailable: 2/2 replicas are ready.",
  ]);
  expect(row?.operatorCommands).toContain(
    "lenso service deploy export support-suite-provider --env staging --target operator --output-dir dist/lenso-service/support-suite-provider/operator/staging"
  );
  expect(row?.operatorCommands).toContain(
    "lenso service deploy status support-suite-provider --env staging --source operator --write-state"
  );
});
```

- [ ] **Step 2: Extend deployment observation types**

Modify `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/available-modules-model.ts`:

```ts
export type ServiceDeploymentObservation = {
  serviceName: string;
  environment: string;
  target: string;
  observedAtUnixMs?: number | null;
  state: string;
  drift: string;
  operator?: {
    resource?: string | null;
    namespace?: string | null;
    observedGeneration?: number | null;
    conditions?: Array<{
      type?: string | null;
      status?: string | null;
      reason?: string | null;
      message?: string | null;
      lastTransitionTime?: string | null;
    }>;
  } | null;
  cluster?: {
    namespace?: string | null;
    deployment?: string | null;
    readyReplicas?: number | null;
    desiredReplicas?: number | null;
    availableReplicas?: number | null;
    image?: string | null;
    releaseId?: string | null;
    manifestReference?: string | null;
    serviceEndpoint?: string | null;
    ingressHost?: string | null;
  } | null;
  host?: {
    releaseId?: string | null;
    candidateVersion?: string | null;
  } | null;
  checks?: Array<{
    name: string;
    status: string;
    detail?: string | null;
  }>;
  nextAction?: string | null;
};
```

- [ ] **Step 3: Extend service row model**

Modify `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/services-model.ts`:

```ts
export type ServiceCenterRow = {
  baseUrls: string[];
  compatibilityStates: string[];
  fixes: string[];
  healthChecks: number;
  environments: ServiceEnvironment[];
  deployments: ServiceDeploymentObservation[];
  deploymentDrift?: string | null;
  deploymentNextAction?: string | null;
  operatorManaged: boolean;
  operatorConditions: string[];
  operatorCommands: string[];
  manifestUrls: string[];
  moduleDetails: ServiceCenterModule[];
  providerName: string;
  state: string;
  modules: string[];
  managedServices: string[];
  nextAction: string;
  operations: ServiceOperation[];
  operationsPath: string;
  remoteCallsPath: string;
  latestRelease?: ServiceReleaseRecord | null;
  releaseHistory: ServiceReleaseRecord[];
  runtimePath: string;
  storyPath: string;
};
```

Inside `serviceCenterRows`, compute deployments once and set:

```ts
const deployments = uniqueServiceDeployments(
  modules.flatMap((module) => module.deployments ?? [])
);
```

Then assign:

```ts
deployments,
operatorManaged: deployments.some((deployment) => deployment.target === "operator"),
operatorConditions: operatorConditionLabels(deployments),
operatorCommands: serviceOperatorCommands(providerName, environments),
```

Add helpers:

```ts
function operatorConditionLabels(deployments: ServiceDeploymentObservation[]) {
  return deployments.flatMap((deployment) =>
    (deployment.operator?.conditions ?? []).map((condition) =>
      compactStrings([
        condition.type && condition.status
          ? `${condition.type}=${condition.status}`
          : condition.type ?? condition.status ?? undefined,
        condition.reason ?? undefined,
        condition.message ? `: ${condition.message}` : undefined,
      ])
        .join(" ")
        .replace(" : ", ": ")
    )
  );
}
```

Update `serviceOperatorCommands`:

```ts
function serviceOperatorCommands(
  providerName: string,
  environments: ServiceEnvironment[]
) {
  return environments.flatMap((environment) => {
    const target = environment.target === "operator" ? "operator" : "kubernetes";
    const outputDir = `dist/lenso-service/${providerName}/${target}/${environment.name}`;
    if (target === "operator") {
      return [
        "lenso operator export-crd --output dist/lenso-operator/crds",
        `kubectl apply -k dist/lenso-operator/crds`,
        `lenso service deploy export ${providerName} --env ${environment.name} --target operator --output-dir ${outputDir}`,
        `kubectl apply -k ${outputDir}`,
        `lenso service deploy status ${providerName} --env ${environment.name} --source operator --write-state`,
        `lenso service release rollback ${providerName} --env ${environment.name}`,
      ];
    }
    return [
      `lenso service deploy export ${providerName} --env ${environment.name} --target kubernetes --output-dir ${outputDir}`,
      `kubectl apply -k ${outputDir}`,
      `lenso service deploy status ${providerName} --env ${environment.name} --write-state`,
      `lenso service release rollback ${providerName} --env ${environment.name}`,
    ];
  });
}
```

- [ ] **Step 4: Update services page**

Modify `/Users/leosouthey/Projects/framework/lenso-runtime-console/src/pages/services-page.tsx`.

In the provider header, display `operator managed` only when `row.operatorManaged` is true:

```tsx
<div className="mt-1 flex flex-wrap gap-1">
  <ServiceStateBadge state={row.state} />
  {row.operatorManaged ? (
    <span className="border border-(--line) px-1 py-0.5 text-[10px] uppercase text-(--fg-secondary)">
      operator managed
    </span>
  ) : null}
</div>
```

Rename the rollout section title based on the active deployment:

```tsx
<DetailSection
  title={activeDeployment?.target === "operator" ? "operator rollout" : "kubernetes rollout"}
>
```

Add operator CRD detail section after rollout:

```tsx
{activeDeployment?.target === "operator" ? (
  <DetailSection title="operator conditions">
    <DetailList
      items={nonEmpty(
        [
          activeDeployment.operator?.resource
            ? `resource=${activeDeployment.operator.resource}`
            : undefined,
          activeDeployment.operator?.observedGeneration
            ? `generation=${activeDeployment.operator.observedGeneration}`
            : undefined,
          ...row.operatorConditions,
        ],
        ["-"]
      )}
    />
  </DetailSection>
) : null}
```

- [ ] **Step 5: Verify Console**

Run:

```sh
pnpm check
```

Expected:

```text
Test Files  57 passed
Tests  402 passed
```

The exact file/test count may increase after adding the new test; all checks must pass.

- [ ] **Step 6: Commit**

Run:

```sh
git -C /Users/leosouthey/Projects/framework/lenso-runtime-console add src/pages/available-modules-model.ts src/pages/services-model.ts src/pages/services-model.test.ts src/pages/services-page.tsx
git -C /Users/leosouthey/Projects/framework/lenso-runtime-console commit -m "feat: show operator managed services"
```

## Task 8: Support Ticket Operator Proof

**Files:**

- Create: `/Users/leosouthey/Projects/framework/lenso-examples/examples/support-ticket/kubernetes/operator/staging/lensoserviceprovider.yaml`
- Create: `/Users/leosouthey/Projects/framework/lenso-examples/examples/support-ticket/kubernetes/operator/staging/kustomization.yaml`
- Modify: `/Users/leosouthey/Projects/framework/lenso-examples/examples/support-ticket/README.md`
- Modify: `/Users/leosouthey/Projects/framework/lenso-examples/docs/support-ticket-service-module-run.md`
- Modify: `/Users/leosouthey/Projects/framework/lenso-examples/package.json`

- [ ] **Step 1: Add fixture CR**

Create `/Users/leosouthey/Projects/framework/lenso-examples/examples/support-ticket/kubernetes/operator/staging/lensoserviceprovider.yaml`:

```yaml
apiVersion: lenso.dev/v1alpha1
kind: LensoServiceProvider
metadata:
  name: support-suite-provider
  namespace: lenso-staging
  labels:
    app.kubernetes.io/part-of: lenso
    app.kubernetes.io/component: service-provider
    lenso.dev/service-provider: support-suite-provider
    lenso.dev/environment: staging
spec:
  serviceName: support-suite-provider
  environment: staging
  image: ghcr.io/lenso-dev/support-suite-provider:0.4.0
  releaseId: rel_support_ticket_staging
  manifestReference: https://support-staging.example.com/lenso/service/v1/manifest
  modules:
    - support-ticket
    - support-knowledge-base
    - support-notification
  replicas: 2
  port: 4110
  envFrom:
    configMap: support-suite-provider-config
    secret: support-suite-provider-secrets
  ingress:
    host: support-staging.example.com
  autoscaling:
    enabled: true
    minReplicas: 2
    maxReplicas: 6
    targetCpuUtilization: 70
  disruptionBudget:
    enabled: true
    minAvailable: 1
  networkPolicy:
    enabled: true
```

Create `/Users/leosouthey/Projects/framework/lenso-examples/examples/support-ticket/kubernetes/operator/staging/kustomization.yaml`:

```yaml
resources:
  - lensoserviceprovider.yaml
```

- [ ] **Step 2: Add local fixture validation script**

Modify `/Users/leosouthey/Projects/framework/lenso-examples/package.json` scripts:

```json
{
  "scripts": {
    "check:operator-fixtures": "node scripts/check-operator-fixtures.mjs"
  }
}
```

Create `/Users/leosouthey/Projects/framework/lenso-examples/scripts/check-operator-fixtures.mjs`:

```js
import fs from "node:fs";
import path from "node:path";

const fixture = path.join(
  process.cwd(),
  "examples/support-ticket/kubernetes/operator/staging/lensoserviceprovider.yaml"
);
const contents = fs.readFileSync(fixture, "utf8");
const required = [
  "apiVersion: lenso.dev/v1alpha1",
  "kind: LensoServiceProvider",
  "serviceName: support-suite-provider",
  "environment: staging",
  "modules:",
  "support-ticket",
  "autoscaling:",
  "networkPolicy:",
];

for (const value of required) {
  if (!contents.includes(value)) {
    throw new Error(`${fixture} is missing ${value}`);
  }
}

console.log("operator fixture ok");
```

- [ ] **Step 3: Update support-ticket README**

Append this section to `/Users/leosouthey/Projects/framework/lenso-examples/examples/support-ticket/README.md`:

```markdown
## Kubernetes Operator Path

The V16 path uses the Lenso Operator when Kubernetes should continuously own the service provider process shape.

```sh
lenso operator export-crd --output dist/lenso-operator/crds
kubectl apply -k dist/lenso-operator/crds

lenso service env add staging \
  --service support-suite-provider \
  --target operator \
  --namespace lenso-staging \
  --image ghcr.io/lenso-dev/support-suite-provider:0.4.0 \
  --public-base-url https://support-staging.example.com \
  --manifest-reference https://support-staging.example.com/lenso/service/v1/manifest \
  --config port=4110 \
  --config replicas=2 \
  --config ingressHost=support-staging.example.com \
  --config autoscaling=true \
  --config disruptionBudget=true \
  --config networkPolicy=true

lenso service deploy export support-suite-provider \
  --env staging \
  --target operator \
  --output-dir dist/lenso-service/support-suite-provider/operator/staging

kubectl apply -k dist/lenso-service/support-suite-provider/operator/staging

lenso service deploy status support-suite-provider \
  --env staging \
  --source operator \
  --write-state
```

The Host still reads local Lenso state and runtime evidence. It does not need kubeconfig.
```

- [ ] **Step 4: Update run guide**

In `/Users/leosouthey/Projects/framework/lenso-examples/docs/support-ticket-service-module-run.md`, add an operator section after the raw Kubernetes section:

```markdown
## V16 Operator Managed Delivery

Use this path when the service provider is expected to stay in Kubernetes and be reconciled continuously.

1. Export and apply the operator bundle.
2. Export the provider `LensoServiceProvider`.
3. Apply the provider CR with `kubectl apply -k`.
4. Read CRD status with `lenso service deploy status --source operator --write-state`.
5. Open Runtime Console and inspect Services, Remote Calls, Runtime Story, and Technical Operations.

The example fixture lives at:

```sh
examples/support-ticket/kubernetes/operator/staging/lensoserviceprovider.yaml
```
```

- [ ] **Step 5: Verify examples**

Run:

```sh
pnpm run check:operator-fixtures
pnpm --filter @lenso/example-support-ticket smoke
```

Expected:

```text
operator fixture ok
```

and the existing support-ticket service-level check passes.

- [ ] **Step 6: Commit**

Run:

```sh
git -C /Users/leosouthey/Projects/framework/lenso-examples add examples/support-ticket/kubernetes/operator/staging examples/support-ticket/README.md docs/support-ticket-service-module-run.md package.json scripts/check-operator-fixtures.mjs
git -C /Users/leosouthey/Projects/framework/lenso-examples commit -m "docs: add support ticket operator proof"
```

## Task 9: Site Documentation

**Files:**

- Inspect: `/Users/leosouthey/Projects/framework/lenso-site`
- Modify: current service/Kubernetes docs pages in `/Users/leosouthey/Projects/framework/lenso-site`

- [ ] **Step 1: Find docs entry points**

Run:

```sh
find /Users/leosouthey/Projects/framework/lenso-site -maxdepth 4 -type f | rg "service|kubernetes|module|docs|mdx"
```

Expected: a small set of docs files. Choose the existing service delivery page if present; otherwise add one in the existing docs group.

- [ ] **Step 2: Add operator docs content**

The docs must include this exact conceptual split:

```markdown
## Kubernetes Delivery Modes

Lenso now supports two Kubernetes-facing service delivery modes.

| Mode | Use When | Lenso Owns | Kubernetes Owns |
| --- | --- | --- | --- |
| Raw Kubernetes export | You want reviewable YAML and CI-controlled apply | service/module/release semantics, manifest export, local deployment observations | workload scheduling and rollout status |
| Operator managed | You want a durable desired-state object in the cluster | service/module/release semantics, CR export, status import into Console | reconciliation, workload resources, replica availability |

In both modes, the Host keeps owning auth, capability checks, runtime queues, retries, Outbox, Runtime Story, Remote Calls, and Technical Operations.
```

Add the primary operator commands:

```markdown
```sh
lenso operator export-crd --output dist/lenso-operator/crds
kubectl apply -k dist/lenso-operator/crds

lenso service deploy export support-suite-provider \
  --env staging \
  --target operator \
  --output-dir dist/lenso-service/support-suite-provider/operator/staging

kubectl apply -k dist/lenso-service/support-suite-provider/operator/staging

lenso service deploy status support-suite-provider \
  --env staging \
  --source operator \
  --write-state
```
```

Add boundaries:

```markdown
The operator does not install modules into a Host, write Host runtime tables, consume the Host Outbox, or receive browser bearer tokens. It reconciles service provider process resources only.
```

- [ ] **Step 3: Verify site**

Run:

```sh
pnpm lint
```

Expected:

```text
No lint errors
```

- [ ] **Step 4: Commit**

Run:

```sh
git -C /Users/leosouthey/Projects/framework/lenso-site add .
git -C /Users/leosouthey/Projects/framework/lenso-site commit -m "docs: document lenso operator delivery"
```

## Task 10: End-To-End Local Verification

**Files:**

- All files changed by Tasks 1-9.

- [ ] **Step 1: Verify main Rust workspace**

Run:

```sh
cargo test -p lenso-operator
HTTP_HOST=127.0.0.1 cargo test -p lenso-api --test admin_data_console service_modules_include_operator_managed_deployment_state
```

Expected:

```text
test result: ok
```

- [ ] **Step 2: Verify CLI**

Run:

```sh
cargo test
rm -rf /tmp/lenso-operator-v16 /tmp/lenso-provider-v16
cargo run -- operator export-crd --output /tmp/lenso-operator-v16 --namespace lenso-system --image ghcr.io/acme/lenso-operator:test
find /tmp/lenso-operator-v16 -maxdepth 1 -type f -print | sort
```

Expected includes:

```text
/tmp/lenso-operator-v16/lenso.dev_lensoserviceproviders.yaml
/tmp/lenso-operator-v16/rbac.yaml
/tmp/lenso-operator-v16/deployment.yaml
```

- [ ] **Step 3: Verify Console**

Run:

```sh
pnpm check
```

Expected:

```text
Test Files
Tests
```

All Console tests and checks must pass.

- [ ] **Step 4: Verify examples and site**

Run:

```sh
pnpm run check:operator-fixtures
pnpm --filter @lenso/example-support-ticket smoke
pnpm lint
```

Run these in their respective repositories:

- first two commands in `/Users/leosouthey/Projects/framework/lenso-examples`;
- final command in `/Users/leosouthey/Projects/framework/lenso-site`.

Expected:

```text
operator fixture ok
```

and existing checks pass.

- [ ] **Step 5: Optional live cluster proof**

Run only when `kubectl config current-context` points at a disposable local cluster such as OrbStack or kind:

```sh
kubectl create namespace lenso-system --dry-run=client -o yaml | kubectl apply -f -
kubectl create namespace lenso-staging --dry-run=client -o yaml | kubectl apply -f -
kubectl apply -k /tmp/lenso-operator-v16
kubectl get crd lensoserviceproviders.lenso.dev
kubectl apply -k /Users/leosouthey/Projects/framework/lenso-examples/examples/support-ticket/kubernetes/operator/staging
kubectl get lensoserviceprovider support-suite-provider -n lenso-staging -o yaml
```

Expected:

```text
lensoserviceproviders.lenso.dev
```

If the provider image is not pullable, the CR should still exist and the operator should report a progressing or failed rollout condition instead of silently succeeding.

- [ ] **Step 6: Final status**

Run:

```sh
for repo in lenso lenso-cli lenso-runtime-console lenso-examples lenso-site; do
  git -C /Users/leosouthey/Projects/framework/$repo log --oneline -5
  git -C /Users/leosouthey/Projects/framework/$repo status --short --branch
done
```

Expected:

```text
## feat/operator-core-v16
```

Each repo should have only intentional committed V16 changes.

## Commit Sequence

Use this sequence unless implementation discoveries force a smaller split:

1. `/Users/leosouthey/Projects/framework/lenso`
   - `feat: add lenso service provider crd`
   - `feat: build operator managed resources`
   - `feat: reconcile lenso service providers`
   - `feat: expose operator deployment observations`
2. `/Users/leosouthey/Projects/framework/lenso-cli`
   - `feat: export lenso operator bundle`
   - `feat: add operator service deployment flow`
3. `/Users/leosouthey/Projects/framework/lenso-runtime-console`
   - `feat: show operator managed services`
4. `/Users/leosouthey/Projects/framework/lenso-examples`
   - `docs: add support ticket operator proof`
5. `/Users/leosouthey/Projects/framework/lenso-site`
   - `docs: document lenso operator delivery`

## Self-Review Checklist

- CRD exists and names the product-level concept `LensoServiceProvider`.
- Operator reconciles only provider process resources.
- Host runtime ownership boundaries remain unchanged.
- CLI can export operator install bundle.
- CLI can export provider CR from V15 environment/release state.
- CLI can read CRD status from file or kubectl and persist local observations.
- Host admin API forwards operator observation evidence without kube access.
- Console shows operator managed status, conditions, release/image drift, and commands.
- support-ticket has a committed operator fixture and docs path.
- Site docs explain raw Kubernetes export versus operator-managed delivery.
- Regression checks keep linked modules, service installs, and raw Kubernetes export working.
