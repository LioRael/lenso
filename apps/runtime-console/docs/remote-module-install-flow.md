# Remote Module Install Flow

Use this flow when a third-party module should stay outside the host workspace
but still contribute a Runtime Console frontend package.

## Module Author

Create the standalone remote package:

```sh
pnpm create:module billing --remote --output-dir ../module-packages
```

Expose the remote module protocol from a stable base URL:

```text
GET https://example.com/lenso/module/v1/manifest
```

Publish or otherwise make the console package named in the manifest available
to the host application.

## Host Developer

Install a remote module from the manifest URL:

```sh
lenso module add https://example.com/lenso/module/v1/manifest
```

The marketplace namespace exposes the same low-friction install path:

```sh
lenso module marketplace install https://example.com/lenso/module/v1/manifest
```

If the manifest is read from a local file, pass the runtime base URL:

```sh
lenso module add ./lenso.module.json --base-url https://example.com/lenso/module/v1
```

The install command reads the manifest, derives the remote base URL when the
manifest URL ends in `/manifest`, then writes host-local state:

- `.env`: adds or replaces the module entry in `REMOTE_MODULES`.
- `.lenso/console-package-install-plan.json`: records requested Runtime Console
  packages and their install commands.

Apply the generated console package install plan:

```sh
lenso console-package apply-plan
```

Install package dependencies, then restart the API and worker so
`REMOTE_MODULES` is loaded:

```sh
pnpm --dir apps/runtime-console install
```

Validate the host wiring:

```sh
lenso module doctor
```

`module doctor` checks `REMOTE_MODULES`, the install plan, Runtime Console
dependencies, manifest exports, and module export mappings. Failed checks are
grouped by remote source and console package, with a `fix:` command next to
each issue.

When the host API is running, the Runtime Console can show available modules
from:

```text
GET /admin/data/module-registry/snapshot
```

The Available Modules panel keeps that view lightweight: it shows module name,
version, source, capability count, console package count, and copyable install
commands. It does not require publisher keys, review, history, or bundle
import/export for the default install path.

## Advanced Hardening

Hosts that want a curated or production-controlled flow can still use the
registry, publisher trust, signature, provenance, doctor, history, and bundle
tools:

```sh
lenso module publisher list
lenso module publisher doctor
lenso module publisher trust "Acme Billing" acme-ed25519-2026 --public-key-file ./acme-ed25519.pem
lenso module publisher revoke "Acme Billing" acme-ed25519-2026
lenso module registry add billing --manifest https://example.com/lenso/module/v1/manifest --version 0.1.0
lenso module registry list --registry-file .lenso/module-registry.json
lenso module registry doctor --registry-file .lenso/module-registry.json
lenso module registry inspect billing --registry-file .lenso/module-registry.json
lenso module registry review billing --registry-file .lenso/module-registry.json
lenso module registry install billing --registry-file .lenso/module-registry.json
lenso module registry history
lenso module registry remove billing --reason "replaced by billing-v2"
lenso module registry restore billing --reason "billing-v2 rollback"
lenso module marketplace export
lenso module marketplace import .lenso/marketplace-bundle.json
```

These commands are optional aids. They are not required for a user-driven
marketplace install.

For automation, emit the registry preflight as a machine-readable snapshot:

```sh
lenso module registry doctor --registry-file .lenso/module-registry.json --json
```

The JSON snapshot includes catalog metadata, per-module manifest status,
console package hint counts, grouped issue details, and an overall
`passed`/`failed` status. It does not install packages or mutate host
configuration.

## Smoke Demo

Run the temporary-host smoke demo without mutating the working tree:

```sh
pnpm --dir apps/runtime-console run demo:remote-module-install
pnpm --dir apps/runtime-console run demo:module-registry-install
```

Set `LENSO_KEEP_REMOTE_MODULE_INSTALL_DEMO=1` to keep the generated temp
directory for inspection.
Set `LENSO_KEEP_MODULE_REGISTRY_INSTALL_DEMO=1` to keep the registry demo
directory.

Expected success output ends with:

```text
Module registry doctor passed.
Module doctor passed.
Remote module install demo passed
Module registry install demo passed
```

## Troubleshooting

Run `lenso module registry doctor` before install and `lenso module doctor`
after install. They group failures by the part of the flow that needs attention
and print a `fix:` command next to each issue.

### Catalog

If registry install refuses an untrusted entry, review the catalog and manifest,
then explicitly mark the entry trusted:

```json
{
  "installPolicy": "trusted"
}
```

If registry review cannot find a trusted publisher key, add or update the
host-local publisher key registry:

```text
fix: lenso module publisher trust <publisher> <public-key-id> --public-key-file <pem>
```

If a registry catalog entry is missing a base URL and the manifest reference is
not an HTTP URL ending in `/manifest`, add the runtime module base URL:

```text
fix: add baseUrl or use a manifest URL ending with /manifest
```

This lets registry install write the correct `REMOTE_MODULES` source.

### Manifest

If a catalog entry points at the wrong module manifest or stale version, update
the catalog or manifest before installing:

```text
fix: update catalog name/version or point the entry at the correct manifest
```

### Console package hint

If catalog console package hints drift from the manifest, sync the catalog with
the manifest `console` declarations:

```text
fix: sync consolePackages with the remote manifest console declarations
```

### Remote source

If `REMOTE_MODULES` is missing a module or points at the wrong base URL, add the
module source again:

```text
fix: lenso module add <manifest-url> --base-url <base-url>
```

This updates `.env` and refreshes the local console package install plan.

### Console package

If the Runtime Console dependency is missing, install the package requested by
the module manifest:

```text
fix: pnpm --dir apps/runtime-console add <package-name>
```

Run the host package install afterward so the lockfile matches the registered
frontend package.

### Registry mapping

If manifest exports or module export mappings are missing, re-apply the install
plan:

```text
fix: lenso console-package apply-plan
```

This updates Runtime Console package dependencies, manifest exports, and module
export mappings from `.lenso/console-package-install-plan.json`.
