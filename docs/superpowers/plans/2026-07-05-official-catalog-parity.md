# Official Catalog Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Align the official catalog publication surfaces so `organization` is discoverable from both the host embedded fallback and `catalog.lenso.dev`.

**Architecture:** Keep the official catalog as data, not a new runtime abstraction. Update the host-side test that validates the embedded official catalog, then update the worker catalog and worker tests to serve the same `organization` entry. Defer `audit-log` from the catalog until `lenso-cli` has a `builtin:audit-log` linked descriptor, because the catalog entry would otherwise be discoverable but not installable by name.

**Tech Stack:** Rust `lenso-api` integration tests, JSON official catalog files, TypeScript Cloudflare Worker with Hono, Vitest, pnpm, wrangler dry-run.

## Global Constraints

- Do not restore the old project-local catalog maintenance workflow.
- Named installs such as `lenso module install organization` continue resolving names from `https://catalog.lenso.dev/v1/modules.json` by default, with `--catalog-url` as the override.
- Keep `organization` as a linked official module with `manifestReference: "builtin:organization"`.
- Keep `@lenso/organization-console@0.1.0` as the advertised organization console package.
- Do not add `@lenso/audit-log-console` in this slice.
- Do not add `audit-log` to the official catalog in this slice; record the deferral because `lenso-cli/src/module.rs` has no `builtin:audit-log` descriptor yet.
- Keep unrelated local changes out of staged sets, especially `lenso-runtime-console` dirty `cnfast` changes and `lenso-organization-module` branch divergence.

---

## File Structure

- `lenso/crates/platform-admin-data/catalogs/lenso-official-module-catalog.json`
  - Host embedded official catalog. Current local state already contains the `organization` entry. Only touch this file if the entry is missing in the execution worktree.
- `lenso/crates/lenso-api/tests/admin_data_console.rs`
  - Host-side test for `/admin/data/available-modules` when it falls back to the embedded official catalog.
- `lenso-catalog-worker/catalogs/lenso-official-module-catalog.json`
  - Remote official catalog served by `catalog.lenso.dev`.
- `lenso-catalog-worker/src/app.test.ts`
  - Worker tests for `/v1/modules.json`, `/catalog/modules.json`, `/v1/modules/:name`, and `/healthz`.

No new source files are required.

---

### Task 1: Align Host Embedded Catalog Test

**Files:**
- Modify: `lenso/crates/platform-admin-data/catalogs/lenso-official-module-catalog.json`
- Modify: `lenso/crates/lenso-api/tests/admin_data_console.rs`

**Interfaces:**
- Consumes: existing `/admin/data/available-modules` response generated from `OFFICIAL_MODULE_CATALOG_SOURCE`.
- Produces: host-side proof that the embedded official catalog includes `organization` and still includes `support-ticket`.

- [ ] **Step 1: Verify current lenso repo state**

Run:

```bash
git -C /Users/leosouthey/Projects/framework/lenso status --short --branch
git -C /Users/leosouthey/Projects/framework/lenso log --oneline --decorate -3
```

Expected: branch is ahead of `origin/main` with existing catalog/spec commits, and no unstaged files except changes made during this task.

- [ ] **Step 2: Confirm the stale host test fails before editing it**

Run:

```bash
cd /Users/leosouthey/Projects/framework/lenso
HTTP_HOST=127.0.0.1 cargo test -p lenso-api --test admin_data_console available_modules_reads_official_catalog_when_no_local_catalog_exists -- --nocapture
```

Expected: FAIL because the embedded catalog now has 10 modules while the test still asserts 9 and fixed support-ticket indexes.

- [ ] **Step 3: Ensure the embedded catalog contains the organization entry**

Open `crates/platform-admin-data/catalogs/lenso-official-module-catalog.json`.
If the `organization` entry is missing, insert this object immediately before the `support-ticket` entry:

