# Capability generation in a source App

Use this branch when the selected CLI supplies local App authoring. Inspect
Engine `crates/lenso-engine-app/src/app/contracts.rs` and the CLI's
`docs/capability-authoring.md` before relying on its commands. This path is a
local implementation; check the selected binary rather than assuming release.

`lenso app contract new example.text` creates a Descriptor/JSON Schema package
in `contracts/` with a TypeScript projection and no Cargo requirement.
`--source rust` creates Rust contract source and a regular Cargo package;
Rust authoring needs Cargo. One Capability still has one authoritative source.

`app build` and `app dev` synchronize App-owned contracts, selected Plugin
contract declarations and local Cargo dependency closures before Plugin builds.
Unadopted shared candidates do not trigger generation. Production startup runs
no generator. Generated outputs stay within their owning package and must not
overwrite authored files or create watcher regeneration loops.

Normal package metadata declares Descriptor, projection and output paths.
Rust source extraction uses the package's resolved build-dependencies; consumer
build scripts retain freshness checks outside the App workflow. Keep Operation
schemas, compatibility and failure semantics in the existing Capability workflow.
A convention does not choose providers or add authorization.

Prove one schema/source edit regenerates the selected projections, both consumer
and provider compile, freshness catches stale output outside the App workflow,
and one real invocation observes the change.
