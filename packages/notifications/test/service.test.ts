import { afterEach, describe, expect, it } from "bun:test";
import { createNotificationService } from "../src/service";
import { renderTemplate } from "../src/render";
import { input, localFixture, template } from "./helpers";

const cleanups: (() => void)[] = [];
afterEach(() => {
  cleanups.splice(0).forEach((close) => close());
});
async function fixture(...args: Parameters<typeof localFixture>) {
  const result = await localFixture(...args);
  cleanups.push(result.close);
  return result;
}

describe("notification service with real SQLite and local HTTP", () => {
  it("validates variables and safely renders plain text into HTML", async () => {
    const message = await renderTemplate(template, { name: "<script>&\"'</script>" }, input.email);
    expect(message.html).toContain("&lt;script&gt;&amp;&quot;&#39;&lt;/script&gt;");
    expect(message.html).not.toContain("<script>");
    expect(message.text).toContain("<script>&\"'</script>");
    await expect(renderTemplate(template, { name: "A\r\nB" }, input.email)).rejects.toMatchObject({
      code: "invalid-input",
    });
    await expect(renderTemplate(template, { name: 1 }, input.email)).rejects.toMatchObject({
      code: "invalid-input",
    });
    await expect(
      renderTemplate(template, { name: "Alice", actor: "admin" }, input.email),
    ).rejects.toMatchObject({ code: "invalid-input" });
    const { service } = await fixture();
    await expect(service.create({ ...input, variables: { name: 1 } })).rejects.toMatchObject({
      code: "invalid-input",
    });
    expect(await service.list({ tenantId: input.tenantId, limit: 100 })).toEqual([]);
  });

  it("suppresses optional notifications by default and explicitly respects opt-in and opt-out", async () => {
    let sent = 0;
    const { service } = await fixture(
      () => {
        sent++;
        return Response.json({ id: "provider-1" });
      },
      {
        templates: [{ ...template, necessity: "optional" }],
      },
    );
    const suppressed = await service.create(input);
    expect(suppressed.state).toBe("suppressed");
    await service.deliver(suppressed.id);
    expect(sent).toBe(0);
    const key = {
      tenantId: input.tenantId,
      recipientId: input.recipientId,
      category: "orders",
      channelId: "email",
    };
    await service.setPreference({ ...key, enabled: true });
    const pending = await service.create({ ...input, idempotencyKey: "order-2" });
    expect(pending.state).toBe("pending");
    await service.setPreference({ ...key, enabled: false });
    expect((await service.deliver(pending.id))?.state).toBe("suppressed");
    expect(await service.attempts(pending.id)).toEqual([]);
    expect(sent).toBe(0);
    await service.setPreference({ ...key, enabled: true });
    const allowed = await service.create({ ...input, idempotencyKey: "order-3" });
    expect((await service.deliver(allowed.id))?.state).toBe("accepted");
    expect(sent).toBe(1);
  });

  it("uses business-declared required necessity and chooses an enabled channel without later fallback", async () => {
    const { store, channel } = await fixture();
    const alternate = { ...channel, id: "alternate" };
    const service = createNotificationService({
      store,
      channels: [channel, alternate],
      templates: [
        template,
        { ...template, id: "optional", necessity: "optional", channels: ["email", "alternate"] },
      ],
    });
    await service.setPreference({
      tenantId: input.tenantId,
      recipientId: input.recipientId,
      category: "orders",
      channelId: "email",
      enabled: false,
    });
    expect((await service.create(input)).state).toBe("pending");
    await service.setPreference({
      tenantId: input.tenantId,
      recipientId: input.recipientId,
      category: "orders",
      channelId: "alternate",
      enabled: true,
    });
    const selected = await service.create({
      ...input,
      templateId: "optional",
      idempotencyKey: "optional-1",
    });
    expect(selected.channelId).toBe("alternate");
    await expect(
      service.create({ ...input, channels: ["unknown"], idempotencyKey: "invalid" }),
    ).rejects.toMatchObject({ code: "channel-unavailable" });
  });

  it("resolves duplicate concurrent creates through a persistent unique key and rejects changed parameters", async () => {
    const { service } = await fixture();
    const created = await Promise.all(Array.from({ length: 20 }, () => service.create(input)));
    expect(new Set(created.map((record) => record.id)).size).toBe(1);
    await expect(
      service.create({ ...input, email: "different@example.test" }),
    ).rejects.toMatchObject({ code: "idempotency-conflict" });
    const race = await Promise.allSettled([
      service.create({ ...input, idempotencyKey: "race" }),
      service.create({ ...input, idempotencyKey: "race", variables: { name: "Bob" } }),
    ]);
    expect(race.filter((result) => result.status === "fulfilled")).toHaveLength(1);
    expect(race.filter((result) => result.status === "rejected")).toHaveLength(1);
    const otherTenant = await service.create({ ...input, tenantId: "tenant-b" });
    const otherScope = await service.create({ ...input, scope: "order.updated" });
    expect(new Set([created[0].id, otherTenant.id, otherScope.id]).size).toBe(3);
    expect(await service.list({ tenantId: "tenant-b", limit: 100 })).toHaveLength(1);
  });

  it("records acceptance separately from delivery, with immutable template and business snapshots", async () => {
    const { service, store } = await fixture();
    const created = await service.create(input);
    const accepted = await service.deliver(created.id);
    expect(accepted).toMatchObject({
      state: "accepted",
      attemptCount: 1,
      providerMessageId: "provider-1",
    });
    expect(await service.attempts(created.id)).toMatchObject([{ state: "accepted", number: 1 }]);
    expect(await store.get(created.id)).toMatchObject({
      businessId: "order-1",
      templateVersion: "1",
      recipientId: "alice",
    });
    expect(await service.markDelivered(created.id, "wrong-id")).toBe(false);
    expect(await service.markDelivered(created.id, "provider-1")).toBe(true);
    expect((await service.deliver(created.id))?.state).toBe("delivered");
    expect((await service.get(created.id))?.attemptCount).toBe(1);
    expect(JSON.stringify(accepted)).not.toContain(input.email);
    expect(JSON.stringify(accepted)).not.toContain("Order for");
  });

  it("retries a rejected transient attempt with the same logical record, provider key and body", async () => {
    const requests: { key: string | null; body: unknown }[] = [];
    const { service } = await fixture(async (request) => {
      requests.push({ key: request.headers.get("idempotency-key"), body: await request.json() });
      return requests.length === 1
        ? Response.json({ name: "rate_limit_exceeded" }, { status: 429 })
        : Response.json({ id: "provider-retry" });
    });
    const created = await service.create(input);
    expect((await service.deliver(created.id))?.state).toBe("failed");
    expect((await service.deliver(created.id))?.state).toBe("accepted");
    expect(requests).toHaveLength(2);
    expect(requests[0]).toEqual(requests[1]);
    expect((await service.attempts(created.id)).map((attempt) => attempt.state)).toEqual([
      "failed",
      "accepted",
    ]);
    expect(await service.list({ tenantId: input.tenantId, limit: 100 })).toHaveLength(1);
  });

  it("keeps a timeout uncertain and reuses the original snapshot on retry", async () => {
    const requests: { key: string | null; body: unknown }[] = [];
    const { service, store, channel } = await fixture(
      async (request) => {
        requests.push({ key: request.headers.get("idempotency-key"), body: await request.json() });
        if (requests.length === 1) await Bun.sleep(60);
        return Response.json({ id: "provider-uncertain" });
      },
      { timeoutMs: 10 },
    );
    const created = await service.create(input);
    expect(await service.deliver(created.id)).toMatchObject({ state: "unknown", retryable: true });
    const restarted = createNotificationService({
      store,
      channels: [channel],
      templates: [{ ...template, version: "2", text: "Changed template {{name}}" }],
    });
    expect((await restarted.deliver(created.id))?.state).toBe("accepted");
    expect(requests[0]).toEqual(requests[1]);
    expect((await restarted.get(created.id))?.templateVersion).toBe("1");
    expect((await restarted.attempts(created.id)).map((attempt) => attempt.state)).toEqual([
      "unknown",
      "accepted",
    ]);
  });

  it("does not erase earlier uncertainty on a definite rejection and blocks retry after key expiry", async () => {
    let now = 1_000_000;
    let requests = 0;
    const { service } = await fixture(
      () => {
        requests++;
        return requests === 1
          ? new Response("broken", { status: 200 })
          : Response.json({ name: "rate_limit_exceeded" }, { status: 429 });
      },
      { clock: () => now },
    );
    const created = await service.create(input);
    expect((await service.deliver(created.id))?.state).toBe("unknown");
    expect((await service.deliver(created.id))?.state).toBe("unknown");
    now += 24 * 60 * 60 * 1000;
    expect(await service.deliver(created.id)).toMatchObject({
      state: "unknown",
      error: "deduplication-expired",
      retryable: false,
      attemptCount: 2,
    });
    expect(requests).toBe(2);
    expect(await service.recoverable()).toEqual([]);
  });

  it("recovers a crashed sending attempt and prevents an old claim overwriting the winner", async () => {
    let now = 1_000_000;
    const { service, store } = await fixture(undefined, { clock: () => now });
    const created = await service.create(input);
    const original = (await store.get(created.id))!;
    const stale = {
      ...original,
      revision: 1,
      state: "sending" as const,
      attemptCount: 1,
      firstRequestAt: now,
      leaseUntil: now + 1000,
    };
    expect(
      await store.save(stale, 0, {
        id: "crashed-attempt",
        notificationId: created.id,
        number: 1,
        state: "sending",
        startedAt: now,
        finishedAt: null,
        providerMessageId: null,
        error: null,
      }),
    ).toBe(true);
    now += 2000;
    expect(await service.recoverable()).toHaveLength(1);
    expect(await service.deliver(created.id)).toMatchObject({ state: "accepted", attemptCount: 2 });
    expect((await service.attempts(created.id)).map((attempt) => attempt.state)).toEqual([
      "unknown",
      "accepted",
    ]);
    expect(await store.save({ ...stale, revision: 2, state: "accepted" }, 1)).toBe(false);
  });

  it("allows only one live claimant to call HTTP concurrently and emits only safe log events", async () => {
    const events: unknown[] = [];
    let sent = 0;
    const { service } = await fixture(
      async () => {
        sent++;
        await Bun.sleep(10);
        return Response.json({ id: "provider-1" });
      },
      { onDelivery: (event) => events.push(event) },
    );
    const created = await service.create(input);
    const results = await Promise.allSettled([
      service.deliver(created.id),
      service.deliver(created.id),
    ]);
    expect(results.filter((result) => result.status === "fulfilled")).toHaveLength(1);
    expect(results.filter((result) => result.status === "rejected")).toHaveLength(1);
    expect(sent).toBe(1);
    const logged = JSON.stringify(events);
    expect(logged).not.toContain(input.email);
    expect(logged).not.toContain("Alice");
    expect(logged).not.toContain("Order for");
    expect(logged).not.toContain("local-fixture-key");
  });

  it("rechecks provider expiry after a delayed claim commit before making another HTTP request", async () => {
    let now = 1_000_000;
    let requests = 0;
    const { service, store, channel } = await fixture(
      () => {
        requests++;
        return new Response("broken", { status: 200 });
      },
      { clock: () => now },
    );
    const created = await service.create(input);
    await service.deliver(created.id);
    const firstRequestAt = (await store.get(created.id))!.firstRequestAt!;
    now = firstRequestAt + channel.idempotencyWindowMs - 200_000;
    const delayed = createNotificationService({
      store: {
        ...store,
        async save(record, expected, attempt) {
          const result = await store.save(record, expected, attempt);
          if (record.state === "sending") now += 300_000;
          return result;
        },
      },
      templates: [template],
      channels: [channel],
      clock: () => now,
    });
    expect(await delayed.deliver(created.id)).toMatchObject({
      state: "unknown",
      error: "deduplication-expired",
      retryable: false,
    });
    expect(requests).toBe(1);
    expect((await store.attempts(created.id)).map((attempt) => attempt.state)).toEqual([
      "unknown",
      "failed",
    ]);
  });
});
