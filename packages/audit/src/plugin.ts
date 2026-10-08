import type { Plugin } from "@lenso/core";
import { bindConfig, definePluginConfig, type ConfigSource } from "@lenso/core/config";
import { z } from "zod";
import type { AuditAuthority, AuditReporter, AuditRepository, SummaryPolicy } from "./contracts";
import { createAuditService, type AuditService } from "./service";

const configSchema = z.strictObject({
  maxPageSize: z.number().int().min(1).max(500).default(100),
});

export const auditConfig = definePluginConfig({
  description: "Audit query page budget; identity and scope policy are trusted service bindings",
  schema: configSchema,
});

export function createAuditPlugin<P>(options: {
  id: string;
  repository: Plugin<AuditRepository>;
  authority: Plugin<AuditAuthority<P>>;
  diagnostics?: Plugin<AuditReporter>;
  summaryPolicy?: SummaryPolicy;
  config?: z.input<typeof configSchema> | readonly ConfigSource[];
}): Plugin<AuditService<P>> {
  return bindConfig(auditConfig, options.config ?? {}, {
    id: options.id,
    requires: [
      options.repository,
      options.authority,
      ...(options.diagnostics ? [options.diagnostics] : []),
    ],
    setup(context, config) {
      return createAuditService({
        repository: context.get(options.repository),
        authority: context.get(options.authority),
        ...(options.diagnostics ? { report: context.get(options.diagnostics) } : {}),
        summaryPolicy: options.summaryPolicy,
        maxPageSize: config.maxPageSize,
      });
    },
  });
}
