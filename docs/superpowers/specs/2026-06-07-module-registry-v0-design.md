# Module Registry V0 Design

## Goal

Add a small local Module Registry lane for discovering and installing remote
modules from a curated catalog, while keeping marketplace trust, signatures,
automatic package installation, and dynamic JavaScript loading deferred.

## Scope

Registry v0 is a developer/operator workflow, not a marketplace. It should:

- Read a catalog JSON file.
- List available remote modules.
- Inspect one module by fetching or reading its manifest and validating that it
  matches the catalog entry.
- Install one module by delegating to the existing `lenso module add` path, so
  `.env` and `.lenso/console-package-install-plan.json` stay the single local
  install handoff.

Registry v0 must not:

- Trust packages automatically.
- Install npm packages automatically.
- Load arbitrary JavaScript from a manifest.
- Add signatures, provenance, ratings, payments, publishing, or marketplace
  governance.

## Catalog Contract

The catalog is a JSON object:

```json
{
  "version": 1,
  "modules": [
    {
      "name": "billing",
      "version": "0.1.0",
      "source": "remote",
      "manifestReference": "https://example.com/lenso/module/v1/manifest",
      "baseUrl": "https://example.com/lenso/module/v1",
      "installPolicy": "trusted",
      "summary": "Billing workspace and operations",
      "capabilities": ["billing.read"],
      "consolePackages": [
        {
          "packageName": "@vendor/lenso-billing-console",
          "exportName": "billingConsoleModule",
          "route": "/data/billing"
        }
      ]
    }
  ]
}
```

Only `version`, `modules`, `name`, `version`, `source`, and
`manifestReference` are required. `source` must be `remote` for v0. `baseUrl`
is optional when the manifest URL ends with `/manifest`, matching the existing
remote install behavior.

`installPolicy` defaults to `review_required`. `registry install` only proceeds
when a curated catalog entry sets `installPolicy` to `trusted`, after the host
developer has reviewed the manifest reference, base URL, capabilities, and
console package hints. This is a local operator gate, not cryptographic
provenance or automatic package trust.

## CLI

The CLI extends the existing module tooling:

```sh
lenso module registry list --registry-file .lenso/module-registry.json
lenso module registry inspect billing --registry-file .lenso/module-registry.json
lenso module registry install billing --registry-file .lenso/module-registry.json
```

`install` calls the same remote module install implementation as
`lenso module add`, preserving `module doctor` as the canonical consistency
check after installation.

## Validation

Focused verification:

```sh
pnpm --dir apps/runtime-console exec vitest run packages/console-package-cli/src/index.test.mjs
just console-check
```
