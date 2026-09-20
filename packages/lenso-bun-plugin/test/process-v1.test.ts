import { expect, test } from "bun:test";
import { createHash, createHmac } from "node:crypto";
import {
  HANDSHAKE_METHOD,
  PROCESS_PROFILE,
  PROVIDE_REQUEST_PROFILE,
  REQUEST_METHOD,
  SHUTDOWN_METHOD,
  VALUE_PROFILE,
  childProofMessage,
  decodeBase64Url32,
  encodeBase64Url,
  handshakeProofPayload,
  hostProofMessage,
  type HandshakeIdentity,
  type HandshakeParams,
} from "@lenso/process-protocol";
import {
  startProcessV1,
  type ProcessV1ProviderBinding,
} from "../src/index.ts";

const digest = (character: string) => `sha256:${character.repeat(64)}`;

function identity(): HandshakeIdentity {
  return {
    protocol_profile: PROCESS_PROFILE,
    value_profile: VALUE_PROFILE,
    plugin_instance: "tool-provider",
    plugin_generation: "7",
    generation_spec_digest: digest("a"),
    artifact_digest: digest("b"),
    effective_host_grant_set_digest: digest("c"),
    interaction_profiles: [PROVIDE_REQUEST_PROFILE],
    provided_capabilities: [{
      capability_id: "lenso.agent.tool-provider@1",
      descriptor_version: "1.0.0",
      descriptor_digest: digest("d"),
      operations: [{ operation: "catalog", interaction: "request" }],
    }],
    outbound_bindings: [],
    peer_limits: {
      max_http_body_bytes: 65_536,
      max_control_http_body_bytes: 16_384,
      max_concurrent_requests: 32,
      child_request_queue_capacity: 32,
      max_retired_correlation_ids: 65_536,
      control_queue_capacity: 32,
    },
  };
}

const provider: ProcessV1ProviderBinding = {
  descriptor: {
    capability_id: "lenso.agent.tool-provider@1",
    descriptor_version: "1.0.0",
    descriptor_digest: digest("d"),
    operations: ["catalog"],
    stream_operations: [],
    event_operations: [],
  },
  async invokeRequest(_operation, _context, payload, _instance) {
    return { kind: "success", value: { echoed: payload } };
  },
  validateRequest(operation) {
    if (operation !== "catalog") throw new Error("invalid request");
  },
  validateSuccess(operation) {
    if (operation !== "catalog") throw new Error("invalid success");
  },
  validateDomainError(operation) {
    if (operation !== "catalog") throw new Error("invalid Domain Error");
  },
};

test("Process V1 authenticates, dispatches, and shuts down on distinct listeners", async () => {
  const secret = new Uint8Array(32).fill(1);
  const server = startProcessV1({
    identity: identity(),
    bootstrapSecret: secret,
    providers: [provider],
    instance: Object.freeze({}),
  });
  expect(server.readiness.data_port).not.toBe(server.readiness.control_port);
  expect(JSON.parse(server.readinessLine)).toEqual(server.readiness);

  const hostNonce = encodeBase64Url(new Uint8Array(32).fill(2));
  const unsigned: HandshakeParams = {
    identity: identity(),
    host_nonce: hostNonce,
    host_proof: encodeBase64Url(new Uint8Array(32)),
  };
  const handshakeDigest = createHash("sha256")
    .update(handshakeProofPayload(unsigned))
    .digest();
  const hostProof = createHmac("sha256", secret)
    .update(hostProofMessage(handshakeDigest))
    .digest();
  const accepted = await rpc(
    server.readiness.control_port,
    "/control",
    "0",
    HANDSHAKE_METHOD,
    { ...unsigned, host_proof: encodeBase64Url(hostProof) },
  );
  const session = accepted.result.session as string;
  const childProof = createHmac("sha256", secret)
    .update(childProofMessage(handshakeDigest, session))
    .digest();
  expect(decodeBase64Url32(accepted.result.child_proof as string)).toEqual(
    new Uint8Array(childProof),
  );

  const outcome = await rpc(
    server.readiness.data_port,
    "/rpc",
    "42",
    REQUEST_METHOD,
    {
      session,
      correlation_id: "42",
      capability_id: "lenso.agent.tool-provider@1",
      descriptor_version: "1.0.0",
      descriptor_digest: digest("d"),
      operation: "catalog",
      interaction: "request",
      caller_instance: null,
      remaining_timeout_nanos: null,
      extensions: [],
      payload: { selected: true },
    },
  );
  expect(outcome.result).toEqual({
    session,
    correlation_id: "42",
    outcome: { kind: "success", value: { echoed: { selected: true } } },
  });

  const shutdown = await rpc(
    server.readiness.control_port,
    "/control",
    "1",
    SHUTDOWN_METHOD,
    { session },
  );
  expect(shutdown.result).toEqual({ session, accepted: true });
});

async function rpc(
  port: number,
  path: string,
  id: string,
  method: string,
  params: unknown,
): Promise<Record<string, any>> {
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id, method, params }),
  });
  expect(response.status).toBe(200);
  return await response.json() as Record<string, any>;
}
