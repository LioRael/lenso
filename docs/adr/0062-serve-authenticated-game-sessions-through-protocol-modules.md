# ADR 0062: Serve Authenticated Game Sessions through Protocol Modules

- Status: accepted
- Date: 2026-08-22
- Extends: ADR 0033, ADR 0038, ADR 0039, ADR 0047, ADR 0052, ADR 0054,
  ADR 0056, ADR 0059
- Implements: #598

## Context

An App may need to expose a long-lived authenticated session over a native
protocol that is not HTTP. The portable Kernel already owns the typed
bidirectional Stream Operation, but it must not acquire a socket listener,
wire framing, credential syntax, or game authorization policy. Those concerns
belong at Module boundaries selected by Composition.

The implementation must make malformed input, authentication rejection,
authorization denial, bounded resource exhaustion, deadlines, cancellation,
provider restart, and managed shutdown observable outcomes. A client fixture
also needs a public network seam that proves the complete session rather than
only exercising an in-memory stream.

## Decision

The example App composes three replaceable native Modules:

- A protocol Module owns a loopback TCP listener, framing, connection limits,
  idle/session deadlines, transport errors, and socket teardown.
- An Auth Module consumes protocol-selected `CredentialEvidence` and returns
  an `ActorAssertion` through the Auth Capability.
- A game-session Module owns `PlayerActor` projection, room authorization, and
  the final `GameSession` Stream provider. The provider can be replaced by the
  fixture's alternate package identity selected in Composition without
  changing Kernel or the protocol Module.

Both the protocol and game-session fixture Modules have alternate package
identities selected only by App Composition. Target Modules receive an
`ActorAssertionVerifier`, not the Auth Module's assertion-signing authority.

The documented fixture wire is a four-byte unsigned big-endian payload length
followed by one UTF-8 JSON object. Client frames are `hello`, `message`,
`close_send`, and `cancel`. The first frame is `hello` with an optional
`game-bearer` token, a room, and an optional relative deadline. Server frames
are `ready`, `message`, `peer_half_closed`, `terminal`, `rejected`, and
`runtime`. The length limit is checked before allocation and before every
outbound write.

The protocol Module creates a Kernel Invocation Context through its explicit
Module dependencies, calls Auth with selected credential evidence, attaches
the verified assertion, and opens the typed game-session Stream with the same
deadline and cancellation token. It never interprets the assertion as a game
actor; the game provider performs the target-owned `PlayerActor` projection
and final authorization.

Every accepted connection is a managed generation task with bounded active
connection admission. Read and receive waits are limited by the configured
idle timeout and the session deadline. The connection bridge drives socket
input and provider output concurrently, preserves pending work in either
direction, and does not impose one-response-per-message ordering. Stream
cancellation is idempotent and does not replay frames. Provider Runtime Failure
causes the Kernel's declared finite restart policy to take effect; existing
sessions terminate with a bounded Runtime outcome while later connections
resolve the new generation. App shutdown cancels the managed listener and
connection tasks and closes active sockets without adding protocol behavior to
Kernel.

## Consequences

- Native protocol Modules can expose non-HTTP sessions while the Kernel stays
  transport-neutral and portable.
- Credential extraction, assertion verification, actor projection, and game
  authorization remain visible at separate seams with explicit ownership.
- The length-prefixed JSON fixture is intentionally a documented example wire,
  not a universal Kernel wire or a replacement for the Bun Adapter's selected
  JSON-RPC transport.
- Connection limits and timeouts produce bounded outcomes at the protocol
  edge; business Domain Errors remain typed by the game Capability.

## Removal test

Removing the game-session Capability, protocol/Auth/game fixture Modules, Bun
client fixture, and this ADR leaves the portable Kernel Stream and Auth seams,
their generated contract machinery, and the existing Bun Adapter wires
unchanged. No socket, framing, or game-specific policy remains in Kernel.
