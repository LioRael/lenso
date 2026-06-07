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
lenso module publisher list
lenso module publisher doctor
lenso module registry add billing \
  --manifest https://example.com/lenso/module/v1/manifest \
  --base-url https://example.com/lenso/module/v1 \
  --version 0.1.0 \
  --capability billing.read \
  --console-package '@vendor/lenso-billing-console#billingConsoleModule' \
  --route /data/billing \
  --publisher "Acme Billing" \
  --source-repository https://github.com/acme/lenso-billing-module \
  --package-url https://packages.example.com/lenso-billing-0.1.0.tgz \
  --checksum sha256:<hex> \
  --signature-url https://packages.example.com/lenso-billing-0.1.0.tgz.sig \
  --public-key-id acme-ed25519-2026
lenso module registry list --registry-file .lenso/module-registry.json
lenso module registry doctor --registry-file .lenso/module-registry.json
lenso module registry inspect billing --registry-file .lenso/module-registry.json
lenso module registry review billing --registry-file .lenso/module-registry.json
```

For automation or a future Runtime Console data source, emit the same registry
preflight as a machine-readable snapshot:

```sh
lenso module registry doctor --registry-file .lenso/module-registry.json --json
```

The JSON snapshot includes catalog metadata, per-module manifest status,
console package hint counts, grouped issue details, and an overall
`passed`/`failed` status. It does not install packages or mutate host
configuration.

When the host API is running, the Runtime Console reads the host-side snapshot
from:

```text
GET /admin/data/module-registry/snapshot
```

The Modules page shows this snapshot in the Available Modules panel. That panel
is a read-only operator aid: it shows loading, error, empty, ready, and issue
states; lets the operator select a module row; and exposes copyable CLI handoff
commands for catalog inspection, preflight, install, apply-plan, and doctor
verification. It does not install modules or mutate host configuration.

Install from the registry when the entry looks right:

```sh
lenso module registry review billing --registry-file .lenso/module-registry.json
lenso module registry install billing --registry-file .lenso/module-registry.json
lenso module registry history
lenso module registry remove billing --reason "replaced by billing-v2"
lenso module registry restore billing --reason "billing-v2 rollback"
lenso module marketplace export
lenso module marketplace import .lenso/marketplace-bundle.json
```

Registry install is deliberately gated. Catalog entries default to
`installPolicy: "review_required"`; set `installPolicy: "trusted"` only after
reviewing the manifest reference, base URL, capabilities, and console package
hints. This is a curated-operator allow bit, not package signing or automatic
trust.
Catalog entries may also declare `compatibility.lenso.minVersion`,
`compatibility.lenso.maxVersion`, and `compatibility.consolePackageApi`.
Registry review blocks incompatible entries before installation.
Trusted catalog entries must include provenance metadata:
`provenance.publisher`, `provenance.sourceRepository`, and
`provenance.checksum`, plus signature metadata:
`provenance.signatureUrl`, `provenance.signatureAlgorithm`, and
`provenance.publicKeyId`. Registry review loads trusted publisher keys from
`.lenso/module-publishers.json`, verifies `ed25519-detached` signatures against
the package artifact, records this snapshot in install history, and blocks
entries that do not name who published the module, what artifact was reviewed,
and which signature policy applies.
Manage the host-local publisher key registry through the CLI:

```sh
lenso module publisher list
lenso module publisher doctor
lenso module publisher trust "Acme Billing" acme-ed25519-2026 --public-key-file ./acme-ed25519.pem
lenso module publisher revoke "Acme Billing" acme-ed25519-2026
```

The install command runs the same review gate and refuses to mutate host files
unless the review decision is `ready_to_install`.

Use `lenso module registry add <module>` to create or update local catalog
entries instead of hand-editing `.lenso/module-registry.json`. The command
defaults to `installPolicy: "review_required"`; pass `--trusted` only after
operator review.
Use `lenso module registry remove <module>` to archive a catalog entry and
record the action in registry history. Pass `--delete` only when the entry
should be physically removed from the catalog.
Use `lenso module registry restore <module>` to restore an archived entry for
review; it returns the entry to `installPolicy: "review_required"` unless
`--trusted` is passed.
Use `lenso module marketplace export` to write
`.lenso/marketplace-bundle.json`, a local bundle containing the registry
catalog, publisher keys, and registry history for handoff or review.
Use `lenso module marketplace import <bundle>` to merge registry entries and
publisher keys into a host. Pass `--include-history` only when the importing
host should also append the source registry history.

Registry install still writes the same host-local source configuration and
console package install plan as `lenso module add`.
It also appends a local audit entry to:

```text
.lenso/module-registry-install-history.json
```

The history records the module name/version, manifest reference, base URL,
install policy, console package hint count, source, action, and install time.
View it from the CLI:

```sh
lenso module registry history
lenso module registry history --json
```

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
