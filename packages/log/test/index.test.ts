import { describe, expect, test } from "bun:test";
import { context, trace } from "@opentelemetry/api";
import pino from "pino";
import { createLogger } from "../src/index";

function captureLogger() {
  const lines: string[] = [];
  const logger = createLogger({
    level: "trace",
    stream: { write: (line: string) => lines.push(line) },
  });
  return { logger, lines };
}

describe("@lenso/log", () => {
  test("emits structured fields, child bindings and redacts sensitive keys", async () => {
    const { logger, lines } = captureLogger();
    logger
      .child({ pluginId: "demo" })
      .info(
        { authorization: "Bearer hidden", nested: { password: "hidden" }, answer: 42 },
        "ready",
      );
    await new Promise<void>((resolve) => logger.flush(() => resolve()));
    const record = JSON.parse(lines[0]!);
    expect(record.pluginId).toBe("demo");
    expect(record.answer).toBe(42);
    expect(record.authorization).toBe("[REDACTED]");
    expect(record.nested.password).toBe("[REDACTED]");
    expect(record.msg).toBe("ready");
  });

  test("adds only valid active trace context at write time", async () => {
    const { logger, lines } = captureLogger();
    logger.info({}, "without span");
    const spanContext = {
      traceId: "0123456789abcdef0123456789abcdef",
      spanId: "0123456789abcdef",
      traceFlags: 1,
    };
    const span = trace.wrapSpanContext(spanContext);
    const originalActive = context.active;
    (context as any).active = () => trace.setSpan(originalActive.call(context), span);
    try {
      const fields = { traceId: "wrong", spanId: "wrong" };
      logger.info(fields, "with span");
      expect(fields).toEqual({ traceId: "wrong", spanId: "wrong" });
    } finally {
      (context as any).active = originalActive;
    }
    await new Promise<void>((resolve) => logger.flush(() => resolve()));

    expect(JSON.parse(lines[0]!).traceId).toBeUndefined();
    expect(JSON.parse(lines[1]!).traceId).toBe(spanContext.traceId);
    expect(JSON.parse(lines[1]!).spanId).toBe(spanContext.spanId);
  });

  test("does not close an externally supplied logger", async () => {
    const { logger: parent, lines } = captureLogger();
    const child = createLogger({ logger: parent });
    child.info({}, "child");
    await new Promise<void>((resolve) => child.flush(() => resolve()));
    parent.info({}, "parent remains usable");
    await new Promise<void>((resolve) => parent.flush(() => resolve()));
    expect(lines).toHaveLength(2);
  });

  test("borrowed Pino preserves child options and its error serializer", () => {
    const lines: string[] = [];
    const parent = pino(
      {
        level: "info",
        serializers: { err: () => ({ type: "custom-safe-error" }) },
      },
      { write: (line) => lines.push(line) },
    );
    const child = createLogger({ logger: parent }).child(
      { pluginId: "borrowed" },
      {
        level: "debug",
        redact: ["private"],
        msgPrefix: "child: ",
      },
    );
    child.debug({ private: "hidden" }, "ready");
    child.error(new Error("hidden"), "failed");
    expect(JSON.parse(lines[0]!).msg).toBe("child: ready");
    expect(JSON.parse(lines[0]!).private).toBe("[Redacted]");
    expect(JSON.parse(lines[1]!).err).toEqual({ type: "custom-safe-error" });
    parent.info({}, "still open");
    expect(lines).toHaveLength(3);
  });

  test("default JSON and readable output use stderr, never protocol stdout", async () => {
    for (const pretty of [false, true]) {
      const entry = new URL("../src/index.ts", import.meta.url).pathname;
      const child = Bun.spawn(
        [
          process.execPath,
          "-e",
          `import { createLogger } from ${JSON.stringify(entry)};
         const logger = createLogger({ pretty: ${pretty} });
         logger.info({ pluginId: "output-test" }, "ready");
         logger.flush();`,
        ],
        { stdout: "pipe", stderr: "pipe" },
      );
      const [stdout, stderr, exit] = await Promise.all([
        new Response(child.stdout).text(),
        new Response(child.stderr).text(),
        child.exited,
      ]);
      expect(exit).toBe(0);
      expect(stdout).toBe("");
      expect(stderr).toContain("ready");
      if (!pretty) expect(JSON.parse(stderr).pluginId).toBe("output-test");
    }
  });

  test("omits default Error details and full payloads", () => {
    const { logger, lines } = captureLogger();
    logger.error(
      {
        err: new Error("session credential hidden-value"),
        payload: { session: "hidden-value" },
        request: { headers: { authorization: "Bearer hidden-value" } },
      },
      "operation failed",
    );
    expect(lines[0]).not.toContain("hidden-value");
    expect(JSON.parse(lines[0]!).err).toEqual({ type: "Error" });
    logger.error(new Error("hidden-value"));
    logger.error({ err: new Error("hidden-value") });
    logger.error(new Error("hidden-value"), undefined);
    logger.error({ err: new Error("hidden-value") }, undefined);
    expect(lines.join("")).not.toContain("hidden-value");
  });

  test("supports typed custom levels and safely handles a configured error key", () => {
    const lines: string[] = [];
    const logger = createLogger({
      customLevels: { notice: 35 },
      errorKey: "failure",
      stream: { write: (line) => lines.push(line) },
    });
    logger.notice({}, "ready");
    logger.error({ failure: new Error("hidden-value") });
    logger.error(new Error("hidden-value"));
    expect(JSON.parse(lines[0]!).level).toBe(35);
    expect(JSON.parse(lines[1]!).failure).toEqual({ type: "Error" });
    expect(lines.join("")).not.toContain("hidden-value");
  });

  test("omits unknown structured errors, causes and untrusted codes", () => {
    const { logger, lines } = captureLogger();
    const error = new Error("private-message", { cause: new Error("private-cause") });
    error.stack = "private-stack";
    const unknown = {
      message: "private-message",
      stack: "private-stack",
      code: "private-code",
      cause: error,
    };
    for (const value of [error, unknown, "private-primitive", null]) {
      logger.error({ err: value });
      logger.error({ error: value });
      logger.child({ pluginId: "safe-plugin" }).error({ err: value });
    }
    expect(lines.join("")).not.toContain("private-");
    expect(lines.map((line) => JSON.parse(line).msg)).toEqual(
      Array.from({ length: 12 }, () => "Error details omitted"),
    );
  });

  test("only classifier code and phase survive, and classifier failures are optional", () => {
    const lines: string[] = [];
    const original = new Error("private-message");
    const logger = createLogger({
      classifyError(error) {
        expect(error).toBe(original);
        return { code: "PUBLIC_FAILURE", phase: "handler", message: "private-message" };
      },
      stream: { write: (line) => lines.push(line) },
    });
    logger.child({ requestId: "safe-request" }).error({ err: original });
    expect(JSON.parse(lines[0]!).err).toEqual({
      type: "Error",
      code: "PUBLIC_FAILURE",
      phase: "handler",
    });
    expect(lines[0]).not.toContain("private-");
    const failing = createLogger({
      classifyError() {
        throw new Error("private-classifier-failure");
      },
      stream: { write: (line) => lines.push(line) },
    });
    expect(() => failing.error(original)).not.toThrow();
    expect(JSON.parse(lines[1]!).err).toEqual({ type: "Error" });
    expect(lines[1]).not.toContain("private-");
  });

  test("custom serializers and external logger policies remain explicit owner choices", () => {
    const lines: string[] = [];
    const stream = { write: (line: string) => lines.push(line) };
    const raw = new Error("private-owner-choice");
    createLogger({ stream, serializers: { err: pino.stdSerializers.err } }).error(raw);
    const external = pino({}, stream);
    createLogger({ logger: external }).error(raw);
    createLogger({ logger: external, traceContext: false }).error(raw);
    expect(lines).toHaveLength(3);
    expect(lines.every((line) => line.includes("private-owner-choice"))).toBe(true);
  });
});
