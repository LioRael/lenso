import coreModule from "./guest.core.wasm";
import { instantiate } from "./guest.js";
import plan from "./plan.mjs";
import { createWorkersComponentRequestAdapter } from "./component-requests.mjs";
import { createWorkersHttpHandler } from "./workers-http.mjs";

const instanceKey = plan.plugin_instances?.[0]?.instance_key;
const component = createWorkersComponentRequestAdapter({
  plan,
  instanceKey,
  coreModule,
  instantiate,
});

export default { fetch: createWorkersHttpHandler(component) };
