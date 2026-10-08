import { experimental_CloudflareTracer as CloudflareTracer } from "@orpc/cloudflare";

const ownerKey = Symbol.for("@lenso/otel/orpc-tracer-owner");
const host = globalThis as unknown as Record<
  symbol,
  { kind: "workers" | "otel"; owner: object } | undefined
>;

/**
 * Call once from the Worker entry, with Wrangler Workers Traces enabled.
 * The platform owns export and request spans; this never registers an OTel SDK.
 */
export function bootstrapWorkerTracing(): void {
  const existing = host[ownerKey];
  if (existing?.kind === "workers") return;
  if (existing) {
    throw new Error("oRPC tracing already belongs to OpenTelemetry in this host.");
  }
  const tracer = new CloudflareTracer();
  tracer.enable();
  host[ownerKey] = { kind: "workers", owner: tracer };
}
