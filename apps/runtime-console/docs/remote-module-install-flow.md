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
```

Set `LENSO_KEEP_REMOTE_MODULE_INSTALL_DEMO=1` to keep the generated temp
directory for inspection.

Expected success output ends with:

```text
Module doctor passed.
Remote module install demo passed
```

## Troubleshooting

Run `lenso module doctor` first. It groups failures by the part of the install
flow that needs attention and prints a `fix:` command next to each issue.

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
