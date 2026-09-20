import { createHash, createHmac, timingSafeEqual } from "node:crypto";
import { closeSync, readSync, writeSync } from "node:fs";
import type { InvocationContext } from "@lenso/contract-runtime";
import {
  CANCEL_METHOD,
  HANDSHAKE_METHOD,
  PROCESS_PROFILE,
  REQUEST_METHOD,
  SHUTDOWN_METHOD,
  childProofMessage,
  decodeBase64Url32,
  encodeBase64Url,
  handshakeProofPayload,
  hostProofMessage,
  parseStrictJson,
  validateCancelParams,
  validateHandshakeParams,
  validateJsonRpcRequest,
  validateRequestParams,
  validateShutdownParams,
  type CancelParams,
  type HandshakeIdentity,
  type HandshakeParams,
  type JsonRpcRequest,
  type ProcessOutcome,
  type ReadinessRecord,
  type RequestParams,
  type ShutdownParams,
} from "@lenso/process-protocol";
import type {
  CapabilityProviderBinding,
  CapabilityProviderDescriptor,
  ProviderDispatchOutcome,
} from "./index.js";

const JSON_CONTENT_TYPE = "application/json";

/** A Process V1 provider adds the exact wire identity and generated Schema checks. */
export type ProcessV1ProviderDescriptor = Omit<
  CapabilityProviderDescriptor,
  "descriptor_digest"
> & {
  readonly descriptor_digest: string;
};

/**
 * Process V1 accepts only request Providers with exact descriptor identity and
 * generated value validation. The Host owns protocol fields; generated code
 * owns the Capability payload schema.
 */
export type ProcessV1ProviderBinding<Instance = unknown> = Omit<
  CapabilityProviderBinding<Instance>,
  "descriptor"
> & {
  readonly descriptor: ProcessV1ProviderDescriptor;
  validateRequest(operation: string, payload: unknown): void;
  validateSuccess(operation: string, payload: unknown): void;
  validateDomainError(operation: string, payload: unknown): void;
};

export interface ProcessV1Options<Instance = unknown> {
  readonly identity?: HandshakeIdentity;
  readonly bootstrapSecret: Uint8Array;
  readonly providers: readonly ProcessV1ProviderBinding<Instance>[];
  /** The already-created Plugin Instance passed to generated Provider bindings. */
  readonly instance: Instance;
}

export interface ProcessV1Server {
  readonly readiness: ReadinessRecord;
  readonly readinessLine: string;
  stop(closeActiveConnections?: boolean): void;
}

const BOOTSTRAP_FD = 3;
const READINESS_FD = 4;

/** Reads the one-use secret from fd 3 and writes readiness once to fd 4. */
export function serveProcessV1<Instance>(
  providers: readonly ProcessV1ProviderBinding<Instance>[],
  instance: Instance,
): ProcessV1Server {
  const secret = readBootstrapSecret();
  const server = startProcessV1({ bootstrapSecret: secret, providers, instance });
  secret.fill(0);
  try {
    const encoded = new TextEncoder().encode(server.readinessLine);
    let offset = 0;
    while (offset < encoded.length) {
      offset += writeSync(READINESS_FD, encoded, offset, encoded.length - offset, null);
    }
  } finally {
    closeSync(READINESS_FD);
  }
  return server;
}

interface ActiveRequest {
  cancelled: boolean;
  cancelObserved: Promise<void>;
  observeCancel(): void;
}

interface JsonRpcEnvelope {
  readonly jsonrpc: "2.0";
  readonly id: string;
  readonly method: string;
  readonly params: unknown;
}

