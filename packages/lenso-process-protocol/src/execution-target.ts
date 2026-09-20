/** Versioned, closed feature vocabulary for execution-target profiles. */
export const EXECUTION_TARGET_CAPABILITIES = [
  "browser",
  "event",
  "host-imports",
  "native-process",
  "remote",
  "request",
  "stream",
  "wasm-component",
  "websocket",
  "workers",
] as const;

export const EXECUTION_TARGET_CAPABILITY_PROFILE =
  "lenso.execution-target-capability-profile@1" as const;

export type ExecutionTargetCapability = typeof EXECUTION_TARGET_CAPABILITIES[number];

/**
 * Explicit feature declaration for one exact Adapter or Driver profile.
 *
 * The `target_profile` names the implementation profile selected by a Host.
 * It does not imply any feature; every supported feature is listed explicitly
 * in `capabilities`.
 */
export interface ExecutionTargetCapabilityProfile {
  readonly profile: typeof EXECUTION_TARGET_CAPABILITY_PROFILE;
  readonly target_profile: string;
  /** Canonical-sorted and unique; an empty list explicitly supports nothing. */
  readonly capabilities: readonly ExecutionTargetCapability[];
}

const TOKEN = /^[A-Za-z0-9._@-]{1,128}$/;
const capabilitySet = new Set<string>(EXECUTION_TARGET_CAPABILITIES);

/** Returns whether a value belongs to profile version 1's closed vocabulary. */
export function isExecutionTargetCapability(
  value: unknown,
): value is ExecutionTargetCapability {
  return typeof value === "string" && capabilitySet.has(value);
}

/**
 * Validates the exact V1 profile shape.
 *
 * Unknown fields, unknown capabilities, omitted capability lists, and
 * non-canonical ordering are all rejected. Schema consumers should use the
 * published JSON Schema for structural validation and this helper for the
 * canonical ordering rule JSON Schema cannot express.
 */
export function validateExecutionTargetCapabilityProfile(
  value: unknown,
): asserts value is ExecutionTargetCapabilityProfile {
  const profile = record(value, "execution target capability profile");
  exactKeys(profile, ["profile", "target_profile", "capabilities"]);
  if (profile.profile !== EXECUTION_TARGET_CAPABILITY_PROFILE) {
    throw new Error("unsupported execution target capability profile");
  }
  token(profile.target_profile, "target_profile");
  if (!Array.isArray(profile.capabilities)) {
    throw new Error("capabilities must be an array");
  }
  let previous: string | undefined;
  for (const capability of profile.capabilities) {
    if (!isExecutionTargetCapability(capability)) {
      throw new Error("unknown execution target capability");
    }
    if (previous !== undefined && previous >= capability) {
      throw new Error("capabilities must be strictly canonical-sorted and unique");
    }
    previous = capability;
  }
}

/**
 * Checks one untyped requested feature without granting an implicit fallback.
 * Invalid, missing, or unknown profiles and unknown feature names all return
 * `false`.
 */
export function supportsExecutionTargetCapability(
  profile: unknown,
  capability: string,
): boolean {
  if (!isExecutionTargetCapability(capability)) return false;
  try {
    validateExecutionTargetCapabilityProfile(profile);
    return profile.capabilities.includes(capability);
  } catch {
    return false;
  }
}

/**
 * Returns every requested feature that the profile cannot prove it supports.
 *
 * Requirement order is preserved so a Host can give an actionable rejection.
 * Unknown requested feature names stay in the result and therefore fail closed.
 */
export function missingExecutionTargetCapabilities(
  profile: unknown,
  required: readonly string[],
): readonly string[] {
  return required.filter(
    (capability) => !supportsExecutionTargetCapability(profile, capability),
  );
}

function record(value: unknown, name: string): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error(`${name} must be an object`);
  }
  return value as Record<string, unknown>;
}

function exactKeys(value: Record<string, unknown>, required: readonly string[]): void {
  const allowed = new Set(required);
  for (const key of Object.keys(value)) {
    if (!allowed.has(key)) throw new Error(`unknown field ${key}`);
  }
  for (const key of required) {
    if (!Object.hasOwn(value, key)) throw new Error(`missing field ${key}`);
  }
}

function token(value: unknown, name: string): asserts value is string {
  if (typeof value !== "string" || !TOKEN.test(value)) {
    throw new Error(`${name} must be a 1..=128 byte portable token`);
  }
}
