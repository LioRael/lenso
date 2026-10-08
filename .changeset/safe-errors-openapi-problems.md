---
"@lenso/core": patch
"@lenso/engine": minor
"@lenso/cli": patch
"@lenso/manage": minor
"@lenso/mcp": patch
"@lenso/auth": minor
"@lenso/storage": minor
"@lenso/tasks": minor
"@lenso/log": patch
"@lenso/otel": minor
"@lenso/web": minor
---

Add explicit operation-level domain error projection and consistent safe Manage status mappings while preserving internal causes, cleanup aggregation and native oRPC envelopes. Bound public diagnostic fields and validation paths without exposing dynamic input keys or raw exception text.

Provide optional Web OpenAPI endpoints with RFC 9457 Problem Details, shared procedure selection and schema generation, safe late stream errors, and bounded OpenAPILink error decoding. OpenAPI remains an optional peer and adds no routes by default.

Omit raw structured error details in default logs and owned OpenTelemetry exports. Preserve existing SDK ownership, service results, cancellation and resource-drain behavior.