/** Starts distinct loopback data and reserved control listeners for Process V1. */
export function startProcessV1<Instance>(options: ProcessV1Options<Instance>): ProcessV1Server {
  if (options.bootstrapSecret.length !== 32) {
    throw new Error("Process V1 bootstrap secret must contain exactly 32 bytes");
  }
  const secret = new Uint8Array(options.bootstrapSecret);
  const providers = collectProviders(options.providers);
  const active = new Map<string, ActiveRequest>();
  const retired = new Set<string>();
  let accepting = true;
  let handshakeAttempted = false;
  let session: string | undefined;
  let admittedIdentity: HandshakeIdentity | undefined;
  let controlInFlight = 0;
  let runningDataRequests = 0;
  const dataWaiters: Array<() => void> = [];

  const control = Bun.serve({
    hostname: "127.0.0.1",
    port: 0,
    async fetch(request) {
      const controlCapacity = admittedIdentity?.peer_limits.control_queue_capacity ?? 32;
      if (controlInFlight >= controlCapacity) {
        return new Response("control queue full", { status: 503 });
      }
      controlInFlight += 1;
      try {
      const rejected = rejectHttpShape(request, "/control");
      if (rejected) return rejected;
      const parsed = await parseEnvelope(
        request,
        admittedIdentity?.peer_limits.max_control_http_body_bytes ?? 16_384,
      );
      if (parsed instanceof Response) return parsed;
      const { envelope } = parsed;
      try {
        if (envelope.method === HANDSHAKE_METHOD) {
          if (handshakeAttempted) return rpcError(envelope.id, -32602, "handshake already attempted");
          handshakeAttempted = true;
          try {
            validateJsonRpcRequest(envelope, HANDSHAKE_METHOD, validateHandshakeParams);
            const params = envelope.params as HandshakeParams;
            if (options.identity && !sameIdentity(params.identity, options.identity)) {
              return rpcError(envelope.id, -32602, "handshake identity mismatch");
            }
            validateProviderIdentity(params.identity, providers);
            const payload = handshakeProofPayload(params);
            const digest = new Uint8Array(createHash("sha256").update(payload).digest());
            const expected = hmac(secret, hostProofMessage(digest));
            const actual = decodeBase64Url32(params.host_proof, "host_proof");
            if (!timingSafeEqual(expected, actual)) {
              return rpcError(envelope.id, -32602, "host proof mismatch");
            }
            const sessionBytes = crypto.getRandomValues(new Uint8Array(32));
            session = encodeBase64Url(sessionBytes);
            admittedIdentity = params.identity;
            return rpcResult(envelope.id, {
              identity: params.identity,
              session,
              child_proof: encodeBase64Url(hmac(secret, childProofMessage(digest, session))),
            });
          } finally {
            secret.fill(0);
          }
        }
        if (envelope.method === CANCEL_METHOD) {
          validateJsonRpcRequest(envelope, CANCEL_METHOD, validateCancelParams);
          const params = envelope.params as CancelParams;
          requireSession(params.session, session);
          const current = active.get(params.correlation_id);
          if (current) {
            current.cancelled = true;
            current.observeCancel();
          }
          return rpcResult(envelope.id, { session: params.session, accepted: true });
        }
        if (envelope.method === SHUTDOWN_METHOD) {
          validateJsonRpcRequest(envelope, SHUTDOWN_METHOD, validateShutdownParams);
          const params = envelope.params as ShutdownParams;
          requireSession(params.session, session);
          accepting = false;
          for (const state of active.values()) {
            state.cancelled = true;
            state.observeCancel();
          }
          setTimeout(() => {
            data.stop(false);
            control.stop(false);
          }, 0);
          return rpcResult(envelope.id, { session: params.session, accepted: true });
        }
        return rpcError(envelope.id, -32601, "Method not found");
      } catch {
        return rpcError(envelope.id, -32602, "Invalid params");
      }
      } finally {
        // Parsing/HTTP-shape early returns also release the bounded control slot.
        if (controlInFlight > 0) controlInFlight -= 1;
      }
    },
  });

  const data = Bun.serve({
    hostname: "127.0.0.1",
    port: 0,
    async fetch(request) {
      const rejected = rejectHttpShape(request, "/rpc");
      if (rejected) return rejected;
      const parsed = await parseEnvelope(
        request,
        admittedIdentity?.peer_limits.max_http_body_bytes ?? 65_536,
      );
      if (parsed instanceof Response) return parsed;
      const { envelope } = parsed;
      if (envelope.method !== REQUEST_METHOD) {
        return rpcError(envelope.id, -32601, "Method not found");
      }
      try {
        validateJsonRpcRequest(envelope, REQUEST_METHOD, validateRequestParams);
      } catch {
        return rpcError(envelope.id, -32602, "Invalid params");
      }
      const params = envelope.params as RequestParams;
      if (!accepting || session === undefined || admittedIdentity === undefined) {
        return rpcError(envelope.id, -32602, "request before handshake or after shutdown");
      }
      if (params.session !== session || envelope.id !== params.correlation_id) {
        return rpcError(envelope.id, -32602, "request identity mismatch");
      }
      if (active.has(params.correlation_id) || retired.has(params.correlation_id)) {
        accepting = false;
        return rpcError(envelope.id, -32602, "duplicate or retired correlation ID");
      }
      if (retired.size >= admittedIdentity.peer_limits.max_retired_correlation_ids) {
        accepting = false;
        return rpcError(envelope.id, -32602, "retired correlation ID capacity reached");
      }
      if (active.size >= admittedIdentity.peer_limits.max_concurrent_requests) {
        if (dataWaiters.length >= admittedIdentity.peer_limits.child_request_queue_capacity) {
          return requestResult(params, {
            kind: "runtime",
            failure: { kind: "resource_exhausted", operation: params.operation },
          }, admittedIdentity.peer_limits.max_http_body_bytes);
        }
      }
      const provider = providers.get(params.capability_id);
      const admittedDescriptor = admittedIdentity.provided_capabilities.find(
        (descriptor) => descriptor.capability_id === params.capability_id,
      );
      if (
        !provider || !admittedDescriptor ||
        params.descriptor_version !== admittedDescriptor.descriptor_version ||
        params.descriptor_digest !== admittedDescriptor.descriptor_digest ||
        !provider.descriptor.operations.includes(params.operation)
      ) {
        accepting = false;
        return rpcError(envelope.id, -32602, "operation is absent from admitted identity");
      }
      const cancellation = cancellationState();
      active.set(params.correlation_id, cancellation);
      const timeout = startChildTimer(params.remaining_timeout_nanos, cancellation);
      const acquired = await acquireDataSlot(
        admittedIdentity.peer_limits.max_concurrent_requests,
        admittedIdentity.peer_limits.child_request_queue_capacity,
        () => runningDataRequests,
        (value) => { runningDataRequests = value; },
        dataWaiters,
      );
      if (!acquired) {
        if (timeout !== undefined) clearTimeout(timeout);
        active.delete(params.correlation_id);
        retired.add(params.correlation_id);
        return requestResult(params, {
          kind: "runtime",
          failure: { kind: "resource_exhausted", operation: params.operation },
        }, admittedIdentity.peer_limits.max_http_body_bytes);
      }
      try {
        if (cancellation.cancelled) {
          return requestResult(params, {
            kind: "runtime",
            failure: { kind: "resource_exhausted", operation: params.operation },
          }, admittedIdentity.peer_limits.max_http_body_bytes);
        }
        provider.validateRequest?.(params.operation, params.payload);
        const outcome = await provider.invokeRequest(
          params.operation,
          invocationContext(params, cancellation),
          params.payload,
          options.instance,
        );
        if (cancellation.cancelled) await cancellation.cancelObserved;
        validateProviderOutcome(provider, params.operation, outcome);
        return requestResult(
          params,
          toProcessOutcome(outcome, params.operation),
          admittedIdentity.peer_limits.max_http_body_bytes,
        );
      } catch (error) {
        accepting = false;
        return requestResult(params, {
          kind: "runtime",
          failure: { kind: "plugin_failure", detail: boundedError(error) },
        }, admittedIdentity.peer_limits.max_http_body_bytes);
      } finally {
        if (timeout !== undefined) clearTimeout(timeout);
        active.delete(params.correlation_id);
        retired.add(params.correlation_id);
        releaseDataSlot(
          () => runningDataRequests,
          (value) => { runningDataRequests = value; },
          dataWaiters,
        );
      }
    },
  });

  if (data.port === undefined || control.port === undefined || data.port === control.port) {
    data.stop(true);
    control.stop(true);
    throw new Error("Process V1 could not bind distinct loopback listeners");
  }
  const readiness: ReadinessRecord = {
    protocol: PROCESS_PROFILE,
    data_port: data.port,
    control_port: control.port,
  };
  return {
    readiness,
    readinessLine: `${JSON.stringify(readiness)}\n`,
    stop(closeActiveConnections = true) {
      accepting = false;
      for (const state of active.values()) {
        state.cancelled = true;
        state.observeCancel();
      }
      data.stop(closeActiveConnections);
      control.stop(closeActiveConnections);
    },
  };
}

