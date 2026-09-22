import { expect, test } from "bun:test";
import { createHash, createHmac } from "node:crypto";
import fixture from "./conformance.json";
import strictJsonFixture from "./strict-json-conformance.json";
import {
  childProofMessage,
  encodeBase64Url,
  handshakeProofPayload,
  hostProofMessage,
  parseStrictJson,
  validateHandshakeIdentity,
  type HandshakeIdentity,
} from "@lenso/process-protocol";

const hex = (value: Uint8Array) => Buffer.from(value).toString("hex");

test("TypeScript matches the Process Protocol V1 proof vectors", () => {
  validateHandshakeIdentity(fixture.identity);
  const identity = fixture.identity as HandshakeIdentity;
  const payload = handshakeProofPayload({
    identity,
    host_nonce: fixture.host_nonce,
    host_proof: encodeBase64Url(new Uint8Array(32)),
  });
  const digest = createHash("sha256").update(payload).digest();
  const hostMessage = hostProofMessage(digest);
  const childMessage = childProofMessage(digest, fixture.session);
  const secret = Buffer.from(fixture.secret_hex, "hex");
  const proof = (message: Uint8Array) =>
    encodeBase64Url(createHmac("sha256", secret).update(message).digest());

  expect(new TextDecoder().decode(payload)).toBe(fixture.canonical_payload);
  expect(hex(digest)).toBe(fixture.handshake_digest_hex);
  expect(hex(hostMessage)).toBe(fixture.host_message_hex);
  expect(hex(childMessage)).toBe(fixture.child_message_hex);
  expect(proof(hostMessage)).toBe(fixture.host_proof);
  expect(proof(childMessage)).toBe(fixture.child_proof);
});

test("TypeScript matches the shared strict JSON conformance corpus", () => {
  for (const vector of strictJsonFixture) {
    try {
      parseStrictJson(vector.wire);
      expect(vector.accepted, vector.name).toBeTrue();
    } catch (error) {
      expect(vector.accepted, vector.name).toBeFalse();
      const message = error instanceof Error ? error.message : String(error);
      expect(message, vector.name).toBe("invalid strict JSON");
      if (vector.forbidden_error_fragment) {
        expect(message, vector.name).not.toContain(vector.forbidden_error_fragment);
      }
    }
  }
});
