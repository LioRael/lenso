import { expect, test } from "bun:test";
import { startApp } from "@lenso/core";
import { cachedApp, cachedGreeting, messageCache } from "./cached";

test("optional message cache preserves greet count and validates every call", async () => {
  const app = await startApp(cachedApp);
  try {
    const greeting = app.get(cachedGreeting);
    expect(await greeting.greet({ name: "Bun" })).toEqual({ message: "Hello, Bun!", count: 1 });
    expect(await greeting.greet({ name: "Bun" })).toEqual({ message: "Hello, Bun!", count: 2 });
    const digest = new Uint8Array(
      await crypto.subtle.digest("SHA-256", new TextEncoder().encode(JSON.stringify("Bun"))),
    );
    const key = Array.from(digest, (byte) => byte.toString(16).padStart(2, "0")).join("");
    expect(await app.get(messageCache).get(key)).toEqual({ status: "hit", value: "Hello, Bun!" });
    await expect(greeting.greet({ name: "" })).rejects.toThrow();
    await app.get(messageCache).invalidate();
    expect(await greeting.greet({ name: "Bun" })).toEqual({ message: "Hello, Bun!", count: 3 });
    expect((await greeting.greet({ name: "x".repeat(300) })).count).toBe(4);
    expect((await greeting.greet({ name: "x".repeat(20_000) })).count).toBe(5);
  } finally {
    await app.stop();
  }
});

test("projection keys preserve lone surrogates and size gate includes JSON expansion", async () => {
  const app = await startApp(cachedApp);
  try {
    const greeting = app.get(cachedGreeting);
    for (const name of ["a\ud800", "a\ud801", "\u0000".repeat(16_384)]) {
      expect((await greeting.greet({ name })).message).toBe(`Hello, ${name}!`);
    }
  } finally {
    await app.stop();
  }
});