function collectProviders<Instance>(
  bindings: readonly ProcessV1ProviderBinding<Instance>[],
): Map<string, ProcessV1ProviderBinding<Instance>> {
  const providers = new Map<string, ProcessV1ProviderBinding<Instance>>();
  for (const provider of bindings) {
    if (
      provider.descriptor.stream_operations.length > 0 ||
      provider.descriptor.event_operations.length > 0 ||
      providers.has(provider.descriptor.capability_id)
    ) {
      throw new Error("Process V1 provider table is invalid");
    }
    providers.set(provider.descriptor.capability_id, provider);
  }
  if (providers.size === 0) throw new Error("Process V1 needs at least one provider");
  return providers;
}

function validateProviderIdentity<Instance>(
  identity: HandshakeIdentity,
  providers: ReadonlyMap<string, ProcessV1ProviderBinding<Instance>>,
): void {
  const expected = new Map(
    identity.provided_capabilities.map((capability) => [
      capability.capability_id,
      capability,
    ]),
  );
  for (const provider of providers.values()) {
    const descriptor = expected.get(provider.descriptor.capability_id);
    const operations = descriptor?.operations.map(({ operation }) => operation);
    if (
      descriptor === undefined ||
      descriptor.descriptor_version !== provider.descriptor.descriptor_version ||
      descriptor.descriptor_digest !== provider.descriptor.descriptor_digest ||
      JSON.stringify(operations) !== JSON.stringify(provider.descriptor.operations) ||
      provider.descriptor.stream_operations.length > 0 ||
      provider.descriptor.event_operations.length > 0
    ) {
      throw new Error("Process V1 provider table does not match the admitted identity");
    }
  }
  if (providers.size !== expected.size) {
    throw new Error("Process V1 provider table is incomplete");
  }
}

