import { createHash } from "node:crypto";
import type { EventEnvelope, JsonValue } from "./contracts";
import { verifyWebhook, type SigningKey } from "./signing";

export interface WebhookReceipt {
  /** Stable host-selected trust/tenant namespace, never taken from the request. */
  readonly receiverId: string;
  readonly eventId: string;
  readonly bodySha256: string;
  readonly receivedAt: number;
}

export interface WebhookInbox {
  /** Must atomically commit the unique receipt AND business effects; never acknowledge before commit. */
  commitOnce(
    receipt: WebhookReceipt,
    event: EventEnvelope,
  ): Promise<"accepted" | "duplicate" | "conflict">;
}

export interface WebhookReceiverOptions {
  readonly receiverId: string;
  readonly source: string;
  readonly eventTypes: readonly string[];
  readonly maxBodyBytes: number;
  readonly bodyTimeoutMs: number;
  readonly toleranceSeconds: number;
  /** Host-controlled resolver, scoped to this receiver. No request-controlled key lookup. */
  readonly keys: () => Promise<readonly SigningKey[]>;
  readonly inbox: WebhookInbox;
  readonly now?: () => number;
}

class Rejected extends Error {
  constructor(readonly status: number) {
    super("Webhook rejected");
  }
}

const boundedText = (value: unknown, maximum: number): value is string =>
  typeof value === "string" && value.length > 0 && value.length <= maximum;

function envelope(body: Uint8Array, eventId: string): EventEnvelope {
  try {
    const value = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(body));
    if (
      !value ||
      Array.isArray(value) ||
      value.version !== 1 ||
      value.id !== eventId ||
      !boundedText(value.type, 128) ||
      !boundedText(value.source, 256) ||
      !boundedText(value.occurredAt, 64) ||
      !Number.isFinite(Date.parse(value.occurredAt)) ||
      !Object.hasOwn(value, "data") ||
      Object.keys(value).some(
        (key) => !["version", "id", "type", "source", "occurredAt", "data"].includes(key),
      )
    )
      throw new Error();
    let nodes = 0;
    function check(data: JsonValue, depth: number): void {
      if (++nodes > 50_000 || depth > 32) throw new Error();
      if (typeof data === "number" && !Number.isFinite(data)) throw new Error();
      if (data && typeof data === "object") {
        for (const child of Object.values(data)) check(child, depth + 1);
        Object.freeze(data);
      }
    }
    check(value.data, 0);
    return Object.freeze(value);
  } catch {
    throw new Rejected(400);
  }
}

async function readBody(request: Request, maximum: number, timeoutMs: number): Promise<Uint8Array> {
  const length = request.headers.get("content-length");
  if (length !== null && (!/^\d+$/.test(length) || Number(length) > maximum))
    throw new Rejected(413);
  if (!request.body) throw new Rejected(400);
  const reader = request.body.getReader();
  let timer: ReturnType<typeof setTimeout> | undefined;
  let abort = () => {};
  const interrupted = new Promise<never>((_, reject) => {
    abort = () => reject(new Rejected(408));
    timer = setTimeout(abort, timeoutMs);
    request.signal.addEventListener("abort", abort, { once: true });
    if (request.signal.aborted) abort();
  });
  let finished = false;
  try {
    const chunks: Uint8Array[] = [];
    let size = 0;
    while (true) {
      const part = await Promise.race([reader.read(), interrupted]);
      if (part.done) break;
      size += part.value.byteLength;
      if (size > maximum) throw new Rejected(413);
      chunks.push(part.value);
    }
    finished = true;
    const body = new Uint8Array(size);
    let offset = 0;
    for (const chunk of chunks) {
      body.set(chunk, offset);
      offset += chunk.length;
    }
    return body;
  } finally {
    clearTimeout(timer);
    request.signal.removeEventListener("abort", abort);
    // An uncooperative source's cancel promise must not defeat the read deadline.
    if (!finished) void reader.cancel().catch(() => {});
    reader.releaseLock();
  }
}

/** Fetch adapter only: the host owns listener admission, keys, database and lifecycle. */
export function createWebhookReceiver(options: WebhookReceiverOptions) {
  if (
    !boundedText(options.receiverId, 256) ||
    !boundedText(options.source, 256) ||
    !options.eventTypes.length ||
    options.eventTypes.length > 1000 ||
    options.eventTypes.some((type) => !boundedText(type, 128)) ||
    !Number.isSafeInteger(options.maxBodyBytes) ||
    options.maxBodyBytes < 1 ||
    !Number.isSafeInteger(options.bodyTimeoutMs) ||
    options.bodyTimeoutMs < 1 ||
    options.bodyTimeoutMs > 2_147_483_647 ||
    !Number.isSafeInteger(options.toleranceSeconds) ||
    options.toleranceSeconds < 0 ||
    typeof options.keys !== "function" ||
    typeof options.inbox?.commitOnce !== "function"
  )
    throw new Error("Invalid webhook receiver configuration");
  const { receiverId, source, maxBodyBytes, bodyTimeoutMs, toleranceSeconds, keys, inbox } =
    options;
  const types = new Set(options.eventTypes);
  const now = options.now ?? Date.now;
  return {
    async handle(request: Request): Promise<Response> {
      try {
        if (request.method !== "POST")
          return new Response(null, { status: 405, headers: { Allow: "POST" } });
        if (
          request.headers.get("content-type")?.split(";")[0]?.trim().toLowerCase() !==
            "application/json" ||
          (request.headers.has("content-encoding") &&
            request.headers.get("content-encoding") !== "identity")
        )
          throw new Rejected(415);
        const body = await readBody(request, maxBodyBytes, bodyTimeoutMs);
        const eventId = request.headers.get("x-lenso-event-id") ?? "";
        const verificationKeys = await keys();
        const receivedAt = now();
        if (!Number.isSafeInteger(receivedAt) || receivedAt < 0) throw new Error();
        if (
          !verifyWebhook({
            body,
            eventId,
            timestamp: request.headers.get("x-lenso-timestamp") ?? "",
            signature: request.headers.get("x-lenso-signature") ?? "",
            keys: verificationKeys,
            now: receivedAt / 1000,
            toleranceSeconds,
          })
        )
          throw new Rejected(401);
        const event = envelope(body, eventId);
        if (event.source !== source || !types.has(event.type)) throw new Rejected(403);
        const result = await inbox.commitOnce(
          Object.freeze({
            receiverId,
            eventId,
            receivedAt,
            bodySha256: createHash("sha256").update(body).digest("hex"),
          }),
          event,
        );
        if (result === "conflict") throw new Rejected(409);
        if (result !== "accepted" && result !== "duplicate") throw new Error();
        return new Response(null, { status: 204 });
      } catch (error) {
        // Never expose key resolver, payload, driver or application error details.
        return new Response(null, { status: error instanceof Rejected ? error.status : 503 });
      }
    },
  };
}

export type WebhookReceiver = ReturnType<typeof createWebhookReceiver>;
