---
"@lenso/core": minor
"@lenso/engine": minor
"@lenso/manage": minor
---

Expose a borrowed operation runtime and validate explicit management selections without requiring the full application dependency graph. Require every selected plugin to be the exact installed instance.

Provide immutable startup configuration metadata through RunningApp and PluginContext without exposing resolved values, source locations, environment bindings or opaque revisions.

Forward management AbortSignals and check cancellation after confirmation and approval waits and before dispatch. Cancellation does not roll back completed side effects. Preserve original error causes and the current trusted domain-error projection.
