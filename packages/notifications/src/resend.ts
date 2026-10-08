import type { DeliveryResult, EmailMessage, NotificationChannel } from "./contracts";
import { hasControlCharacters } from "./render";

const DEFAULT_ENDPOINT = "https://api.resend.com/emails";
const IDEMPOTENCY_WINDOW_MS = 24 * 60 * 60 * 1000;
const DEFAULT_TIMEOUT_MS = 10_000;
const UNKNOWN: DeliveryResult = {
  state: "unknown",
  code: "transport-unknown",
  retryable: true,
};
const INVALID_RESPONSE: DeliveryResult = {
  state: "unknown",
  code: "invalid-response",
  retryable: true,
};
const REJECTED: DeliveryResult = {
  state: "failed",
  code: "rejected",
  retryable: false,
};

export interface ResendChannelOptions {
  readonly id: string;
  readonly apiKey: string;
  readonly timeoutMs?: number;
  readonly endpoint?: string;
}

function validEndpoint(endpoint: string): boolean {
  let url: URL;
  try {
    url = new URL(endpoint);
  } catch {
    return false;
  }
  if (url.username || url.password || url.search || url.hash) return false;
  if (url.protocol === "https:") return true;
  return (
    url.protocol === "http:" &&
    (url.hostname === "localhost" || url.hostname === "127.0.0.1" || url.hostname === "[::1]")
  );
}

function providerErrorCode(payload: unknown): string | undefined {
  if (!payload || typeof payload !== "object") return undefined;
  const error = (payload as { error?: unknown }).error;
  const topLevelName = (payload as { name?: unknown }).name;
  if (typeof topLevelName === "string") return topLevelName;
  if (typeof error === "string") return error;
  if (error && typeof error === "object") {
    const name = (error as { name?: unknown }).name;
    const message = (error as { message?: unknown }).message;
    if (typeof name === "string") return name;
    if (typeof message === "string") return message;
  }
  return undefined;
}

export function createResendChannel(options: ResendChannelOptions): NotificationChannel {
  const { id, apiKey } = options;
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const endpoint = options.endpoint ?? DEFAULT_ENDPOINT;
  if (typeof id !== "string" || id.trim() === "") {
    throw new TypeError("Resend channel id must be a non-empty string");
  }
  if (typeof apiKey !== "string" || apiKey.length === 0) {
    throw new TypeError("Resend API key must be a non-empty string");
  }
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs <= 0) {
    throw new TypeError("Resend timeout must be a positive integer");
  }
  if (typeof endpoint !== "string" || !validEndpoint(endpoint)) {
    throw new TypeError("Resend endpoint must be HTTPS or local HTTP");
  }

  return {
    id,
    kind: "email",
    idempotencyWindowMs: IDEMPOTENCY_WINDOW_MS,
    async send(
      message: EmailMessage,
      { idempotencyKey, signal }: { idempotencyKey: string; signal?: AbortSignal },
    ): Promise<DeliveryResult> {
      if (signal?.aborted) return UNKNOWN;
      const controller = new AbortController();
      const abortFromCaller = () => controller.abort();
      signal?.addEventListener("abort", abortFromCaller, { once: true });
      const timer = setTimeout(() => {
        controller.abort();
      }, timeoutMs);
      try {
        const response = await fetch(endpoint, {
          method: "POST",
          redirect: "error",
          signal: controller.signal,
          headers: {
            Authorization: `Bearer ${apiKey}`,
            "Content-Type": "application/json",
            "Idempotency-Key": idempotencyKey,
          },
          body: JSON.stringify({
            from: message.from,
            to: [message.to],
            subject: message.subject,
            text: message.text,
            html: message.html,
          }),
        });
        let payload: unknown;
        try {
          payload = await response.json();
        } catch {
          return INVALID_RESPONSE;
        }
        if (response.ok) {
          const providerMessageId =
            payload && typeof payload === "object" ? (payload as { id?: unknown }).id : undefined;
          return typeof providerMessageId === "string" &&
            providerMessageId.length > 0 &&
            providerMessageId.length <= 256 &&
            !hasControlCharacters(providerMessageId)
            ? { state: "accepted", providerMessageId }
            : INVALID_RESPONSE;
        }
        const errorCode = providerErrorCode(payload);
        if (errorCode === undefined) return INVALID_RESPONSE;
        return classifyHttp(response.status, errorCode);
      } catch {
        return UNKNOWN;
      } finally {
        clearTimeout(timer);
        signal?.removeEventListener("abort", abortFromCaller);
      }
    },
  };
}

function classifyHttp(status: number, code: string | undefined): DeliveryResult {
  if (status === 409 && code === "invalid_idempotent_request") {
    return { state: "failed", code: "idempotency-conflict", retryable: false };
  }
  if (status === 409 && code === "concurrent_idempotent_requests") return UNKNOWN;
  if (status === 429) {
    return { state: "failed", code: "rate-limited", retryable: true };
  }
  if (status >= 500) {
    return { state: "unknown", code: "provider-unavailable", retryable: true };
  }
  if (status >= 400) return REJECTED;
  return INVALID_RESPONSE;
}