```json
{
  "name": "organization",
  "version": "0.1.0",
  "source": "linked",
  "manifestReference": "builtin:organization",
  "summary": "Official linked organization, membership, role, and invitation module",
  "capabilities": [
    "organization.read",
    "organization.manage",
    "organization.members.manage",
    "organization.roles.manage",
    "organization.invitations.manage"
  ],
  "dependencies": ["auth"],
  "consolePackages": [
    {
      "packageName": "@lenso/organization-console",
      "exportName": "organizationConsoleModule",
      "bundleUrl": "https://cdn.jsdelivr.net/npm/@lenso/organization-console@0.1.0/dist/organization-console.js",
      "entry": "/console/extensions/organization/organization-console.js",
      "hostApi": "1",
      "route": "/data/organization",
      "requiredCapabilities": ["organization.read"],
      "styles": [
        "https://cdn.jsdelivr.net/npm/@lenso/organization-console@0.1.0/dist/organization-console.css"
      ],
      "version": "0.1.0"
    }
  ]
}
```

- [ ] **Step 4: Replace the brittle fixed-index assertions**

In `crates/lenso-api/tests/admin_data_console.rs`, inside `available_modules_reads_official_catalog_when_no_local_catalog_exists`, replace the block from:

```rust
    assert_eq!(body["catalog"]["modules"], 9);
```

through:

```rust
    assert_eq!(body["modules"][8]["providedBy"], "support-suite-provider");
```

with:

```rust
    assert_eq!(body["catalog"]["modules"], 10);
    let modules = body["modules"]
        .as_array()
        .expect("available modules is an array");

    let auth = modules
        .iter()
        .find(|module| module["name"] == "auth")
        .expect("official catalog includes auth");
    assert_eq!(auth["source"], "linked");
    assert_eq!(auth["catalogVersion"], "0.1.4");
    assert_eq!(auth["consolePackageHints"], 1);

    let auth_oauth = modules
        .iter()
        .find(|module| module["name"] == "auth-oauth")
        .expect("official catalog includes auth-oauth");
    assert_eq!(auth_oauth["source"], "linked");
    assert_eq!(auth_oauth["consolePackageHints"], 1);

    let auth_anonymous = modules
        .iter()
        .find(|module| module["name"] == "auth-anonymous")
        .expect("official catalog includes auth-anonymous");
    assert_eq!(auth_anonymous["source"], "linked");

    let auth_password = modules
        .iter()
        .find(|module| module["name"] == "auth-password")
        .expect("official catalog includes auth-password");
    assert_eq!(auth_password["source"], "linked");

    let auth_device = modules
        .iter()
        .find(|module| module["name"] == "auth-device")
        .expect("official catalog includes auth-device");
    assert_eq!(auth_device["source"], "linked");
    assert_eq!(auth_device["consolePackageHints"], 1);

    let auth_github = modules
        .iter()
        .find(|module| module["name"] == "auth-github")
        .expect("official catalog includes auth-github");
    assert_eq!(auth_github["source"], "linked");
    assert_eq!(auth_github["consolePackageHints"], 1);

    let auth_google = modules
        .iter()
        .find(|module| module["name"] == "auth-google")
        .expect("official catalog includes auth-google");
    assert_eq!(auth_google["source"], "linked");
    assert_eq!(auth_google["consolePackageHints"], 1);

    let auth_oidc = modules
        .iter()
        .find(|module| module["name"] == "auth-oidc")
        .expect("official catalog includes auth-oidc");
    assert_eq!(auth_oidc["source"], "linked");
    assert_eq!(auth_oidc["consolePackageHints"], 1);

    let organization = modules
        .iter()
        .find(|module| module["name"] == "organization")
        .expect("official catalog includes organization");
    assert_eq!(organization["source"], "linked");
    assert_eq!(organization["manifestReference"], "builtin:organization");
    assert_eq!(organization["catalogVersion"], "0.1.0");
    assert_eq!(organization["consolePackageHints"], 1);
    assert_eq!(organization["capabilities"][0], "organization.read");
    assert_eq!(
        organization["summary"],
        "Official linked organization, membership, role, and invitation module"
    );
```

