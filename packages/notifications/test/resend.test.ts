import { afterEach, describe, expect, it } from "bun:test";
import { createResendChannel } from "../src/resend";

const ownedServers: ReturnType<typeof Bun.serve>[] = [];

afterEach(() => {
  for (const server of ownedServers.splice(0)) server.stop(true);
});

const message = {
  from: "sender@example.test",
  to: "recipient@example.test",
  subject: "Subject",
  text: "Text body",
  html: "<p>HTML body</p>",
};

function fixture(handler: (request: Request) => Response | Promise<Response>): string {
  const server = Bun.serve({ port: 0, hostname: "127.0.0.1", fetch: handler });
  ownedServers.push(server);
  return `http://127.0.0.1:${server.port}/emails`;
}

const send = (endpoint: string, options: { timeoutMs?: number; signal?: AbortSignal } = {}) =>
  createResendChannel({
    id: "resend-test",
    apiKey: "test-only-key",
    endpoint,
    timeoutMs: options.timeoutMs,
  }).send(message, { idempotencyKey: "notification/test-1", signal: options.signal });

describe("Resend channel", () => {
  it("sends the expected request and maps a valid id to accepted", async () => {
    let captured:
      | {
          method: string;
          url: string;
          authorization: string | null;
          key: string | null;
          body: unknown;
        }
      | undefined;
    const endpoint = fixture(async (request) => {
      captured = {
        method: request.method,
        url: request.url,
        authorization: request.headers.get("authorization"),
        key: request.headers.get("idempotency-key"),
        body: await request.json(),
      };
      return Response.json({ id: "email_123" });
    });
    const channel = createResendChannel({
      id: "resend-test",
      apiKey: "test-only-key",
      endpoint,
    });

    expect(channel.kind).toBe("email");
    expect(channel.idempotencyWindowMs).toBe(24 * 60 * 60 * 1000);
    expect(await channel.send(message, { idempotencyKey: "notification/test-1" })).toEqual({
      state: "accepted",
      providerMessageId: "email_123",
    });
    expect(captured?.method).toBe("POST");
    expect(new URL(captured!.url).pathname).toBe("/emails");
    expect(captured?.authorization).toBe("Bearer test-only-key");
    expect(captured?.key).toBe("notification/test-1");
    expect(await captured?.body).toEqual({
      from: message.from,
      to: [message.to],
      subject: message.subject,
      text: message.text,
      html: message.html,
    });
  });

  it("maps rate limits, idempotency conflicts and concurrent duplicates", async () => {
    expect(
      await send(fixture(() => Response.json({ name: "rate_limit_exceeded" }, { status: 429 }))),
    ).toEqual({ state: "failed", code: "rate-limited", retryable: true });
    expect(
      await send(
        fixture(() => Response.json({ name: "invalid_idempotent_request" }, { status: 409 })),
      ),
    ).toEqual({ state: "failed", code: "idempotency-conflict", retryable: false });
    expect(
      await send(
        fixture(() => Response.json({ name: "concurrent_idempotent_requests" }, { status: 409 })),
      ),
    ).toEqual({ state: "unknown", code: "transport-unknown", retryable: true });
  });

  it("maps validation/auth failures to rejected and 5xx to unknown", async () => {
    expect(
      await send(fixture(() => Response.json({ name: "validation_error" }, { status: 400 }))),
    ).toEqual({ state: "failed", code: "rejected", retryable: false });
    expect(
      await send(fixture(() => Response.json({ name: "missing_api_key" }, { status: 401 }))),
    ).toEqual({ state: "failed", code: "rejected", retryable: false });
    expect(
      await send(fixture(() => Response.json({ name: "application_error" }, { status: 500 }))),
    ).toEqual({ state: "unknown", code: "provider-unavailable", retryable: true });
  });

  it("treats malformed success bodies as unknown", async () => {
    expect(await send(fixture(() => new Response("{", { status: 200 })))).toEqual({
      state: "unknown",
      code: "invalid-response",
      retryable: true,
    });
    expect(await send(fixture(() => new Response("not json", { status: 400 })))).toEqual({
      state: "unknown",
      code: "invalid-response",
      retryable: true,
    });
  });

  it("treats a timeout and caller abort as unknown", async () => {
    const slowEndpoint = fixture(
      () =>
        new Promise<Response>((resolve) =>
          setTimeout(() => resolve(Response.json({ id: "late" })), 100),
        ),
    );
    expect(await send(slowEndpoint, { timeoutMs: 10 })).toEqual({
      state: "unknown",
      code: "transport-unknown",
      retryable: true,
    });

    const controller = new AbortController();
    const result = send(slowEndpoint, { timeoutMs: 1_000, signal: controller.signal });
    controller.abort();
    expect(await result).toEqual({
      state: "unknown",
      code: "transport-unknown",
      retryable: true,
    });
  });

  it("rejects unsafe endpoint overrides and invalid timeout options", () => {
    expect(() =>
      createResendChannel({
        id: "test",
        apiKey: "test-only",
        endpoint: "http://example.com/emails",
      }),
    ).toThrow();
    expect(() => createResendChannel({ id: "test", apiKey: "test-only", timeoutMs: 0 })).toThrow();
  });
});
