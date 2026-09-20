import * as bindings from "./pkg/lenso_workers_g2_host.js";
import module from "./pkg/lenso_workers_g2_host_bg.wasm";
import { clearTimers } from "./clock.mjs";
import { targetQualification } from "./target-qualification.mjs";

export default {
  async test(_controller, env) {
    const cases = [];
    targetQualification((name, run) => cases.push({ name, run }), {
      bindings,
      module,
      clearTimers,
      callbackService: env.POSTGRES_CALLBACK,
      targetService: env.TARGET,
    });
    const evidence = {
      schema: "workers-target-local-workerd-v1",
      passed: true,
      cases: [],
    };
    for (const { name, run } of cases) {
      try {
        await run();
        evidence.cases.push({ name, passed: true });
      } catch (error) {
        evidence.passed = false;
        evidence.cases.push({ name, passed: false, error: String(error) });
      }
    }
    evidence.execution =
      "actual local workerd test runtime; its test Worker reaches the generated G2 Rust/Wasm target Worker through a Workerd service-binding fetch handler, and separately exercises a Workerd service-binding Host callback; no listening socket, external client, D1, PostgreSQL, Hyperdrive, deployment or production claim";
    console.log("TARGET_QUALIFICATION_EVIDENCE " + JSON.stringify(evidence));
    if (!evidence.passed) throw Error("target local qualification failed");
  },
};