Leave the existing `support_ticket` lookup and assertions below this replacement in place.

- [ ] **Step 5: Run the targeted host test**

Run:

```bash
cd /Users/leosouthey/Projects/framework/lenso
HTTP_HOST=127.0.0.1 cargo test -p lenso-api --test admin_data_console available_modules_reads_official_catalog_when_no_local_catalog_exists -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Commit the host catalog test alignment**

Run:

```bash
cd /Users/leosouthey/Projects/framework/lenso
git diff -- crates/platform-admin-data/catalogs/lenso-official-module-catalog.json crates/lenso-api/tests/admin_data_console.rs
git add crates/platform-admin-data/catalogs/lenso-official-module-catalog.json crates/lenso-api/tests/admin_data_console.rs
git commit -m "test: align official catalog available modules"
```

Expected: commit includes only the embedded catalog file if it was missing `organization`, and the host-side test update.

---

### Task 2: Sync Catalog Worker Data And Tests

**Files:**
- Modify: `lenso-catalog-worker/catalogs/lenso-official-module-catalog.json`
- Modify: `lenso-catalog-worker/src/app.test.ts`

**Interfaces:**
- Consumes: worker `officialCatalog` parsed from `catalogs/lenso-official-module-catalog.json`.
- Produces: remote catalog response containing `organization` with console package metadata.

- [ ] **Step 1: Verify current worker repo state**

Run:

```bash
git -C /Users/leosouthey/Projects/framework/lenso-catalog-worker status --short --branch
```

Expected: `main...origin/main` and no local changes.

- [ ] **Step 2: Add failing worker assertions for organization**

In `src/app.test.ts`, inside `serves the bundled official catalog`, update the `modules: expect.arrayContaining([...])` list to include `organization`:

```ts
      modules: expect.arrayContaining([
        expect.objectContaining({ name: "auth", source: "linked" }),
        expect.objectContaining({ name: "auth-device", source: "linked" }),
        expect.objectContaining({
          name: "organization",
          source: "linked",
          manifestReference: "builtin:organization",
          consolePackages: expect.arrayContaining([
            expect.objectContaining({
              packageName: "@lenso/organization-console",
              exportName: "organizationConsoleModule",
              bundleUrl:
                "https://cdn.jsdelivr.net/npm/@lenso/organization-console@0.1.0/dist/organization-console.js",
              route: "/data/organization",
              requiredCapabilities: ["organization.read"],
              version: "0.1.0",
            }),
          ]),
        }),
        expect.objectContaining({ name: "support-ticket", source: "service" }),
      ]),
```

In the `serves a single module by name` test, change the request from `auth-password` to `organization` and replace the expected module object:

```ts
  it("serves a single module by name", async () => {
    const response = await app.request("/v1/modules/organization");

    expect(response.status).toBe(200);
    expect(await readJson(response)).toMatchObject({
      catalogVersion: 1,
      module: {
        capabilities: [
          "organization.read",
          "organization.manage",
          "organization.members.manage",
          "organization.roles.manage",
          "organization.invitations.manage",
        ],
        dependencies: ["auth"],
        manifestReference: "builtin:organization",
        name: "organization",
        source: "linked",
      },
    });
  });
```

In the `reports health without caching it` test, update the expected module count:

```ts
      modules: 10,
