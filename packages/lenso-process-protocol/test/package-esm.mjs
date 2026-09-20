import assert from "node:assert/strict";
import {
  EXECUTION_TARGET_CAPABILITY_PROFILE,
  PROCESS_PROFILE,
  encodeBase64Url,
  supportsExecutionTargetCapability,
  validateShutdownParams,
} from "@lenso/process-protocol";

assert.equal(PROCESS_PROFILE, "lenso-process-jsonrpc-http-v1");
assert.doesNotThrow(() => validateShutdownParams({
  session: encodeBase64Url(new Uint8Array(32)),
}));
assert.equal(EXECUTION_TARGET_CAPABILITY_PROFILE, "lenso.execution-target-capability-profile@1");
assert.equal(supportsExecutionTargetCapability({
  profile: EXECUTION_TARGET_CAPABILITY_PROFILE,
  target_profile: "example.request-only@1",
  capabilities: ["request"],
}, "request"), true);
