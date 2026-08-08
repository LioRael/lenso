# Module Console Surfaces

Status: current after the Console ESM and typed Module Operations contract.

A Module may declare Shell-rendered declarative surfaces or executable ESM
surfaces through `ModuleManifest.console`. An executable surface uses
`ConsoleSurfacePresentation::Esm { entry }`; it does not name a bridge protocol,
iframe, proxy endpoint, or package to be installed into the Console.

An executable UI is delivered only as a `ConsoleUiArtifact` inside the same
immutable Module Release. The artifact format is `console_ui_esm` and binds:

- `lenso.console-module.v1` and its protocol major;
- the independent `hostApi` and `consoleUi` compatibility requirements;
- the default entry and every named entry path;
- ordered style assets and their media metadata;
- the generated `ConsoleModuleManifest`, requested permissions, artifact
  digest, and provenance.

The release validator rejects protocol, Module identity, compatibility-range,
entry, style-path, digest, manifest, permission, or provenance drift. Relative
asset paths are normalized and traversal or absolute paths are invalid. Every
ESM surface entry must be present in the artifact entry table, and a release
containing an ESM surface must contain the corresponding artifact.

The Console Shell loads a reviewed `console_ui_esm` artifact in the same realm
from a content-addressed receipt. Module code uses the public typed Console UI
API and the host API exposed by that receipt. The Shell validates the loaded
manifest and surface identity before mounting it; an artifact cannot silently
change its Module identity, compatibility range, or executable entry.

The Console Service does not execute arbitrary Module HTTP routes, data queries,
or generic key/value configuration through the UI artifact. Managed-Service
interaction uses the typed System Plane Module Operations contract,
`lenso.system-plane.module-operations.v1`, with four fixed operations:

- inventory of installed Module Releases and their runtime/ESM metadata;
- resolution of declarative, data-only action contributions against an
  explicitly typed slot context;
- descriptor-bound configuration reads with per-key read capabilities;
- descriptor-bound configuration writes with per-key write capabilities,
  typed validation, target revision binding, and audit evidence.

Every request carries the target Service, environment, calling Module, delegated
actor, authority digest, and capabilities. The target Service verifies the
transport identity, enrollment grant, service principal, and capability subset.
Configuration access is restricted to the calling Module namespace. Sensitive
fields are write-only: their values never appear in inventory, read responses,
or audit evidence; only presence and value digests are exposed.

The contract does not provide an arbitrary endpoint proxy, query language,
cross-Module configuration namespace, secret reader, or generic operation
dispatch. Business behavior remains owned by the business Module and its
declared Service-side implementation.

The deterministic public artifacts are generated from the Rust contract source:

- `contracts/console/lenso.console-module.v1.schema.json`;
- `contracts/console/lenso.console-ui-esm.v1.schema.json`;
- `contracts/console/lenso.console-contract-vectors.v1.json`;
- `contracts/system-plane/lenso.system-plane.module-operations.v1.schema.json`.

The contract vectors include one valid release and negative cases for identity,
protocol, compatibility, paths, entry/style assets, digests, retired Bridge
shapes, and configuration capability rules. The former
`lenso.console-bridge.v1` runtime route and authority seam are retired; old
Bridge or isolated-web release shapes remain recognizable only so validation
can reject them explicitly.
