import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { WebStandardStreamableHTTPServerTransport } from "@modelcontextprotocol/sdk/server/webStandardStreamableHttp.js";
import {
  SUPPORTED_PROTOCOL_VERSIONS,
  isInitializeRequest,
  type JSONRPCMessage,
} from "@modelcontextprotocol/sdk/types.js";
import type { Operation } from "@lenso/engine/operations";
import { createMcpAdapter, type McpAdapterOptions } from "./adapter";
import { bindMcpServer } from "./protocol";

/** Verified by the host's signature/introspection and current revocation policy, not decoded JSON. */
export interface McpIdentity {
  readonly subject: string;
  readonly tenant: string;
  readonly issuer: string;
  readonly audience: readonly string[];
  readonly scopes: readonly string[];
  /** Unix seconds. */
  readonly expiresAt: number;
}

export interface HttpMcpOptions<
  I extends McpIdentity,
  O extends Operation = Operation,
> extends McpAdapterOptions<I, O> {
  /** Canonical public MCP resource URL. Never inferred from forwarded headers. */
  readonly resource: string;
  readonly authorizationServers: readonly string[];
  readonly requiredScopes: readonly string[];
  readonly allowedOrigins: readonly string[];
  /** Must cryptographically verify or introspect the token, including current validity/revocation. */
  readonly verifyToken: (token: string, signal: AbortSignal) => I | Promise<I>;
  readonly maxFrameBytes?: number;
  readonly maxResponseBytes?: number;
  readonly maxHttpRequests?: number;
  readonly maxSessions?: number;
  readonly sessionIdleMs?: number;
  readonly rateLimit?: {
    readonly requests: number;
    readonly windowMs: number;
    readonly maxIdentities: number;
  };
}

function positive(value: number | undefined, fallback: number): number {
  const result = value ?? fallback;
  if (!Number.isSafeInteger(result) || result < 1) throw new TypeError("Invalid MCP limit.");
  return result;
}

function trustedUrl(value: string): URL {
  const url = new URL(value);
  const loopback = ["127.0.0.1", "localhost", "[::1]"].includes(url.hostname);
  if (
    (url.protocol !== "https:" && !(url.protocol === "http:" && loopback)) ||
    url.username ||
    url.password ||
    url.hash ||
    url.search
  )
    throw new TypeError(
      "MCP URLs require HTTPS (HTTP only on loopback), without credentials, query or fragment.",
    );
  return url;
}

function reply(status: number, code: string, headers?: HeadersInit): Response {
  return Response.json(
    { error: code },
    { status, headers: { "cache-control": "no-store", ...headers } },
  );
}

async function readBody(request: Request, maxBytes: number, signal: AbortSignal): Promise<unknown> {
  const declared = request.headers.get("content-length");
  if (declared && (!/^\d+$/.test(declared) || Number(declared) > maxBytes))
    throw new RangeError("Body limit.");
  const reader = request.body?.getReader();
  if (!reader) throw new SyntaxError("Missing body.");
  const chunks: Uint8Array[] = [];
  let size = 0;
  const abort = () => {
    void reader.cancel().catch(() => {});
  };
  signal.addEventListener("abort", abort, { once: true });
  try {
    while (true) {
      signal.throwIfAborted();
      const { value, done } = await reader.read();
      signal.throwIfAborted();
      if (done) break;
      size += value.byteLength;
      if (size > maxBytes) throw new RangeError("Body limit.");
      chunks.push(value);
    }
    const bytes = new Uint8Array(size);
    let offset = 0;
    for (const chunk of chunks) {
      bytes.set(chunk, offset);
      offset += chunk.byteLength;
    }
    return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  } finally {
    signal.removeEventListener("abort", abort);
    void reader.cancel().catch(() => {});
    reader.releaseLock();
  }
}

