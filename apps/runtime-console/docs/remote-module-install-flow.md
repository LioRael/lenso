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

If your host has a curated module registry catalog, inspect the available
entries before installing:

```sh
lenso module registry list --registry-file .lenso/module-registry.json
lenso module registry doctor --registry-file .lenso/module-registry.json
lenso module registry inspect billing --registry-file .lenso/module-registry.json
```

Install from the registry when the entry looks right:

```sh
lenso module registry install billing --registry-file .lenso/module-registry.json
```

Registry install still writes the same host-local source configuration and
console package install plan as `lenso module add`.

Add the remote module source:

```sh
lenso module add https://example.com/lenso/module/v1/manifest
```

If the manifest is inspected from a local file, pass the runtime base URL:

```sh
lenso module add ./lenso.module.json --base-url https://example.com/lenso/module/v1
```

Apply the generated console package install plan:

```sh
lenso console-package apply-plan
```

Install package dependencies, then validate the host wiring:

```sh
pnpm --dir apps/runtime-console install
lenso module doctor
```

`module doctor` checks `REMOTE_MODULES`, the install plan, Runtime Console
dependencies, manifest exports, and module export mappings. Failed checks are
grouped by remote source, console package, and registry mapping, with a `fix:`
command next to each issue.

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
