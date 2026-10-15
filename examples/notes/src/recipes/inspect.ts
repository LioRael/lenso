import { inspect } from "@lenso/cli";
import { diagnostic } from "@lenso/engine/diagnostics";
import type { ApplicationTarget } from "@lenso/engine/application";

/** A recipe-specific view of existing inspect declarations, not a service registry. */
export async function inspectRecipes(target: string | ApplicationTarget) {
  try {
    const report = await inspect(target);
    const entries = report.plugins.flatMap((plugin) => {
      const values = plugin.contributions;
      return Array.isArray(values)
        ? values.flatMap((value: unknown) => {
            if (typeof value !== "object" || value === null) return [];
            const entry = value as Record<string, unknown>;
            if (
              entry.kind !== "recipe-http-entry" ||
              !["login", "private-files"].includes(String(entry.capability)) ||
              typeof entry.provider !== "string" ||
              typeof entry.path !== "string" ||
              typeof entry.method !== "string"
            )
              return [];
            return [
              {
                capability: entry.capability,
                adapter: plugin.id,
                provider: entry.provider,
                method: entry.method,
                path: entry.path,
                source: plugin.source,
              },
            ];
          })
        : [];
    });
    return {
      inspection: "static" as const,
      configPath: report.configPath,
      plugins: report.plugins.map(({ id, requires, source }) => ({ id, requires, source })),
      entries,
      capabilities: ["login", "private-files"].map((capability) => ({
        capability,
        state: entries.some((entry) => entry.capability === capability)
          ? "declared"
          : "entry-not-declared",
      })),
      cliOperations: report.operations,
      limitations: [
        "Reports only this example's explicit HTTP contributions and CLI declarations.",
        "An entry-not-declared result means no matching declaration in this selected config; it does not infer runtime routes.",
        "No setup, Config source reads, credentials, migration checks or network health checks. Declared does not mean listening.",
        "Console/MCP mounting and permission are separately configured; no Console entry is required here.",
      ],
    };
  } catch (error) {
    return { inspection: "static" as const, error: diagnostic(error) };
  }
}

if (import.meta.main) {
  const report = await inspectRecipes({ root: process.cwd(), config: "recipes.config.ts" });
  console.log(JSON.stringify(report, null, 2));
  if ("error" in report) process.exitCode = 1;
}