async function protocolResponse(
  response: Response,
  maxBytes: number,
  signal: AbortSignal,
): Promise<Response> {
  if (!response.body) return response;
  const reader = response.body.getReader();
  const abort = () => {
    void reader.cancel().catch(() => {});
  };
  signal.addEventListener("abort", abort, { once: true });
  const chunks: Uint8Array[] = [];
  let size = 0;
  try {
    while (true) {
      signal.throwIfAborted();
      const { value, done } = await reader.read();
      signal.throwIfAborted();
      if (done) break;
      size += value.byteLength;
      if (size > maxBytes) return reply(500, "response-too-large");
      chunks.push(value);
    }
    const bytes = new Uint8Array(size);
    let offset = 0;
    for (const chunk of chunks) {
      bytes.set(chunk, offset);
      offset += chunk.byteLength;
    }
    const text = new TextDecoder().decode(bytes);
    // This finite tools-only entry sends no progress/server notifications. Let
    // the SDK frame and clean up its POST stream, then normalize its one reply.
    const frames = response.headers.get("content-type")?.startsWith("text/event-stream")
      ? text
          .split("\n")
          .filter((line) => line.startsWith("data: "))
          .map((line) => line.slice(6))
      : [text];
    if (frames.length !== 1) return reply(500, "invalid-protocol-response");
    const payload = JSON.parse(frames[0]!);
    if (payload && typeof payload === "object" && payload.error)
      payload.error = { code: payload.error.code, message: "MCP request rejected." };
    const headers = new Headers(response.headers);
    headers.delete("content-length");
    headers.set("content-type", "application/json");
    headers.set("cache-control", "no-store");
    if (Buffer.byteLength(JSON.stringify(payload)) > maxBytes)
      return reply(500, "response-too-large");
    return Response.json(payload, { status: response.status, headers });
  } finally {
    signal.removeEventListener("abort", abort);
    void reader.cancel().catch(() => {});
    reader.releaseLock();
  }
}

type Session = {
  readonly owner: string;
  readonly server: Server;
  readonly transport: WebStandardStreamableHTTPServerTransport;
  readonly protocol: ReturnType<typeof bindMcpServer>;
  readonly ids: Set<string>;
  lastUsed: number;
  active: number;
};

/**
 * Owns protocol sessions only. Mount fetch in the host's existing listener;
 * close never stops the borrowed app or changes global logging.
 */