function rejectHttpShape(request: Request, path: string): Response | undefined {
  const url = new URL(request.url);
  if (url.pathname !== path || url.search !== "") return new Response("not found", { status: 404 });
  if (request.method !== "POST") return new Response("method not allowed", { status: 405 });
  if (request.headers.get("content-type")?.toLowerCase() !== JSON_CONTENT_TYPE) {
    return new Response("unsupported media type", { status: 415 });
  }
  return undefined;
}

async function parseEnvelope(
  request: Request,
  maximumBytes: number,
): Promise<{ envelope: JsonRpcEnvelope } | Response> {
  let wire: string;
  try {
    wire = await readBounded(request, maximumBytes);
  } catch {
    return new Response("request too large", { status: 413 });
  }
  let value: unknown;
  try {
    value = parseStrictJson(wire);
  } catch {
    return rpcError(null, -32700, "Parse error");
  }
  if (
    typeof value !== "object" || value === null || Array.isArray(value) ||
    Object.keys(value).some((key) => !["jsonrpc", "id", "method", "params"].includes(key)) ||
    (value as Partial<JsonRpcEnvelope>).jsonrpc !== "2.0" ||
    typeof (value as Partial<JsonRpcEnvelope>).id !== "string" ||
    typeof (value as Partial<JsonRpcEnvelope>).method !== "string" ||
    !Object.hasOwn(value, "params")
  ) {
    return rpcError(null, -32600, "Invalid Request");
  }
  return { envelope: value as JsonRpcEnvelope };
}

async function readBounded(request: Request, maximumBytes: number): Promise<string> {
  const reader = request.body?.getReader();
  if (!reader) return "";
  const chunks: Uint8Array[] = [];
  let length = 0;
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    length += value.length;
    if (length > maximumBytes) throw new Error("body too large");
    chunks.push(value);
  }
  const joined = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) {
    joined.set(chunk, offset);
    offset += chunk.length;
  }
  return new TextDecoder("utf-8", { fatal: true }).decode(joined);
}

function rpcResult(id: string, result: unknown): Response {
  return jsonResponse({ jsonrpc: "2.0", id, result });
}

function rpcError(id: string | null, code: number, message: string): Response {
  return jsonResponse({ jsonrpc: "2.0", id, error: { code, message } });
}

function jsonResponse(value: unknown): Response {
  return Response.json(value, { status: 200, headers: { "content-type": JSON_CONTENT_TYPE } });
}

function requestResult(
  params: RequestParams,
  outcome: ProcessOutcome,
  maximumBytes: number,
): Response {
  const envelope = {
    jsonrpc: "2.0",
    id: params.correlation_id,
    result: {
    session: params.session,
    correlation_id: params.correlation_id,
    outcome,
    },
  };
  const body = JSON.stringify(envelope);
  if (new TextEncoder().encode(body).length > maximumBytes) {
    return new Response("response too large", { status: 413 });
  }
  return new Response(body, {
    status: 200,
    headers: { "content-type": JSON_CONTENT_TYPE },
  });
}

function hmac(secret: Uint8Array, message: Uint8Array): Uint8Array {
  return new Uint8Array(createHmac("sha256", secret).update(message).digest());
}

