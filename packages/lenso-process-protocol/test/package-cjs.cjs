const assert = require("node:assert/strict");
const {
  EXECUTION_TARGET_CAPABILITY_PROFILE,
  PROCESS_PROFILE,
  encodeBase64Url,
  supportsExecutionTargetCapability,
  validateShutdownParams,
} = require("@lenso/process-protocol");
const schema = require("@lenso/process-protocol/schemas/execution-target-capability-profile-v1.schema.json");

assert.equal(PROCESS_PROFILE, "lenso-process-jsonrpc-http-v1");
assert.doesNotThrow(() => validateShutdownParams({
  session: encodeBase64Url(new Uint8Array(32)),
}));
assert.equal(EXECUTION_TARGET_CAPABILITY_PROFILE, "lenso.execution-target-capability-profile@1");
assert.equal(supportsExecutionTargetCapability({
  profile: EXECUTION_TARGET_CAPABILITY_PROFILE,
  target_profile: "example.request-only@1",
  capabilities: ["request"],
}, "stream"), false);
assert.equal(
  schema.$id,
  "https://lenso.dev/schemas/execution-target-capability-profile-v1.schema.json",
);
