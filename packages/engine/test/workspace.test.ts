import { expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  createWorkspaceSupervisor,
  defineWorkspace,
  type WorkspaceProcessService,
  type WorkspaceServiceState,
} from "../src/workspace";
import { EngineError } from "../src/diagnostics";

const command = [process.execPath, "-e", "setInterval(()=>{},1000)"];
function service(
  id: string,
  extra: Partial<WorkspaceProcessService> = {},
): WorkspaceProcessService {
  return {
    id,
    kind: "process",
    cwd: ".",
    command,
    ready: async () => {},
    stopTimeoutMs: 100,
    ...extra,
  };
}
async function until(check: () => boolean) {
  const limit = Date.now() + 5000;
  while (!check()) {
    if (Date.now() > limit) throw new Error("Condition timed out");
    await Bun.sleep(10);
  }
}
function alive(pid: number) {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

test("workspace rejects duplicate IDs, missing dependencies and cycles", () => {
  expect(() => defineWorkspace({ services: [service("a"), service("a")] })).toThrow("duplicate");
  expect(() => defineWorkspace({ services: [service("a", { dependsOn: ["missing"] })] })).toThrow(
    "Missing",
  );
  expect(() =>
    defineWorkspace({
      services: [service("a", { dependsOn: ["b"] }), service("b", { dependsOn: ["a"] })],
    }),
  ).toThrow("Cyclic");
});

test("dependencies await actual readiness, activation precedes old shutdown, close repeats", async () => {
  let release!: () => void;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  const order: string[] = [];
  let oldPid: number | undefined;
  const events: WorkspaceServiceState[] = [];
  const startup = createWorkspaceSupervisor({
    workspace: defineWorkspace({
      services: [
        service("consumer", {
          dependsOn: ["producer"],
          ready: async () => {
            order.push("consumer");
          },
        }),
        service("producer", {
          ready: async ({ instance, process: child }) => {
            expect(instance).toBeTruthy();
            order.push("producer-start");
            await gate;
            order.push("producer-ready");
            oldPid ??= child.pid;
            return { urls: ["http://localhost:1234"] };
          },
          activate: async () => {
            expect(alive(oldPid!)).toBe(true);
            order.push("activate");
          },
        }),
      ],
    }),
    stdout: "ignore",
    stderr: "ignore",
    onEvent(event) {
      events.push(event);
      throw new Error("observer");
    },
  });
  await until(() => order.includes("producer-start"));
  expect(order).not.toContain("consumer");
  release();
  const supervisor = await startup;
  try {
    expect(order).toEqual(["producer-start", "producer-ready", "activate", "consumer"]);
    const old = supervisor.states.find((state) => state.id === "producer")!;
    await supervisor.rebuild("producer");
    const next = supervisor.states.find((state) => state.id === "producer")!;
    expect(next.instance).not.toBe(old.instance);
    expect(next.version).toBe(2);
    expect(next.pid).not.toBe(old.pid);
    expect(alive(old.pid!)).toBe(false);
    expect(next.urls).toEqual(["http://localhost:1234"]);
  } finally {
    const close = supervisor.close();
    expect(supervisor.close()).toBe(close);
    await close;
    await supervisor.done;
  }
  for (const event of events) if (event.pid) expect(alive(event.pid)).toBe(false);
});

test("shutdown drains dependents before stopping their dependencies", async () => {
  const root = mkdtempSync(join(tmpdir(), "lenso-workspace-drain-"));
  const logfile = join(root, "closed");
  const make = (id: string, dependsOn: string[] = []) =>
    service(id, {
      dependsOn,
      stopTimeoutMs: 1000,
      command: [
        process.execPath,
        "-e",
        `
      import {appendFileSync,writeFileSync} from 'node:fs';
      process.once('SIGTERM', async () => {
        appendFileSync(${JSON.stringify(logfile)}, ${JSON.stringify(id + "\n")});
        await Bun.sleep(50);
        process.exit(0);
      });
      writeFileSync(${JSON.stringify(join(root, id))}, 'ready');
      setInterval(()=>{},1000);
    `,
      ],
      async ready() {
        while (!(await Bun.file(join(root, id)).exists())) await Bun.sleep(5);
      },
    });
  const supervisor = await createWorkspaceSupervisor({
    workspace: { services: [make("consumer", ["producer"]), make("producer")] },
    stdout: "ignore",
    stderr: "ignore",
  });
  try {
    await supervisor.close();
    await supervisor.done;
    expect(await Bun.file(logfile).text()).toBe("consumer\nproducer\n");
  } finally {
    await supervisor.close();
    rmSync(root, { recursive: true, force: true });
  }
});

test("failed build, ready and activation retain the previous instance", async () => {
  const root = mkdtempSync(join(tmpdir(), "lenso-workspace-"));
  let readyFail = false,
    activateFail = false;
  writeFileSync(join(root, "build.ts"), "process.exit(0)");
  const supervisor = await createWorkspaceSupervisor({
    root,
    stdout: "ignore",
    stderr: "ignore",
    workspace: {
      services: [
        service("worker", {
          build: [process.execPath, "build.ts"],
          ready: async () => {
            if (readyFail) throw new Error("not ready");
          },
          activate: async () => {
            if (activateFail) throw new Error("incompatible");
          },
        }),
      ],
    },
  });
  try {
    const old = supervisor.states[0]!;
    writeFileSync(join(root, "build.ts"), "process.exit(1)");
    await supervisor.rebuild("worker");
    expect(supervisor.states[0]!.retained).toEqual({
      instance: old.instance!,
      version: 1,
      pid: old.pid,
    });
    expect(alive(old.pid!)).toBe(true);
    const failedInstance = supervisor.states[0]!.instance;
    writeFileSync(join(root, "build.ts"), "process.exit(0)");
    readyFail = true;
    await supervisor.rebuild("worker");
    expect(supervisor.states[0]!.instance).not.toBe(failedInstance);
    expect(alive(old.pid!)).toBe(true);
    readyFail = false;
    activateFail = true;
    await supervisor.rebuild("worker");
    expect(supervisor.states[0]!.status).toBe("failed");
    expect(alive(old.pid!)).toBe(true);
  } finally {
    await supervisor.close();
    rmSync(root, { recursive: true, force: true });
  }
});

test("initial partial failure rolls back every process and throws", async () => {
  const pids: number[] = [];
  await expect(
    createWorkspaceSupervisor({
      stdout: "ignore",
      stderr: "ignore",
      workspace: {
        services: [
          service("first", {
            ready: async ({ process }) => {
              pids.push(process.pid);
            },
          }),
          service("second", {
            dependsOn: ["first"],
            ready: async ({ process }) => {
              pids.push(process.pid);
              throw new Error("startup");
            },
          }),
        ],
      },
    }),
  ).rejects.toThrow("startup");
  expect(pids).toHaveLength(2);
  for (const pid of pids) expect(alive(pid)).toBe(false);
});

test("explicit affected watch rebuilds only its owner", async () => {
  const root = mkdtempSync(join(tmpdir(), "lenso-workspace-watch-"));
  writeFileSync(join(root, "a.txt"), "one");
  writeFileSync(join(root, "b.txt"), "one");
  const supervisor = await createWorkspaceSupervisor({
    root,
    stdout: "ignore",
    stderr: "ignore",
    workspace: {
      services: [
        service("a", { watch: ["a.txt"] }),
        service("b", { watch: ["b.txt"], dependsOn: ["a"] }),
      ],
    },
  });
  try {
    const b = supervisor.states.find((state) => state.id === "b")!;
    writeFileSync(join(root, "a.txt"), "two");
    await until(
      () =>
        supervisor.states.find((state) => state.id === "a")!.version === 2 &&
        supervisor.states.find((state) => state.id === "a")!.status === "ready",
    );
    expect(supervisor.states.find((state) => state.id === "b")!.instance).toBe(b.instance);
    expect(supervisor.states.find((state) => state.id === "b")!.pid).toBe(b.pid);
  } finally {
    await supervisor.close();
    rmSync(root, { recursive: true, force: true });
  }
});

test("serial rebuilds coalesce and close cancels owned builds", async () => {
  const root = mkdtempSync(join(tmpdir(), "lenso-workspace-build-"));
  writeFileSync(join(root, "build.ts"), "process.exit(0)");
  const controller = new AbortController();
  const events: WorkspaceServiceState[] = [];
  const supervisor = await createWorkspaceSupervisor({
    root,
    signal: controller.signal,
    stdout: "ignore",
    stderr: "ignore",
    onEvent: (event) => {
      events.push(event);
    },
    workspace: { services: [service("a", { build: [process.execPath, "build.ts"] })] },
  });
  try {
    const pid = supervisor.states[0]!.pid!;
    writeFileSync(join(root, "build.ts"), "setInterval(()=>{},1000)");
    const first = supervisor.rebuild("a");
    await until(() => events.filter((event) => event.status === "building").length === 2);
    expect(supervisor.rebuild("a")).toBe(first);
    expect(supervisor.rebuild("a")).toBe(first);
    controller.abort();
    await supervisor.close();
    await first;
    expect(alive(pid)).toBe(false);
    expect(events.filter((event) => event.status === "building")).toHaveLength(2);
    for (const event of events) if (event.pid) expect(alive(event.pid)).toBe(false);
  } finally {
    await supervisor.close();
    rmSync(root, { recursive: true, force: true });
  }
});

test("startup readiness is bounded and unexpected exits clean peers", async () => {
  let pid = 0;
  await expect(
    createWorkspaceSupervisor({
      stdout: "ignore",
      stderr: "ignore",
      workspace: {
        services: [
          service("hang", {
            startupTimeoutMs: 50,
            ready: async ({ process, signal }) => {
              pid = process.pid;
              await new Promise<void>((resolve) =>
                signal.addEventListener("abort", () => resolve(), { once: true }),
              );
            },
          }),
        ],
      },
    }),
  ).rejects.toThrow();
  expect(alive(pid)).toBe(false);
  const supervisor = await createWorkspaceSupervisor({
    stdout: "ignore",
    stderr: "ignore",
    workspace: { services: [service("a"), service("b")] },
  });
  const pids = supervisor.states.map((state) => state.pid!);
  process.kill(pids[0]!, "SIGKILL");
  await expect(supervisor.done).rejects.toThrow("exited");
  for (const childPid of pids) expect(alive(childPid)).toBe(false);
});

test("application delegates native TS watch, carries Engine env and fresh tokens/PIDs", async () => {
  const root = mkdtempSync(join(tmpdir(), "lenso-workspace-app-"));
  mkdirSync(join(root, "src"));
  writeFileSync(join(root, "lenso.config.ts"), "export default {plugins:[]}");
  writeFileSync(
    join(root, "lenso.engine.ts"),
    `
    export default {plugins:[{name:'env',setup(context){
      context.dev('env',async event=>{
        if(event==='beforeStart')await Bun.write(context.root+'/engine-env.json',JSON.stringify({
          flag:process.env.LENSO_RUNTIME_EXTERNAL,instance:process.env.LENSO_WORKSPACE_INSTANCE
        }));
      });
    }}]};
  `,
  );
  const entry = `
    const timer=setInterval(()=>{},1000);
    await Bun.write(import.meta.dir+'/../runtime-env.json',JSON.stringify({
      flag:process.env.LENSO_RUNTIME_EXTERNAL,instance:process.env.LENSO_WORKSPACE_INSTANCE
    }));
    process.send?.({type:'lenso:dev-ready'});
    process.on('SIGTERM',()=>{clearInterval(timer);process.disconnect?.()});
  `;
  writeFileSync(join(root, "src/server.ts"), entry);
  const supervisor = await createWorkspaceSupervisor({
    root,
    stdout: "ignore",
    stderr: "ignore",
    workspace: {
      services: [
        {
          id: "frontend",
          kind: "application",
          target: { root: "." },
          env: { LENSO_RUNTIME_EXTERNAL: "1" },
        },
      ],
    },
  });
  try {
    const first = supervisor.states[0]!;
    expect(first.pid).toBeGreaterThan(0);
    const engine = await Bun.file(join(root, "engine-env.json")).json();
    const runtime = await Bun.file(join(root, "runtime-env.json")).json();
    expect(engine).toEqual({ flag: "1", instance: first.instance });
    expect(runtime).toEqual(engine);
    writeFileSync(join(root, "src/server.ts"), `${entry}\n// second version\n`);
    await until(
      () => supervisor.states[0]!.version === 2 && supervisor.states[0]!.status === "ready",
    );
    const second = supervisor.states[0]!;
    expect(second.instance).not.toBe(first.instance);
    expect(second.pid).not.toBe(first.pid);
    expect(alive(first.pid!)).toBe(false);
  } finally {
    await supervisor.close();
    rmSync(root, { recursive: true, force: true });
  }
});

test("overlapping changes queue only one serial follow-up candidate", async () => {
  let count = 0,
    running = 0,
    maxRunning = 0,
    release!: () => void;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  const supervisor = await createWorkspaceSupervisor({
    stdout: "ignore",
    stderr: "ignore",
    workspace: {
      services: [
        service("a", {
          ready: async () => {
            count++;
            running++;
            maxRunning = Math.max(maxRunning, running);
            if (count === 2) await gate;
            running--;
          },
        }),
      ],
    },
  });
  try {
    const rebuild = supervisor.rebuild("a");
    await until(() => count === 2);
    expect(supervisor.rebuild("a")).toBe(rebuild);
    expect(supervisor.rebuild("a")).toBe(rebuild);
    release();
    await rebuild;
    expect(count).toBe(3);
    expect(maxRunning).toBe(1);
    expect(supervisor.states[0]!.version).toBe(3);
  } finally {
    release();
    await supervisor.close();
  }
});

test("shutdown forces an uncooperative owned child within its drain bound", async () => {
  const root = mkdtempSync(join(tmpdir(), "lenso-workspace-drain-"));
  const supervisor = await createWorkspaceSupervisor({
    root,
    stdout: "ignore",
    stderr: "ignore",
    workspace: {
      services: [
        service("a", {
          stopTimeoutMs: 50,
          command: [
            process.execPath,
            "-e",
            "process.on('SIGTERM',()=>{});await Bun.write('ready','yes');setInterval(()=>{},1000)",
          ],
          ready: async () => {
            await until(() => Bun.file(join(root, "ready")).size > 0);
          },
        }),
      ],
    },
  });
  const pid = supervisor.states[0]!.pid!;
  try {
    await expect(supervisor.close()).rejects.toThrow("cleanup");
    await expect(supervisor.done).rejects.toThrow("cleanup");
    expect(alive(pid)).toBe(false);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("startup cancellation rolls back and an already aborted launch does not prepare", async () => {
  const controller = new AbortController();
  let pid = 0;
  const startup = createWorkspaceSupervisor({
    signal: controller.signal,
    stdout: "ignore",
    stderr: "ignore",
    workspace: {
      services: [
        service("a", {
          ready: async ({ process, signal }) => {
            pid = process.pid;
            await new Promise<void>((resolve) =>
              signal.addEventListener("abort", () => resolve(), { once: true }),
            );
          },
        }),
      ],
    },
  });
  await until(() => pid > 0);
  controller.abort(new Error("cancelled"));
  await expect(startup).rejects.toThrow();
  expect(alive(pid)).toBe(false);
  let prepared = false;
  await expect(
    createWorkspaceSupervisor({
      signal: controller.signal,
      workspace: {
        services: [],
        prepare: async () => {
          prepared = true;
        },
      },
    }),
  ).rejects.toThrow("cancelled");
  await Bun.sleep(10);
  expect(prepared).toBe(false);
});

test("reentrant close from a cancellation callback shares shutdown", async () => {
  let repeat: Promise<void> | undefined;
  const events: WorkspaceServiceState[] = [];
  const supervisor = await createWorkspaceSupervisor({
    stdout: "ignore",
    stderr: "ignore",
    onEvent: (event) => {
      events.push(event);
    },
    workspace: {
      services: [
        service("a", {
          ready: async ({ signal }) => {
            signal.addEventListener(
              "abort",
              () => {
                repeat = supervisor.close();
              },
              { once: true },
            );
          },
        }),
      ],
    },
  });
  const close = supervisor.close();
  await close;
  expect(repeat).toBe(close);
  expect(events.filter((event) => event.status === "stopped")).toHaveLength(1);
});

test("uncertain or timed-out activation fails workspace without claiming retention", async () => {
  for (const mode of ["explicit", "timeout", "exit"] as const) {
    const events: WorkspaceServiceState[] = [];
    let count = 0;
    const supervisor = await createWorkspaceSupervisor({
      stdout: "ignore",
      stderr: "ignore",
      onEvent: (event) => {
        events.push(event);
      },
      workspace: {
        services: [
          service("a", {
            startupTimeoutMs: 100,
            activate: async ({ process: child }) => {
              if (++count === 1) return;
              if (mode === "explicit")
                throw new EngineError({
                  code: "workspace-cutover-unknown",
                  phase: "workspace-dev",
                  message: "Unknown selected instance",
                });
              if (mode === "exit") child.kill("SIGKILL");
              await new Promise<void>(() => {});
            },
          }),
        ],
      },
    });
    const oldPid = supervisor.states[0]!.pid!;
    try {
      await expect(supervisor.rebuild("a")).rejects.toThrow("unknown");
      await expect(supervisor.done).rejects.toThrow("unknown");
      expect(alive(oldPid)).toBe(false);
      const failures = events.filter((event) => event.status === "failed");
      expect(failures.length).toBeGreaterThan(0);
      for (const event of failures) expect(event.retained).toBeUndefined();
      for (const event of events) if (event.pid) expect(alive(event.pid)).toBe(false);
    } finally {
      await supervisor.close();
    }
  }
});
