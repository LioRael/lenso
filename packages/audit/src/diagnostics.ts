import { metrics } from "@opentelemetry/api";
import type { AuditReporter } from "./contracts";

export function createAuditReporter(options: {
  logger: { warn(fields: Record<string, unknown>, message: string): unknown };
}): AuditReporter {
  const failures = metrics.getMeter("@lenso/audit").createCounter("lenso.audit.write_failures", {
    description: "Audit persistence failures; no event or identity labels",
  });
  return (diagnostic) => {
    const fields = {
      mode: diagnostic.mode,
      stage: diagnostic.stage,
      code: diagnostic.code,
    };
    try {
      failures.add(1, fields);
    } finally {
      options.logger.warn(fields, "Audit persistence failed");
    }
  };
}
