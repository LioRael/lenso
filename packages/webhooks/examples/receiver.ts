import { verifyWebhook, type EventEnvelope, type SigningKey } from "@lenso/webhooks";

/** Receiver's host owns listener, authentication key distribution and its transactional dedupe store. */
export async function receiveWebhook(input: {
  request: Request;
  keys: readonly SigningKey[];
  commitOnce: (eventId: string, event: EventEnvelope) => Promise<"accepted" | "duplicate">;
}) {
  // Bound streaming input before accumulating bytes. Never parse/re-serialize before verification.
  const reader = input.request.body?.getReader();
  if (!reader) return new Response(null, { status: 400 });
  const chunks: Uint8Array[] = [];
  let size = 0;
  try {
    while (true) {
      const part = await reader.read();
      if (part.done) break;
      size += part.value.byteLength;
      if (size > 1_048_576) {
        await reader.cancel();
        return new Response(null, { status: 413 });
      }
      chunks.push(part.value);
    }
  } finally {
    reader.releaseLock();
  }
  const body = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    body.set(chunk, offset);
    offset += chunk.length;
  }
  const eventId = input.request.headers.get("x-lenso-event-id") ?? "";
  if (
    !verifyWebhook({
      body,
      eventId,
      timestamp: input.request.headers.get("x-lenso-timestamp") ?? "",
      signature: input.request.headers.get("x-lenso-signature") ?? "",
      keys: input.keys,
      now: Math.floor(Date.now() / 1000),
      toleranceSeconds: 300,
    })
  )
    return new Response(null, { status: 401 });
  let event: EventEnvelope;
  try {
    event = JSON.parse(new TextDecoder().decode(body)) as EventEnvelope;
    if (
      event.version !== 1 ||
      event.id !== eventId ||
      typeof event.type !== "string" ||
      typeof event.occurredAt !== "string" ||
      typeof event.source !== "string" ||
      !("data" in event)
    )
      return new Response(null, { status: 400 });
  } catch {
    return new Response(null, { status: 400 });
  }
  // commitOnce atomically couples the event-ID unique record and receiver business effect.
  // Never dedupe by delivery ID, attempt ID, timestamp or signature.
  await input.commitOnce(eventId, event);
  return new Response(null, { status: 204 });
}
