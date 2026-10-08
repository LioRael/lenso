---
"@lenso/core": minor
"@lenso/engine": minor
"@lenso/cli": minor
"@lenso/workers": patch
---

Add typed, instance-bound plugin configuration with ordered value, environment and JSON-file sources. Resolve and validate all configuration before business setup, preserve schema output and provide safe source attribution without exposing values or original errors.

Expose static configuration descriptions through Engine and CLI without reading sources. Keep the filesystem adapter separate from runtime-neutral entrypoints and forward Workers request cancellation to startup preflight.
