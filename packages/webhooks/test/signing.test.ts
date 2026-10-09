import { describe, expect, test } from "bun:test";
import { createHmac } from "node:crypto";
import { signWebhook, verifyWebhook } from "../src/signing";

const key = { id: "current", secret: new TextEncoder().encode("test-secret-not-production") };
const body = new Uint8Array([0, 255, 10, 13, 123, 125]);
const headers = signWebhook(body, "event-123", 1700000000, key);
const input = {
  body,
  eventId: "event-123",
  timestamp: headers["x-lenso-timestamp"]!,
  signature: headers["x-lenso-signature"]!,
  keys: [key],
  now: 1700000000,
};

describe("webhook signatures", () => {
  test("documents the v1 wire format and signs exact bytes with an explicit prefix", () => {
    const expected = createHmac("sha256", key.secret)
      .update("1700000000\nevent-123\n")
      .update(body)
      .digest("hex");
    expect(headers).toEqual({
      "x-lenso-event-id": "event-123",
      "x-lenso-timestamp": "1700000000",
      "x-lenso-signature": `v1;kid=current;mac=${expected}`,
    });
    expect(Object.isFrozen(headers)).toBe(true);
    expect(verifyWebhook(input)).toBe(true);
  });

  test("binds bytes, event ID, timestamp and key ID", () => {
    expect(verifyWebhook({ ...input, body: new TextEncoder().encode("{}") })).toBe(false);
    expect(verifyWebhook({ ...input, eventId: "event-124" })).toBe(false);
    expect(verifyWebhook({ ...input, timestamp: "1700000001" })).toBe(false);
    expect(
      verifyWebhook({ ...input, signature: input.signature.replace("current", "other") }),
    ).toBe(false);
    expect(verifyWebhook({ ...input, keys: [{ ...key, secret: new Uint8Array([1]) }] })).toBe(
      false,
    );
  });

  test("rotation accepts any matching key, including duplicate IDs", () => {
    const previous = { id: "previous", secret: new Uint8Array([1, 2, 3]) };
    const oldHeaders = signWebhook(body, input.eventId, input.now - 1, previous);
    const oldInput = {
      ...input,
      timestamp: oldHeaders["x-lenso-timestamp"]!,
      signature: oldHeaders["x-lenso-signature"]!,
      keys: [previous, key],
    };
    expect(verifyWebhook(oldInput)).toBe(true);
    expect(verifyWebhook({ ...oldInput, keys: [key] })).toBe(false);
    expect(verifyWebhook({ ...input, keys: [previous, key] })).toBe(true);
    expect(verifyWebhook({ ...input, keys: [{ ...key, secret: previous.secret }, key] })).toBe(
      true,
    );
    expect(verifyWebhook({ ...input, keys: [previous] })).toBe(false);
  });

  test("bounded canonical parsing rejects ambiguity and stale or future signatures", () => {
    for (const timestamp of [
      "01700000000",
      "+1700000000",
      "1e9",
      "1700000000\n",
      "9".repeat(1000),
    ]) {
      expect(verifyWebhook({ ...input, timestamp })).toBe(false);
    }
    for (const signature of [
      "",
      `${input.signature};extra=x`,
      input.signature.toUpperCase(),
      "x".repeat(10000),
    ]) {
      expect(verifyWebhook({ ...input, signature })).toBe(false);
    }
    expect(verifyWebhook({ ...input, now: input.now + 300 })).toBe(true);
    expect(verifyWebhook({ ...input, now: input.now + 301 })).toBe(false);
    expect(verifyWebhook({ ...input, now: input.now - 301 })).toBe(false);
    expect(verifyWebhook({ ...input, toleranceSeconds: -1 })).toBe(false);
    expect(verifyWebhook({ ...input, now: NaN })).toBe(false);
    expect(verifyWebhook({ ...input, eventId: "event\n123" })).toBe(false);
    expect(verifyWebhook({ ...input, keys: Array.from({ length: 65 }, () => ({ ...key })) })).toBe(
      false,
    );
  });

  test("invalid signing inputs have fixed errors without secrets", () => {
    for (const invalid of [
      { ...key, id: "secret\nvalue" },
      { ...key, secret: new Uint8Array() },
    ]) {
      expect(() => signWebhook(body, "event-123", input.now, invalid)).toThrow(
        "Invalid webhook signing input",
      );
    }
    expect(() => signWebhook(body, "event\n123", input.now, key)).toThrow(
      "Invalid webhook signing input",
    );
    expect(() => signWebhook(body, "event-123", -1, key)).toThrow("Invalid webhook signing input");
  });
});
