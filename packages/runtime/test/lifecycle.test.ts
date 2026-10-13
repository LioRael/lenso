import { expect, spyOn, test } from "bun:test";
import { startApp } from "@lenso/core";
import { definePlugin, type Plugin } from "@lenso/core/plugin";
import { RuntimeHost } from "@lenso/runtime-sdk";
import {
  activateRuntimeApp,
  createRuntimePlugin,
  startRuntimeApp,
  type RuntimeHandlers,
  type RuntimeService,
} from "../src/index";

// A client factory can fail after connection acquisition. Core rollback tests do not
// establish that this adapter registers host cleanup before constructing clients.
test("failed client setup drains and closes the acquired host exactly once", async () => {
  const events: string[] = [];
  const host = {
    drain: async () => {
      events.push("drain");
    },
    close: async () => {
      events.push("close");
    },
  } as unknown as RuntimeHost;
  const connection = spyOn(RuntimeHost, "connect").mockResolvedValue(host);
  const failure = new Error("Client construction failed.");
  const plugin = createRuntimePlugin({
    id: "runtime",
    connection: {
      namespace: "test",
      service: "client",
      instance: "one",
      contract: { version: "1", methods: {} },
    },
    clients() {
      throw failure;
    },
  });
  try {
    await expect(startRuntimeApp({ plugins: [plugin] })).rejects.toBe(failure);
    expect(events).toEqual(["drain", "close"]);
  } finally {
    connection.mockRestore();
  }
});

// Core rollback alone cannot prove that Runtime readiness waits for native setup,
// or that late binding can use a native plugin requiring this exact Runtime client.
test("native setup failure never activates and closes the registered host once", async () => {
  const events: string[] = [];
  const host = {
    activate: async () => {
      events.push("ready");
    },
    drain: async () => {
      events.push("drain");
    },
    close: async () => {
      events.push("close");
    },
  } as unknown as RuntimeHost;
  const connection = spyOn(RuntimeHost, "connect").mockResolvedValue(host);
  const failure = new Error("Native setup failed.");
  const runtime = createRuntimePlugin({
    id: "runtime",
    connection: { namespace: "test", service: "frontend", contract: { version: "1", methods: {} } },
    clients: () => ({}),
  });
  const native = definePlugin({
    id: "native",
    requires: [runtime],
    setup() {
      throw failure;
    },
  });
  try {
    await expect(startRuntimeApp({ plugins: [runtime, native] })).rejects.toBe(failure);
    expect(connection.mock.calls[0]?.[0].ready).toBe(false);
    expect(events).toEqual(["drain", "close"]);
  } finally {
    connection.mockRestore();
  }
});

test("late exports bind after native setup and activate exact instances only once", async () => {
  const events: string[] = [];
  const handler = async () => "native result";
  const clients = {};
  let releaseDrain!: () => void;
  let drainStarted!: () => void;
  const started = new Promise<void>((resolve) => {
    drainStarted = resolve;
  });
  const drained = new Promise<void>((resolve) => {
    releaseDrain = resolve;
  });
  const host = {
    activate: async (handlers: unknown) => {
      expect(handlers).toEqual({ invoke: handler });
      events.push("ready");
    },
    drain: async () => {
      events.push("drain");
      drainStarted();
      await drained;
    },
    close: async () => {
      events.push("close");
    },
  } as unknown as RuntimeHost;
  const connection = spyOn(RuntimeHost, "connect").mockResolvedValue(host);
  const runtime: Plugin<RuntimeService<typeof clients>> = createRuntimePlugin({
    id: "runtime",
    connection: { namespace: "test", service: "frontend", contract: { version: "1", methods: {} } },
    clients: () => clients,
    exports(running, injected): RuntimeHandlers {
      expect(injected).toBe(clients);
      events.push("exports");
      return { invoke: running.get(native).invoke };
    },
  });
  const native = definePlugin({
    id: "native",
    requires: [runtime],
    setup(context) {
      expect(context.get(runtime).clients).toBe(clients);
      events.push("native");
      context.onCleanup(() => {
        events.push("native cleanup");
      });
      return {
        invoke: handler,
        activate() {
          throw new Error("Must not discover methods.");
        },
      };
    },
  });
  try {
    const running = await startRuntimeApp({ plugins: [runtime, native] });
    try {
      await Promise.all([
        activateRuntimeApp(running, [runtime, native, { ...runtime }]),
        activateRuntimeApp(running, [runtime]),
      ]);
      expect(events).toEqual(["native", "exports", "ready"]);
    } finally {
      const stopping = running.stop();
      expect(running.stop()).toBe(stopping);
      try {
        await started;
        expect(events).toEqual(["native", "exports", "ready", "drain"]);
        expect(await running.get(native).invoke()).toBe("native result");
      } finally {
        releaseDrain();
      }
      await stopping;
    }
    expect(events).toEqual(["native", "exports", "ready", "drain", "close", "native cleanup"]);
  } finally {
    connection.mockRestore();
  }
});