export async function createHttpMcp<I extends McpIdentity, O extends Operation>(
  options: HttpMcpOptions<I, O>,
): Promise<{ fetch(request: Request): Promise<Response>; close(): Promise<void> }> {
  const resource = trustedUrl(options.resource);
  if (!options.authorizationServers.length)
    throw new TypeError("An authorization server is required.");
  const issuers = options.authorizationServers.map((value) => {
    trustedUrl(value);
    return value;
  });
  const origins = new Set(
    options.allowedOrigins.map((value) => {
      const url = trustedUrl(value);
      if (url.href !== `${url.origin}/`) throw new TypeError("Allowed origins must be origins.");
      return url.origin;
    }),
  );
  const scopes = [...options.requiredScopes];
  if (scopes.some((scope) => !/^[\x21\x23-\x5B\x5D-\x7E]+$/.test(scope)))
    throw new TypeError("Invalid OAuth scope.");
  if (typeof options.verifyToken !== "function")
    throw new TypeError("A trusted verifier is required.");
  const metadataUrl = new URL(
    `/.well-known/oauth-protected-resource${resource.pathname}`,
    resource,
  );
  const metadata = {
    resource: resource.href,
    authorization_servers: issuers,
    scopes_supported: scopes,
    bearer_methods_supported: ["header"],
  };
  const maxFrame = positive(options.maxFrameBytes, 1024 * 1024);
  const maxHttp = positive(options.maxHttpRequests, 32);
  const maxSessions = positive(options.maxSessions, 128);
  const idleMs = positive(options.sessionIdleMs, 5 * 60_000);
  const timeoutMs = positive(options.requestTimeoutMs, 30_000);
  const maxResponse = positive(options.maxResponseBytes, 2 * 1024 * 1024 + 4096);
  if (maxResponse < 256) throw new TypeError("MCP response budget must fit a safe error.");
  if (Buffer.byteLength(JSON.stringify(metadata)) > positive(options.maxCatalogBytes, 256 * 1024))
    throw new TypeError("MCP metadata exceeds the catalog budget.");
  const rate = {
    requests: positive(options.rateLimit?.requests, 120),
    windowMs: positive(options.rateLimit?.windowMs, 60_000),
    maxIdentities: positive(options.rateLimit?.maxIdentities, 1024),
  };
  const adapter = await createMcpAdapter(options);
  const sessions = new Map<string, Session>();
  const buckets = new Map<string, { reset: number; count: number }>();
  const active = new Set<Promise<Response>>();
  const controllers = new Set<AbortController>();
  let closing = false;
  let reservations = 0;
  let closePromise: Promise<void> | undefined;
  const challenge = (error?: "invalid_token" | "insufficient_scope") => ({
    "www-authenticate": `Bearer resource_metadata="${metadataUrl.href}", scope="${scopes.join(" ")}"${error ? `, error="${error}"` : ""}`,
  });
  const retire = async (session: Session) => {
    if (session.transport.sessionId) sessions.delete(session.transport.sessionId);
    await session.server.close();
  };
  const sweep = async () => {
    const now = Date.now();
    for (const [key, bucket] of buckets) if (bucket.reset <= now) buckets.delete(key);
    for (const session of sessions.values())
      if (!session.active && now - session.lastUsed > idleMs) await retire(session);
  };
  const startSession = async (owner: string) => {
    const transport = new WebStandardStreamableHTTPServerTransport({
      sessionIdGenerator: () => crypto.randomUUID(),
      // 1.32.1 JSON mode retains completed stream resolvers. SDK SSE mode
      // cleans them; protocolResponse bounds and normalizes its finite reply.
      enableJsonResponse: false,
      maxRequestBodySize: maxFrame,
      keepAliveMs: 0,
      onsessioninitialized: (id) => {
        sessions.set(id, session);
      },
      onsessionclosed: () => retire(session),
    });
    const server = new Server({ name: "lenso", version: "0.2.1" }, { capabilities: { tools: {} } });
    const protocol = bindMcpServer(server, adapter, (extra) => {
      const request = extra.authInfo?.extra?.lenso as
        | { identity: I; requestId: string }
        | undefined;
      if (!request) throw new Error("Missing trusted MCP request context.");
      return request;
    });
    const session: Session = {
      owner,
      server,
      transport,
      protocol,
      ids: new Set(),
      lastUsed: Date.now(),
      active: 0,
    };
    try {
      await server.connect(transport);
      return session;
    } catch (error) {
      await server.close();
      throw error;
    }
  };

  const handle = async (request: Request, controller: AbortController): Promise<Response> => {
    const url = new URL(request.url);
    if (url.origin !== resource.origin) return reply(403, "invalid-host");
    const host = request.headers.get("host");
    if (host && host !== resource.host) return reply(403, "invalid-host");
    const origin = request.headers.get("origin");
    if (origin && !origins.has(origin)) return reply(403, "invalid-origin");
    if (url.pathname === metadataUrl.pathname && request.method === "GET")
      return Response.json(metadata, { headers: { "cache-control": "no-store" } });
    if (url.pathname !== resource.pathname || url.search) return reply(404, "not-found");
    const credential = request.headers.get("authorization");
    const match = credential?.match(/^Bearer ([\x21-\x7E]+)$/i);
    if (!match || match[1]!.length > 8192) return reply(401, "unauthorized", challenge());
    let identity: I;
    try {
      const verified = await options.verifyToken(match[1]!, controller.signal);
      if (
        !verified ||
        typeof verified.subject !== "string" ||
        !verified.subject ||
        verified.subject.length > 512 ||
        typeof verified.tenant !== "string" ||
        !verified.tenant ||
        verified.tenant.length > 512 ||
        typeof verified.issuer !== "string" ||
        !issuers.includes(verified.issuer) ||
        !Array.isArray(verified.audience) ||
        !verified.audience.includes(resource.href) ||
        !Array.isArray(verified.scopes) ||
        verified.scopes.some((scope) => typeof scope !== "string") ||
        !Number.isFinite(verified.expiresAt) ||
        verified.expiresAt <= Date.now() / 1000
      )
        return reply(401, "invalid-token", challenge("invalid_token"));
      // Snapshot each verified request; no session-cached identity or mutable current actor.
      identity = Object.freeze({
        ...verified,
        audience: Object.freeze([...verified.audience]),
        scopes: Object.freeze([...verified.scopes]),
      });
    } catch {
      return reply(401, "invalid-token", challenge("invalid_token"));
    }
    if (scopes.some((scope) => !identity.scopes.includes(scope)))
      return reply(403, "insufficient-scope", challenge("insufficient_scope"));
    controller.signal.throwIfAborted();
    await sweep();
    const owner = JSON.stringify([
      identity.issuer,
      identity.subject,
      identity.tenant,
      resource.href,
    ]);
    let bucket = buckets.get(owner);
    if (!bucket) {
      if (buckets.size >= rate.maxIdentities) return reply(429, "rate-limit-capacity");
      bucket = { reset: Date.now() + rate.windowMs, count: 0 };
      buckets.set(owner, bucket);
    }
    if (++bucket.count > rate.requests)
      return reply(429, "rate-limited", {
        "retry-after": String(Math.ceil((bucket.reset - Date.now()) / 1000)),
      });
    const sessionId = request.headers.get("mcp-session-id");
    let session = sessionId ? sessions.get(sessionId) : undefined;
    if (sessionId && (!session || session.owner !== owner)) return reply(404, "session-not-found");
    if (request.method === "GET") {
      const version = request.headers.get("mcp-protocol-version");
      if (version && !SUPPORTED_PROTOCOL_VERSIONS.includes(version))
        return reply(400, "unsupported-protocol");
      return reply(405, "standalone-stream-not-supported", { allow: "POST, DELETE" });
    }
    if (request.method !== "POST" && request.method !== "DELETE")
      return reply(405, "method-not-allowed", { allow: "POST, DELETE" });
    let body: unknown;
    if (request.method === "POST") {
      try {
        body = await readBody(request, maxFrame, controller.signal);
      } catch (error) {
        return reply(
          error instanceof RangeError ? 413 : 400,
          error instanceof RangeError ? "request-too-large" : "invalid-request",
        );
      }
      if (!body || typeof body !== "object" || Array.isArray(body))
        return reply(400, "invalid-request");
    }
    if (!session) {
      controller.signal.throwIfAborted();
      if (!isInitializeRequest(body as JSONRPCMessage)) return reply(400, "session-required");
      if (sessions.size + reservations >= maxSessions) return reply(503, "session-capacity");
      reservations++;
      try {
        session = await startSession(owner);
      } catch (error) {
        reservations--;
        throw error;
      }
    }
    // Duplicate in-flight IDs would alias SDK cancellation/response maps.
    const id =
      body && typeof body === "object" && "id" in body ? JSON.stringify(body.id) : undefined;
    if (id && session.ids.has(id)) return reply(409, "duplicate-request");
    if (id) session.ids.add(id);
    session.active++;
    session.lastUsed = Date.now();
    const selected = session;
    // Register before SDK's deferred handler dispatch, so an immediately
    // following cancellation cannot miss the admitted invocation.
    const release =
      body &&
      typeof body === "object" &&
      "method" in body &&
      (body.method === "tools/call" || body.method === "tools/list") &&
      "id" in body &&
      (typeof body.id === "string" || typeof body.id === "number")
        ? session.protocol.prepare(body.id)
        : () => {};
    const abort = () => {
      void retire(selected).catch(() => {});
    };
    controller.signal.addEventListener("abort", abort, { once: true });
    try {
      controller.signal.throwIfAborted();
      const transportResponse = session.transport
        .handleRequest(request, {
          parsedBody: body,
          authInfo: {
            // The token is never forwarded to services or a downstream resource.
            token: "",
            clientId: identity.subject,
            scopes: [...identity.scopes],
            expiresAt: identity.expiresAt,
            resource,
            extra: { lenso: { identity, requestId: crypto.randomUUID() } },
          },
        })
        .then((response) => protocolResponse(response, maxResponse, controller.signal));
      // Protocol close can suppress a pending reply. Bound the entire stream
      // read as well as the SDK's initial handleRequest wait.
      let interrupt: () => void = () => {};
      const interrupted = new Promise<Response>((resolve) => {
        interrupt = () => resolve(reply(504, "request-interrupted"));
        controller.signal.addEventListener("abort", interrupt, { once: true });
        if (controller.signal.aborted) interrupt();
      });
      let response: Response;
      try {
        response = await Promise.race([transportResponse, interrupted]);
      } finally {
        controller.signal.removeEventListener("abort", interrupt);
      }
      if (!session.transport.sessionId) await retire(session);
      return response;
    } finally {
      release();
      controller.signal.removeEventListener("abort", abort);
      if (id) selected.ids.delete(id);
      selected.active--;
      selected.lastUsed = Date.now();
      if (!sessionId) {
        reservations--;
        if (!selected.transport.sessionId || controller.signal.aborted) await retire(selected);
      }
    }
  };
  const fetch = (request: Request): Promise<Response> => {
    if (closing) return Promise.resolve(reply(503, "adapter-closed"));
    if (active.size >= maxHttp) return Promise.resolve(reply(503, "adapter-busy"));
    const controller = new AbortController();
    controllers.add(controller);
    let timer: ReturnType<typeof setTimeout>;
    const interrupted = new Promise<Response>((resolve) => {
      controller.signal.addEventListener(
        "abort",
        () => resolve(reply(504, "request-interrupted")),
        { once: true },
      );
      timer = setTimeout(() => controller.abort(), timeoutMs + 1000);
    });
    const execution = handle(request, controller).catch(() => reply(500, "mcp-failed"));
    active.add(execution);
    void execution.finally(() => {
      active.delete(execution);
      controllers.delete(controller);
      clearTimeout(timer);
    });
    return Promise.race([execution, interrupted]);
  };
  const close = (): Promise<void> => {
    if (closePromise) return closePromise;
    closing = true;
    closePromise = (async () => {
      for (const controller of controllers) controller.abort();
      await adapter.close();
      await Promise.all([...sessions.values()].map(retire));
      await Promise.all(active);
      buckets.clear();
    })();
    return closePromise;
  };
  return Object.freeze({ fetch, close });
}
