import { resolve } from "node:path";
import type { RuntimeCompanionOptions } from "@lenso/runtime/engine";

export function companionOptions(): RuntimeCompanionOptions {
  const root = process.env.LENSO_RUNTIME_ROOT;
  if (!root)
    throw new Error("Set LENSO_RUNTIME_ROOT to the lenso-runtime checkout with prebuilt binaries.");
  return {
    port: Number(process.env.LENSO_RUNTIME_PORT ?? 7331),
    kernelBinary: process.env.LENSO_RUNTIME_KERNEL ?? resolve(root, "target/debug/lenso-runtime"),
    services: [
      {
        binary: process.env.LENSO_RUNTIME_BATCH ?? resolve(root, "target/debug/logs-batch"),
        async ready(host) {
          const { services } = await host.status();
          return services.some((value) => {
            if (!value || typeof value !== "object") return false;
            const service = value as Record<string, unknown>;
            return (
              service.namespace === "demo" &&
              service.service === "batch" &&
              service.contract_version === "1" &&
              service.state === "ready"
            );
          });
        },
      },
    ],
  };
}
