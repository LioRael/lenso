import coreModule from "./guest.core.wasm";
import { instantiate } from "./guest.js";
import plan from "./plan.mjs";
import descriptorDigests from "./descriptor-digests.mjs";
import { createWorkersComponentRequestAdapter } from "./component-requests.mjs";
import { createWorkersHttpHandler } from "./workers-http.mjs";

const instanceKey = plan.plugin_instances?.[0]?.instance_key;
const component = createWorkersComponentRequestAdapter({
  plan,
  instanceKey,
  coreModule,
  instantiate,
  expectedDescriptorDigests: descriptorDigests,
});

export default { fetch: createWorkersHttpHandler(component) };
