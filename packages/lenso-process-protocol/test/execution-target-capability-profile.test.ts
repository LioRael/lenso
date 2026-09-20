import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import fixture from "../../../fixtures/execution-target-capability-profile/conformance.json";
import {
  EXECUTION_TARGET_CAPABILITIES,
  EXECUTION_TARGET_CAPABILITY_PROFILE,
  missingExecutionTargetCapabilities,
  supportsExecutionTargetCapability,
  validateExecutionTargetCapabilityProfile,
} from "@lenso/process-protocol";

interface ValidVector {
  readonly name: string;
  readonly profile: unknown;
  readonly required: readonly string[];
  readonly missing: readonly string[];
}

interface InvalidVector {
  readonly name: string;
  readonly profile: unknown;
  readonly error: string;
}

test("execution target capability profiles share the portable conformance vectors", () => {
  for (const vector of fixture.valid as readonly ValidVector[]) {
    validateExecutionTargetCapabilityProfile(vector.profile);
    expect(
      missingExecutionTargetCapabilities(vector.profile, vector.required),
      vector.name,
    ).toEqual(vector.missing);
  }

  for (const vector of fixture.invalid as readonly InvalidVector[]) {
    expect(() => validateExecutionTargetCapabilityProfile(vector.profile), vector.name)
      .toThrow(vector.error);
    expect(
      supportsExecutionTargetCapability(vector.profile, "request"),
      vector.name,
    ).toBeFalse();
  }
});

test("schema and runtime vocabulary stay exactly aligned", () => {
  const schema = JSON.parse(readFileSync(
    new URL("../schemas/execution-target-capability-profile-v1.schema.json", import.meta.url),
    "utf8",
  )) as {
    readonly properties: {
      readonly profile: { readonly const: string };
      readonly capabilities: { readonly items: { readonly enum: readonly string[] } };
    };
  };

  expect(schema.properties.profile.const).toBe(EXECUTION_TARGET_CAPABILITY_PROFILE);
  expect(schema.properties.capabilities.items.enum).toEqual(EXECUTION_TARGET_CAPABILITIES);
});
