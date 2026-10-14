---
"@lenso/mcp": minor
---

Replace borrowed and HTTP MCP entry-local `operation_N` tool names with stable
`operation_` names derived from the exact plugin ID and method tuple. This is a
pre-1.0 protocol naming change: clients must rediscover tools rather than reuse
cached positional names.

Add permission-bound, single-use cursor pagination with fresh visibility checks,
bounded redacted `structuredContent` alongside text results, and an explicit
personal-token HTTP mode that verifies current credentials on every protected
request without advertising an OAuth authorization server.
