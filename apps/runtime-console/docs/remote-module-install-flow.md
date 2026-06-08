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

Expected CLI output points at the same short path:

```text
Added remote module billing.
Updated:
- .env
- .lenso/console-package-install-plan.json
Next steps:
- lenso console-package apply-plan
- pnpm --dir apps/runtime-console install
- restart the API and worker
```

Apply the generated console package install plan:

```sh
lenso console-package apply-plan
```

Install package dependencies, then restart the API and worker so
`REMOTE_MODULES` is loaded:

```sh
pnpm --dir apps/runtime-console install
```

When the host API is running, the Runtime Console can show available modules
from:

```text
GET /admin/data/module-registry/snapshot
```

The Available Modules panel keeps that view lightweight: it shows module name,
version, source, capability count, console package count, and copyable install
commands. Installing from a manifest URL writes local module configuration and
the console package install plan.

## Smoke Demo

Run the temporary-host smoke demo without mutating the working tree:

```sh
pnpm --dir apps/runtime-console run demo:remote-module-install
```

Set `LENSO_KEEP_REMOTE_MODULE_INSTALL_DEMO=1` to keep the generated temp
directory for inspection.

Expected success output ends with:

```text
Remote module install demo passed
```

## Troubleshooting

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

### Console registration

If manifest exports or module export mappings are missing, re-apply the install
plan:

```text
fix: lenso console-package apply-plan
```

This updates Runtime Console package dependencies, manifest exports, and module
export mappings from `.lenso/console-package-install-plan.json`.