```

- [ ] **Step 3: Run worker tests to verify the new assertions fail**

Run:

```bash
cd /Users/leosouthey/Projects/framework/lenso-catalog-worker
pnpm test
```

Expected: FAIL because `organization` is not present in the worker catalog yet and health still reports 9 modules.

- [ ] **Step 4: Add organization to the worker catalog**

In `catalogs/lenso-official-module-catalog.json`, insert this object immediately before the `support-ticket` entry:

```json
{
  "name": "organization",
  "version": "0.1.0",
  "source": "linked",
  "manifestReference": "builtin:organization",
  "summary": "Official linked organization, membership, role, and invitation module",
  "capabilities": [
    "organization.read",
    "organization.manage",
    "organization.members.manage",
    "organization.roles.manage",
    "organization.invitations.manage"
  ],
  "dependencies": ["auth"],
  "consolePackages": [
    {
      "packageName": "@lenso/organization-console",
      "exportName": "organizationConsoleModule",
      "bundleUrl": "https://cdn.jsdelivr.net/npm/@lenso/organization-console@0.1.0/dist/organization-console.js",
      "entry": "/console/extensions/organization/organization-console.js",
      "hostApi": "1",
      "route": "/data/organization",
      "requiredCapabilities": ["organization.read"],
      "styles": [
        "https://cdn.jsdelivr.net/npm/@lenso/organization-console@0.1.0/dist/organization-console.css"
      ],
      "version": "0.1.0"
    }
  ]
}
```

- [ ] **Step 5: Run worker validation**

Run:

```bash
cd /Users/leosouthey/Projects/framework/lenso-catalog-worker
pnpm typecheck
pnpm test
pnpm deploy:dry-run
```

Expected: all three commands PASS. `pnpm deploy:dry-run` writes `dist/worker-dry-run`.

- [ ] **Step 6: Commit the worker catalog parity change**

Run:

```bash
cd /Users/leosouthey/Projects/framework/lenso-catalog-worker
git add catalogs/lenso-official-module-catalog.json src/app.test.ts
git commit -m "chore: add organization to official catalog"
```

Expected: commit includes only the worker catalog JSON and worker tests.

---

### Task 3: Verify Deferred Audit-Log Decision And PR Readiness

**Files:**
- Read: `lenso-cli/src/module.rs`
- Read: `lenso-audit-log-module/crates/audit-log/Cargo.toml`
- Read: `lenso/docs/superpowers/specs/2026-07-05-official-catalog-parity-design.md`

**Interfaces:**
- Consumes: current CLI built-in linked descriptor list.
- Produces: explicit evidence that `audit-log` stays out of this catalog parity PR.

- [ ] **Step 1: Verify audit-log is published**

Run:

```bash
cargo search lenso-module-audit-log --limit 3
```

Expected output includes:

```text
lenso-module-audit-log = "0.1.0"
```

- [ ] **Step 2: Verify current CLI does not know builtin audit-log**

Run:

```bash
rg -n '"audit-log"|lenso-module-audit-log|audit_log::module::linked_module' /Users/leosouthey/Projects/framework/lenso-cli/src/module.rs
```

Expected: no matches in `builtin_linked_module_descriptor` or `builtin_linked_module_names`.

- [ ] **Step 3: Record the implementation decision in the PR body**

Use this exact note in the relevant PR body or merge summary:

```markdown
Audit-log is intentionally deferred from the official catalog in this parity slice. The crate is published as `lenso-module-audit-log@0.1.0`, but `lenso-cli` does not yet have a `builtin:audit-log` linked descriptor, so adding a catalog entry now would make the module discoverable before named install can succeed.
```

- [ ] **Step 4: Check repo cleanliness before PRs**

Run:

```bash
for repo in lenso lenso-catalog-worker lenso-cli lenso-runtime-console lenso-organization-module lenso-audit-log-module; do
  printf '\n[%s]\n' "$repo"
  git -C "/Users/leosouthey/Projects/framework/$repo" status --short --branch
