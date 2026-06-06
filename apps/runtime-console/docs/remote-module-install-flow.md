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
