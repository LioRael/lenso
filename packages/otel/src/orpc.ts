import { ORPCInstrumentation, type ORPCInstrumentationConfig } from "@orpc/opentelemetry";

const key = Symbol.for("@lenso/otel/orpc-tracer-owner");
const host = globalThis as unknown as Record<symbol, { kind: string; owner: object } | undefined>;

class OwnedORPCInstrumentation extends ORPCInstrumentation {
  override enable(): void {
    const current = host[key];
    if (current && current.owner !== this) throw new Error("An oRPC tracer is already active");
    host[key] = { kind: "otel", owner: this };
    try {
      super.enable();
    } catch (error) {
      delete host[key];
      throw error;
    }
  }
  override disable(): void {
    if (host[key]?.owner !== this) return;
    super.disable();
    delete host[key];
  }
}

export function createORPCInstrumentation(options: ORPCInstrumentationConfig = {}) {
  return new OwnedORPCInstrumentation({ propagationEnabled: true, ...options });
}
