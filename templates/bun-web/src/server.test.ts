import { expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { startServer } from "./server";

test("the real Web listener shares one service instance and releases its port", async () => {
  const { app, url } = await startServer(0);
  try {
    for (const count of [1, 2]) {
      const response = await fetch(new URL("rpc/greet", url), {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ json: { name: "Ada" } }),
      });
      expect(response.status).toBe(200);
      expect(await response.json()).toMatchObject({ json: { count } });
    }
  } finally {
    const stopping = app.stop();
    expect(app.stop()).toBe(stopping);
    await stopping;
  }
  const replacement = Bun.serve({
    hostname: "127.0.0.1",
    port: Number(url.port),
    fetch: () => new Response("released"),
  });
  await replacement.stop(true);
});

test.each(["bad-port-sentinel", "", "NaN", "Infinity", "-1", "1.5", "65536"])(
  "invalid LENSO_PORT %j fails at config preflight with a safe diagnostic",
  (port) => {
    const child = spawnSync(process.execPath, [`${import.meta.dir}/server.ts`], {
      env: { ...process.env, LENSO_PORT: port, GREETING_PREFIX: "Hello" },
      encoding: "utf8",
      timeout: 5000,
    });
    expect(child.error).toBeUndefined();
    expect(child.status).toBe(1);
    expect(child.stdout).not.toContain("Ready");
    const error = JSON.parse(child.stderr);
    expect(error).toMatchObject({ code: "config-invalid", phase: "config" });
    expect(error.causes).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          pluginId: "http",
          details: expect.objectContaining({ path: ["port"] }),
        }),
      ]),
    );
    expect(child.stderr).not.toContain("bad-port-sentinel");
    expect(child.stderr).not.toContain("RangeError");
  },
);
