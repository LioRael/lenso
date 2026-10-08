import { PaymentsError } from "./contracts";
import type { PaymentsRuntime } from "./index";

/** Mount only at the configured Stripe endpoint. No browser actor or Auth session is fabricated. */
export function createPaymentsWebhookHandler(options: {
  webhook: PaymentsRuntime<unknown>["webhook"];
  maxBytes?: number;
  /** Enqueue the registered reconciliation Task after durable receipt, including duplicate delivery. */
  wake?: () => Promise<void>;
}) {
  const maxBytes = options.maxBytes ?? 1_048_576;
  if (!Number.isSafeInteger(maxBytes) || maxBytes < 1) throw new PaymentsError("invalid-input");
  return async (request: Request): Promise<Response> => {
    if (request.method !== "POST")
      return new Response(null, { status: 405, headers: { Allow: "POST" } });
    const signature = request.headers.get("stripe-signature");
    if (!signature) return new Response(null, { status: 400 });
    const declared = request.headers.get("content-length");
    if (declared && Number(declared) > maxBytes) return new Response(null, { status: 413 });
    const reader = request.body?.getReader();
    if (!reader) return new Response(null, { status: 400 });
    const chunks: Uint8Array[] = [];
    let size = 0;
    try {
      while (true) {
        const chunk = await reader.read();
        if (chunk.done) break;
        size += chunk.value.byteLength;
        if (size > maxBytes) {
          await reader.cancel();
          return new Response(null, { status: 413 });
        }
        chunks.push(chunk.value);
      }
      const raw = new Uint8Array(size);
      let offset = 0;
      for (const chunk of chunks) {
        raw.set(chunk, offset);
        offset += chunk.byteLength;
      }
      // 2xx is sent only after verified receipt is durably committed (or an ignored event is verified).
      const receipt = await options.webhook.receive(raw, signature);
      if (receipt.accepted && options.wake) {
        try {
          await options.wake();
        } catch {
          return new Response(null, { status: 503 });
        }
      }
      return new Response(null, { status: 204 });
    } catch (error) {
      const invalid =
        error instanceof PaymentsError &&
        ["bad-signature", "provider-mismatch", "invalid-input", "not-found", "forbidden"].includes(
          error.code,
        );
      return new Response(null, { status: invalid ? 400 : 503 });
    } finally {
      reader.releaseLock();
    }
  };
}