done
```

Expected:

- `lenso` has only committed branch changes for catalog parity docs, embedded catalog, and test alignment.
- `lenso-catalog-worker` has only the committed worker catalog parity change.
- `lenso-cli` has no changes for this slice.
- `lenso-runtime-console` may still show unrelated local dirty `cnfast` changes; do not stage or commit them.
- `lenso-organization-module` may still show branch divergence; do not stage or commit it for this slice.
- `lenso-audit-log-module` has no changes for this slice.

---

### Task 4: Open PRs, Merge, Deploy, And Verify

**Files:**
- No source file changes.

**Interfaces:**
- Consumes: committed changes from Task 1 and Task 2.
- Produces: merged official catalog parity and deployed worker catalog.

- [ ] **Step 1: Create lenso PR branch**

Run:

```bash
cd /Users/leosouthey/Projects/framework/lenso
git branch --show-current
git checkout -b codex/official-catalog-parity-lenso
git status --short
```

Expected: branch is `codex/official-catalog-parity-lenso`; status is clean.

- [ ] **Step 2: Push lenso branch and open PR**

Run:

```bash
cd /Users/leosouthey/Projects/framework/lenso
git push -u origin codex/official-catalog-parity-lenso
gh pr create \
  --title "Align official catalog available modules" \
  --body "Updates the host embedded official catalog test for the organization module and records the official catalog parity design/plan. Audit-log is intentionally deferred from the official catalog in this parity slice. The crate is published as \`lenso-module-audit-log@0.1.0\`, but \`lenso-cli\` does not yet have a \`builtin:audit-log\` linked descriptor, so adding a catalog entry now would make the module discoverable before named install can succeed."
```

Expected: PR URL is printed.

- [ ] **Step 3: Create worker PR branch**

Run:

```bash
cd /Users/leosouthey/Projects/framework/lenso-catalog-worker
git branch --show-current
git checkout -b codex/official-catalog-parity-worker
git status --short
```

Expected: branch is `codex/official-catalog-parity-worker`; status is clean.

- [ ] **Step 4: Push worker branch and open PR**

Run:

```bash
cd /Users/leosouthey/Projects/framework/lenso-catalog-worker
git push -u origin codex/official-catalog-parity-worker
gh pr create \
  --title "Add organization to official catalog worker" \
  --body "Adds the organization linked module and @lenso/organization-console metadata to the remote official catalog. Keeps audit-log deferred until lenso-cli has a builtin audit-log linked descriptor."
```

Expected: PR URL is printed.

- [ ] **Step 5: Watch PR checks**

Run for each PR:

```bash
gh pr checks --watch
```

Expected: required checks pass. If `gh pr checks --watch` stalls, run:

```bash
gh run list --limit 5
run_id="$(gh run list --limit 1 --json databaseId --jq '.[0].databaseId')"
gh run view "$run_id" --json status,conclusion,jobs,url
```

Expected: the relevant workflow concludes with `success`.

- [ ] **Step 6: Merge PRs**

Run in each repo after checks pass:

```bash
gh pr merge --squash --delete-branch
git checkout main
git pull --ff-only
```

Expected: PRs merge, local `main` fast-forwards, and PR branches are deleted from origin.

- [ ] **Step 7: Deploy catalog worker**

Run:

```bash
cd /Users/leosouthey/Projects/framework/lenso-catalog-worker
pnpm deploy
```

Expected: wrangler deploy completes successfully and prints the deployed worker URL or version.

- [ ] **Step 8: Verify worker catalog locally and remotely**

Run local post-merge checks:

```bash
cd /Users/leosouthey/Projects/framework/lenso-catalog-worker
pnpm check
```

Expected: PASS.

Run remote verification:

```bash
curl -fsSL https://catalog.lenso.dev/v1/modules.json | jq '.modules[] | select(.name == "organization")'
```

Expected output includes:

```json
{
  "name": "organization",
  "version": "0.1.0",
  "source": "linked",
  "manifestReference": "builtin:organization"
}
```

If this environment returns HTTP 403 for the public endpoint, verify through the deployed worker surface available from `wrangler deploy` output and keep the local `pnpm check` result as the test proof.
