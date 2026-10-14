import { expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const cli = resolve(import.meta.dir, "../src/bin.ts");
const builtCli = resolve(import.meta.dir, "../dist/bin.js");
async function until(check: () => boolean | Promise<boolean>) {
  const deadline = Date.now() + 5000;
  while (!(await check())) {
    if (Date.now() > deadline) throw new Error("Workspace CLI fixture timed out");
    await Bun.sleep(10);
  }
}

test("explicit workspace bypasses app selection, roots at config and stops owned process", async () => {
  const root = await mkdtemp(join(tmpdir(), "lenso-cli-workspace-"));
  await Bun.write(
    join(root, "worker.ts"),
    `
    await Bun.write("instance.txt",JSON.stringify({instance:process.env.LENSO_WORKSPACE_INSTANCE,pid:process.pid}));
    setInterval(()=>{},1000);
  `,
  );
  await Bun.write(
    join(root, "workspace.ts"),
    `
    export default {services:[{
      id:'worker',kind:'process',cwd:'.',command:[process.execPath,'worker.ts'],
      ready:async({instance,signal})=>{
        while(!signal.aborted){
          try {const state=await Bun.file(import.meta.dir+'/instance.txt').json();if(state.instance===instance)return {urls:['http://localhost:4321']};}catch{}
          await Bun.sleep(10);
        }
        signal.throwIfAborted();
      }
    }]};
  `,
  );
  let output = "";
  const child = Bun.spawn(
    [process.execPath, builtCli, "dev", "--workspace", join(root, "workspace.ts")],
    {
      stdout: "pipe",
      stderr: "pipe",
      env: { ...process.env, NO_COLOR: "1" },
    },
  );
  async function collect(stream: ReadableStream<Uint8Array>) {
    for await (const chunk of stream) output += new TextDecoder().decode(chunk);
  }
  const collecting = Promise.all([collect(child.stdout), collect(child.stderr)]);
  let pid: number | undefined;
  try {
    await until(() => output.includes('"status":"ready"') && output.includes("URL:"));
    const state = await Bun.file(join(root, "instance.txt")).json();
    pid = state.pid;
    expect(output).toContain(`"instance":"${state.instance}"`);
    expect(output).toContain(`"pid":${state.pid}`);
    expect(output).toContain("URL:");
    expect(output).not.toContain("missing-application-selection");
  } finally {
    child.kill("SIGTERM");
    const timer = setTimeout(() => child.kill("SIGKILL"), 6000);
    await child.exited;
    clearTimeout(timer);
    await collecting;
    await rm(root, { recursive: true, force: true });
    if (pid) expect(() => process.kill(pid!, 0)).toThrow();
  }
  expect(child.exitCode).toBe(0);
});

test("workspace option is dev-only and rejects app/config/entry selection or JSON", async () => {
  for (const args of [
    ["check", "--workspace", "workspace.ts"],
    ["dev", "--workspace", "workspace.ts", "--app", "app"],
    ["dev", "--workspace", "workspace.ts", "--config", "app.ts"],
    ["dev", "--workspace", "workspace.ts", "--entry", "entry.ts"],
    ["dev", "--workspace", "workspace.ts", "--json"],
  ]) {
    const child = Bun.spawn([process.execPath, "--conditions=lenso-source", cli, ...args], {
      stdout: "pipe",
      stderr: "pipe",
    });
    await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text()]);
    expect(await child.exited).toBe(2);
  }
});
