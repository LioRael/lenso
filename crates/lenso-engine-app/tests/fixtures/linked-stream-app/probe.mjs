// Test-only observer: exercise the generated Host inside the real workerd context.
import host from "./worker.mjs";
const first = new Uint8Array([102, 105, 114, 115, 116, 0, 255]);
const equal = (a, b) => a?.length === b.length && a.every((v, i) => v === b[i]);
const check = (condition, label) => { if (!condition) throw new Error(label); };
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
async function request(mode, env, ctx) {
  return host.fetch(new Request(`https://fixture.test/events?${mode}`), env, ctx);
}
export default {
  async fetch(incoming, env, ctx) {
    if (new URL(incoming.url).pathname !== "/__probe") return host.fetch(incoming, env, ctx);
    const passed = [];
    for (let index = 0; index < 5; index++) {
      const response = await request("hold", env, ctx);
      check(response.status === 200, "incremental head");
      const reader = response.body.getReader();
      check(equal((await reader.read()).value, first), "first chunk before terminal");
      await reader.cancel();
      // Repeated requests make leaked generation leases observable here.
      const next = await request("", env, ctx);
      check(next.status === 200, "capacity restored after cancellation");
      check(equal(new Uint8Array(await next.arrayBuffer()), new Uint8Array([...first, ...new TextEncoder().encode("second")])), "next request clean terminal");
    }
    passed.push("incremental head/first chunk", "body cancellation/cleanup/capacity x5");
    const cancelled = await request("hold", env, ctx);
    const survivor = await request("", env, ctx);
    const cancelledReader = cancelled.body.getReader();
    check(equal((await cancelledReader.read()).value, first), "pending cancel first chunk");
    const pending = cancelledReader.read();
    await delay(10);
    await cancelledReader.cancel();
    await pending.catch(() => {});
    check(equal(new Uint8Array(await survivor.arrayBuffer()), new Uint8Array([...first, ...new TextEncoder().encode("second")])), "concurrent survivor after pending-read cancellation");
    passed.push("pending-read cancellation preserves concurrent session");
    for (const mode of ["fail", "domain", "oversize", "many", "missing-terminal", "hold"]) {
      const response = await request(mode, env, ctx);
      const reader = response.body.getReader();
      check(equal((await reader.read()).value, first), `first chunk: ${mode}`);
      let failed = false;
      try { while (!(await reader.read()).done) {} } catch { failed = true; }
      check(failed, `no successful EOF: ${mode}`);
      const next = await request("half-close", env, ctx);
      check(next.status === 200, `capacity restored: ${mode}`);
      check(equal(new Uint8Array(await next.arrayBuffer()), first), "half close then success terminal");
      passed.push(`failure and cleanup: ${mode}`);
    }
    // The unread body retains its lease, then the runner's session bound retires it.
    const unread = await request("hold", env, ctx);
    await delay(600);
    let timedOut = false;
    try { await unread.arrayBuffer(); } catch { timedOut = true; }
    check(timedOut, "unread response session deadline");
    const final = await request("", env, ctx);
    check(final.status === 200, "final generation ready");
    await final.arrayBuffer();
    passed.push("unread session bound and generation recovery");
    return Response.json({ passed });
  },
};