test("failed export binding stops the app and retains activation and cleanup errors", async () => {
  const events: string[] = [];
  const bindingFailure = new Error("Export binding failed.");
  const cleanupFailure = new Error("Host drain failed.");
  const host = {
    activate: async () => {
      events.push("ready");
    },
    drain: async () => {
      events.push("drain");
      throw cleanupFailure;
    },
    close: async () => {
      events.push("close");
    },
  } as unknown as RuntimeHost;
  const connection = spyOn(RuntimeHost, "connect").mockResolvedValue(host);
  const runtime = createRuntimePlugin({
    id: "runtime",
    connection: { namespace: "test", service: "frontend", contract: { version: "1", methods: {} } },
    clients: () => undefined,
    exports() {
      throw bindingFailure;
    },
  });
  const native = definePlugin({
    id: "native",
    requires: [runtime],
    setup(context) {
      context.onCleanup(() => {
        events.push("native cleanup");
      });
      return {};
    },
  });
  try {
    const error = await startRuntimeApp({ plugins: [runtime, native] }).catch(
      (caught: unknown) => caught,
    );
    expect(error).toBeInstanceOf(AggregateError);
    const [activation, cleanup] = (error as AggregateError).errors;
    expect(activation).toBe(bindingFailure);
    expect(cleanup).toBeInstanceOf(AggregateError);
    expect(cleanup.errors).toEqual([cleanupFailure]);
    expect(events).toEqual(["drain", "close", "native cleanup"]);
  } finally {
    connection.mockRestore();
  }
});

test("Core startup leaves outbound-only callers registered without binding exports", async () => {
  const events: string[] = [];
  const host = {
    activate: async () => {
      events.push("ready");
    },
    drain: async () => {
      events.push("drain");
    },
    close: async () => {
      events.push("close");
    },
  } as unknown as RuntimeHost;
  const connection = spyOn(RuntimeHost, "connect").mockResolvedValue(host);
  const runtime = createRuntimePlugin({
    id: "runtime",
    connection: { namespace: "test", service: "frontend", contract: { version: "1", methods: {} } },
    clients: () => undefined,
    exports() {
      throw new Error("Finite callers must not bind exports.");
    },
  });
  try {
    const running = await startApp({ plugins: [runtime] });
    expect(running.get(runtime).host).toBe(host);
    expect(events).toEqual([]);
    await running.stop();
    expect(events).toEqual(["drain", "close"]);
  } finally {
    connection.mockRestore();
  }
});

// Existing cleanup-order assertions use immediately settled drains. They cannot
// detect native cleanup overtaking in-flight Runtime work or Runtime-only disposal
// incorrectly stopping the borrowed Core app's ordinary services.
test("Runtime disposal waits for drain without stopping native calls", async () => {
  const events: string[] = [];
  let releaseDrain!: () => void;
  let drainStarted!: () => void;
  const started = new Promise<void>((resolve) => {
    drainStarted = resolve;
  });
  const drained = new Promise<void>((resolve) => {
    releaseDrain = resolve;
  });
  const host = {
    drain: async (timeoutMs: number) => {
      expect(timeoutMs).toBe(25);
      events.push("drain");
      drainStarted();
      await drained;
    },
    close: async () => {
      events.push("close");
    },
  } as unknown as RuntimeHost;
  const connection = spyOn(RuntimeHost, "connect").mockResolvedValue(host);
  const runtime = createRuntimePlugin({
    id: "runtime",
    connection: { namespace: "test", service: "caller", contract: { version: "1", methods: {} } },
    clients: () => undefined,
    drainTimeoutMs: 25,
  });
  const native = definePlugin({
    id: "native",
    setup(context) {
      let calls = 0;
      context.onCleanup(() => {
        events.push("native cleanup");
      });
      return {
        async invoke() {
          return ++calls;
        },
      };
    },
  });
  try {
    const running = await startApp({ plugins: [runtime, native] });
    try {
      const disposing = running.get(runtime).disposer!();
      await started;
      expect(events).toEqual(["drain"]);
      expect(await running.get(native).invoke()).toBe(1);
      releaseDrain();
      await disposing;
      expect(events).toEqual(["drain", "close"]);
      expect(await running.get(native).invoke()).toBe(2);
      await running.get(runtime).disposer!();
      expect(events).toEqual(["drain", "close"]);
    } finally {
      releaseDrain();
      await running.stop();
    }
    expect(events).toEqual(["drain", "close", "native cleanup"]);
  } finally {
    connection.mockRestore();
  }
});

test("early handler providers stay supported and SDK activation failure stops the app", async () => {
  const events: string[] = [];
  const failure = new Error("Ready failed.");
  const handler = async () => "provider";
  const host = {
    activate: async () => {
      events.push("ready");
      throw failure;
    },
    drain: async () => {
      events.push("drain");
    },
    close: async () => {
      events.push("close");
    },
  } as unknown as RuntimeHost;
  const connection = spyOn(RuntimeHost, "connect").mockResolvedValue(host);
  const provider = definePlugin({
    id: "provider",
    setup: () => ({ invoke: handler }),
  });
  const runtime = createRuntimePlugin({
    id: "runtime",
    requires: [provider],
    connection: { namespace: "test", service: "frontend", contract: { version: "1", methods: {} } },
    handlers: (context) => ({ invoke: context.get(provider).invoke }),
    clients: () => undefined,
  });
  try {
    await expect(startRuntimeApp({ plugins: [runtime, provider] })).rejects.toBe(failure);
    expect(connection.mock.calls[0]?.[0].handlers).toEqual({ invoke: handler });
    expect(events).toEqual(["ready", "drain", "close"]);
  } finally {
    connection.mockRestore();
  }
});

test("ambiguous early and late handler factories are rejected before setup", () => {
  expect(() =>
    createRuntimePlugin({
      id: "runtime",
      connection: {
        namespace: "test",
        service: "frontend",
        contract: { version: "1", methods: {} },
      },
      handlers: () => ({}),
      exports: () => ({}),
      clients: () => undefined,
    }),
  ).toThrow("either handlers or exports");
});