function sameIdentity(left: HandshakeIdentity, right: HandshakeIdentity): boolean {
  return new TextDecoder().decode(handshakeProofPayload({
    identity: left,
    host_nonce: encodeBase64Url(new Uint8Array(32)),
    host_proof: encodeBase64Url(new Uint8Array(32)),
  })) === new TextDecoder().decode(handshakeProofPayload({
    identity: right,
    host_nonce: encodeBase64Url(new Uint8Array(32)),
    host_proof: encodeBase64Url(new Uint8Array(32)),
  }));
}

function requireSession(candidate: string, expected: string | undefined): void {
  if (expected === undefined || candidate !== expected) throw new Error("session mismatch");
}

function cancellationState(): ActiveRequest {
  let observeCancel = (): void => {};
  const cancelObserved = new Promise<void>((resolve) => { observeCancel = resolve; });
  return { cancelled: false, cancelObserved, observeCancel };
}

function startChildTimer(
  remaining: string | null,
  state: ActiveRequest,
): ReturnType<typeof setTimeout> | undefined {
  if (remaining === null) return undefined;
  const nanos = BigInt(remaining);
  const milliseconds = Number((nanos + 999_999n) / 1_000_000n);
  return setTimeout(() => {
    state.cancelled = true;
    state.observeCancel();
  }, Math.min(milliseconds, 2_147_483_647));
}

function invocationContext(params: RequestParams, state: ActiveRequest): InvocationContext {
  const extensions = Object.fromEntries(params.extensions.map((extension) => [
    extension.key,
    Object.freeze({ ...extension }),
  ]));
  return {
    requestId: params.correlation_id as InvocationContext["requestId"],
    ...(params.caller_instance === null ? {} : { callerInstance: params.caller_instance }),
    ...(params.extensions.length === 0 ? {} : { extensions: Object.freeze(extensions) }),
    get cancelled() { return state.cancelled; },
  };
}

function toProcessOutcome(outcome: ProviderDispatchOutcome, operation: string): ProcessOutcome {
  if (outcome.kind === "success") return { kind: "success", value: outcome.value };
  if (outcome.kind === "domain") return { kind: "domain", error: outcome.value };
  if (outcome.failure.kind === "resource_exhausted") {
    return { kind: "runtime", failure: { kind: "resource_exhausted", operation } };
  }
  return {
    kind: "runtime",
    failure: { kind: "plugin_failure", detail: boundedError(outcome.failure) },
  };
}

function boundedError(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  let bounded = "";
  let bytes = 0;
  for (const character of message || "Plugin failed") {
    const width = new TextEncoder().encode(character).length;
    if (bytes + width > 1_024) break;
    bounded += character;
    bytes += width;
  }
  return bounded || "Plugin failed";
}

function readBootstrapSecret(): Uint8Array {
  const candidate = new Uint8Array(32);
  const eofProbe = new Uint8Array(1);
  let length = 0;
  try {
    while (length < 32) {
      const read = readSync(
        BOOTSTRAP_FD,
        candidate,
        length,
        candidate.length - length,
        null,
      );
      if (read === 0) break;
      length += read;
    }
    if (length === 32 && readSync(BOOTSTRAP_FD, eofProbe, 0, 1, null) !== 0) {
      length += 1;
    }
  } finally {
    closeSync(BOOTSTRAP_FD);
    eofProbe.fill(0);
  }
  if (length !== 32) {
    candidate.fill(0);
    throw new Error("Process V1 bootstrap pipe must contain exactly 32 bytes");
  }
  return candidate;
}

function validateProviderOutcome<Instance>(
  provider: ProcessV1ProviderBinding<Instance>,
  operation: string,
  outcome: ProviderDispatchOutcome,
): void {
  if (outcome.kind === "success") provider.validateSuccess(operation, outcome.value);
  if (outcome.kind === "domain") provider.validateDomainError(operation, outcome.value);
}

async function acquireDataSlot(
  maximum: number,
  capacity: number,
  getRunning: () => number,
  setRunning: (value: number) => void,
  waiters: Array<() => void>,
): Promise<boolean> {
  if (getRunning() < maximum) {
    setRunning(getRunning() + 1);
    return true;
  }
  if (waiters.length >= capacity) return false;
  await new Promise<void>((resolve) => waiters.push(resolve));
  // The releasing request transfers its reserved running slot to this waiter.
  return true;
}

function releaseDataSlot(
  getRunning: () => number,
  setRunning: (value: number) => void,
  waiters: Array<() => void>,
): void {
  const waiter = waiters.shift();
  if (waiter) {
    waiter();
  } else {
    setRunning(Math.max(0, getRunning() - 1));
  }
}
